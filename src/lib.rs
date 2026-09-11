//! Discord bot that searches ebooks through Prowlarr and hands them to qBittorrent.
//!
//! Everything except the Discord gateway glue lives here so it can be unit tested
//! without a live connection: the API clients, the configuration parsing and the
//! whole presentation layer.

pub mod commands;
pub mod config;
pub mod i18n;
pub mod prowlarr;
pub mod qbittorrent;
pub mod ui;

/// Shared state handed to every command invocation.
pub struct Data {
    pub config: config::Config,
    pub prowlarr: prowlarr::Prowlarr,
    pub qbit: qbittorrent::QBittorrent,
}

pub type Error = Box<dyn std::error::Error + Send + Sync>;
pub type Context<'a> = poise::Context<'a, Data, Error>;
