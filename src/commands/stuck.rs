use crate::commands::search::run_search;
use crate::downloads::Download;
use crate::i18n::Lang;
use crate::watchlist::now_secs;
use crate::{Context, Error, ui};
use poise::serenity_prelude as serenity;
use std::time::Duration;

const SELECTION_TIMEOUT: Duration = Duration::from_secs(120);

/// Flag a stuck download, or resolve one already flagged.
#[poise::command(
    slash_command,
    name_localized("fr", "bloque"),
    description_localized(
        "fr",
        "Signale un téléchargement bloqué, ou résous-en un déjà signalé."
    )
)]
pub async fn stuck(ctx: Context<'_>) -> Result<(), Error> {
    ctx.defer_ephemeral().await?;
    let data = ctx.data();
    let lang = Lang::from_locale(ctx.locale(), data.config.default_locale);
    let user_id = ctx.author().id.get();

    let downloads: Vec<Download> = data
        .downloads
        .with(|s| s.for_user(user_id).into_iter().cloned().collect())
        .await;

    if downloads.is_empty() {
        ctx.send(poise::CreateReply::default().content(lang.stuck_none()))
            .await?;
        return Ok(());
    }

    let custom_id = format!("stuck:{}", ctx.id());
    let handle = ctx
        .send(
            poise::CreateReply::default()
                .embed(ui::stuck_list_embed(&downloads, lang))
                .components(vec![serenity::CreateActionRow::SelectMenu(
                    serenity::CreateSelectMenu::new(
                        &custom_id,
                        serenity::CreateSelectMenuKind::String {
                            options: ui::stuck_options(&downloads, lang),
                        },
                    )
                    .placeholder(lang.stuck_placeholder()),
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
                    .embed(ui::stuck_list_embed(&downloads, lang))
                    .components(vec![]),
            )
            .await?;
        return Ok(());
    };

    let serenity::ComponentInteractionDataKind::StringSelect { values } = &interaction.data.kind
    else {
        return Ok(());
    };
    let Some(picked) = values
        .first()
        .and_then(|v| v.parse::<u64>().ok())
        .and_then(|id| downloads.iter().find(|d| d.id == id))
    else {
        return Ok(());
    };

    interaction
        .create_response(ctx, serenity::CreateInteractionResponse::Acknowledge)
        .await?;

    if picked.stuck_marked_at.is_none() {
        data.downloads
            .update(|s| s.mark_stuck(picked.id, user_id, now_secs()))
            .await?;
        interaction
            .edit_response(
                ctx,
                serenity::EditInteractionResponse::new()
                    .content(lang.stuck_marked(&picked.title))
                    .embed(ui::stuck_list_embed(&downloads, lang))
                    .components(vec![]),
            )
            .await?;
        return Ok(());
    }

    resolve_stuck(ctx, &interaction, picked, lang).await
}

/// Offers to remove an already-flagged download, or replace it by re-running
/// its original search.
async fn resolve_stuck(
    ctx: Context<'_>,
    interaction: &serenity::ComponentInteraction,
    picked: &Download,
    lang: Lang,
) -> Result<(), Error> {
    let action_id = format!("stuck-act:{}", ctx.id());
    interaction
        .edit_response(
            ctx,
            serenity::EditInteractionResponse::new()
                .content(lang.stuck_prompt(&picked.title))
                .embed(ui::stuck_list_embed(std::slice::from_ref(picked), lang))
                .components(vec![ui::stuck_actions(&action_id, lang)]),
        )
        .await?;

    let action = serenity::ComponentInteractionCollector::new(ctx)
        .author_id(ctx.author().id)
        .channel_id(ctx.channel_id())
        .timeout(SELECTION_TIMEOUT)
        .filter({
            let action_id = action_id.clone();
            move |mci| mci.data.custom_id.starts_with(&action_id)
        })
        .await;

    let Some(action) = action else {
        interaction
            .edit_response(
                ctx,
                serenity::EditInteractionResponse::new()
                    .content(lang.selection_timed_out())
                    .components(vec![]),
            )
            .await?;
        return Ok(());
    };

    action
        .create_response(ctx, serenity::CreateInteractionResponse::Acknowledge)
        .await?;
    let search_again = action.data.custom_id.ends_with(":search");

    let data = ctx.data();
    data.qbit.delete_by_tag(&picked.tag).await.ok();
    data.downloads
        .update(|s| s.remove(picked.id, ctx.author().id.get()))
        .await?;

    action
        .edit_response(
            ctx,
            serenity::EditInteractionResponse::new()
                .content(lang.stuck_removed(&picked.title))
                .components(vec![]),
        )
        .await?;

    if search_again {
        run_search(ctx, &picked.query, lang).await?;
    }
    Ok(())
}
