//! Everything except the Discord gateway glue, so it can be unit tested
//! without a live connection.

pub mod commands;
pub mod config;
pub mod i18n;
pub mod prowlarr;
pub mod qbittorrent;
pub mod ui;

pub struct Data {
    pub config: config::Config,
    pub prowlarr: prowlarr::Prowlarr,
    pub qbit: qbittorrent::QBittorrent,
}

pub type Error = Box<dyn std::error::Error + Send + Sync>;
pub type Context<'a> = poise::Context<'a, Data, Error>;
