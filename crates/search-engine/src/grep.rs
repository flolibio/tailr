use std::fs::File;
use std::io;
use std::path::Path;

use grep_regex::RegexMatcherBuilder;
use grep_searcher::SearcherBuilder;
use grep_searcher::{Sink, SinkContext, SinkContextKind, SinkMatch};
use memmap2::Mmap;
use tracing::{debug, warn};

const MAX_RESULTS: usize = 10000;

#[derive(Debug, Clone)]
pub struct SearchOptions {
    pub pattern: String,
    pub is_regex: bool,
    pub case_insensitive: bool,
    pub context_before: u32,
    pub context_after: u32,
    pub max_results: usize,
    pub level_filter: Option<String>,
}

impl Default for SearchOptions {
    fn default() -> Self {
        Self {
            pattern: String::new(),
            is_regex: false,
            case_insensitive: false,
            context_before: 0,
            context_after: 0,
            max_results: MAX_RESULTS,
            level_filter: None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct SearchResult {
    pub matches: Vec<SearchMatch>,
    pub total_matches: usize,
    pub has_more: bool,
}

#[derive(Debug, Clone)]
pub struct SearchMatch {
    pub line_num: u64,
    pub content: String,
    pub context_before: Vec<String>,
    pub context_after: Vec<String>,
}

pub struct SearchEngine;

impl SearchEngine {
    pub fn new() -> Self {
        Self
    }

    pub fn search(
        &self,
        path: &Path,
        options: &SearchOptions,
    ) -> io::Result<SearchResult> {
        let file = File::open(path).map_err(|e| {
            warn!(path = %path.display(), error = %e, "Failed to open file");
            e
        })?;

        let mmap = unsafe { Mmap::map(&file).map_err(|e| {
            warn!(path = %path.display(), error = %e, "Failed to create memory map");
            io::Error::new(e.kind(), format!("mmap failed: {}", e))
        })? };

        if mmap.is_empty() {
            return Ok(SearchResult {
                matches: Vec::new(),
                total_matches: 0,
                has_more: false,
            });
        }

        let matcher = if options.is_regex {
            RegexMatcherBuilder::new()
                .case_insensitive(options.case_insensitive)
                .build(&options.pattern)
                .map_err(|e| {
                    io::Error::new(io::ErrorKind::InvalidInput, format!("Invalid regex: {}", e))
                })?
        } else {
            RegexMatcherBuilder::new()
                .case_insensitive(options.case_insensitive)
                .fixed_strings(true)
                .build(&options.pattern)
                .map_err(|e| {
                    io::Error::new(io::ErrorKind::InvalidInput, format!("Invalid pattern: {}", e))
                })?
        };

        let max_results = options.max_results.min(MAX_RESULTS);

        let mut searcher = SearcherBuilder::new()
            .before_context(options.context_before as usize)
            .after_context(options.context_after as usize)
            .line_number(true)
            .build();

        let mut collector = MatchCollector::new(max_results);

        searcher.search_slice(&matcher, &mmap[..], &mut collector)?;

        let total = collector.total_matches;
        let has_more = total > max_results;

        debug!(
            path = %path.display(),
            pattern = %options.pattern,
            total_matches = total,
            returned = collector.matches.len(),
            has_more = has_more,
            "Search completed"
        );

        Ok(SearchResult {
            matches: collector.matches,
            total_matches: total,
            has_more,
        })
    }
}

impl Default for SearchEngine {
    fn default() -> Self {
        Self::new()
    }
}

struct MatchCollector {
    matches: Vec<SearchMatch>,
    total_matches: usize,
    max_results: usize,
    before_context: Vec<String>,
    current_line: u64,
}

impl MatchCollector {
    fn new(max_results: usize) -> Self {
        Self {
            matches: Vec::new(),
            total_matches: 0,
            max_results,
            before_context: Vec::new(),
            current_line: 0,
        }
    }
}

impl Sink for MatchCollector {
    type Error = io::Error;

    fn matched(
        &mut self,
        _searcher: &grep_searcher::Searcher,
        mat: &SinkMatch<'_>,
    ) -> Result<bool, io::Error> {
        self.total_matches += 1;

        // Extract the first line from the match
        let mut line_content = String::new();
        let mut iter = mat.lines();
        if let Some(line) = iter.next() {
            line_content = String::from_utf8_lossy(line)
                .trim_end_matches('\n')
                .trim_end_matches('\r')
                .to_string();
        }

        let line_num = mat.line_number().unwrap_or(0);

        let context_before = std::mem::take(&mut self.before_context);

        if self.matches.len() < self.max_results {
            self.matches.push(SearchMatch {
                line_num,
                content: line_content,
                context_before,
                context_after: Vec::new(),
            });
        }

        self.current_line = line_num;

        Ok(true)
    }

    fn context(
        &mut self,
        _searcher: &grep_searcher::Searcher,
        ctx: &SinkContext<'_>,
    ) -> Result<bool, io::Error> {
        let line_content = String::from_utf8_lossy(ctx.bytes())
            .trim_end_matches('\n')
            .trim_end_matches('\r')
            .to_string();

        match ctx.kind() {
            SinkContextKind::Before => {
                self.before_context.push(line_content);
                if self.before_context.len() > 100 {
                    self.before_context.remove(0);
                }
            }
            SinkContextKind::After => {
                if let Some(last) = self.matches.last_mut() {
                    last.context_after.push(line_content);
                }
            }
            _ => {}
        }

        Ok(true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    fn write_temp_file(content: &[u8]) -> NamedTempFile {
        let mut f = NamedTempFile::new().unwrap();
        f.write_all(content).unwrap();
        f.flush().unwrap();
        f
    }

    #[test]
    fn test_literal_search_basic() {
        let f = write_temp_file(b"hello world\nfoo bar\nhello again\n");
        let engine = SearchEngine::new();
        let opts = SearchOptions {
            pattern: "hello".to_string(),
            is_regex: false,
            case_insensitive: false,
            context_before: 0,
            context_after: 0,
            max_results: 100,
            level_filter: None,
        };

        let result = engine.search(f.path(), &opts).unwrap();
        assert_eq!(result.matches.len(), 2);
        assert_eq!(result.matches[0].line_num, 1);
        assert_eq!(result.matches[0].content, "hello world");
        assert_eq!(result.matches[1].line_num, 3);
        assert_eq!(result.matches[1].content, "hello again");
        assert!(!result.has_more);
    }

    #[test]
    fn test_regex_search() {
        let f = write_temp_file(b"error: something\nwarn: else\nerror: again\n");
        let engine = SearchEngine::new();
        let opts = SearchOptions {
            pattern: r"error:\s+\w+".to_string(),
            is_regex: true,
            case_insensitive: false,
            context_before: 0,
            context_after: 0,
            max_results: 100,
            level_filter: None,
        };

        let result = engine.search(f.path(), &opts).unwrap();
        assert_eq!(result.matches.len(), 2);
        assert_eq!(result.matches[0].content, "error: something");
        assert_eq!(result.matches[1].content, "error: again");
    }

    #[test]
    fn test_case_insensitive() {
        let f = write_temp_file(b"Hello World\nhello world\nHELLO WORLD\n");
        let engine = SearchEngine::new();
        let opts = SearchOptions {
            pattern: "hello".to_string(),
            is_regex: false,
            case_insensitive: true,
            context_before: 0,
            context_after: 0,
            max_results: 100,
            level_filter: None,
        };

        let result = engine.search(f.path(), &opts).unwrap();
        assert_eq!(result.matches.len(), 3);
    }

    #[test]
    fn test_context_before_after() {
        let f = write_temp_file(b"line1\nline2\ntarget\nline4\nline5\n");
        let engine = SearchEngine::new();
        let opts = SearchOptions {
            pattern: "target".to_string(),
            is_regex: false,
            case_insensitive: false,
            context_before: 2,
            context_after: 2,
            max_results: 100,
            level_filter: None,
        };

        let result = engine.search(f.path(), &opts).unwrap();
        assert_eq!(result.matches.len(), 1);
        assert_eq!(result.matches[0].content, "target");
        assert_eq!(result.matches[0].context_before, vec!["line1", "line2"]);
        assert_eq!(result.matches[0].context_after, vec!["line4", "line5"]);
    }

    #[test]
    fn test_max_results() {
        let mut content = String::new();
        for i in 0..100 {
            content.push_str(&format!("match line {}\n", i));
        }
        let f = write_temp_file(content.as_bytes());
        let engine = SearchEngine::new();
        let opts = SearchOptions {
            pattern: "match".to_string(),
            is_regex: false,
            case_insensitive: false,
            context_before: 0,
            context_after: 0,
            max_results: 10,
            level_filter: None,
        };

        let result = engine.search(f.path(), &opts).unwrap();
        assert_eq!(result.matches.len(), 10);
        assert_eq!(result.total_matches, 100);
        assert!(result.has_more);
    }

    #[test]
    fn test_empty_file() {
        let f = write_temp_file(b"");
        let engine = SearchEngine::new();
        let opts = SearchOptions {
            pattern: "test".to_string(),
            ..Default::default()
        };

        let result = engine.search(f.path(), &opts).unwrap();
        assert_eq!(result.matches.len(), 0);
        assert_eq!(result.total_matches, 0);
        assert!(!result.has_more);
    }

    #[test]
    fn test_no_matches() {
        let f = write_temp_file(b"hello world\nfoo bar\n");
        let engine = SearchEngine::new();
        let opts = SearchOptions {
            pattern: "xyz".to_string(),
            ..Default::default()
        };

        let result = engine.search(f.path(), &opts).unwrap();
        assert_eq!(result.matches.len(), 0);
        assert_eq!(result.total_matches, 0);
        assert!(!result.has_more);
    }

    #[test]
    fn test_file_not_found() {
        let engine = SearchEngine::new();
        let opts = SearchOptions {
            pattern: "test".to_string(),
            ..Default::default()
        };

        let result = engine.search(Path::new("/nonexistent/file.txt"), &opts);
        assert!(result.is_err());
    }

    #[test]
    fn test_invalid_regex() {
        let f = write_temp_file(b"test\n");
        let engine = SearchEngine::new();
        let opts = SearchOptions {
            pattern: "[invalid".to_string(),
            is_regex: true,
            ..Default::default()
        };

        let result = engine.search(f.path(), &opts);
        assert!(result.is_err());
    }
}
