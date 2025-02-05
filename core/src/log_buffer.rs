use std::collections::VecDeque;
use std::io::Read;
use std::io::Seek;
use std::io::Write;
use std::mem;
use std::path::PathBuf;
use bytes::Bytes;

pub struct LogBuffer {
	buffer: Vec<u8>,
	chunks: VecDeque<Bytes>,
	chunk_size: usize,
	max_chunks_count: usize,
	max_file_count: usize,
	max_file_size: usize,
	folder_path: Option<PathBuf>,
	file_size: usize,
	file: Option<std::fs::File>,
	file_ids: Vec<usize>
}

impl LogBuffer {
	pub fn new(chunk_size: usize) -> LogBuffer {
		LogBuffer {
			buffer: Vec::with_capacity(chunk_size),
			chunks: VecDeque::new(),
			chunk_size,
			max_chunks_count: 5,
			max_file_count: 5,
			max_file_size: 100,
			folder_path: None,
			file_size: 0,
			file: None,
			file_ids: Vec::new()
		}
	}

	pub fn set_folder_path(&mut self, folder_path: PathBuf) {
		std::fs::create_dir_all(&folder_path).unwrap_or_default();
		self.file_ids.clear();
		for entry in std::fs::read_dir(&folder_path).unwrap() {
			let entry = entry.unwrap();
			let path = entry.path();
			if path.is_file() {
				let file_id = path.file_name().unwrap().to_str().unwrap().to_string().split('.').next().unwrap().parse::<usize>().unwrap();
				self.file_ids.push(file_id);
			}
		}
		self.folder_path = Some(folder_path);
	}

	pub fn size(&self) -> u64 {
		0
	}

	fn read_chunks_from_files(&mut self) {
		if let Some(folder_path) = &self.folder_path {
			match self.file {
				Some(ref mut file) => {
					let read = file.read(&mut self.buffer).unwrap();
					let chunk = Bytes::copy_from_slice(&self.buffer[..read]);
					self.chunks.push_back(chunk);
				}
				None => {
					self.file_ids.sort_by(|a, b| a.cmp(b));
					if let Some(first) = self.file_ids.first() {
						let path = folder_path.join(format!("{}.log", first));
						println!("open file {}", path.display());
						let mut file = std::fs::OpenOptions::new()
							.read(true)
							.open(path)
							.unwrap();
						let size = file.metadata().unwrap().len() as usize;
						file.seek(std::io::SeekFrom::Start(size as u64 - self.chunk_size as u64)).unwrap();
						let read = file.read_to_end(&mut self.buffer).unwrap();
						println!("read {} bytes", read);
						let chunk = Bytes::copy_from_slice(&self.buffer[..read]);
						self.buffer.clear();
						self.chunks.push_back(chunk);
					}
				}
			}
		}
	}

	pub fn next_chunk(&mut self) -> Option<Bytes> {
		println!("self.buffer.len() = {}", self.buffer.len());
		println!("self.chunks.len() = {}", self.chunks.len());
		if self.buffer.len() > 0 {
			self.chunks.push_back(Bytes::copy_from_slice(&self.buffer));
			self.buffer.clear();
		}
		if self.chunks.len() == 0 {
			self.read_chunks_from_files();
		}
		self.chunks.pop_back()
	}

	fn open_file(&mut self) -> &mut std::fs::File {
		if let Some(ref mut file) = self.file {
			file
		} else {
			let folder_path = self.folder_path.as_ref().unwrap();
			let filepath = folder_path.join("0.log");
			println!("open file {}", filepath.display());
			let file = std::fs::OpenOptions::new()
				.read(true)
				.write(true)
				.create(true)
				.append(true)
				.open(filepath)
				.unwrap();
			self.file_size = file.metadata().unwrap().len() as usize;
			self.file = Some(file);
			self.file.as_mut().unwrap()
		}
	}

	fn close_file(&mut self) {
		if let Some(ref mut file) = self.file {
			let _ = file.flush(); // Flush before closing
			self.file = None;      // Dropping the std::fs::File closes it
		}
	}

	fn rotate_logs(&mut self) {
		self.close_file(); // Ensure the file is closed before renaming
		if let Some(folder_path) = &self.folder_path {
			// First, sort file_ids in descending order.
			self.file_ids.sort_by(|a, b| b.cmp(a));
			for &id in self.file_ids.iter() {
				let old_name = folder_path.join(format!("{}.log", id));
				let new_name = folder_path.join(format!("{}.log", id + 1));
				if let Err(err) = std::fs::rename(&old_name, &new_name) {
					println!("failed to rename: {} to {} error: {}", old_name.display(), new_name.display(), err);
				}
			}
			self.file_ids.iter_mut().for_each(|id| *id += 1);
			self.file_ids.push(0);
			self.file_ids.sort();
			println!("file_ids = {:?}", self.file_ids);
			while self.file_ids.len() > self.max_file_count {
				println!("need to delete file");
				if let Some(oldest) = self.file_ids.pop() {
					let path_to_delete = folder_path.join(format!("{}.log", oldest));
					println!("deleting file {}", path_to_delete.display());
					if let Err(err) = std::fs::remove_file(&path_to_delete) {
						println!("failed to delete file {}: {}", path_to_delete.display(), err);
					}
				}
			}
		}
	}

