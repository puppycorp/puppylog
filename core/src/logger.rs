use std::fs::{self, File, OpenOptions};
use std::io::{self, Cursor, Read, Write};
use std::net::TcpStream;
use std::str::FromStr;
use std::sync::mpsc::{self, Receiver};
use std::path::{Path, PathBuf};
use std::thread;
use std::time::{Duration, Instant};
use bytes::Bytes;
use log::{Record, Level, Metadata, SetLoggerError};
use chrono::Utc;
use native_tls::TlsConnector;
use tungstenite::client::client_with_config;
use tungstenite::http::Uri;
use tungstenite::stream::MaybeTlsStream;
use tungstenite::{ClientRequestBuilder, Message, WebSocket};

use crate::log_buffer::LogBuffer;
use crate::{check_expr, parse_log_query, LogEntry, LogLevel, Prop, PuppylogEvent, QueryAst};

pub struct LogRotator {
    folder: PathBuf,
    file: Option<std::fs::File>,
    current_size: u64,
    max_size: u64,
    max_files: u32,
}

impl LogRotator {
    pub fn new(logfolder: PathBuf, max_files: u32, max_size: u64) -> Self {
		if !logfolder.exists() {
			if let Err(err) = std::fs::create_dir_all(&logfolder) {
				eprintln!("Failed to create log folder: {}", err);
			}
		}
        LogRotator {
            current_size: 0,
            file: None,
            folder: logfolder,
            max_files,
            max_size
        }
    }

    fn rename_files(&self) -> Result<(), PuppyLogError> {
        let mut files: Vec<_> = std::fs::read_dir(&self.folder)?
            .filter_map(|f| f.ok())
            .filter(|f| f.file_type().map(|t| t.is_file()).unwrap_or(false))
            .map(|f| f.path())
            .filter_map(|p| {
				let file_name = match p.file_name() {
					Some(name) => name.to_string_lossy().to_string(),
					None => return None,
				};
				let parts: Vec<&str> = file_name.split('.').collect();
				if parts.len() != 3 {
					return None;
				}
				let index = match parts[2].parse::<u32>() {
					Ok(i) => i,
					Err(_) => return None,
				};
				Some(index)
			})
            .collect();
        
        files.sort_by(|a, b| b.cmp(a));
        
        for i in files.iter() {
			let path = self.folder.join(format!("app.log.{}", i));
            if *i >= self.max_files {
				println!("too many files, removing: {}", path.display());
                std::fs::remove_file(path)?;
                continue;
            }
            let new_path = self.folder.join(format!("app.log.{}", i + 1));
            std::fs::rename(path, new_path)?;
        }

        Ok(())
    }

    pub fn choose(&mut self) -> Result<(), PuppyLogError> {
        if let Some(file) = &self.file {
            if self.current_size < self.max_size {
				//println!("current size: {}, max size: {}", self.current_size, self.max_size);
                return Ok(());
            }
        }
        
        self.rename_files()?;
        self.file = None;
        
        let file = OpenOptions::new()
            .create(true)
            .append(true)
            .write(true)
            .open(self.folder.join("app.log.1"))?;
            
        let meta = file.metadata()?;
        self.current_size = meta.len();
        self.file = Some(file);
        Ok(())
    }

    pub fn write(&mut self, data: &[u8]) -> Result<(), PuppyLogError> {
        self.choose()?;
        if let Some(file) = &mut self.file {
            file.write_all(data)?;
            self.current_size += data.len() as u64;
        }
        Ok(())
    }

	/// Truncate the log files by removing a total of `count` bytes,
    /// starting with the newest file (`app.log.1`) and proceeding to older files
    /// if necessary. When a file’s size is less than or equal to the remaining bytes
    /// to remove, the file is deleted; otherwise, it is truncated (by cutting off the end).
    pub fn truncate(&mut self, count: usize) -> Result<(), PuppyLogError> {
        // The number of bytes left to remove.
        let mut remaining = count as u64;

        // Find all log files matching the pattern "app.log.<number>"
        let mut logs = Vec::new();
        for entry in fs::read_dir(&self.folder)? {
            let entry = entry?;
            if !entry.file_type()?.is_file() {
                continue;
            }
            let path = entry.path();
            if let Some(file_name) = path.file_name().and_then(|s| s.to_str()) {
                // Expecting names like "app.log.1", "app.log.2", etc.
                let parts: Vec<&str> = file_name.split('.').collect();
                if parts.len() == 3 && parts[0] == "app" && parts[1] == "log" {
                    if let Ok(index) = parts[2].parse::<u32>() {
                        logs.push((index, path));
                    }
                }
            }
        }

        // Sort logs by index ascending so that app.log.1 (newest) comes first
        logs.sort_by_key(|(index, _)| *index);

        // Process each file in order until we've removed the requested bytes.
        for (index, path) in logs {
            if remaining == 0 {
                break;
            }
            let metadata = fs::metadata(&path)?;
            let file_size = metadata.len();
            if file_size <= remaining {
                // Remove the entire file
                fs::remove_file(&path)?;
                remaining -= file_size;
                // If the active log is removed, clear our in-memory handle.
                if index == 1 {
                    self.file = None;
                    self.current_size = 0;
                }
            } else {
                // We need to remove part of this file.
                let new_size = file_size - remaining;
                let file = OpenOptions::new().write(true).open(&path)?;
                file.set_len(new_size)?;
                // If this is the current (active) log file, update our current_size.
                if index == 1 {
                    self.current_size = new_size;
                }
                remaining = 0;
            }
        }

        Ok(())
    }
}

