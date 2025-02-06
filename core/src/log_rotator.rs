use std::fs::{self, File, OpenOptions};
use std::io::{self, BufWriter, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

pub struct LogRotator {
    base_path: PathBuf,
    max_size: u64,
    max_files: usize,
    current_writer: BufWriter<File>,
    current_size: u64,
    read_buffer: Vec<u8>,
    read_pos: usize,
}

impl LogRotator {
    pub fn new<P: AsRef<Path>>(base_path: P, max_size: u64, max_files: usize) -> io::Result<Self> {
        let base_path = base_path.as_ref().to_path_buf();
        let file_path = Self::get_file_path(&base_path, 0);
        
        // Create directory if it doesn't exist
        if let Some(parent) = file_path.parent() {
            fs::create_dir_all(parent)?;
        }

        let file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&file_path)?;
            
        let current_size = file.metadata()?.len();
        let writer = BufWriter::with_capacity(8192, file); // 8KB buffer

        Ok(LogRotator {
            base_path,
            max_size,
            max_files,
            current_writer: writer,
            current_size,
            read_buffer: Vec::new(),
            read_pos: 0,
        })
    }

    fn get_file_path(base_path: &Path, index: usize) -> PathBuf {
        base_path.with_extension(index.to_string())
    }

    fn rotate(&mut self) -> io::Result<()> {
        // Remove the oldest log file if it exists
        if self.max_files > 1 {
            let oldest = Self::get_file_path(&self.base_path, self.max_files - 1);
            if oldest.exists() {
                fs::remove_file(&oldest)?;
            }
        }

        // Rotate files from second-to-last to first
        for i in (0..self.max_files-1).rev() {
            let current = Self::get_file_path(&self.base_path, i);
            let next = Self::get_file_path(&self.base_path, i + 1);
            
            if current.exists() {
                fs::rename(current, next)?;
            }
        }

        Ok(())
    }

    pub fn truncate(&mut self, mut bytes_to_remove: u64) -> io::Result<()> {
        // Ensure all data is flushed before truncating
        self.current_writer.flush()?;

        let mut files_to_keep = Vec::new();
        
        // First pass: identify which files to keep and their new sizes
        for i in 0..self.max_files {
            let file_path = Self::get_file_path(&self.base_path, i);
            if !file_path.exists() {
                continue;
            }

            let metadata = fs::metadata(&file_path)?;
            let file_size = metadata.len();

            if bytes_to_remove >= file_size {
                bytes_to_remove -= file_size;
            } else {
                // Keep this file and any remaining files
                if bytes_to_remove > 0 {
                    // This file needs partial truncation
                    files_to_keep.push((i, file_size - bytes_to_remove));
                    bytes_to_remove = 0;
                } else {
                    // Keep this file as is
                    files_to_keep.push((i, file_size));
                }
            }
        }

        // Second pass: rotate files to fill gaps
        for (new_index, (old_index, size)) in files_to_keep.iter().enumerate() {
            let old_path = Self::get_file_path(&self.base_path, *old_index);
            let new_path = Self::get_file_path(&self.base_path, new_index);

            if new_index == 0 {
                // For the first file, truncate if needed
                if old_index == &0 && size != &fs::metadata(&old_path)?.len() {
                    let mut file = OpenOptions::new()
                        .write(true)
                        .open(&old_path)?;
                    file.set_len(*size)?;
                    self.current_size = *size;
                } else if old_index != &0 {
                    fs::rename(&old_path, &new_path)?;
                    self.current_writer = BufWriter::with_capacity(
                        8192,
                        OpenOptions::new()
                            .create(true)
                            .append(true)
                            .open(&new_path)?
                    );
                    self.current_size = *size;
                }
            } else {
                // For other files, just rename if needed
                if old_index != &new_index {
                    fs::rename(&old_path, &new_path)?;
                }
            }
        }

        // Remove any remaining files
        for i in files_to_keep.len()..self.max_files {
            let path = Self::get_file_path(&self.base_path, i);
            if path.exists() {
                fs::remove_file(path)?;
            }
        }

        // Reset read buffer since files have changed
        self.read_buffer.clear();
        self.read_pos = 0;

        Ok(())
    }

	pub fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
		// If we have data in the buffer, use it first
		if self.read_pos < self.read_buffer.len() {
			let available = self.read_buffer.len() - self.read_pos;
			let to_copy = available.min(buf.len());
			buf[..to_copy].copy_from_slice(&self.read_buffer[self.read_pos..self.read_pos + to_copy]);
			self.read_pos += to_copy;
			return Ok(to_copy);
		}

		// Buffer is empty or fully read, load more data
		// self.current_writer.flush()?;
		self.read_buffer.clear();
		self.read_pos = 0;

		// Load all files' content
		for i in 0..self.max_files {
			let file_path = Self::get_file_path(&self.base_path, i);
			if !file_path.exists() {
				continue;
			}

			let mut file = File::open(&file_path)?;
			let mut file_buffer = Vec::new();
			file.read_to_end(&mut file_buffer)?;
			self.read_buffer.extend(file_buffer);
		}

		// If we loaded any data, read from it
		if !self.read_buffer.is_empty() {
			let to_copy = buf.len().min(self.read_buffer.len());
			buf[..to_copy].copy_from_slice(&self.read_buffer[..to_copy]);
			self.read_pos = to_copy;
			Ok(to_copy)
		} else {
			Ok(0)
		}
	}

    // Explicitly flush buffered data
    pub fn flush_internal(&mut self) -> io::Result<()> {
        self.current_writer.flush()
    }
}

