//! Background sweep: notices when a tracked download finishes seeding, and
//! reminds once a day about anything still flagged stuck.
//!
//! Needs a live qBittorrent and gateway, so it is excluded from coverage.

use crate::downloads::{Download, Downloads};
use crate::qbittorrent::QBittorrent;
use crate::ui;
use crate::watchlist::{SECONDS_PER_DAY, now_secs};
use poise::serenity_prelude as serenity;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

pub async fn run(
    http: Arc<serenity::Http>,
    qbit: Arc<QBittorrent>,
    downloads: Arc<Downloads>,
    interval_secs: i64,
) {
    let tick = Duration::from_secs(interval_secs.max(1) as u64);
    tracing::info!(tick_secs = tick.as_secs(), "download sweep started");

    loop {
        tokio::time::sleep(tick).await;
        if let Err(e) = sweep(&http, &qbit, &downloads).await {
            tracing::error!(%e, "download sweep failed");
        }
    }
}

async fn sweep(
    http: &Arc<serenity::Http>,
    qbit: &QBittorrent,
    downloads: &Downloads,
) -> anyhow::Result<()> {
    check_completions(http, qbit, downloads).await?;
    send_stuck_digest(http, downloads).await?;
    Ok(())
}

/// One `torrents/info` call covers every tracked download: cheaper than
/// looking each one up individually, and avoids depending on tag-filter
/// support across qBittorrent versions.
async fn check_completions(
    http: &Arc<serenity::Http>,
    qbit: &QBittorrent,
    downloads: &Downloads,
) -> anyhow::Result<()> {
    let tracked = downloads.with(|s| s.all().to_vec()).await;
    if tracked.is_empty() {
        return Ok(());
    }

    let torrents = qbit.info().await?;
    for download in tracked {
        let finished = torrents
            .iter()
            .find(|t| t.has_tag(&download.tag))
            .is_some_and(|t| t.is_finished());
        if !finished {
            continue;
        }

        if let Some(done) = downloads.update(|s| s.take(download.id)).await? {
            tracing::info!(title = %done.title, "download finished");
            notify(
                http,
                done.channel_id,
                done.user_id,
                done.lang.download_ready(&done.title),
                None,
            )
            .await;
        }
    }
    Ok(())
}

async fn send_stuck_digest(
    http: &Arc<serenity::Http>,
    downloads: &Downloads,
) -> anyhow::Result<()> {
    let now = now_secs();
    let due = downloads
        .with(|s| s.due_for_digest(now, SECONDS_PER_DAY))
        .await;
    if due.is_empty() {
        return Ok(());
    }

    let mut groups: HashMap<(u64, u64), Vec<Download>> = HashMap::new();
    for download in due {
        groups
            .entry((download.user_id, download.channel_id))
            .or_default()
            .push(download);
    }

    for ((user_id, channel_id), entries) in groups {
        let lang = entries[0].lang;
        let embed = ui::stuck_digest_embed(&entries, lang);
        notify(http, channel_id, user_id, String::new(), Some(embed)).await;

        for entry in entries {
            downloads.update(|s| s.mark_digested(entry.id, now)).await?;
        }
    }
    Ok(())
}

async fn notify(
    http: &Arc<serenity::Http>,
    channel_id: u64,
    user_id: u64,
    content: String,
    embed: Option<serenity::CreateEmbed>,
) {
    let mut message = serenity::CreateMessage::new().content(format!("<@{user_id}> {content}"));
    if let Some(embed) = embed {
        message = message.embed(embed);
    }

    if let Err(e) = serenity::ChannelId::new(channel_id)
        .send_message(http, message)
        .await
    {
        tracing::warn!(channel = channel_id, %e, "could not deliver a download notification");
    }
}
