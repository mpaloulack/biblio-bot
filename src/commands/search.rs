use crate::downloads::NewDownload;
use crate::i18n::Lang;
use crate::prowlarr::Release;
use crate::watchlist::{NewWatch, RejectedWatch, now_secs};
use crate::{Context, Error, ui};
use poise::serenity_prelude as serenity;
use std::time::Duration;

const SELECTION_TIMEOUT: Duration = Duration::from_secs(120);
/// Asked of Prowlarr before trimming to `max_results`.
const SEARCH_LIMIT: u32 = 100;

/// Search for an ebook and send it to qBittorrent.
#[poise::command(
    slash_command,
    name_localized("fr", "livre"),
    description_localized("fr", "Cherche un ebook et l'envoie dans qBittorrent.")
)]
pub async fn search(
    ctx: Context<'_>,
    #[description = "Title, author, series…"]
    #[name_localized("fr", "recherche")]
    #[description_localized("fr", "Titre, auteur, série…")]
    query: String,
) -> Result<(), Error> {
    ctx.defer().await?;
    let lang = Lang::from_locale(ctx.locale(), ctx.data().config.default_locale);
    run_search(ctx, &query, lang).await
}

/// Runs a search end to end: shows the picker, sends the pick to
/// qBittorrent. Split out of [`search`] so `/stuck`'s "search again" can
/// re-run a stored query without duplicating this whole flow.
pub(crate) async fn run_search(ctx: Context<'_>, query: &str, lang: Lang) -> Result<(), Error> {
    let data = ctx.data();

    let mut results = data
        .prowlarr
        .search(query, &data.config.search_categories, SEARCH_LIMIT)
        .await?;
    let found = results.len();
    results.truncate(data.config.max_results);
    tracing::info!(query = %query, found, offered = results.len(), "search completed");

    if results.is_empty() {
        return offer_to_watch(ctx, query, lang).await;
    }

    let custom_id = format!("search:{}", ctx.id());
    let handle = ctx
        .send(
            poise::CreateReply::default()
                .embed(ui::results_embed(query, &results, lang))
                .components(vec![serenity::CreateActionRow::SelectMenu(
                    serenity::CreateSelectMenu::new(
                        &custom_id,
                        serenity::CreateSelectMenuKind::String {
                            options: ui::select_options(&results, lang),
                        },
                    )
                    .placeholder(lang.menu_placeholder()),
                )]),
        )
        .await?;

    let interaction = serenity::ComponentInteractionCollector::new(ctx)
        .author_id(ctx.author().id)
        .channel_id(ctx.channel_id())
        .timeout(SELECTION_TIMEOUT)
        .filter({
            let custom_id = custom_id.clone();
            move |mci| mci.data.custom_id == custom_id
        })
        .await;

    let Some(interaction) = interaction else {
        handle
            .edit(
                ctx,
                poise::CreateReply::default()
                    .content(lang.selection_timed_out())
                    .embed(ui::expired_embed(query, &results, lang))
                    .components(vec![]),
            )
            .await?;
        return Ok(());
    };

    let serenity::ComponentInteractionDataKind::StringSelect { values } = &interaction.data.kind
    else {
        return Ok(());
    };
    let clicked = std::time::Instant::now();
    let picked = ui::parse_selection(values, &results).ok_or(lang.invalid_selection())?;
    tracing::info!(
        query = %query,
        title = %picked.title,
        indexer = %picked.indexer,
        size = picked.size,
        "release picked"
    );

    // Adding takes longer than the 3 s Discord allows for a response.
    interaction
        .create_response(ctx, serenity::CreateInteractionResponse::Acknowledge)
        .await?;

    let (embed, note) = match send_to_client(ctx, picked, query, lang).await {
        Ok(save_path) => {
            tracing::info!(
                title = %picked.title,
                category = %data.config.qbit_category,
                save_path = %save_path,
                "sent to qbittorrent"
            );
            // Downloading is what closes a standing search, so clear any the
            // user was keeping for these words.
            let fulfilled = data
                .watchlist
                .update(|state| state.fulfil(ctx.author().id.get(), query))
                .await?;
            let note = fulfilled
                .first()
                .map(|watch| lang.watch_fulfilled(&watch.query))
                .unwrap_or_default();
            (
                ui::added_embed(picked, &data.config.qbit_category, &save_path, lang),
                note,
            )
        }
        Err(e) => {
            tracing::warn!(title = %picked.title, %e, "could not send to qbittorrent");
            (
                ui::failed_embed(picked, &e.to_string(), lang),
                String::new(),
            )
        }
    };
    tracing::debug!(
        elapsed_ms = clicked.elapsed().as_millis(),
        "answering the click"
    );

    interaction
        .edit_response(
            ctx,
            serenity::EditInteractionResponse::new()
                .content(note)
                .embed(embed)
                .components(vec![]),
        )
        .await?;
    Ok(())
}

