//! Background sweep that re-runs standing searches.
//!
//! Decides nothing on its own: what is due and what has expired comes from
//! [`crate::watchlist`], which is tested. Needs a live gateway, so it is
//! excluded from coverage.

use crate::config::Config;
use crate::prowlarr::Prowlarr;
use crate::ui;
use crate::watchlist::{Watch, Watchlist, now_secs};
use poise::serenity_prelude as serenity;
use std::sync::Arc;
use std::time::Duration;

/// Ticks never sleep longer than this, so a watch is picked up close to its due
/// time even when the configured interval is a whole day.
const MAX_TICK: Duration = Duration::from_secs(15 * 60);
const SEARCH_LIMIT: u32 = 100;

pub async fn run(
    http: Arc<serenity::Http>,
    prowlarr: Prowlarr,
    watchlist: Arc<Watchlist>,
    config: Config,
) {
    let interval = config.watch_interval_secs();
    let tick = Duration::from_secs(interval.max(1) as u64).min(MAX_TICK);
    tracing::info!(
        checks_per_day = config.watch_checks_per_day,
        tick_secs = tick.as_secs(),
        "watch sweep started"
    );

    loop {
        tokio::time::sleep(tick).await;
        if let Err(e) = sweep(&http, &prowlarr, &watchlist, &config, interval).await {
            tracing::error!(%e, "watch sweep failed");
        }
    }
}

async fn sweep(
    http: &Arc<serenity::Http>,
    prowlarr: &Prowlarr,
    watchlist: &Watchlist,
    config: &Config,
    interval: i64,
) -> anyhow::Result<()> {
    let now = now_secs();

    for watch in watchlist
        .update(|s| s.prune_expired(now, config.watch_max_age_secs()))
        .await?
    {
        let message = watch
            .lang
            .watch_gave_up(&watch.query, config.watch_max_days);
        notify(http, &watch, message, None).await;
    }

    for watch in watchlist.with(|s| s.due(now, interval)).await {
        match prowlarr
            .search(&watch.query, &config.search_categories, SEARCH_LIMIT)
            .await
        {
            Ok(mut results) if !results.is_empty() => {
                results.truncate(config.max_results);
                let message = watch.lang.watch_found(&watch.query);
                let embed = ui::results_embed(&watch.query, &results, watch.lang);
                notify(http, &watch, message, Some(embed)).await;
                // Kept until the user actually downloads it; marking it only
                // stops the searching.
                watchlist.update(|s| s.mark_notified(watch.id, now)).await?;
            }
            Ok(_) => {
                watchlist.update(|s| s.mark_checked(watch.id, now)).await?;
            }
            Err(e) => {
                // Counting a failed sweep as a check keeps a broken Prowlarr
                // from being retried on every single tick.
                tracing::warn!(query = %watch.query, %e, "watch check failed");
                watchlist.update(|s| s.mark_checked(watch.id, now)).await?;
            }
        }
    }
    Ok(())
}

async fn notify(
    http: &Arc<serenity::Http>,
    watch: &Watch,
    content: String,
    embed: Option<serenity::CreateEmbed>,
) {
    let mut message =
        serenity::CreateMessage::new().content(format!("<@{}> {content}", watch.user_id));
    if let Some(embed) = embed {
        message = message.embed(embed);
    }

    if let Err(e) = serenity::ChannelId::new(watch.channel_id)
        .send_message(http, message)
        .await
    {
        tracing::warn!(channel = watch.channel_id, %e, "could not deliver a watch notification");
    }
}
