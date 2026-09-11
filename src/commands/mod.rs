//! Discord glue. Everything these commands decide lives in [`crate::ui`]; what
//! is left here is the interaction lifecycle, which needs a live gateway and is
//! therefore excluded from coverage.

mod search;
mod status;

pub use search::search;
pub use status::status;
