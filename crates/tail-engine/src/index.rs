use memmap2::Mmap;
use std::fs::File;
use std::io;
use std::path::Path;
use tracing::debug;

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
}
