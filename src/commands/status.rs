use crate::i18n::Lang;
use crate::{Context, Error, ui};

/// Check that Prowlarr and qBittorrent are reachable.
#[poise::command(
    slash_command,
    name_localized("fr", "etat"),
    description_localized("fr", "Vérifie que Prowlarr et qBittorrent répondent.")
)]
pub async fn status(ctx: Context<'_>) -> Result<(), Error> {
    ctx.defer_ephemeral().await?;
    let data = ctx.data();
    let lang = Lang::from_locale(ctx.locale(), data.config.default_locale);

    let (prowlarr, qbit) = tokio::join!(data.prowlarr.ping(), data.qbit.version());
    let category = &data.config.qbit_category;
    let destination = match data.qbit.categories().await {
        Ok(categories) => ui::destination_label(&categories, category, lang),
        Err(e) => format!("❌ {e}"),
    };

    ctx.send(poise::CreateReply::default().embed(ui::status_embed(
        &prowlarr,
        &qbit,
        category,
        &destination,
        lang,
    )))
    .await?;
    Ok(())
}
