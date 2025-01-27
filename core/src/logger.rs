use std::fs::{create_dir_all, File, OpenOptions};
use std::io::{Cursor, Write};
use std::net::TcpStream;
use std::sync::mpsc::{self, Receiver};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::thread;
use std::time::Instant;
use log::{Record, Level, Metadata, SetLoggerError};
use chrono::{DateTime, Local, Utc};
use rustls::client::ClientConnectionData;
use rustls::{ClientConnection, RootCertStore, Stream};
use url::Url;

use crate::{LogEntry, LogLevel};

pub struct TLSConn {
	conn: ClientConnection,
	sock: TcpStream,
}

impl TLSConn {
	pub fn new(sock: TcpStream) -> Self {
		let root_store = RootCertStore {
			roots: webpki_roots::TLS_SERVER_ROOTS.into(),
		};
		let mut config = rustls::ClientConfig::builder()
			.with_root_certificates(root_store)
			.with_no_client_auth();
	
		// Allow using SSLKEYLOGFILE.
		config.key_log = Arc::new(rustls::KeyLogFile::new());
	
		let server_name = "www.rust-lang.org".try_into().unwrap();
		let conn = rustls::ClientConnection::new(Arc::new(config), server_name).unwrap();
		TLSConn {
			conn,
			sock,
		}
	}

	fn stream(&mut self) -> Stream<'_, ClientConnection, TcpStream> {
		Stream {
			conn: &mut self.conn,
			sock: &mut self.sock,
		}
	}
}

impl Write for TLSConn {
	fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
		self.stream().write(buf)
	}

	fn flush(&mut self) -> std::io::Result<()> {
		self.stream().flush()
	}
}

#[derive(Debug)]
pub struct ChunkedEncoder<T> {
   stream: T,
   min_buffer_size: u64,
   max_buffer_size: u64,
   last_write_at: Instant,
   buffer: Vec<u8>,
}

impl<T> ChunkedEncoder<T>
where
   T: Write, 
{
	pub fn new(mut stream: T, url: Url, min_buffer_size: u64, max_buffer_size: u64) -> Self {
		let body = format!(r"POST {} HTTP/1.1
Host: {}
Content-Type: application/octet-stream
Transfer-Encoding: chunked", url.path(), url.host_str().unwrap());
		stream.write_all(body.as_bytes()).unwrap(); 

		ChunkedEncoder {
			stream,
			min_buffer_size,
			max_buffer_size,
			last_write_at: Instant::now(),
			buffer: Vec::with_capacity(min_buffer_size as usize),
		}
	}

   	pub fn close(&mut self) -> std::io::Result<()> {
		self.flush()?;
		// Send zero-length chunk to indicate end
		self.stream.write_all(b"0\r\n\r\n")?;
		self.stream.flush()
	}	
}

impl<T: Write> Write for ChunkedEncoder<T> {
   fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
	   self.buffer.extend_from_slice(buf);
	   
	   if self.buffer.len() >= self.min_buffer_size as usize 
		   || self.last_write_at.elapsed().as_secs() >= 15 
		{
			self.flush()?;
		}
		
		Ok(buf.len())
	}

	fn flush(&mut self) -> std::io::Result<()> {
		if self.buffer.is_empty() {
			return Ok(());
		}
		for chunk in self.buffer.chunks(self.max_buffer_size as usize) {
			let size_hex = format!("{:X}\r\n", chunk.len());
			self.stream.write_all(size_hex.as_bytes())?;
			self.stream.write_all(chunk)?;
			self.stream.write_all(b"\r\n")?;
		}	
		self.last_write_at = Instant::now();
		self.buffer.clear();
		self.stream.flush()
	}
	}

// impl<T> Drop for ChunkedEncoder<T>
// where
// 	T: Write,
// {
// 	fn drop(&mut self) {
// 		let _ = self.close();
// 	}
// }

#[cfg(test)]
mod tests {
   use super::*;
   use std::io::Cursor;

	#[test]
	fn test_basic_write() -> std::io::Result<()> {
		let cursor = Cursor::new(Vec::new());
		let mut encoder = ChunkedEncoder::new(cursor, 10, 100);
		encoder.write_all(b"Hello World")?;
		encoder.flush()?;
		let output = encoder.stream.into_inner();
		assert_eq!(output, b"B\r\nHello World\r\n");
		Ok(())
	}

	#[test]
	fn test_close() -> std::io::Result<()> {
		let cursor = Cursor::new(Vec::new());
		let mut encoder = ChunkedEncoder::new(cursor, 10, 100);
		encoder.write_all(b"data")?;
		encoder.close()?;
		let output = encoder.stream.into_inner();
		assert_eq!(output, b"4\r\ndata\r\n0\r\n\r\n");
		Ok(())
	}
}

// fn create_stream(url: &str) -> impl Write {
// 	let url = Url::parse(url).unwrap();
// 	let stream = TcpStream::connect(url.host_str().unwrap()).unwrap();
// 	match url.scheme() {
// 		"http" => {
// 			let client = HTTPClient::new(url);
// 			ChunkedEncoder::new(client, url, 1024, 1024 * 1024)
// 		}
// 		"https" => {
// 			let client = HTTPSClient::new(url);
// 			ChunkedEncoder::new(client, url, 1024, 1024 * 1024)
// 		}
// 		_ => panic!("Invalid scheme"),
// 	}
// }

