use puppylog::LogEntry;
use tokio::sync::mpsc;
use crate::types::LogsQuery;
use crate::types::SubscribeReq;

#[derive(Debug)]
pub struct Subscriber {
	tx: mpsc::Sender<SubscribeReq>,
}

impl Subscriber {
	pub fn new(tx: mpsc::Sender<SubscribeReq>) -> Self {
		Subscriber {
			tx
		}
	}

	pub async fn subscribe(&self, query: LogsQuery) -> mpsc::Receiver<LogEntry> {
		let (res_tx, res_rx) = mpsc::channel(100);
		self.tx.send(SubscribeReq {
			res_tx,
			query,
		}).await;
		res_rx
	}
}