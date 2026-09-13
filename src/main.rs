//! Bootstrap only; everything testable lives in the library crate.

use anyhow::{Context as _, Result};
use biblio_bot::downloads::Downloads;
use biblio_bot::watchlist::Watchlist;
use biblio_bot::{
    Context, Data, Error, commands, config::Config, download_watcher, prowlarr::Prowlarr,
    qbittorrent::QBittorrent, watcher,
};
use poise::serenity_prelude as serenity;
use std::sync::Arc;

#[tokio::main]
async fn main() -> Result<()> {
    // In a container everything comes from the environment instead.
    let _ = dotenvy::dotenv();
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "biblio_bot=info,serenity=warn".into()),
        )
        .init();

    let config = Config::from_env()?;
    tracing::info!(version = env!("CARGO_PKG_VERSION"), "starting");
    tracing::info!("{}", config.summary());
    let prowlarr = Prowlarr::new(&config.prowlarr_url, &config.prowlarr_api_key)?;
    // Arc'd: shared with the download sweep below, so both use the same
    // logged-in session instead of each holding an independent one.
    let qbit = Arc::new(QBittorrent::new(
        &config.qbit_url,
        config.qbit_user.clone(),
        config.qbit_pass.clone(),
    )?);

    // Fail now rather than on the first command.
    let version = prowlarr
        .ping()
        .await
        .context("Prowlarr unreachable at startup")?;
    tracing::info!(%version, "Prowlarr reachable");
    let version = qbit
        .version()
        .await
        .context("qBittorrent unreachable at startup")?;
    tracing::info!(%version, "qBittorrent reachable");

    let watchlist = Arc::new(Watchlist::load(&config.watchlist_path).await?);
    tracing::info!(
        watches = watchlist.with(|s| s.len()).await,
        path = %config.watchlist_path.display(),
        "watchlist loaded"
    );
    let downloads = Arc::new(Downloads::load(&config.downloads_path).await?);
    tracing::info!(
        downloads = downloads.with(|s| s.len()).await,
        path = %config.downloads_path.display(),
        "downloads loaded"
    );

    let token = config.discord_token.clone();
    let guild_id = config.guild_id;
    // Moved into the setup closure, which is where a gateway http client exists.
    let watch_sweep = (prowlarr.clone(), Arc::clone(&watchlist), config.clone());
    let download_sweep = (
        Arc::clone(&qbit),
        Arc::clone(&downloads),
        config.download_check_interval_secs,
    );
    let data = Data {
        config,
        prowlarr,
        qbit,
        watchlist,
        downloads,
    };

    let framework = poise::Framework::builder()
        .options(poise::FrameworkOptions {
            commands: vec![
                commands::search(),
                commands::status(),
                commands::stuck(),
                commands::watchlist(),
                commands::watchlist_all(),
            ],
            pre_command: |ctx| Box::pin(log_invocation(ctx)),
            post_command: |ctx| Box::pin(log_completion(ctx)),
            on_error: |error| Box::pin(on_error(error)),
            ..Default::default()
        })
        .setup(move |ctx, ready, framework| {
            Box::pin(async move {
                let registry = &framework.options().commands;
                // Guild scoped is instant; global takes up to an hour to propagate.
                match guild_id {
                    Some(id) => {
                        poise::builtins::register_in_guild(
                            ctx,
                            registry,
                            serenity::GuildId::new(id),
                        )
                        .await?;
                        tracing::info!(guild = id, "commands registered in guild");
                    }
                    None => {
                        poise::builtins::register_globally(ctx, registry).await?;
                        tracing::info!("commands registered globally");
                    }
                }
                let (prowlarr, watchlist, config) = watch_sweep;
                tokio::spawn(watcher::run(ctx.http.clone(), prowlarr, watchlist, config));
                let (qbit, downloads, interval_secs) = download_sweep;
                tokio::spawn(download_watcher::run(
                    ctx.http.clone(),
                    qbit,
                    downloads,
                    interval_secs,
                ));

                tracing::info!(bot = %ready.user.name, "connected");
                Ok(data)
            })
        })
        .build();

    let mut client =
        serenity::ClientBuilder::new(token, serenity::GatewayIntents::non_privileged())
            .framework(framework)
            .await
            .context("could not build the Discord client")?;

    let shard_manager = Arc::clone(&client.shard_manager);
    tokio::spawn(async move {
        if tokio::signal::ctrl_c().await.is_ok() {
            tracing::info!("shutdown requested");
            shard_manager.shutdown_all().await;
        }
    });

    client.start().await.context("the Discord client stopped")?;
    Ok(())
}

async fn log_invocation(ctx: Context<'_>) {
    ctx.set_invocation_data(std::time::Instant::now()).await;
    tracing::info!(
        command = %ctx.command().qualified_name,
        user = %ctx.author().name,
        user_id = ctx.author().id.get(),
        guild = ctx.guild_id().map_or(0, |g| g.get()),
        channel = ctx.channel_id().get(),
        "command received"
    );
}

async fn log_completion(ctx: Context<'_>) {
    // Includes however long the user took to pick from a menu, so it says how
    // long the whole exchange lasted rather than how much work was done.
    let elapsed = ctx
        .invocation_data::<std::time::Instant>()
        .await
        .map_or(0, |start| start.elapsed().as_millis());
    tracing::info!(
        command = %ctx.command().qualified_name,
        elapsed_ms = elapsed,
        "command completed"
    );
}

async fn on_error(error: poise::FrameworkError<'_, Data, Error>) {
    match error {
        poise::FrameworkError::Command {
            ref error, ref ctx, ..
        } => {
            tracing::error!(command = %ctx.command().qualified_name, %error, "command failed");
            let _ = ctx
                .send(
                    poise::CreateReply::default()
                        .content(format!("❌ {error}"))
                        .ephemeral(true),
                )
                .await;
        }
        other => {
            if let Err(e) = poise::builtins::on_error(other).await {
                tracing::error!(%e, "error while handling an error");
            }
        }
    }
}
