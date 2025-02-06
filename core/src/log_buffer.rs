use std::collections::VecDeque;
use std::io::Write;
use std::mem;
use bytes::Bytes;
use crate::log_rotator::LogRotator;
use crate::PuppylogBuilder;

#[derive(Default)]
pub struct LogBuffer {
	buffer: Vec<u8>,
	chunks: VecDeque<Bytes>,
	chunk_size: usize,
	max_chunks_count: usize,
	max_file_count: usize,
	max_file_size: usize,
	log_rotator: Option<LogRotator>,
}

impl LogBuffer {
	pub fn new(builder: &PuppylogBuilder) -> LogBuffer {
		LogBuffer {
			buffer: Vec::with_capacity(builder.chunk_size),
			chunks: VecDeque::new(),
			chunk_size: builder.chunk_size,
			max_chunks_count: 20,
			max_file_count: builder.max_file_count,
			max_file_size: 100,
			log_rotator: None,
		}
	}

	pub fn set_folder_path(&mut self, builder: &PuppylogBuilder) {
		// self.log_rotator = Some(LogRotator::new(folder_path, self.max_file_size, self.max_file_count));
	}

	pub fn buffer_size(&self) -> usize {
		self.buffer.len()
	}

	fn freeze(&mut self) {
		let old_buffer = mem::replace(&mut self.buffer, Vec::with_capacity(self.chunk_size));
		self.chunks.push_back(Bytes::from(old_buffer));
		self.buffer.clear();
		if self.chunks.len() > self.max_chunks_count {
			println!("need to drop oldest chunk");
			self.chunks.pop_front().unwrap();
		}
	}

	pub fn next_chunk(&mut self) -> Option<Bytes> {
		if self.buffer.len() > 0 {
			self.freeze();
		}
		// if self.chunks.len() == 0 {
		// 	self.read_chunks_from_files();
		// }
		self.chunks.pop_back()
	}

	pub fn pop_newest_chunk(&mut self) -> Option<Bytes> {
		self.chunks.pop_back()
	}
}

impl Write for LogBuffer {
	fn write(&mut self, buf: &[u8]) -> Result<usize, std::io::Error> {
		if self.buffer.len() >= self.chunk_size {
			self.freeze();
		}
		self.buffer.extend_from_slice(buf);
		Ok(buf.len())
	}

	fn flush(&mut self) -> Result<(), std::io::Error> {
		Ok(())
	}
}


#[cfg(test)]
mod tests {
	use std::fs;
	use bytes::buf;
	use super::*;

	// fn remove_all(folder_path: &PathBuf) {
	// 	if (!folder_path.exists()) {
	// 		return;
	// 	}
	// 	for entry in std::fs::read_dir(folder_path).unwrap() {
	// 		let entry = entry.unwrap();
	// 		let path = entry.path();
	// 		if path.is_file() {
	// 			if path.extension().unwrap() == "log" {
	// 				std::fs::remove_file(path).unwrap();
	// 			}
	// 		}
	// 	}
	// }

	// #[test]
	// fn basic_buffer() {
	// 	let mut buffer = LogBuffer::new(100);
	// 	let data = b"Hello, world!";
	// 	buffer.write(data).unwrap();
	// 	let chunk = buffer.next_chunk().unwrap();
	// 	assert_eq!(chunk.as_ref(), b"Hello, world!");
	// }

	// #[test]
	// fn get_newest_chunk() {
	// 	let mut buffer = LogBuffer::new(5);
	// 	let data = b"Hello,";
	// 	buffer.write(data).unwrap();
	// 	let data = b" world!";
	// 	buffer.write(data).unwrap();
	// 	let chunk = buffer.next_chunk().unwrap();
	// 	assert_eq!(chunk.as_ref(), b" world!");
	// }

	// #[test]
	// fn load_chunk_from_folder() {
	// 	let path = std::path::PathBuf::from("./workdir/load_chunk_from_folder");
	// 	remove_all(&path);
	// 	let mut buffer = LogBuffer::new(5);
	// 	buffer.set_folder_path(path.clone());
	// 	buffer.write(b"12345").unwrap();
	// 	buffer.write(b"67891").unwrap();
	// 	let chunk = buffer.next_chunk().unwrap();
	// 	assert_eq!(chunk.as_ref(), b"67891");
	// 	let mut buffer = LogBuffer::new(5);
	// 	buffer.set_folder_path(path);
	// 	let chunk = buffer.next_chunk().unwrap();
	// 	println!("chunk = {:?}", chunk);
	// 	assert_eq!(chunk.as_ref(), b"67891");
	// 	buffer.truncate(chunk.len());
	// 	let chunk = buffer.next_chunk().unwrap();
	// 	println!("chunk = {:?}", chunk);
	// 	assert_eq!(chunk.as_ref(), b"12345");
	// 	buffer.truncate(chunk.len());
	// }

	// #[test]
	// fn test_file_rotation() {
	// 	let path = PathBuf::from("./test_rotation");
	// 	remove_all(&path);
		
	// 	let mut buffer = LogBuffer::new(10);
	// 	buffer.max_file_size = 20;
	// 	buffer.max_file_count = 2;
	// 	buffer.set_folder_path(path.clone());
		
	// 	// Write enough data to trigger multiple rotations
	// 	for _ in 0..5 {
	// 		buffer.write_all(&[0; 15]).unwrap();
	// 		buffer.flush().unwrap();
	// 	}
		
	// 	// Verify only max_file_count files remain
	// 	let entries = std::fs::read_dir(&path).unwrap().count();
	// 	assert_eq!(entries, buffer.max_file_count);
		
	// 	remove_all(&path);
	// }