enum WorkerMessage {
    LogEntry(LogEntry),
    Flush(mpsc::Sender<()>),
	FlushClose(mpsc::Sender<()>),
}

fn worker(rx: Receiver<WorkerMessage>, builder: PuppylogBuilder) {
	let url = match &builder.log_server {
		Some(url) => url.clone(),
		None => return,
	};
	let mut client: Option<WebSocket<MaybeTlsStream<TcpStream>>> = None;
	let mut logquery: Option<QueryAst> = None;
	let mut connect_timer = Instant::now();
	let mut buffer = LogBuffer::new(builder.max_buffer_size as usize);

	'main: loop {
		loop {
			match rx.recv_timeout(Duration::from_millis(100)) {
				Ok(WorkerMessage::LogEntry(entry)) => {
					if let Some(q) = &logquery {
						if let Ok(true) = check_expr(&q.root, &entry) {
							entry.serialize(&mut buffer).unwrap_or_default();
						}
					}
					if buffer.size() > builder.max_buffer_size {
						break;
					}
				},
				Ok(WorkerMessage::Flush(ack)) => {
					let _ = ack.send(());
				},
				Ok(WorkerMessage::FlushClose(ack)) => {
					let _ = ack.send(());
					break;
				},
				Err(mpsc::RecvTimeoutError::Timeout) => break,
				Err(mpsc::RecvTimeoutError::Disconnected) => break 'main,
			};
		}

		let mut client_broken = false;
		match &mut client {
			Some(c) => {
				while let Ok(msg) = c.read() {
					match msg {
						Message::Text(utf8_bytes) => {
							println!("received text: {}", utf8_bytes);
							match serde_json::from_str::<PuppylogEvent>(&utf8_bytes) {
								Ok(event) => {
									match event {
										PuppylogEvent::QueryChanged { query } => {
											if let Ok(q) = parse_log_query(&query) {
												logquery = Some(q);
											}
										}
									}
								},
								Err(e) => {
									eprintln!("Failed to parse log entry: {}", e);
									continue;
								}
							}
						},
						Message::Binary(bytes) => {},
						Message::Ping(bytes) => {},
						Message::Pong(bytes) => {},
						Message::Close(close_frame) => {},
						Message::Frame(frame) => {},
					}
				}

				while let Some(chunk) = buffer.next_chunk() {
					let len = chunk.len();
					match c.send(Message::Binary(chunk)) {
						Ok(_) => { buffer.truncate(len); },
						Err(e) => {
							eprintln!("Failed to send message: {}", e);
							client_broken = true;
						}
					};
				};
			},
			None => {
				if connect_timer.elapsed().as_secs() < 1 {
					continue;
				}
				connect_timer = Instant::now();

				let https = match &url.scheme() {
					Some(scheme) => match scheme.as_str() {
						"ws" => false,
						"wss" => true,
						_ => {
							eprintln!("unsupported scheme: {}", scheme);
							continue;
						}
					}
					None => {
						eprintln!("No scheme in url");
						continue;
					}
				};

				let port = match url.port() {
					Some(p) => p.as_u16(),
					None => if https { 443 } else { 80 }
				};
				let host = url.host().ok_or(PuppyLogError::new("no host in url")).unwrap();
				let host = format!("{}:{}", host, port);
				let socket = match TcpStream::connect(host) {
					Ok(socket) => socket,
					Err(e) => {
						eprintln!("Failed to connect: {}", e);
						continue;
					}
				};
				socket.set_read_timeout(Some(Duration::from_millis(500))).unwrap();
				println!("tcp socket connected");
				let stream = if https {
					let connector = match  TlsConnector::builder().build() {
						Ok(c) => c,
						Err(_) => {
							eprintln!("Failed to create Tlsconnector");
							continue;
						}
					};
					let stream = match connector.connect(&url.host().unwrap(), socket) {
						Ok(s) => s,
						Err(_) => {
							eprintln!("Failed to connect");
							continue;
						},
					};
					println!("tls connected");
					MaybeTlsStream::NativeTls(stream)
				}
				else { MaybeTlsStream::Plain(socket) };
				println!("creating ws client addr: {}", url);
				let req = ClientRequestBuilder::new(url.clone())
					.with_header("Authorization", builder.authorization.clone().unwrap_or_default());
				let c = match client_with_config(req, stream, None) {
					Ok((c, _)) => c,
					Err(e) => {
						eprintln!("Failed to connect: {}", e);
						continue;
					}
				};
				println!("connected");
				client = Some(c);
			},
		};

		if client_broken {
			client = None;
		}
	}

	println!("worker done");
}

