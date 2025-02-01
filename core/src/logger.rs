use std::io::{self, Cursor, Read, Write};
use std::net::TcpStream;
use std::sync::mpsc::{self, Receiver};
use std::path::{Path, PathBuf};
use std::thread;
use std::time::{Duration, Instant};
use bytes::Bytes;
use log::{Record, Level, Metadata, SetLoggerError};
use chrono::{DateTime, Local, Utc};
use native_tls::TlsConnector;
use tungstenite::client::client_with_config;
use tungstenite::stream::MaybeTlsStream;
use tungstenite::{connect, ClientRequestBuilder, Message, WebSocket};
use url::Url;

use crate::{check_expr, parse_log_query, query_eval, LogEntry, LogLevel, Prop, PuppylogEvent, QueryAst};

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
	let mut buffer = Cursor::new(Vec::new());
	let mut logquery: Option<QueryAst> = None;
	let mut connect_timer = Instant::now();
	'main: loop {
		loop {
			match rx.recv_timeout(Duration::from_millis(100)) {
				Ok(WorkerMessage::LogEntry(entry)) => {
					if let Some(q) = &logquery {
						if let Ok(true) = check_expr(&q.root, &entry) {
							entry.serialize(&mut buffer).unwrap_or_default();
						}
					}
					if buffer.position() > builder.max_buffer_size {
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

				if buffer.position() > 0 {
					println!("sending {} bytes", buffer.position());
					if let Err(err) = c.send(Message::Binary(Bytes::from(buffer.get_ref().to_vec()))) {
						eprintln!("Failed to send message: {}", err);
						client = None;
					}
					buffer.get_mut().clear();
					buffer.set_position(0);
				}
			},
			None => {
				if connect_timer.elapsed().as_secs() < 1 {
					continue;
				}
				connect_timer = Instant::now();

				let port = match url.port() {
					Some(p) => p,
					None => if url.scheme() == "https" { 443 } else { 80 },
				};
				let host = url.host_str().ok_or(PuppyLogError::new("no host in url")).unwrap();
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
				let stream = if url.scheme() == "https" {
					let connector = match  TlsConnector::builder().build() {
						Ok(c) => c,
						Err(_) => {
							eprintln!("Failed to create Tlsconnector");
							continue;
						}
					};
					let stream = match connector.connect(&url.host_str().unwrap(), socket) {
						Ok(s) => s,
						Err(_) => {
							eprintln!("Failed to connect");
							continue;
						},
					};
					MaybeTlsStream::NativeTls(stream)
				} else {
					MaybeTlsStream::Plain(socket)
				};
				let req = ClientRequestBuilder::new(url.to_string().parse().unwrap())
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
			let entry = LogEntry {
				version: 1,
				level,
				timestamp: Utc::now(),
				random: 0,
				props: self.props.clone(),
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

pub struct PuppylogBuilder {
	max_log_file_size: u64,
	max_log_files: u32,
	min_buffer_size: u64,
	max_buffer_size: u64,
	log_folder: Option<PathBuf>,
	log_server: Option<Url>,
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
		self.log_server = Some(Url::parse(url)?);
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