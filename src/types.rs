use chrono::{DateTime, Utc};
use puppylog::{LogEntry, LogLevel};
use serde::Deserialize;
use tokio::sync::mpsc::{self, Sender};

use crate::{log_query::QueryAst, subscriber::Subscriber, worker::Worker};

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

pub struct SubscribeReq {
	pub res_tx: mpsc::Sender<LogEntry>,
	pub query: QueryAst
}