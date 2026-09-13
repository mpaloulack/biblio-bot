//! Interaction lifecycle only; the decisions live in [`crate::ui`]. Needs a
//! live gateway to run, so it is excluded from coverage.

mod search;
mod status;
mod stuck;
mod watchlist;

pub use search::search;
pub use status::status;
pub use stuck::stuck;
pub use watchlist::{watchlist, watchlist_all};
