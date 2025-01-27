use std::fs::{create_dir_all, File, OpenOptions};
use std::io::{Cursor, Write};
use std::sync::mpsc::{self, Receiver};
use std::path::{Path, PathBuf};
use std::thread;
use log::{Record, Level, Metadata, SetLoggerError};
use chrono::{DateTime, Local, Utc};

use crate::{LogEntry, LogLevel};

fn worker(rx: Receiver<LogEntry>, builder: LoggerBuilder) {
	let mut file: Option<File> = None;
	fn get_logfile(timestamp: &DateTime<Utc>) -> &mut File {
		todo!()
	}
    fn http_client() -> Option<impl Write> {
        Some(Vec::new())
    }
	let mut buffer = Vec::new();
	let mut buffer = Cursor::new(buffer);

	for entry in rx {
		println!("{:?}", entry);
		if builder.log_folder.is_some() {
			let file = get_logfile(&entry.timestamp);
			entry.serialize(file);
		}
		if builder.log_server.is_some() {
			// send to server
			entry.serialize(&mut buffer);
		}

		if buffer.position() > builder.min_buffer_size {
			// send buffer to server
			buffer.set_position(0);
			let mut client = http_client().unwrap();
			std::io::copy(&mut buffer, &mut client).unwrap();
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

        // thread::spawn(move || match output {
        //     LogOutput::File => {
        //         fn get_current_date() -> String {
        //             Local::now().format("%Y-%m-%d").to_string()
        //         }

        //         fn get_current_timestamp() -> String {
        //             Local::now().format("%Y-%m-%d %H:%M:%S").to_string()
        //         }

        //         let base_path = if Path::new("/var/log").exists() {
        //             "/var/log/lockerapi"
        //         } else {
        //             "logs"
        //         };

        //         // Ensure the base directory exists
        //         create_dir_all(base_path).expect("Failed to create log directory");

        //         let mut current_date = get_current_date();
        //         let mut filename = format!("{}/lockerapi_{}.log", base_path, current_date);
        //         let mut file = OpenOptions::new()
        //             .create(true)
        //             .append(true)
        //             .open(&filename)
        //             .expect("Failed to open log file");

        //         for message in receiver {
        //             let message_date = get_current_date();
        //             if message_date != current_date {
        //                 // Date has changed, rotate the log file
        //                 current_date = message_date.clone();
        //                 filename = format!("{}/app_log_{}.log", base_path, current_date);
        //                 file = OpenOptions::new()
        //                     .create(true)
        //                     .append(true)
        //                     .open(&filename)
        //                     .expect("Failed to open log file");
        //             }
        //             let timestamped_message = format!("{} - {}", get_current_timestamp(), message);
        //             writeln!(file, "{}", timestamped_message).expect("Failed to write to log file");
        //         }
        //     }
        //     LogOutput::Stdout => {
        //         for message in receiver {
        //             println!("{}", message);
        //         }
        //     }
        //     LogOutput::Http(url) => {
        //         panic!("HTTP logging not implemented");
        //     }
        // });

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
	max_buffer_size: u32,
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

    pub fn with_folder<P: AsRef<Path>>(mut self, path: P) -> Self {
		let path: &Path = path.as_ref();
        self.log_folder = Some(path.to_path_buf());
        self
    }

	pub fn with_server(mut self, url: &str) -> Self {
		self.log_server = Some(url.to_string());
		self
	}

    pub fn with_level(mut self, level: Level) -> Self {
        self.level_filter = level;
        self
    }

	pub fn stdout(mut self, value: bool) -> Self {
		self.log_stdout = value;
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