	// #[test]
    // fn test_empty_next_chunk() {
    //     let mut buffer = LogBuffer::new(10);
    //     // If nothing has been written, next_chunk should return None.
    //     assert!(buffer.next_chunk().is_none());
    // }

    // #[test]
    // fn test_set_folder_path_creates_directory() {
    //     let path = PathBuf::from("./workdir/test_set_folder");
    //     // Remove the directory first if it exists.
    //     let _ = fs::remove_dir_all(&path);
    //     {
    //         let mut buffer = LogBuffer::new(10);
    //         buffer.set_folder_path(path.clone());
    //     }
    //     // The folder should now exist.
    //     assert!(path.exists());
    //     remove_all(&path);
    // }

    // #[test]
    // fn test_chunk_split_behavior() {
    //     // Test that writing in pieces causes the buffer to flush into chunks properly.
    //     let mut buffer = LogBuffer::new(5);
    //     buffer.write(b"123").unwrap();
    //     buffer.write(b"45").unwrap(); // total 5 bytes -> should trigger a chunk push
    //     let chunk = buffer.next_chunk().unwrap();
    //     assert_eq!(chunk.as_ref(), b"12345");
    // }

    // #[test]
    // fn test_file_rotation_detailed() {
    //     let path = PathBuf::from("./workdir/test_file_rotation_detailed");
    //     remove_all(&path);
    //     let mut buffer = LogBuffer::new(10);
    //     buffer.max_file_size = 50;
    //     buffer.max_file_count = 3;
    //     buffer.set_folder_path(path.clone());

    //     // Write enough data to trigger rotations.
    //     for i in 0..10 {
    //         let data = vec![i as u8; 15];
    //         buffer.write(&data).unwrap();
    //     }

    //     // Check the directory for file names.
    //     let files: Vec<_> = fs::read_dir(&path)
    //         .unwrap()
    //         .filter_map(|entry| {
    //             let entry = entry.unwrap();
    //             entry.file_name().into_string().ok()
    //         })
    //         .collect();
    //     // There should be at most max_file_count files.
    //     assert!(files.len() <= buffer.max_file_count);
    //     remove_all(&path);
    // }

    // #[test]
    // fn test_truncate_reduces_file_size() {
    //     let path = PathBuf::from("./workdir/test_truncate");
    //     remove_all(&path);
    //     let mut buffer = LogBuffer::new(10);
    //     buffer.set_folder_path(path.clone());
    //     let data = b"abcdefghij"; // 10 bytes
    //     buffer.write(data).unwrap();

    //     // Now truncate the file by 5 bytes.
    //     buffer.truncate(5);

    //     // Read the file content from disk.
    //     let file_path = path.join("0.log");
    //     let metadata = fs::metadata(&file_path).unwrap();
    //     assert_eq!(metadata.len(), 5);
    //     remove_all(&path);
    // }

    // #[test]
    // fn test_open_file_twice_returns_same_file() {
    //     let path = PathBuf::from("./workdir/test_open_file_twice");
    //     remove_all(&path);
    //     let mut buffer = LogBuffer::new(10);
    //     buffer.set_folder_path(path.clone());
    //     let file1 = buffer.open_file() as *const _;
    //     let file2 = buffer.open_file() as *const _;
    //     assert_eq!(file1, file2);
    //     remove_all(&path);
    // }

	// #[test]
	// fn test_log_rotation_file_deletion() {
	// 	use std::fs;

	// 	// Create a temporary directory for testing.
	// 	let path = std::path::PathBuf::from("./workdir/test_log_rotation_file_deletion");
	// 	// Clean up the directory if it already exists.
	// 	remove_all(&path);

	// 	// Configure a small file size to force rotations quickly,
	// 	// and limit max_file_count to 3.
	// 	let mut buffer = LogBuffer::new(10);
	// 	buffer.max_file_size = 20; // Small threshold to trigger rotation
	// 	buffer.max_file_count = 3; // Allow a maximum of 3 files
	// 	buffer.set_folder_path(path.clone());

	// 	// Write enough data to force multiple rotations.
	// 	for _ in 0..10 {
	// 		// Each write is 15 bytes; this should trigger several rotations.
	// 		buffer.write_all(&[0; 15]).unwrap();
	// 		buffer.flush().unwrap();
	// 	}

	// 	// Read all files with the ".log" extension from the folder.
	// 	let log_files: Vec<_> = fs::read_dir(&path)
	// 		.unwrap()
	// 		.filter_map(|entry| {
	// 			let entry = entry.unwrap();
	// 			let path = entry.path();
	// 			if path.is_file() && path.extension().and_then(|s| s.to_str()) == Some("log") {
	// 				Some(path)
	// 			} else {
	// 				None
	// 			}
	// 		})
	// 		.collect();
	// 	println!("log_files = {:?}", log_files);

	// 	// Assert that the number of log files does not exceed max_file_count.
	// 	assert!(
	// 		log_files.len() <= buffer.max_file_count,
	// 		"Expected at most {} log files, found {}",
	// 		buffer.max_file_count,
	// 		log_files.len()
	// 	);
	// 	// Clean up after test.
	// 	remove_all(&path);
	// }

	// #[test]
	// fn reading_chuck_from_multiple_files() {
	// 	let path = std::path::PathBuf::from("./workdir/reading_chuck_from_multiple_files");
	// 	remove_all(&path);
	// 	let mut buffer = LogBuffer::new(5);
	// 	buffer.max_file_size = 10;
	// 	buffer.set_folder_path(path.clone());
	// 	for i in 0..200 {
	// 		buffer.write(format!("Hello {}\n", i).as_bytes()).unwrap();
	// 	}
	// 	println!("buffer written");
	// 	while let Some(chunk) = buffer.next_chunk() {
	// 		println!("chunk = {:?}", chunk);
	// 		buffer.truncate(chunk.len());
	// 	}
	// }
}