use std::fs::{create_dir_all, File, OpenOptions};
use std::io::{self, Cursor, Read, Write};
use std::net::TcpStream;
use std::sync::mpsc::{self, Receiver};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};
use bytes::buf;
use log::{Record, Level, Metadata, SetLoggerError};
use chrono::{DateTime, Local, Utc};
use rustls::client::{self, ClientConnectionData};
use rustls::{ClientConnection, RootCertStore, Stream};
use url::Url;

use crate::{LogEntry, LogLevel, Prop};

pub struct TLSConn {
	conn: ClientConnection,
	sock: TcpStream,
}

impl TLSConn {
	pub fn new(sock: TcpStream, server_name: String) -> Self {
		let root_store = RootCertStore {
			roots: webpki_roots::TLS_SERVER_ROOTS.into(),
		};
		let mut config = rustls::ClientConfig::builder()
			.with_root_certificates(root_store)
			.with_no_client_auth();
	
		// Allow using SSLKEYLOGFILE.
		config.key_log = Arc::new(rustls::KeyLogFile::new());

		let server_name = server_name.try_into().unwrap();
		let conn = rustls::ClientConnection::new(Arc::new(config), server_name).unwrap();
		TLSConn {
			conn,
			sock,
		}
	}

	fn stream(&mut self) -> Stream<'_, ClientConnection, TcpStream> {
		rustls::Stream::new(&mut self.conn, &mut self.sock)
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

impl Read for TLSConn {
	fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
		self.stream().read(buf)
	}
}

#[derive(Debug)]
pub struct ChunkedEncoder<T: Write + Read> {
    stream: T,
    last_write_at: Instant,
    total_bytes_sent: u64,
}

impl<T> ChunkedEncoder<T>
where
    T: Write + Read, 
{
	pub fn new(mut stream: T, url: Url, authorization: Option<String>) -> Result<Self, PuppyLogError> {
		let auth_header = match authorization {
			Some(token) => format!("Authorization: {}\n\n", token),
			None => format!("\n"),
		};
		let body = format!(
			"POST {} HTTP/1.1\r\n\
			Host: {}\r\n\
			Content-Type: application/octet-stream\r\n\
			Transfer-Encoding: chunked\r\n\
			Connection: keep-alive\r\n\
			{}",
			url.path(),
			url.host_str().unwrap(),
			auth_header,
		);
		loop {
			match stream.write_all(body.as_bytes()) {
				Ok(_) => break,
				Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => continue,
				Err(e) => panic!("Failed to write: {}", e),
			}
		}
		let mut response_buf = vec![0u8; 4096];
		let time = Instant::now();
		loop {
			match stream.read(&mut response_buf) {
				Ok(0) => {
					eprintln!("Connection closed by server");
					break;
				},
				Ok(n) => {
					let response = String::from_utf8_lossy(&response_buf[..n]);
					println!("response: {}", response);
					if response.starts_with("HTTP/1.1 200") {
						println!("Server accepted connection");
					} else {
						return Err(PuppyLogError::new(&response));
					}
				},
				Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => {
					if time.elapsed().as_millis() > 100 {
						println!("nothing to read");
						break;
					}
				},
				Err(e) => panic!("Failed to read: {}", e),
			}
		}
		Ok(ChunkedEncoder {
			stream,
			last_write_at: Instant::now(),
			total_bytes_sent: 0,
		})
	}

    pub fn close(&mut self) -> std::io::Result<()> {
		println!("ChunkedEncoder::close");
        self.flush()?;
        if self.total_bytes_sent > 0 {
            // Only send terminating chunk if we sent data
            self.stream.write_all(b"0\r\n\r\n")?;
        }
        self.stream.flush()
    }
}

impl<T: Write + Read> Write for ChunkedEncoder<T> {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        if buf.is_empty() {
            return Ok(0);
        }
		let buf_len = buf.len();
		let size_hex = format!("{:X}\r\n", buf_len);
		self.stream.write_all(size_hex.as_bytes())?;
		self.stream.write_all(buf)?;
        self.stream.write_all(b"\r\n")?;
		self.total_bytes_sent += buf_len as u64;
		self.last_write_at = Instant::now();
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        self.stream.flush()
    }
}

