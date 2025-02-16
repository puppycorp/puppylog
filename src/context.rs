use puppylog::LogEntry;
use puppylog::LogLevel;
use puppylog::PuppylogEvent;
use puppylog::QueryAst;
use serde::Serialize;
use tokio::sync::broadcast;
use tokio::sync::mpsc;
use tokio::sync::mpsc::Sender;
use tokio::sync::Mutex;
use crate::config::log_path;
use crate::db::open_db;
use crate::db::DB;
use crate::segment::save_segment;
use crate::segment::LogSegmentManager;
use crate::settings::Settings;
use crate::subscriber::Subscriber;
use crate::wal::load_logs_from_wal;
use crate::wal::Wal;
use crate::worker::Worker;

#[derive(Debug)]
pub struct Context {
	pub subscriber: Subscriber,
	pub publisher: Sender<LogEntry>,
	pub settings: Settings,
	pub event_tx: broadcast::Sender<PuppylogEvent>,
	pub db: DB,
	pub logsegments: Mutex<LogSegmentManager>,
	pub wal: Wal
}

impl Context {
	pub fn new() -> Self {
		let (subtx, subrx) = mpsc::channel(100);
		let (pubtx, pubrx) = mpsc::channel(100);
		tokio::spawn(async move {
			Worker::new(subrx, pubrx).run().await;
		});
		let (event_tx, _) = broadcast::channel(100);
		let wal = Wal::new();
		let logs = load_logs_from_wal();
		Context {
			subscriber: Subscriber::new(subtx),
			publisher: pubtx,
			settings: Settings::load().unwrap(),
			event_tx,
			db: DB::new(open_db()),
			logsegments: Mutex::new(LogSegmentManager::with_logs(logs)),
			wal
		}
	}

	pub async fn save_logs(&self, logs: &[LogEntry]) {
		let mut manager = self.logsegments.lock().await;
		for entry in logs {
			self.wal.write(entry.clone());
			manager.current.add_log_entry(entry.clone());
			if let Err(e) = self.publisher.send(entry.clone()).await {
				log::error!("Failed to publish log entry: {}", e);
			}
		}
		if manager.current.buffer.len() > 50_000 {
			log::info!("flushing segment to disk");
			save_segment(&manager.current, &log_path());
			self.wal.clear();
			manager.rotate();
		}
	}
}

pub struct SubscribeReq {
	pub res_tx: mpsc::Sender<LogEntry>,
	pub query: QueryAst
}

#[derive(Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct DeviceStatus {
	pub query: Option<String>,
	pub level: Option<LogLevel>,
	pub send_logs: bool
}