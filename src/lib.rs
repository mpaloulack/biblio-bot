//! Everything except the Discord gateway glue, so it can be unit tested
//! without a live connection.

pub mod commands;
pub mod config;
pub mod download_watcher;
pub mod downloads;
pub mod i18n;
pub mod prowlarr;
pub mod qbittorrent;
pub mod ui;
pub mod watcher;
pub mod watchlist;

pub struct Data {
    pub config: config::Config,
    pub prowlarr: prowlarr::Prowlarr,
    /// Shared with the background sweep, so both use the same logged-in
    /// session instead of each holding an independent one.
    pub qbit: std::sync::Arc<qbittorrent::QBittorrent>,
    pub watchlist: std::sync::Arc<watchlist::Watchlist>,
    pub downloads: std::sync::Arc<downloads::Downloads>,
}

pub type Error = Box<dyn std::error::Error + Send + Sync>;
pub type Context<'a> = poise::Context<'a, Data, Error>;