	fn write_file(&mut self, data: &[u8]) {
		println!("write_file");
		if let Some(folder_path) = &self.folder_path {
			println!("folder_path = {:?} file {:?}", folder_path, self.file);
			let file = self.open_file();
			file.write_all(&data).unwrap();
			self.file_size += data.len();
			if self.file_size > self.max_file_size {
				self.rotate_logs();
			}
		}
	}

	pub fn truncate(&mut self, size: usize) {
		self.chunks.pop_back();
		match self.file {
			Some(ref mut file) => {
				let new_size = self.file_size as u64 - size as u64;
				println!("truncating file {:?} to {}", file, new_size);
				file.set_len(new_size).unwrap();
				self.file_size -= size;
			}
			None => {
				if let Some(folder_path) = &self.folder_path {
					let file_id = self.file_ids.last().unwrap();
					let path = folder_path.join(format!("{}.log", file_id));
					let new_size = std::fs::metadata(&path).unwrap().len() - size as u64;
					println!("truncating file {:?} to {}", path, new_size);
					std::fs::OpenOptions::new()
						.write(true)
						.open(&path)
						.unwrap()
						.set_len(new_size)
						.unwrap();
					// if new_size == 0 {
					// 	std::fs::remove_file(path).ok();
					// 	self.file_ids.retain(|&id| id != *file_id);
					// }
				}
			}
		}
	}
}

impl Write for LogBuffer {
	fn write(&mut self, buf: &[u8]) -> Result<usize, std::io::Error> {
		println!("buffer.len() = {}, chunk_size = {}", self.buffer.len(), self.chunk_size);
		if self.buffer.len() >= self.chunk_size {
			println!("push chunk back");
			let old_buffer = mem::replace(&mut self.buffer, Vec::with_capacity(self.chunk_size));
			self.chunks.push_back(Bytes::from(old_buffer));
			self.buffer.clear();
			if self.chunks.len() > self.max_chunks_count {
				self.chunks.pop_front().unwrap();
			}
		}
		self.buffer.extend_from_slice(buf);
		self.write_file(buf);
		Ok(buf.len())
	}

	fn flush(&mut self) -> Result<(), std::io::Error> {
		Ok(())
	}
}


#[cfg(test)]
mod tests {
	use std::fs;
	use super::*;

	fn remove_all(folder_path: &PathBuf) {
		if (!folder_path.exists()) {
			return;
		}
		for entry in std::fs::read_dir(folder_path).unwrap() {
			let entry = entry.unwrap();
			let path = entry.path();
			if path.is_file() {
				if path.extension().unwrap() == "log" {
					std::fs::remove_file(path).unwrap();
				}
			}
		}
	}

	#[test]
	fn basic_buffer() {
		let mut buffer = LogBuffer::new(100);
		let data = b"Hello, world!";
		buffer.write(data).unwrap();
		let chunk = buffer.next_chunk().unwrap();
		assert_eq!(chunk.as_ref(), b"Hello, world!");
	}

	#[test]
	fn get_newest_chunk() {
		let mut buffer = LogBuffer::new(5);
		let data = b"Hello,";
		buffer.write(data).unwrap();
		let data = b" world!";
		buffer.write(data).unwrap();
		let chunk = buffer.next_chunk().unwrap();
		assert_eq!(chunk.as_ref(), b" world!");
	}

	#[test]
	fn load_chunk_from_folder() {
		let path = std::path::PathBuf::from("./workdir/load_chunk_from_folder");
		remove_all(&path);
		let mut buffer = LogBuffer::new(5);
		buffer.set_folder_path(path.clone());
		buffer.write(b"12345").unwrap();
		buffer.write(b"67891").unwrap();
		let chunk = buffer.next_chunk().unwrap();
		assert_eq!(chunk.as_ref(), b"67891");
		let mut buffer = LogBuffer::new(5);
		buffer.set_folder_path(path);
		let chunk = buffer.next_chunk().unwrap();
		println!("chunk = {:?}", chunk);
		assert_eq!(chunk.as_ref(), b"67891");
		buffer.truncate(chunk.len());
		let chunk = buffer.next_chunk().unwrap();
		println!("chunk = {:?}", chunk);
		assert_eq!(chunk.as_ref(), b"12345");
		buffer.truncate(chunk.len());
	}