#[derive(Clone)]
pub struct PuppylogClient {
	sender: mpsc::Sender<WorkerMessage>,
	level: Level,
	stdout: bool,
	props: Vec<Prop>,
}

impl PuppylogClient {
	fn new(builder: PuppylogBuilder) -> Self {
		let props = builder.props.clone();
		let level = builder.level_filter;
		let stdout = builder.log_stdout;
		let (sender, rx) = mpsc::channel();
		thread::spawn(move || { worker(rx, builder) });
		PuppylogClient {
			sender,
			level,
			stdout,
			props,
		}
	}

	pub fn send_logentry(&self, entry: LogEntry) {
		if let Err(err) = self.sender.send(WorkerMessage::LogEntry(entry)) {
			eprintln!("Failed to send log entry: {}", err);
		}
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
			if self.stdout {
				println!(
					"{} [{}] {}",
					match record.level() {
						Level::Error => "\x1b[31mERROR\x1b[0m",
						Level::Warn => "\x1b[33mWARN\x1b[0m",
						Level::Info => "\x1b[32mINFO\x1b[0m",
						Level::Debug => "\x1b[34mDEBUG\x1b[0m",
						Level::Trace => "\x1b[37mTRACE\x1b[0m",
					},
					record.target(),
					record.args()
				);
			}
			let level = match record.level() {
				Level::Error => LogLevel::Error,
				Level::Warn => LogLevel::Warn,
				Level::Info => LogLevel::Info,
				Level::Debug => LogLevel::Debug,
				Level::Trace => LogLevel::Debug,
			};
			let mut props = self.props.clone();
			props.push(Prop {
				key: "module".to_string(),
				value: record.target().to_string(),
			});
			let entry = LogEntry {
				version: 1,
				level,
				timestamp: Utc::now(),
				random: 0,
				props,
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

impl From<io::Error> for PuppyLogError {
	fn from(error: io::Error) -> Self {
		PuppyLogError::new(&error.to_string())
	}
}

pub struct PuppylogBuilder {
	max_log_file_size: u64,
	max_log_files: u32,
	min_buffer_size: u64,
	max_buffer_size: u64,
	max_batch_size: u64,
	log_folder: Option<PathBuf>,
	log_server: Option<Uri>,
	authorization: Option<String>,
	log_stdout: bool,
	level_filter: Level,
	props: Vec<Prop>,
	internal_logging: bool,
}

impl PuppylogBuilder {
	pub fn new() -> Self {
		PuppylogBuilder {
			max_log_file_size: 1024 * 1024 * 10,
			max_log_files: 10,
			min_buffer_size: 1024,
			max_buffer_size: 1024 * 1024,
			max_batch_size: 1024 * 1024,
			log_folder: None,
			log_server: None,
			log_stdout: true,
			authorization: None,
			level_filter: Level::Info,
			props: Vec::new(),
			internal_logging: false,
		}
	}

	pub fn folder<P: AsRef<Path>>(mut self, path: P) -> Self {
		let path: &Path = path.as_ref();
		self.log_folder = Some(path.to_path_buf());
		self
	}

	pub fn server(mut self, url: &str) -> Result<Self, PuppyLogError> {
		self.log_server = Some(Uri::from_str(url).map_err(|e| PuppyLogError::new(&e.to_string()))?);
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

	pub fn prop(mut self, key: &str, value: &str) -> Self {
		self.props.push(Prop {
			key: key.to_string(),
			value: value.to_string(),
		});
		self
	}

	/// Enable internal logging for the logger itself. This is useful for debugging the logger.
	pub fn internal_logging(mut self) -> Self {
		self.internal_logging = true;
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