impl<T: Write + Read> Read for ChunkedEncoder<T> {
	fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
		let timer = Instant::now();
		loop {
			match self.stream.read(buf) {
				Ok(0) => {
					break Err(io::Error::new(
						io::ErrorKind::ConnectionAborted,
						"Connection closed by server"
					))
				},
				Ok(n) => break Ok(n),
				Err(e) if e.kind() == io::ErrorKind::WouldBlock => {
					if timer.elapsed().as_millis() > 2000 {
						println!("nothing to read");
						break Ok(0)
					}
				}
				Err(e) => break Err(e)
			}
		}
	}
}

impl<T> Drop for ChunkedEncoder<T> 
where 
    T: Write + Read
{
    fn drop(&mut self) {
        let _ = self.close();
    }
}

#[cfg(test)]
mod tests {
   use super::*;
   use std::io::Cursor;

	// #[test]
	// fn test_basic_write() -> std::io::Result<()> {
	// 	let url = Url::parse("http://localhost:8080").unwrap();
	// 	let cursor = Cursor::new(Vec::new());
	// 	let mut encoder = ChunkedEncoder::new(cursor, url, None).unwrap();
	// 	encoder.write_all(b"Hello World")?;
	// 	encoder.flush()?;
	// 	let output = encoder.stream.into_inner();
	// 	assert_eq!(output, b"B\r\nHello World\r\n");
	// 	Ok(())
	// }

	// #[test]
	// fn test_close() -> std::io::Result<()> {
	// 	let url = Url::parse("http://localhost:8080").unwrap();
	// 	let cursor = Cursor::new(Vec::new());
	// 	let mut encoder = ChunkedEncoder::new(cursor, url, None).unwrap();
	// 	encoder.write_all(b"data")?;
	// 	encoder.close()?;
	// 	let output = encoder.stream.into_inner();
	// 	assert_eq!(output, b"4\r\ndata\r\n0\r\n\r\n");
	// 	Ok(())
	// }
}

struct ResourceManager {
	builder: LoggerBuilder,
	file: Option<File>,
	client: Option<Box<dyn Write>>,
	url: Option<Url>,
	buffer: Vec<u8>,
}

impl ResourceManager {
	fn new(builder: LoggerBuilder) -> Self {
		ResourceManager {
			builder,
			file: None,
			client: None,
			url: None,
			buffer: Vec::new(),
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
	fn create_client(&mut self) -> Result<(), PuppyLogError> {
		match &self.builder.log_server {
			Some(url) => {
				let should_create = match &self.client {
					Some(_) => match &self.url {
						Some(u) => u != url,
						None => true,
					},
					None => true,
				};
				if should_create {
					self.url = Some(url.clone());
					let port = match url.port() {
						Some(p) => p,
						None => if url.scheme() == "https" { 443 } else { 80 },
					};
					let host = url.host_str().ok_or(PuppyLogError::new("no host in url"))?;
					let host = format!("{}:{}", host, port);
					let socket = TcpStream::connect(host)?;
					socket.set_nonblocking(true).unwrap();
					match url.scheme() {
						"http" => {
							self.client = Some(Box::new(ChunkedEncoder::new(socket, url.clone(), self.builder.authorization.clone())?));
						}
						"https" => {
							let tls = TLSConn::new(socket, url.host_str().unwrap().to_string());
							self.client = Some(Box::new(ChunkedEncoder::new(tls, url.clone(), self.builder.authorization.clone())?));
						}
						_ => {}
					};
				}
			}
			None => {},
		}
		Ok(())
	}

	pub fn flush(&mut self) {
		if let Some(file) = &mut self.file {
			if let Err(err) = file.flush() {
				eprintln!("Failed to flush file: {}", err);
			}
		}
		self.create_client();
		if let Some(client) = &mut self.client {
			if self.buffer.len() > 0 {
				println!("sending {} bytes", self.buffer.len());
				if let Err(err) = client.write_all(&self.buffer) {
					eprintln!("Failed to write to client: {}", err);
				}
				if let Err(err) = client.flush() {
					eprintln!("Failed to flush client: {}", err);
				}
			}
		}
		self.buffer.clear();
	}

	fn close(&mut self) {
		self.flush();
	}
}

impl Write for ResourceManager {
	fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
		if let Some(file) = &mut self.file {
			file.write(buf)?;
		}
		self.buffer.extend_from_slice(buf);
		if self.buffer.len() > self.builder.max_buffer_size as usize {
			println!("buffer full, flushing");
			self.flush();
		}
		Ok(buf.len())
	}

	fn flush(&mut self) -> io::Result<()> {
		self.flush();
		Ok(())
	}
}

enum WorkerMessage {
    LogEntry(LogEntry),
    Flush(mpsc::Sender<()>),
	FlushClose(mpsc::Sender<()>),
}

fn worker(rx: Receiver<WorkerMessage>, builder: LoggerBuilder) {
	let mut manager = ResourceManager::new(builder);

	loop {
		match rx.recv_timeout(Duration::from_millis(100)) {
			Ok(WorkerMessage::LogEntry(entry)) => entry.serialize(&mut manager).unwrap_or_default(),
			Ok(WorkerMessage::Flush(ack)) => {
				println!("WorkerMessage::Flush");
				manager.flush();
				let _ = ack.send(());
			},
			Ok(WorkerMessage::FlushClose(ack)) => {
				println!("WorkerMessage::FlushClose");
				manager.close();
				let _ = ack.send(());
				break;
			},
			Err(mpsc::RecvTimeoutError::Timeout) => manager.flush(),
			Err(mpsc::RecvTimeoutError::Disconnected) => break,
		};
	}

	println!("worker done");
}

#[derive(Clone)]
pub struct PuppylogClient {
	sender: mpsc::Sender<WorkerMessage>,
	level: Level,
	log_stdout: bool,
}

impl PuppylogClient {
	fn new(builder: LoggerBuilder) -> Self {
		let level = builder.level_filter;
		let stdout = builder.log_stdout;
		let (sender, rx) = mpsc::channel();
		thread::spawn(move || { worker(rx, builder) });
		PuppylogClient {
			sender,
			level,
			log_stdout: stdout,
		}
	}

