//! Bootstrap only; everything testable lives in the library crate.

use anyhow::{Context as _, Result};
use biblio_bot::watchlist::Watchlist;
use biblio_bot::{
    Data, Error, commands, config::Config, prowlarr::Prowlarr, qbittorrent::QBittorrent, watcher,
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
    let prowlarr = Prowlarr::new(&config.prowlarr_url, &config.prowlarr_api_key)?;
    let qbit = QBittorrent::new(
        &config.qbit_url,
        config.qbit_user.clone(),
        config.qbit_pass.clone(),
    )?;

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

    let token = config.discord_token.clone();
    let guild_id = config.guild_id;
    // Moved into the setup closure, which is where a gateway http client exists.
    let sweep = (prowlarr.clone(), Arc::clone(&watchlist), config.clone());
    let data = Data {
        config,
        prowlarr,
        qbit,
        watchlist,
    };

    let framework = poise::Framework::builder()
        .options(poise::FrameworkOptions {
            commands: vec![
                commands::search(),
                commands::status(),
                commands::watchlist(),
                commands::watchlist_all(),
            ],
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
                let (prowlarr, watchlist, config) = sweep;
                tokio::spawn(watcher::run(ctx.http.clone(), prowlarr, watchlist, config));

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
