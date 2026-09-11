use crate::i18n::Lang;
use crate::watchlist::{Watch, now_secs};
use crate::{Context, Error, ui};
use poise::serenity_prelude as serenity;
use std::time::Duration;

const SELECTION_TIMEOUT: Duration = Duration::from_secs(120);

/// List the searches still running for you, and stop one.
#[poise::command(
    slash_command,
    name_localized("fr", "veilles"),
    description_localized("fr", "Liste tes recherches en cours et permet d'en arrêter une.")
)]
pub async fn watchlist(ctx: Context<'_>) -> Result<(), Error> {
    ctx.defer_ephemeral().await?;
    let data = ctx.data();
    let lang = Lang::from_locale(ctx.locale(), data.config.default_locale);
    let now = now_secs();

    let watches: Vec<Watch> = data
        .watchlist
        .with(|state| {
            state
                .for_user(ctx.author().id.get())
                .into_iter()
                .cloned()
                .collect()
        })
        .await;

    let embed = ui::watchlist_embed(&watches, now, data.config.watch_max_age_secs(), lang);
    if watches.is_empty() {
        ctx.send(poise::CreateReply::default().embed(embed)).await?;
        return Ok(());
    }

    let custom_id = format!("unwatch:{}", ctx.id());
    let handle = ctx
        .send(poise::CreateReply::default().embed(embed).components(
            vec![serenity::CreateActionRow::SelectMenu(
                    serenity::CreateSelectMenu::new(
                        &custom_id,
                        serenity::CreateSelectMenuKind::String {
                            options: ui::watchlist_options(&watches, lang),
                        },
                    )
                    .placeholder(lang.watchlist_placeholder()),
                )],
        ))
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
            .edit(ctx, poise::CreateReply::default().components(vec![]))
            .await?;
        return Ok(());
    };

    let serenity::ComponentInteractionDataKind::StringSelect { values } = &interaction.data.kind
    else {
        return Ok(());
    };
    let Some(id) = values.first().and_then(|v| v.parse::<u64>().ok()) else {
        return Ok(());
    };

    let removed = data
        .watchlist
        .update(|state| state.remove(id, ctx.author().id.get()))
        .await?;

    let content = match removed {
        Some(watch) => lang.watch_stopped(&watch.query),
        None => lang.watchlist_empty().to_owned(),
    };

    interaction
        .create_response(
            ctx,
            serenity::CreateInteractionResponse::UpdateMessage(
                serenity::CreateInteractionResponseMessage::new()
                    .content(content)
                    .embed(serenity::CreateEmbed::new().title(lang.watchlist_title()))
                    .components(vec![]),
            ),
        )
        .await?;
    Ok(())
}
