use memmap2::Mmap;
use std::fs::File;
use std::io;
use std::path::Path;
use tracing::debug;

pub struct TailResult {
    pub start_byte: u64,
    pub total_lines: u64,
}

#[derive(Clone, Debug, Default)]
pub struct LineIndex {
    pub offsets: Vec<u64>,
    pub file_size: u64,
    ends_with_newline: bool,
}

impl LineIndex {
    pub fn new() -> Self {
        Self {
            offsets: Vec::new(),
            file_size: 0,
            ends_with_newline: false,
        }
    }

    pub fn count_lines(path: &Path) -> io::Result<u64> {
        let file = File::open(path)?;
        let metadata = file.metadata()?;
        let file_size = metadata.len();

        if file_size == 0 {
            return Ok(0);
        }

        let mmap = unsafe { Mmap::map(&file)? };
        let nl_count = memchr::memchr_iter(b'\n', &mmap).count() as u64;
        let ends_with_newline = mmap[file_size as usize - 1] == b'\n';

        Ok(if ends_with_newline || nl_count == 0 {
            nl_count
        } else {
            nl_count + 1
        })
    }

    pub fn build(path: &Path) -> io::Result<Self> {
        let file = File::open(path)?;
        let metadata = file.metadata()?;
        let file_size = metadata.len();

        if file_size == 0 {
            return Ok(Self {
                offsets: vec![0],
                file_size: 0,
                ends_with_newline: false,
            });
        }

        let mmap = unsafe { Mmap::map(&file)? };
        let mut offsets = Vec::with_capacity(file_size as usize / 100 + 1);
        offsets.push(0);

        for (i, byte) in mmap.iter().enumerate() {
            if *byte == b'\n' {
                let next = (i + 1) as u64;
                if next < file_size {
                    offsets.push(next);
                }
            }
        }

        let ends_with_newline = mmap[file_size as usize - 1] == b'\n';

        debug!(
            path = %path.display(),
            lines = offsets.len(),
            file_size = file_size,
            "LineIndex built"
        );

        Ok(Self {
            offsets,
            file_size,
            ends_with_newline,
        })
    }

    pub fn update(&mut self, path: &Path, new_size: u64) -> io::Result<()> {
        if new_size <= self.file_size {
            return Ok(());
        }

        let file = File::open(path)?;
        let metadata = file.metadata()?;
        let actual_size = metadata.len();

        if actual_size <= self.file_size {
            return Ok(());
        }

        let mmap = unsafe { Mmap::map(&file)? };
        let start = self.file_size as usize;
        let end = actual_size as usize;

        if self.ends_with_newline && self.file_size < actual_size {
            self.offsets.push(self.file_size);
        }

        for i in start..end {
            if mmap[i] == b'\n' {
                let next = (i + 1) as u64;
                if next < actual_size {
                    self.offsets.push(next);
                }
            }
        }

        self.ends_with_newline = mmap[end - 1] == b'\n';
        self.file_size = actual_size;

        debug!(
            path = %path.display(),
            lines = self.offsets.len(),
            file_size = actual_size,
            "LineIndex updated"
        );

        Ok(())
    }

    pub fn offset_of_line(&self, line: u64) -> Option<u64> {
        self.offsets.get(line as usize).copied()
    }

