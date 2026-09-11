use crate::i18n::Lang;
use crate::prowlarr::Release;
use crate::watchlist::{RejectedWatch, now_secs};
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
    let data = ctx.data();
    let lang = Lang::from_locale(ctx.locale(), data.config.default_locale);

    let mut results = data
        .prowlarr
        .search(&query, &data.config.search_categories, SEARCH_LIMIT)
        .await?;
    results.truncate(data.config.max_results);

    if results.is_empty() {
        return offer_to_watch(ctx, &query, lang).await;
    }

    let custom_id = format!("search:{}", ctx.id());
    let handle = ctx
        .send(
            poise::CreateReply::default()
                .embed(ui::results_embed(&query, &results, lang))
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
                    .embed(ui::expired_embed(&query, &results, lang))
                    .components(vec![]),
            )
            .await?;
        return Ok(());
    };

    let serenity::ComponentInteractionDataKind::StringSelect { values } = &interaction.data.kind
    else {
        return Ok(());
    };
    let picked = ui::parse_selection(values, &results).ok_or(lang.invalid_selection())?;

    // Adding takes longer than the 3 s Discord allows for a response.
    interaction
        .create_response(ctx, serenity::CreateInteractionResponse::Acknowledge)
        .await?;

    let embed = match send_to_client(ctx, picked).await {
        Ok(save_path) => ui::added_embed(picked, &data.config.qbit_category, &save_path, lang),
        Err(e) => ui::failed_embed(picked, &e.to_string(), lang),
    };

    interaction
        .edit_response(
            ctx,
            serenity::EditInteractionResponse::new()
                .embed(embed)
                .components(vec![]),
        )
        .await?;
    Ok(())
}

async fn send_to_client(ctx: Context<'_>, release: &Release) -> anyhow::Result<String> {
    let data = ctx.data();
    let download = data.prowlarr.fetch(release).await?;
    data.qbit
        .add(&download, &data.config.qbit_category, None, false)
        .await
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

    let data = ctx.data();
    let outcome = data
        .watchlist
        .update(|state| {
            state
                .add(
                    ctx.author().id.get(),
                    ctx.channel_id().get(),
                    query,
                    lang,
                    now_secs(),
                    data.config.watch_max_per_user,
                )
                .map(|_| ())
        })
        .await?;

    let message = match outcome {
        Ok(()) => lang.watch_created(
            query,
            data.config.watch_checks_per_day,
            data.config.watch_max_days,
        ),
        Err(RejectedWatch::AlreadyWatching) => lang.watch_already(query),
        Err(RejectedWatch::TooMany) => lang.watch_too_many(data.config.watch_max_per_user),
    };

    interaction
        .create_response(
            ctx,
            serenity::CreateInteractionResponse::UpdateMessage(
                serenity::CreateInteractionResponseMessage::new()
                    .content(message)
                    .components(vec![]),
            ),
        )
        .await?;
    Ok(())
}