	fn send_logentry(&self, entry: LogEntry) {
		self.sender.send(WorkerMessage::LogEntry(entry)).unwrap();
	}

    fn flush(&self) {
        let (ack_tx, ack_rx) = mpsc::channel();
        self.sender.send(WorkerMessage::Flush(ack_tx)).ok();
        let _ = ack_rx.recv(); // blocks until worker finishes flushing
    }

	pub fn close(&self) {
		let (ack_tx, ack_rx) = mpsc::channel();
		self.sender.send(WorkerMessage::FlushClose(ack_tx)).ok();
		let _ = ack_rx.recv();
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
				props: vec![
					Prop {
						key: "app".to_string(),
						value: "puppyapp".to_string()
					}
				],
				msg: record.args().to_string()
			};
			self.send_logentry(entry);
		}
	}

	fn flush(&self) {
		self.flush();
	}
}

#[derive(Debug)]
pub struct PuppyLogError {
	message: String,
}

impl PuppyLogError {
	pub fn new(message: &str) -> Self {
		PuppyLogError {
			message: message.to_string(),
		}
	}
}

impl std::fmt::Display for PuppyLogError {
	fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
		write!(f, "{}", self.message)
	}
}

impl From<url::ParseError> for PuppyLogError {
    fn from(error: url::ParseError) -> Self {
        PuppyLogError::new(&error.to_string())
    }
}

impl From<io::Error> for PuppyLogError {
	fn from(error: io::Error) -> Self {
		PuppyLogError::new(&error.to_string())
	}
}

pub struct LoggerBuilder {
	max_log_file_size: u64,
	max_log_files: u32,
	min_buffer_size: u64,
	max_buffer_size: u64,
	log_folder: Option<PathBuf>,
	log_server: Option<Url>,
	authorization: Option<String>,
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
			authorization: None,
			level_filter: Level::Info,
		}
	}

	pub fn folder<P: AsRef<Path>>(mut self, path: P) -> Self {
		let path: &Path = path.as_ref();
		self.log_folder = Some(path.to_path_buf());
		self
	}

	pub fn server(mut self, url: &str) -> Result<Self, PuppyLogError> {
		let url = Url::parse(url)?;
		self.log_server = Some(url);
		Ok(self)
	}

	pub fn authorization(mut self, token: &str) -> Self {
		self.authorization = Some(token.to_string());
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

	pub fn build(self) -> Result<&'static mut PuppylogClient, SetLoggerError> {
		let logger = PuppylogClient::new(self);
		unsafe {
			LOGGER = Some(logger);
			log::set_logger(LOGGER.as_ref().unwrap())
				.map(|()| log::set_max_level(LOGGER.as_ref().unwrap().level.to_level_filter()))
		};
		Ok(unsafe { LOGGER.as_mut().expect("Logger not initialized") })
	}
}

pub fn get_logger() -> &'static mut PuppylogClient {
	unsafe { LOGGER.as_mut().expect("Logger not initialized") }
}

static mut LOGGER: Option<PuppylogClient> = None;