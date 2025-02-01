use puppylog::LogEntry;
use tokio::sync::mpsc;
use tokio::sync::mpsc::Sender;
use crate::log_query::QueryAst;
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
	pub logentry_saver: LogEntrySaver
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
			logentry_saver: LogEntrySaver::new()
		}
	}
}

pub struct SubscribeReq {
	pub res_tx: mpsc::Sender<LogEntry>,
	pub query: QueryAst
}