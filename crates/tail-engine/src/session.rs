use logtailer_protocol::{LogEntry, LogLevel};
use std::path::PathBuf;
use tokio::fs::File;
use tokio::io::{AsyncBufReadExt, AsyncSeekExt, BufReader, SeekFrom};
use tracing::{debug, info};

#[cfg(unix)]
use std::os::unix::fs::MetadataExt;

pub struct TailSession {
    pub path: PathBuf,
    fd: Option<File>,
    pub offset: u64,
    inode: u64,
    pub seq: u64,
    line_num: u64,
}

impl TailSession {
    pub async fn new(path: PathBuf) -> std::io::Result<Self> {
        let meta = tokio::fs::metadata(&path).await?;
        let inode = meta.ino();
        let size = meta.len();

        let file = File::open(&path).await?;

        // Count existing non-empty lines so line_num continues correctly
        let existing_lines = count_lines_from_file(&path).await;

        info!(path = %path.display(), inode, size, existing_lines, "TailSession opened");

        Ok(Self {
            path,
            fd: Some(file),
            offset: size,
            inode,
            seq: 0,
            line_num: existing_lines,
        })
    }

    pub async fn check(&mut self) -> std::io::Result<Vec<LogEntry>> {
        let meta = match tokio::fs::metadata(&path_display(&self.path)).await {
            Ok(m) => m,
            Err(e) => {
                if e.kind() == std::io::ErrorKind::NotFound {
                    debug!(path = %self.path.display(), "file not found, waiting");
                    return Ok(vec![]);
                }
                return Err(e);
            }
        };

        let current_inode = meta.ino();
        let current_size = meta.len();

        if current_inode != self.inode {
            info!(
                path = %self.path.display(),
                old_inode = self.inode,
                new_inode = current_inode,
                "inode changed (logrotate), reopening"
            );
            self.fd = None;
            self.offset = 0;
            self.inode = current_inode;
            let file = File::open(&self.path).await?;
            self.fd = Some(file);
            return self.read_lines_from_offset().await;
        }

        if current_size < self.offset {
            info!(
                path = %self.path.display(),
                old_offset = self.offset,
                new_size = current_size,
                "file truncated, resetting offset"
            );
            self.offset = 0;
            self.line_num = 0;
            if self.fd.is_none() {
                self.fd = Some(File::open(&self.path).await?);
            }
            return self.read_lines_from_offset().await;
        }

        if current_size > self.offset {
            debug!(
                path = %self.path.display(),
                offset = self.offset,
                size = current_size,
                "new content available"
            );
            if self.fd.is_none() {
                self.fd = Some(File::open(&self.path).await?);
            }
            return self.read_lines_from_offset().await;
        }

        Ok(vec![])
    }

    pub async fn read_lines_from_offset(&mut self) -> std::io::Result<Vec<LogEntry>> {
        let file = match &mut self.fd {
            Some(f) => f,
            None => {
                self.fd = Some(File::open(&self.path).await?);
                self.fd.as_mut().unwrap()
            }
        };

        file.seek(SeekFrom::Start(self.offset)).await?;

        let mut reader = BufReader::new(file);
        let mut entries = Vec::new();
        let mut buf = String::new();

        loop {
            buf.clear();
            let n = reader.read_line(&mut buf).await?;
            if n == 0 {
                break;
            }

            let trimmed = buf.trim_end_matches('\n').trim_end_matches('\r');
            if trimmed.is_empty() {
                self.offset += n as u64;
                continue;
            }

            let level = detect_level(trimmed);
            let timestamp = try_parse_timestamp(trimmed);
            let fields = try_parse_json_fields(trimmed);

            let entry = LogEntry {
                line_num: self.line_num,
                raw: trimmed.to_string(),
                level,
                timestamp,
                fields,
            };

            self.line_num += 1;
            self.offset += n as u64;
            self.seq += 1;
            entries.push(entry);
        }

        if !entries.is_empty() {
            debug!(
                path = %self.path.display(),
                new_lines = entries.len(),
                offset = self.offset,
                seq = self.seq,
                "read new lines"
            );
        }

        Ok(entries)
    }
}

fn path_display(path: &PathBuf) -> &std::path::Path {
    path.as_path()
}

async fn count_lines_from_file(path: &std::path::Path) -> u64 {
    use tokio::io::{AsyncBufReadExt, BufReader};

    let file = match tokio::fs::File::open(path).await {
        Ok(f) => f,
        Err(_) => return 0,
    };

    let reader = BufReader::new(file);
    let mut lines = reader.lines();
    let mut count: u64 = 0;

    while let Ok(Some(line)) = lines.next_line().await {
        if !line.trim().is_empty() {
            count += 1;
        }
    }

    count
}

