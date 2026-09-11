use crate::i18n::Lang;
use crate::watchlist::{Watch, now_secs};
use crate::{Context, Error, ui};
use poise::serenity_prelude as serenity;
use std::time::Duration;

const SELECTION_TIMEOUT: Duration = Duration::from_secs(120);

/// Who a listing belongs to, and therefore what stopping an entry is allowed
/// to touch.
enum Scope {
    /// Only the caller's own watches.
    Own(u64),
    /// Every watch in this server, for someone who moderates it.
    Guild(u64),
}

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
    let user_id = ctx.author().id.get();

    let watches: Vec<Watch> = data
        .watchlist
        .with(|state| state.for_user(user_id).into_iter().cloned().collect())
        .await;

    tracing::info!(user_id, watches = watches.len(), "listing own watches");
    let embed = ui::watchlist_embed(&watches, now_secs(), data.config.watch_max_age_secs(), lang);
    let options = ui::watchlist_options(&watches, lang);
    show_and_stop(ctx, lang, embed, options, &watches, Scope::Own(user_id)).await
}

/// List every standing search on this server, and stop any of them.
#[poise::command(
    slash_command,
    rename = "watchlist-all",
    guild_only,
    // default_member_permissions hides it in the Discord UI, required_permissions
    // enforces it at dispatch. poise treats the two as independent, so a command
    // that sets only the latter is still offered to everyone.
    default_member_permissions = "MANAGE_GUILD",
    required_permissions = "MANAGE_GUILD",
    name_localized("fr", "veilles-serveur"),
    description_localized(
        "fr",
        "Liste toutes les recherches du serveur et permet d'en arrêter une."
    )
)]
pub async fn watchlist_all(ctx: Context<'_>) -> Result<(), Error> {
    ctx.defer_ephemeral().await?;
    let data = ctx.data();
    let lang = Lang::from_locale(ctx.locale(), data.config.default_locale);
    // guild_only guarantees this is set.
    let guild_id = ctx
        .guild_id()
        .ok_or("this command only works in a server")?
        .get();

    let watches: Vec<Watch> = data
        .watchlist
        .with(|state| state.for_guild(guild_id).into_iter().cloned().collect())
        .await;

    tracing::info!(
        guild_id,
        watches = watches.len(),
        "listing every watch on the server"
    );
    let embed =
        ui::admin_watchlist_embed(&watches, now_secs(), data.config.watch_max_age_secs(), lang);
    let options = ui::admin_watchlist_options(&watches, lang);
    show_and_stop(ctx, lang, embed, options, &watches, Scope::Guild(guild_id)).await
}

async fn show_and_stop(
    ctx: Context<'_>,
    lang: Lang,
    embed: serenity::CreateEmbed,
    options: Vec<serenity::CreateSelectMenuOption>,
    watches: &[Watch],
    scope: Scope,
) -> Result<(), Error> {
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
                        serenity::CreateSelectMenuKind::String { options },
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

    let removed = ctx
        .data()
        .watchlist
        .update(|state| match scope {
            Scope::Own(user_id) => state.remove(id, user_id),
            Scope::Guild(guild_id) => state.remove_within_guild(id, guild_id),
        })
        .await?;

    match &removed {
        Some(watch) => tracing::info!(
            watch = watch.id,
            query = %watch.query,
            owner = watch.user_id,
            by = ctx.author().id.get(),
            "watch stopped"
        ),
        None => tracing::warn!(watch = id, "watch to stop was not found or not allowed"),
    }

    let content = match (removed, scope) {
        (Some(watch), Scope::Own(_)) => lang.watch_stopped(&watch.query),
        (Some(watch), Scope::Guild(_)) => lang.watch_stopped_for(&watch.query, watch.user_id),
        (None, _) => lang.watchlist_empty().to_owned(),
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