    pub fn tail_start(path: &Path, n: usize) -> io::Result<TailResult> {
        use std::io::{Read, Seek, SeekFrom};

        let mut file = File::open(path)?;
        let file_size = file.metadata()?.len();

        if file_size == 0 {
            return Ok(TailResult {
                start_byte: 0,
                total_lines: 0,
            });
        }

        let mut last_byte = [0u8; 1];
        file.seek(SeekFrom::End(-1))?;
        file.read_exact(&mut last_byte)?;
        let ends_with_newline = last_byte[0] == b'\n';

        let target = if ends_with_newline { n + 1 } else { n };

        const CHUNK_SIZE: usize = 8192;
        let mut pos = file_size;
        let mut buf = vec![0u8; CHUNK_SIZE];
        let mut newline_count = 0usize;
        let mut start_byte: u64 = 0;
        let mut found = false;

        while pos > 0 {
            let read_len = CHUNK_SIZE.min(pos as usize);
            let chunk_start = pos - read_len as u64;

            file.seek(SeekFrom::Start(chunk_start))?;
            file.read_exact(&mut buf[..read_len])?;

            for i in (0..read_len).rev() {
                if buf[i] == b'\n' {
                    newline_count += 1;
                    if newline_count == target {
                        let after = chunk_start + i as u64 + 1;
                        if after < file_size {
                            start_byte = after;
                            found = true;
                        }
                        break;
                    }
                }
            }

            if found {
                break;
            }

            pos = chunk_start;
        }

        let total_lines = if !found {
            let base = newline_count as u64;
            if ends_with_newline {
                base
            } else if base == 0 {
                1
            } else {
                base + 1
            }
        } else {
            let tail_bytes = file_size - start_byte;
            if tail_bytes > 0 && n > 0 {
                let avg = tail_bytes as f64 / n as f64;
                (file_size as f64 / avg) as u64
            } else {
                n as u64
            }
        };

        debug!(
            path = %path.display(),
            start_byte,
            total_lines,
            file_size,
            "tail_start computed"
        );

        Ok(TailResult {
            start_byte,
            total_lines,
        })
    }

    pub fn line_of_offset(&self, offset: u64) -> u64 {
        match self.offsets.binary_search(&offset) {
            Ok(idx) => idx as u64,
            Err(idx) => {
                if idx == 0 {
                    0
                } else {
                    (idx - 1) as u64
                }
            }
        }
    }