	#[test]
	fn test_file_rotation() {
		let path = PathBuf::from("./test_rotation");
		remove_all(&path);
		
		let mut buffer = LogBuffer::new(10);
		buffer.max_file_size = 20;
		buffer.max_file_count = 2;
		buffer.set_folder_path(path.clone());
		
		// Write enough data to trigger multiple rotations
		for _ in 0..5 {
			buffer.write_all(&[0; 15]).unwrap();
			buffer.flush().unwrap();
		}
		
		// Verify only max_file_count files remain
		let entries = std::fs::read_dir(&path).unwrap().count();
		assert_eq!(entries, buffer.max_file_count);
		
		remove_all(&path);
	}

	#[test]
    fn test_empty_next_chunk() {
        let mut buffer = LogBuffer::new(10);
        // If nothing has been written, next_chunk should return None.
        assert!(buffer.next_chunk().is_none());
    }

    #[test]
    fn test_set_folder_path_creates_directory() {
        let path = PathBuf::from("./workdir/test_set_folder");
        // Remove the directory first if it exists.
        let _ = fs::remove_dir_all(&path);
        {
            let mut buffer = LogBuffer::new(10);
            buffer.set_folder_path(path.clone());
        }
        // The folder should now exist.
        assert!(path.exists());
        remove_all(&path);
    }

    #[test]
    fn test_chunk_split_behavior() {
        // Test that writing in pieces causes the buffer to flush into chunks properly.
        let mut buffer = LogBuffer::new(5);
        buffer.write(b"123").unwrap();
        buffer.write(b"45").unwrap(); // total 5 bytes -> should trigger a chunk push
        let chunk = buffer.next_chunk().unwrap();
        assert_eq!(chunk.as_ref(), b"12345");
    }

    #[test]
    fn test_file_rotation_detailed() {
        let path = PathBuf::from("./workdir/test_file_rotation_detailed");
        remove_all(&path);
        let mut buffer = LogBuffer::new(10);
        buffer.max_file_size = 50;
        buffer.max_file_count = 3;
        buffer.set_folder_path(path.clone());

        // Write enough data to trigger rotations.
        for i in 0..10 {
            let data = vec![i as u8; 15];
            buffer.write(&data).unwrap();
        }

        // Check the directory for file names.
        let files: Vec<_> = fs::read_dir(&path)
            .unwrap()
            .filter_map(|entry| {
                let entry = entry.unwrap();
                entry.file_name().into_string().ok()
            })
            .collect();
        // There should be at most max_file_count files.
        assert!(files.len() <= buffer.max_file_count);
        remove_all(&path);
    }

    #[test]
    fn test_truncate_reduces_file_size() {
        let path = PathBuf::from("./workdir/test_truncate");
        remove_all(&path);
        let mut buffer = LogBuffer::new(10);
        buffer.set_folder_path(path.clone());
        let data = b"abcdefghij"; // 10 bytes
        buffer.write(data).unwrap();

        // Now truncate the file by 5 bytes.
        buffer.truncate(5);

        // Read the file content from disk.
        let file_path = path.join("0.log");
        let metadata = fs::metadata(&file_path).unwrap();
        assert_eq!(metadata.len(), 5);
        remove_all(&path);
    }

    #[test]
    fn test_open_file_twice_returns_same_file() {
        let path = PathBuf::from("./workdir/test_open_file_twice");
        remove_all(&path);
        let mut buffer = LogBuffer::new(10);
        buffer.set_folder_path(path.clone());
        let file1 = buffer.open_file() as *const _;
        let file2 = buffer.open_file() as *const _;
        assert_eq!(file1, file2);
        remove_all(&path);
    }

	#[test]
	fn test_log_rotation_file_deletion() {
		use std::fs;

		// Create a temporary directory for testing.
		let path = std::path::PathBuf::from("./workdir/test_log_rotation_file_deletion");
		// Clean up the directory if it already exists.
		remove_all(&path);

		// Configure a small file size to force rotations quickly,
		// and limit max_file_count to 3.
		let mut buffer = LogBuffer::new(10);
		buffer.max_file_size = 20; // Small threshold to trigger rotation
		buffer.max_file_count = 3; // Allow a maximum of 3 files
		buffer.set_folder_path(path.clone());

		// Write enough data to force multiple rotations.
		for _ in 0..10 {
			// Each write is 15 bytes; this should trigger several rotations.
			buffer.write_all(&[0; 15]).unwrap();
			buffer.flush().unwrap();
		}

		// Read all files with the ".log" extension from the folder.
		let log_files: Vec<_> = fs::read_dir(&path)
			.unwrap()
			.filter_map(|entry| {
				let entry = entry.unwrap();
				let path = entry.path();
				if path.is_file() && path.extension().and_then(|s| s.to_str()) == Some("log") {
					Some(path)
				} else {
					None
				}
			})
			.collect();
		println!("log_files = {:?}", log_files);

		// Assert that the number of log files does not exceed max_file_count.
		assert!(
			log_files.len() <= buffer.max_file_count,
			"Expected at most {} log files, found {}",
			buffer.max_file_count,
			log_files.len()
		);
		// Clean up after test.
		remove_all(&path);
	}
}