impl Write for LogRotator {
    fn write(&mut self, data: &[u8]) -> io::Result<usize> {
        // Check if rotation is needed
        if self.current_size + data.len() as u64 > self.max_size {
            // Flush current writer before rotation
            self.current_writer.flush()?;
            
            // Perform rotation
            self.rotate()?;
            
            // Create new writer
            let file = OpenOptions::new()
                .create(true)
                .write(true)
                .truncate(true)
                .open(Self::get_file_path(&self.base_path, 0))?;
                
            self.current_writer = BufWriter::with_capacity(8192, file);
            self.current_size = 0;
        }

        // Write to the underlying BufWriter
        let bytes_written = self.current_writer.write(data)?;
        self.current_size += bytes_written as u64;
        
        // Optionally flush based on buffer size or other criteria
        if self.current_size % 8192 == 0 {
            self.current_writer.flush()?;
        }

        Ok(bytes_written)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.flush_internal()
    }
}

impl Drop for LogRotator {
    fn drop(&mut self) {
        // Attempt to flush any remaining data when the LogRotator is dropped
        let _ = self.current_writer.flush();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_write_and_rotate() -> io::Result<()> {
        let temp_dir = tempdir()?;
        let log_path = temp_dir.path().join("test.log");
        let mut rotator = LogRotator::new(log_path, 5, 3)?;

        // Write data that will cause rotation
        rotator.write(b"12345")?;
        rotator.write(b"67890")?;
        rotator.write(b"abcde")?;
        rotator.flush()?;

        // Check if files exist with correct content
        let base_path = &rotator.base_path;
        assert!(base_path.with_extension("0").exists());
        assert!(base_path.with_extension("1").exists());
        assert!(base_path.with_extension("2").exists());

        Ok(())
    }

    #[test]
    fn test_read() -> io::Result<()> {
        let temp_dir = tempdir()?;
        let log_path = temp_dir.path().join("test.log");
        let mut rotator = LogRotator::new(log_path, 10, 2)?;

        rotator.write(b"first")?;
        rotator.write(b"second")?;
        rotator.flush()?;

        let mut content = [0_u8; 1024];
        rotator.read(&mut content)?;

        assert!(!content.is_empty());
        Ok(())
    }

    #[test]
    fn test_truncate() -> io::Result<()> {
        let temp_dir = tempdir()?;
        let log_path = temp_dir.path().join("test.log");
        let mut rotator = LogRotator::new(log_path, 10, 2)?;

        rotator.write(b"12345")?;
        rotator.flush()?;
        rotator.truncate(3)?;

        let mut content = [0_u8; 1024];
        let n = rotator.read(&mut content)?;

        assert_eq!(content[..n], *b"12");
        Ok(())
    }

	#[test]
	fn truncate_many_files() {
		let temp_dir = tempdir().unwrap();
		let log_path = temp_dir.path().join("truncate.log");
		let mut rotator = LogRotator::new(log_path, 5, 3).unwrap();

		println!("first");
		rotator.write(b"12345").unwrap();
		println!("second");
		rotator.write(b"67890").unwrap();
		println!("third");
		rotator.write(b"abcde").unwrap();
		println!("flush");
		rotator.flush().unwrap();

		let path1 = rotator.base_path.with_extension("0");
		let path2 = rotator.base_path.with_extension("1");
		let path3 = rotator.base_path.with_extension("2");

		assert!(path1.exists());
		assert!(path2.exists());
		assert!(path3.exists());

		rotator.truncate(10).unwrap();

		assert!(path1.exists());
		assert!(!path2.exists());
		assert!(!path3.exists());

		let mut content = [0_u8; 1024];
		rotator.read(&mut content).unwrap();

		assert_eq!(content[0..5], *b"12345");
	}

	#[test]
	fn test_truncate_entire_files() {
		let temp_dir = tempdir().unwrap();
		let log_path = temp_dir.path().join("test.log");
		let mut rotator = LogRotator::new(log_path, 10, 3).unwrap();

		rotator.write(b"12345").unwrap();
		rotator.write(b"67890").unwrap();
		rotator.write(b"abcde").unwrap();
		rotator.flush().unwrap();
		rotator.truncate(15).unwrap();

		let mut content = [0_u8; 1024];
		let n = rotator.read(&mut content).unwrap();

		assert_eq!(n, 0);
	}


    #[test]
    fn test_rotation_multiple_times() -> io::Result<()> {
        let temp_dir = tempdir()?;
        let log_path = temp_dir.path().join("multi.log");
        let mut rotator = LogRotator::new(log_path.clone(), 5, 2)?;

        // Each write triggers rotation when size exceeds 5 bytes
        rotator.write(b"12345")?;
        rotator.write(b"67890")?;
        rotator.write(b"abcde")?;
        rotator.flush()?;

        // Check rotated files
        assert!(log_path.with_extension("0").exists());
        assert!(log_path.with_extension("1").exists());

        let content_0 = std::fs::read_to_string(log_path.with_extension("0"))?;
        let content_1 = std::fs::read_to_string(log_path.with_extension("1"))?;

        assert_eq!(content_0, "abcde");
		assert_eq!(content_1, "67890");

        Ok(())
    }

    #[test]
    fn test_flush_behavior() {
        let temp_dir = tempdir().unwrap();
        let log_path = temp_dir.path().join("flush.log");
        let mut rotator = LogRotator::new(log_path.clone(), 10, 2).unwrap();

        rotator.write(b"test flush").unwrap();
        rotator.flush().unwrap();

        let content = std::fs::read_to_string(&log_path.with_extension("0")).unwrap();
        assert_eq!(content, "test flush");
    }

    #[test]
    fn test_drop_flushes_data() {
        let temp_dir = tempdir().unwrap();
        let log_path = temp_dir.path().join("drop_flush.log");

        {
            let mut rotator = LogRotator::new(log_path.clone(), 10, 2).unwrap();
            rotator.write(b"drop flush test").unwrap();
            // No explicit flush here
        } // Dropping rotator should flush

        let content = std::fs::read_to_string(&log_path.with_extension("0")).unwrap();
        assert_eq!(content, "drop flush test");
    }
}