use std::fs::{create_dir_all, File, OpenOptions};
use std::io::{self, Cursor, Read, Write};
use std::net::TcpStream;
use std::sync::mpsc::{self, Receiver};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::thread;
use std::time::Instant;
use log::{Record, Level, Metadata, SetLoggerError};
use chrono::{DateTime, Local, Utc};
use rustls::client::{self, ClientConnectionData};
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

impl Read for TLSConn {
	fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
		self.stream().read(buf)
	}
}

#[derive(Debug)]
pub struct ChunkedEncoder<T: Write + Read> {
    stream: T,
    min_buffer_size: u64,
    max_buffer_size: u64,
    last_write_at: Instant,
    buffer: Vec<u8>,
    total_bytes_sent: u64,
}

impl<T> ChunkedEncoder<T>
where
    T: Write + Read, 
{
    pub fn new(mut stream: T, url: Url, min_buffer_size: u64, max_buffer_size: u64) -> Self {
        let body = format!(
            "POST {} HTTP/1.1\r\n\
             Host: {}\r\n\
             Content-Type: application/octet-stream\r\n\
             Transfer-Encoding: chunked\r\n\
             Connection: keep-alive\r\n\
             \r\n",
            url.path(),
            url.host_str().unwrap()
        );
        stream.write_all(body.as_bytes()).unwrap();

        ChunkedEncoder {
            stream,
            min_buffer_size,
            max_buffer_size,
            last_write_at: Instant::now(),
            buffer: Vec::with_capacity(min_buffer_size as usize),
            total_bytes_sent: 0,
        }
    }

    pub fn close(&mut self) -> std::io::Result<()> {
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
            let chunk_len = chunk.len();
            let size_hex = format!("{:X}\r\n", chunk_len);
            self.stream.write_all(size_hex.as_bytes())?;
            self.stream.write_all(chunk)?;
            self.stream.write_all(b"\r\n")?;
            self.total_bytes_sent += chunk_len as u64;
        }

        self.last_write_at = Instant::now();
        self.buffer.clear();
        self.stream.flush()?;

        // Handle response
        let mut response_buf = vec![0u8; 4096];
        match self.stream.read(&mut response_buf) {
            Ok(0) => {
                Err(io::Error::new(
                    io::ErrorKind::ConnectionAborted,
                    "Connection closed by server"
                ))
            },
            Ok(n) => {
                let response = String::from_utf8_lossy(&response_buf[..n]);
                
                // Check for HTTP error responses
                if response.starts_with("HTTP/1.1 4") || response.starts_with("HTTP/1.1 5") {
                    Err(io::Error::new(
                        io::ErrorKind::Other,
                        format!("Server error: {}", response)
                    ))
                } else {
                    Ok(())
                }
            }
            Err(e) if e.kind() == io::ErrorKind::WouldBlock => Ok(()),
            Err(e) => Err(e)
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

	#[test]
	fn test_basic_write() -> std::io::Result<()> {
		let url = Url::parse("http://localhost:8080").unwrap();
		let cursor = Cursor::new(Vec::new());
		let mut encoder = ChunkedEncoder::new(cursor, url, 10, 100);
		encoder.write_all(b"Hello World")?;
		encoder.flush()?;
		let output = encoder.stream.into_inner();
		assert_eq!(output, b"B\r\nHello World\r\n");
		Ok(())
	}

	#[test]
	fn test_close() -> std::io::Result<()> {
		let cursor = Cursor::new(Vec::new());
		let mut encoder = ChunkedEncoder::new(cursor, url, 10, 100);
		encoder.write_all(b"data")?;
		encoder.close()?;
		let output = encoder.stream.into_inner();
		assert_eq!(output, b"4\r\ndata\r\n0\r\n\r\n");
		Ok(())
	}
}

struct ResourceManager {
	builder: LoggerBuilder,
	file: Option<File>,
	client: Option<Box<dyn Write>>,
	url: Option<Url>,
}

impl ResourceManager {
	fn new(builder: LoggerBuilder) -> Self {
		ResourceManager {
			builder,
			file: None,
			client: None,
			url: None,
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
				let should_create = match &self.client {
					Some(_) => match &self.url {
						Some(u) => u != &url,
						None => true,
					},
					None => true,
				};
				if should_create {
					self.url = Some(url.clone());
					let host = format!("{}:{}", url.host_str().unwrap(), url.port().unwrap_or(80));
					println!("connecting to {}", host);
					let socket = TcpStream::connect(host).unwrap();
					socket.set_nonblocking(true).unwrap();
					println!("connected to {}", url);
					println!("scheme: {}", url.scheme());
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
				}
				Some(self.client.as_mut().unwrap())
			}
			None => None,
		}
	}

	pub fn flush(&mut self) {
		if let Some(file) = &mut self.file {
			file.flush().unwrap();
		}
		if let Some(client) = &mut self.client {
			client.flush().unwrap();
		}
	}
}

enum WorkerMessage {
    LogEntry(LogEntry),
    Flush(mpsc::Sender<()>),
	FlushClose(mpsc::Sender<()>),
}

fn worker(rx: Receiver<WorkerMessage>, builder: LoggerBuilder) {
	println!("worker started");
	let mut manager = ResourceManager::new(builder);
	for msg in rx {
        match msg {
            WorkerMessage::LogEntry(entry) => {
                println!("{:?}", entry);
                if let Some(file) = manager.get_logfile(&entry.timestamp) {
                    entry.serialize(file);
                }
                if let Some(client) = manager.get_client() {
                    entry.serialize(client);
                }
            }
            WorkerMessage::Flush(ack) => {
				manager.flush();
                let _ = ack.send(());
            },
			WorkerMessage::FlushClose(ack) => {
				manager.flush();
				if let Some(ref client) = manager.client {
					drop(client);
				}
			}
        }
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
		self.flush();
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
				props: vec![],
				msg: record.args().to_string()
			};
			self.send_logentry(entry);
		}
	}

	fn flush(&self) {
		self.flush();
	}
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