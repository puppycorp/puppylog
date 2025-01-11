use chrono::{DateTime, Utc};
use puppylog::{LogEntry, LogLevel};
use serde::Deserialize;
use tokio::sync::mpsc::{self, Sender};

use crate::{subscriber::Subscriber, worker::Worker};

pub struct Logfile {
	pub log_entries: Vec<LogEntry>,
}

#[derive(Debug)]
pub struct Context {
	pub subscriber: Subscriber,
	pub publisher: Sender<LogEntry>,
}

impl Context {
	pub fn new() -> Self {
		let (subtx, subrx) = mpsc::channel(100);
		let (pubtx, pubrx) = mpsc::channel(100);
		tokio::spawn(async move {
			Worker::new(subrx, pubrx).run().await;
		});

		Context {
			subscriber: Subscriber::new(subtx),
			publisher: pubtx,
		}
	}
}

#[derive(Debug, Clone, Deserialize)]
pub struct LogsQuery {
	pub start: Option<DateTime<Utc>>,
	pub end: Option<DateTime<Utc>>,
	pub level: Option<LogLevel>,
	pub props: Vec<(String, String)>,
	pub search: Option<String>,
}

impl LogsQuery {
	pub fn matches(&self, entry: &LogEntry) -> bool {
		if let Some(start) = &self.start {
			if entry.timestamp < *start {
				return false;
			}
		}
		if let Some(end) = &self.end {
			if entry.timestamp > *end {
				return false;
			}
		}
		if let Some(level) = &self.level {
			if entry.level != *level {
				return false;
			}
		}
		for (key, value) in &self.props {
			if entry.props.iter().find(|(k, v)| k == key && v == value).is_none() {
				return false;
			}
		}
		true
	}
}

pub struct SubscribeReq {
	pub res_tx: mpsc::Sender<LogEntry>,
	pub query: LogsQuery
}