fn detect_level(line: &str) -> LogLevel {
    let upper: String = line
        .chars()
        .take(256)
        .map(|c| c.to_ascii_uppercase())
        .collect();

    if upper.contains("ALERT") || upper.contains("[ALERT]") {
        LogLevel::ALERT
    } else if upper.contains("ERROR") || upper.contains("[ERROR]") || upper.contains(" E ") {
        LogLevel::ERROR
    } else if upper.contains("WARN") || upper.contains("[WARN]") || upper.contains(" W ") {
        LogLevel::WARN
    } else if upper.contains("INFO") || upper.contains("[INFO]") || upper.contains(" I ") {
        LogLevel::INFO
    } else if upper.contains("DEBUG") || upper.contains("[DEBUG]") || upper.contains(" D ") {
        LogLevel::DEBUG
    } else if upper.contains("TRACE") || upper.contains("[TRACE]") {
        LogLevel::TRACE
    } else {
        LogLevel::UNKNOWN
    }
}

fn try_parse_timestamp(line: &str) -> Option<chrono::DateTime<chrono::Utc>> {
    use chrono::NaiveDateTime;

    // Try ISO 8601: 2024-01-15T10:30:00Z or with offset
    if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(line.get(..30).unwrap_or(line)) {
        return Some(dt.with_timezone(&chrono::Utc));
    }

    // Try common log format: 2024-01-15 10:30:00
    let patterns: &[&str] = &[
        "%Y-%m-%d %H:%M:%S%.3f",
        "%Y-%m-%d %H:%M:%S",
        "%d/%b/%Y:%H:%M:%S",
    ];

    for pattern in patterns {
        let len = pattern.len() + 10;
        if let Some(slice) = line.get(..len.min(line.len())) {
            if let Ok(dt) = NaiveDateTime::parse_from_str(slice.trim(), pattern) {
                return Some(dt.and_utc());
            }
        }
    }

    None
}

fn try_parse_json_fields(line: &str) -> Option<serde_json::Value> {
    if let Some(start) = line.find('{') {
        if let Ok(val) = serde_json::from_str::<serde_json::Value>(&line[start..]) {
            return Some(val);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detect_level_error() {
        assert_eq!(detect_level("2024-01-15 ERROR something failed"), LogLevel::ERROR);
        assert_eq!(detect_level("[ERROR] connection refused"), LogLevel::ERROR);
    }

    #[test]
    fn test_detect_level_warn() {
        assert_eq!(detect_level("WARN: disk almost full"), LogLevel::WARN);
        assert_eq!(detect_level("[WARN] slow query"), LogLevel::WARN);
    }

    #[test]
    fn test_detect_level_info() {
        assert_eq!(detect_level("INFO: server started"), LogLevel::INFO);
    }

    #[test]
    fn test_detect_level_debug() {
        assert_eq!(detect_level("DEBUG: request received"), LogLevel::DEBUG);
    }

    #[test]
    fn test_detect_level_unknown() {
        assert_eq!(detect_level("just some text"), LogLevel::UNKNOWN);
    }

    #[test]
    fn test_try_parse_json_fields() {
        let line = r#"2024-01-15 INFO {"user": "alice", "action": "login"}"#;
        let fields = try_parse_json_fields(line).unwrap();
        assert_eq!(fields["user"], "alice");
    }

    #[test]
    fn test_try_parse_json_fields_none() {
        assert!(try_parse_json_fields("no json here").is_none());
    }

    #[tokio::test]
    async fn test_session_check_truncation() {
        use std::io::Write;
        let mut f = tempfile::NamedTempFile::new().unwrap();
        writeln!(f, "line0").unwrap();
        writeln!(f, "line1").unwrap();
        f.flush().unwrap();

        let initial_size = f.as_file().metadata().unwrap().len();
        let mut session = TailSession::new(f.path().to_path_buf()).await.unwrap();
        assert_eq!(session.offset, initial_size);

        // Truncate and rewrite
        f.as_file_mut().set_len(0).unwrap();
        use std::io::Seek;
        f.seek(std::io::SeekFrom::Start(0)).unwrap();
        writeln!(f, "new line").unwrap();
        f.flush().unwrap();

        let entries = session.check().await.unwrap();
        assert!(!entries.is_empty());
        assert!(entries[0].raw.contains("new line"));
    }
}