/// Reserves a [`crate::downloads::Download`] record before handing the
/// release to qBittorrent, tagging the torrent with it so the background
/// sweep can find it again later. Rolls the reservation back if the send
/// fails, so a failed download never leaves a phantom tracked entry.
async fn send_to_client(
    ctx: Context<'_>,
    release: &Release,
    query: &str,
    lang: Lang,
) -> anyhow::Result<String> {
    let data = ctx.data();
    let download = data.prowlarr.fetch(release).await?;

    let record = data
        .downloads
        .update(|s| {
            s.add(
                NewDownload {
                    user_id: ctx.author().id.get(),
                    channel_id: ctx.channel_id().get(),
                    guild_id: ctx.guild_id().map(|g| g.get()),
                    query: query.to_owned(),
                    title: release.title.clone(),
                    lang,
                },
                now_secs(),
            )
            .clone()
        })
        .await?;

    match data
        .qbit
        .add(
            &download,
            &data.config.qbit_category,
            None,
            false,
            Some(&record.tag),
        )
        .await
    {
        Ok(save_path) => Ok(save_path),
        Err(e) => {
            let _ = data.downloads.update(|s| s.take(record.id)).await;
            Err(e)
        }
    }
}

/// A search that found nothing can be left running instead of being retyped later.
async fn offer_to_watch(ctx: Context<'_>, query: &str, lang: Lang) -> Result<(), Error> {
    let custom_id = format!("watch:{}", ctx.id());
    let handle = ctx
        .send(
            poise::CreateReply::default()
                .content(lang.no_results(query))
                .components(vec![ui::watch_button(&custom_id, lang)]),
        )
        .await?;

    let interaction = serenity::ComponentInteractionCollector::new(ctx)
        .author_id(ctx.author().id)
        .channel_id(ctx.channel_id())
        .timeout(SELECTION_TIMEOUT)
        .filter({
            let custom_id = custom_id.clone();
            move |mci| mci.data.custom_id == custom_id
        })
        .await;

    let Some(interaction) = interaction else {
        handle
            .edit(
                ctx,
                poise::CreateReply::default()
                    .content(lang.no_results(query))
                    .components(vec![]),
            )
            .await?;
        return Ok(());
    };

    // Discord allows three seconds to answer a click, and persisting the watch
    // happens after that. Acknowledge first, then edit in the outcome.
    interaction
        .create_response(ctx, serenity::CreateInteractionResponse::Acknowledge)
        .await?;

    let data = ctx.data();
    let outcome = data
        .watchlist
        .update(|state| {
            state
                .add(
                    NewWatch {
                        user_id: ctx.author().id.get(),
                        channel_id: ctx.channel_id().get(),
                        guild_id: ctx.guild_id().map(|g| g.get()),
                        query: query.to_owned(),
                        lang,
                    },
                    now_secs(),
                    data.config.watch_max_per_user,
                )
                .map(|_| ())
        })
        .await?;

    let message = match outcome {
        Ok(()) => {
            tracing::info!(
                query = %query,
                user_id = ctx.author().id.get(),
                channel = ctx.channel_id().get(),
                "watch created"
            );
            lang.watch_created(
                query,
                data.config.watch_checks_per_day,
                data.config.watch_max_days,
            )
        }
        Err(reason) => {
            tracing::info!(query = %query, ?reason, "watch refused");
            match reason {
                RejectedWatch::AlreadyWatching => lang.watch_already(query),
                RejectedWatch::TooMany => lang.watch_too_many(data.config.watch_max_per_user),
            }
        }
    };

    interaction
        .edit_response(
            ctx,
            serenity::EditInteractionResponse::new()
                .content(message)
                .components(vec![]),
        )
        .await?;
    Ok(())
}
