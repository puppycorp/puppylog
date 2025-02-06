use std::collections::VecDeque;
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
use crate::parse_log_query;
use crate::LogEntry;
use crate::LogLevel;
use crate::Prop;
use crate::PuppylogEvent;
use crate::QueryAst;

struct Settings {

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
	let mut buffer = LogBuffer::new(&builder);
	if let Some(path) = &builder.log_folder {
		buffer.set_folder_path(&builder);
	}
	let mut send_timer = Instant::now();
	let mut serialize_buffer = Vec::with_capacity(builder.max_buffer_size);
	let mut queue = VecDeque::new();

	'main: loop {
		loop {
			match rx.recv_timeout(Duration::from_millis(100)) {
				Ok(WorkerMessage::LogEntry(entry)) => {
					// if let Some(q) = &logquery {
					// 	if let Ok(true) = check_expr(&q.root, &entry) {
					// 		entry.serialize(&mut buffer).unwrap_or_default();
					// 	}
					// }
					entry.serialize(&mut serialize_buffer).unwrap_or_default();
					if serialize_buffer.len() > builder.max_buffer_size {
						println!("max serialize buffer size reached");
						break;
					}
					if send_timer.elapsed() > builder.send_interval {
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
				Err(mpsc::RecvTimeoutError::Timeout) => {
					println!("timeout");
					break;
				},
				Err(mpsc::RecvTimeoutError::Disconnected) => {
					eprintln!("channel disconnected");
					break 'main
				}
			};
		}

		if serialize_buffer.len() > 10 {
			queue.push_back(Bytes::copy_from_slice(&serialize_buffer));
			serialize_buffer.clear();
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
						Message::Close(close_frame) => {
							println!("received close frame: {:?}", close_frame);
							client_broken = true;
							break;
						},
						msg => {
							println!("unhandled msg: {:?}", msg);
						}
					}
				}

				send_timer = Instant::now();
				if let Some(batch) = queue.pop_front() {
					match c.send(Message::Binary(batch)) {
						Ok(_) => { serialize_buffer.clear(); },
						Err(e) => {
							eprintln!("Failed to send message: {}", e);
							client_broken = true;
						}
					};
				}
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

impl From<std::io::Error> for PuppyLogError {
	fn from(error: std::io::Error) -> Self {
		PuppyLogError::new(&error.to_string())
	}
}

pub struct PuppylogBuilder {
	pub chunk_size: usize,
	pub max_file_count: usize,
	pub max_file_size: usize,
	pub min_buffer_size: u64,
	pub max_buffer_size: usize,
	pub max_batch_size: u64,
	pub send_interval: Duration,
	pub log_folder: Option<PathBuf>,
	pub log_server: Option<Uri>,
	pub authorization: Option<String>,
	pub log_stdout: bool,
	pub level_filter: Level,
	pub props: Vec<Prop>,
	pub internal_logging: bool,
}

impl PuppylogBuilder {
	pub fn new() -> Self {
		PuppylogBuilder {
			chunk_size: 4096,
			max_file_count: 5,
			max_file_size: 1024 * 1024 * 10,
			min_buffer_size: 1024,
			max_buffer_size: 1024 * 1024,
			max_batch_size: 1024 * 1024,
			send_interval: Duration::from_secs(5),
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