use puppylog::LogEntry;
use puppylog::LogLevel;
use puppylog::PuppylogEvent;
use puppylog::QueryAst;
use serde::Serialize;
use tokio::sync::broadcast;
use tokio::sync::mpsc;
use tokio::sync::mpsc::Sender;
use crate::settings::Settings;
use crate::storage::LogEntrySaver;
use crate::subscriber::Subscriber;
use crate::worker::Worker;

pub struct Logfile {
	pub log_entries: Vec<LogEntry>,
}

#[derive(Debug)]
pub struct Context {
	pub subscriber: Subscriber,
	pub publisher: Sender<LogEntry>,
	pub logentry_saver: LogEntrySaver,
	pub settings: Settings,
	pub event_tx: broadcast::Sender<PuppylogEvent>
}

impl Context {
	pub fn new() -> Self {
		let (subtx, subrx) = mpsc::channel(100);
		let (pubtx, pubrx) = mpsc::channel(100);
		tokio::spawn(async move {
			Worker::new(subrx, pubrx).run().await;
		});
		let (event_tx, _) = broadcast::channel(100);

		Context {
			subscriber: Subscriber::new(subtx),
			publisher: pubtx,
			logentry_saver: LogEntrySaver::new(),
			settings: Settings::load().unwrap(),
			event_tx
		}
	}
}

pub struct SubscribeReq {
	pub res_tx: mpsc::Sender<LogEntry>,
	pub query: QueryAst
}

#[derive(Serialize, Default)]
pub struct DeviceStatus {
	pub query: Option<String>,
	pub level: Option<LogLevel>,
	pub send_logs: bool
}