struct ResourceManager {
	builder: LoggerBuilder,
	file: Option<File>,
	client: Option<Box<dyn Write>>,
}

impl ResourceManager {
	fn new(builder: LoggerBuilder) -> Self {
		ResourceManager {
			builder,
			file: None,
			client: None,
		}
	}

	fn get_logfile(&mut self, timestamp: &DateTime<Utc>) -> Option<&mut File> {
		match &self.builder.log_folder {
			Some(d) => {
				let path = d.join(format!("{}.log", timestamp.format("%Y-%m-%d")));
				if self.file.is_none() {
					self.file = Some(OpenOptions::new().create(true).append(true).open(&path).unwrap());
				}
				Some(self.file.as_mut().unwrap())
			},
			None => None,
		}
	}
	fn get_client(&mut self) -> Option<&mut impl Write> {
		match &self.builder.log_server {
			Some(url) => {
				let url = Url::parse(url).unwrap();
				let socket = TcpStream::connect(url.host_str().unwrap()).unwrap();
				match url.scheme() {
					"http" => {
						self.client = Some(Box::new(ChunkedEncoder::new(socket, url, self.builder.min_buffer_size, self.builder.max_buffer_size)));
					}
					"https" => {
						let tls = TLSConn::new(socket);
						self.client = Some(Box::new(ChunkedEncoder::new(tls, url, self.builder.min_buffer_size, self.builder.max_buffer_size)));
					}
					_ => {}
				};
				Some(self.client.as_mut().unwrap())
			}
			None => None,
		}
	}
}

fn worker(rx: Receiver<LogEntry>, builder: LoggerBuilder) {
	let min_buffer_size = builder.min_buffer_size;
	let mut manager = ResourceManager::new(builder);
	for entry in rx {
		println!("{:?}", entry);
		if let Some(file) = manager.get_logfile(&entry.timestamp) {
			entry.serialize(file);
		}
		if let Some(client) = manager.get_client() {
			entry.serialize(client);
		}
	}
}

#[derive(Clone)]
pub struct PuppylogClient {
	sender: mpsc::Sender<LogEntry>,
	level: Level,
	log_stdout: bool,
}

impl PuppylogClient {
	fn new(builder: LoggerBuilder) -> Self {
		let level = builder.level_filter;
		let stdout = builder.log_stdout;
		let (sender, rx) = mpsc::channel();
		worker(rx, builder);
		PuppylogClient {
			sender,
			level,
			log_stdout: stdout,
		}
	}

	fn send_logentry(&self, entry: LogEntry) {
		self.sender.send(entry).unwrap();
	}
}

impl log::Log for PuppylogClient {
	fn enabled(&self, metadata: &Metadata) -> bool {
		metadata.level() <= self.level
	}

	fn log(&self, record: &Record) {
		if self.enabled(record.metadata()) {
			if self.log_stdout {
				println!("{} [{}] {}", record.level(), record.target(), record.args());
			}
			let level = match record.level() {
				Level::Error => LogLevel::Error,
				Level::Warn => LogLevel::Warn,
				Level::Info => LogLevel::Info,
				Level::Debug => LogLevel::Debug,
				Level::Trace => LogLevel::Debug,
			};
			let entry = LogEntry {
				version: 1,
				level,
				timestamp: Utc::now(),
				random: 0,
				props: vec![],
				msg: record.args().to_string()
			};
			self.send_logentry(entry);
		}
	}

	fn flush(&self) {}
}

pub struct LoggerBuilder {
	max_log_file_size: u64,
	max_log_files: u32,
	min_buffer_size: u64,
	max_buffer_size: u64,
	log_folder: Option<PathBuf>,
	log_server: Option<String>,
	log_stdout: bool,
	level_filter: Level,
}

impl LoggerBuilder {
	pub fn new() -> Self {
		LoggerBuilder {
			max_log_file_size: 1024 * 1024 * 10,
			max_log_files: 10,
			min_buffer_size: 1024,
			max_buffer_size: 1024 * 1024,
			log_folder: None,
			log_server: None,
			log_stdout: true,
			level_filter: Level::Info,
		}
	}

	pub fn folder<P: AsRef<Path>>(mut self, path: P) -> Self {
		let path: &Path = path.as_ref();
		self.log_folder = Some(path.to_path_buf());
		self
	}

	pub fn server(mut self, url: &str) -> Self {
		self.log_server = Some(url.to_string());
		self
	}

	pub fn level(mut self, level: Level) -> Self {
		self.level_filter = level;
		self
	}

	pub fn stdout(mut self) -> Self {
		self.log_stdout = true;
		self
	}

	pub fn build(self) -> Result<(), SetLoggerError> {
		let logger = PuppylogClient::new(self);
		unsafe {
			LOGGER = Some(logger);
			log::set_logger(LOGGER.as_ref().unwrap())
				.map(|()| log::set_max_level(LOGGER.as_ref().unwrap().level.to_level_filter()))
		}
	}
}

pub fn get_logger() -> &'static mut PuppylogClient {
	unsafe { LOGGER.as_mut().expect("Logger not initialized") }
}

static mut LOGGER: Option<PuppylogClient> = None;