    pub fn total_lines(&self) -> u64 {
        self.offsets.len() as u64
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    #[test]
    fn test_build_empty_file() {
        let mut f = NamedTempFile::new().unwrap();
        f.write_all(b"").unwrap();
        let idx = LineIndex::build(f.path()).unwrap();
        assert_eq!(idx.offsets, vec![0]);
        assert_eq!(idx.file_size, 0);
    }

    #[test]
    fn test_build_single_line_no_newline() {
        let mut f = NamedTempFile::new().unwrap();
        f.write_all(b"hello world").unwrap();
        let idx = LineIndex::build(f.path()).unwrap();
        assert_eq!(idx.offsets, vec![0]);
        assert_eq!(idx.file_size, 11);
    }

    #[test]
    fn test_build_multiple_lines() {
        let mut f = NamedTempFile::new().unwrap();
        f.write_all(b"line0\nline1\nline2\n").unwrap();
        let idx = LineIndex::build(f.path()).unwrap();
        assert_eq!(idx.offsets, vec![0, 6, 12]);
        assert_eq!(idx.file_size, 18);
        assert!(idx.ends_with_newline);
    }

    #[test]
    fn test_build_no_trailing_newline() {
        let mut f = NamedTempFile::new().unwrap();
        f.write_all(b"line0\nline1").unwrap();
        let idx = LineIndex::build(f.path()).unwrap();
        assert_eq!(idx.offsets, vec![0, 6]);
        assert!(!idx.ends_with_newline);
    }

    #[test]
    fn test_offset_of_line() {
        let mut f = NamedTempFile::new().unwrap();
        f.write_all(b"aaa\nbbb\nccc\n").unwrap();
        let idx = LineIndex::build(f.path()).unwrap();
        assert_eq!(idx.offset_of_line(0), Some(0));
        assert_eq!(idx.offset_of_line(1), Some(4));
        assert_eq!(idx.offset_of_line(2), Some(8));
        assert_eq!(idx.offset_of_line(3), None);
    }

    #[test]
    fn test_line_of_offset() {
        let mut f = NamedTempFile::new().unwrap();
        f.write_all(b"aaa\nbbb\nccc\n").unwrap();
        let idx = LineIndex::build(f.path()).unwrap();
        assert_eq!(idx.line_of_offset(0), 0);
        assert_eq!(idx.line_of_offset(3), 0);
        assert_eq!(idx.line_of_offset(4), 1);
        assert_eq!(idx.line_of_offset(7), 1);
        assert_eq!(idx.line_of_offset(8), 2);
    }

    #[test]
    fn test_update_incremental() {
        let mut f = NamedTempFile::new().unwrap();
        f.write_all(b"line0\nline1\n").unwrap();
        f.flush().unwrap();
        let mut idx = LineIndex::build(f.path()).unwrap();
        assert_eq!(idx.offsets, vec![0, 6]);

        use std::io::Seek;
        f.seek(std::io::SeekFrom::End(0)).unwrap();
        f.write_all(b"line2\n").unwrap();
        f.flush().unwrap();

        let new_size = f.as_file().metadata().unwrap().len();
        idx.update(f.path(), new_size).unwrap();
        assert_eq!(idx.offsets, vec![0, 6, 12]);
    }

    #[test]
    fn test_update_no_trailing_newline() {
        let mut f = NamedTempFile::new().unwrap();
        f.write_all(b"line0\nline1\n").unwrap();
        f.flush().unwrap();
        let mut idx = LineIndex::build(f.path()).unwrap();
        assert_eq!(idx.offsets, vec![0, 6]);

        use std::io::Seek;
        f.seek(std::io::SeekFrom::End(0)).unwrap();
        f.write_all(b"line2").unwrap();
        f.flush().unwrap();

        let new_size = f.as_file().metadata().unwrap().len();
        idx.update(f.path(), new_size).unwrap();
        assert_eq!(idx.offsets, vec![0, 6, 12]);
    }

    #[test]
    fn test_tail_start_empty_file() {
        let mut f = NamedTempFile::new().unwrap();
        f.write_all(b"").unwrap();
        let result = LineIndex::tail_start(f.path(), 10).unwrap();
        assert_eq!(result.start_byte, 0);
        assert_eq!(result.total_lines, 0);
    }

    #[test]
    fn test_tail_start_fewer_lines_than_requested() {
        let mut f = NamedTempFile::new().unwrap();
        f.write_all(b"a\nb\n").unwrap();
        let result = LineIndex::tail_start(f.path(), 10).unwrap();
        assert_eq!(result.start_byte, 0);
        assert_eq!(result.total_lines, 2);
    }

    #[test]
    fn test_tail_start_with_trailing_newline() {
        let mut f = NamedTempFile::new().unwrap();
        f.write_all(b"aaa\nbbb\nccc\nddd\n").unwrap();
        let result = LineIndex::tail_start(f.path(), 2).unwrap();
        assert_eq!(result.start_byte, 8);
    }

    #[test]
    fn test_tail_start_without_trailing_newline() {
        let mut f = NamedTempFile::new().unwrap();
        f.write_all(b"aaa\nbbb\nccc\nddd").unwrap();
        let result = LineIndex::tail_start(f.path(), 2).unwrap();
        assert_eq!(result.start_byte, 8);
    }

    #[test]
    fn test_tail_start_last_line() {
        let mut f = NamedTempFile::new().unwrap();
        f.write_all(b"aaa\nbbb\nccc\n").unwrap();
        let result = LineIndex::tail_start(f.path(), 1).unwrap();
        assert_eq!(result.start_byte, 8);
    }

    #[test]
    fn test_tail_start_single_line_no_newline() {
        let mut f = NamedTempFile::new().unwrap();
        f.write_all(b"hello").unwrap();
        let result = LineIndex::tail_start(f.path(), 5).unwrap();
        assert_eq!(result.start_byte, 0);
        assert_eq!(result.total_lines, 1);
    }

    #[test]
    fn test_tail_start_large_n_estimate() {
        let mut f = NamedTempFile::new().unwrap();
        for i in 0..1000 {
            writeln!(f, "line{}", i).unwrap();
        }
        f.flush().unwrap();
        let result = LineIndex::tail_start(f.path(), 50).unwrap();
        assert!(result.start_byte > 0);
        assert!(result.total_lines > 0);
    }
}
