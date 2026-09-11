//! Interaction lifecycle only; the decisions live in [`crate::ui`]. Needs a
//! live gateway to run, so it is excluded from coverage.

mod search;
mod status;
mod watchlist;

pub use search::search;
pub use status::status;
pub use watchlist::watchlist;
