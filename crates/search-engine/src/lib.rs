pub mod filter;
pub mod grep;

pub use filter::{apply_filter, LogFilter};
pub use grep::{SearchEngine, SearchMatch, SearchOptions, SearchResult};
