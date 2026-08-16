pub mod detector;
pub mod scanner;

pub use detector::LevelDetector;
pub use scanner::{
    file_stats, scan_file, ContextWindow, FileStats, LineEntry, ScanParams, ScanResult,
    DEFAULT_CONTEXT, DEFAULT_MAX_BYTES, DEFAULT_MAX_MATCHES, DEFAULT_MAX_LINE_BYTES,
    DEFAULT_TIME_BUDGET,
};
