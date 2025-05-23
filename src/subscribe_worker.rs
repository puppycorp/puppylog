use puppylog::LogEntry;
use puppylog::QueryAst;
use tokio::sync::mpsc;
use tokio::sync::mpsc::error::TrySendError;

pub struct SubscribeReq {
	pub res_tx: mpsc::Sender<LogEntry>,
	pub query: QueryAst,
}

#[derive(Debug)]
pub struct Subscriber {
	tx: mpsc::Sender<SubscribeReq>,
}

impl Subscriber {
	pub fn new(tx: mpsc::Sender<SubscribeReq>) -> Self {
		Self { tx }
	}
	pub async fn subscribe(&self, query: QueryAst) -> mpsc::Receiver<LogEntry> {
		let (res_tx, res_rx) = mpsc::channel(100);
		let _ = self.tx.send(SubscribeReq { res_tx, query }).await;
		res_rx
	}
}

struct SubscriberInfo {
	res_tx: mpsc::Sender<LogEntry>,
	query: QueryAst,
}

pub struct Worker {
	subrx: mpsc::Receiver<SubscribeReq>,
	pubrx: mpsc::Receiver<LogEntry>,
	subs: Vec<SubscriberInfo>,
}

impl Worker {
	pub fn new(subrx: mpsc::Receiver<SubscribeReq>, pubrx: mpsc::Receiver<LogEntry>) -> Self {
		Worker {
			subrx,
			pubrx,
			subs: Vec::new(),
		}
	}
	async fn handle_entry(&mut self, entry: LogEntry) {
		let mut i = self.subs.len();
		while i > 0 {
			i -= 1;
			if let Ok(m) = self.subs[i].query.matches(&entry) {
				if m {
					if self.subs[i].res_tx.is_closed() {
						self.subs.remove(i);
						continue;
					}
					match self.subs[i].res_tx.try_send(entry.clone()) {
						Ok(_) => {}
						Err(TrySendError::Full(_)) => {}
						Err(TrySendError::Closed(_)) => {
							self.subs.remove(i);
						}
					}
				}
			}
		}
	}
	pub async fn run(mut self) {
		loop {
			tokio::select! {
				req = self.subrx.recv() => {
					if let Some(req) = req {
						self.subs.push(SubscriberInfo { res_tx: req.res_tx, query: req.query });
					} else { break; }
				}
				entry = self.pubrx.recv() => {
					if let Some(entry) = entry { self.handle_entry(entry).await; } else { break; }
				}
			}
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use puppylog::{Condition, Expr, Value};
	use tokio::sync::mpsc;
	use tokio::time::{sleep, timeout, Duration};

	#[tokio::test]
	async fn test_matching_subscription() {
		let (subtx, subrx) = mpsc::channel(10);
		let (pubtx, pubrx) = mpsc::channel(10);
		let worker = Worker::new(subrx, pubrx);
		let worker_handle = tokio::spawn(worker.run());
		{
			let (res_tx, mut res_rx) = mpsc::channel(10);
			let query = QueryAst {
				root: Expr::Condition(Condition {
					left: Box::new(Expr::Value(Value::String("msg".to_string()))),
					operator: puppylog::Operator::Equal,
					right: Box::new(Expr::Value(Value::String(
						"This is a test message".to_string(),
					))),
				}),
				..Default::default()
			};
			let req = SubscribeReq { res_tx, query };
			subtx.send(req).await.unwrap();
			sleep(Duration::from_millis(100)).await;
			let entry = LogEntry {
				msg: "This is a test message".to_string(),
				..Default::default()
			};
			pubtx.send(entry.clone()).await.unwrap();
			let received = res_rx.recv().await;
			assert_eq!(received, Some(entry));
			drop(subtx);
			drop(pubtx);
		}
		timeout(Duration::from_millis(500), worker_handle)
			.await
			.unwrap()
			.unwrap();
	}

	#[tokio::test]
	async fn test_non_matching_subscription() {
		let (subtx, subrx) = mpsc::channel(10);
		let (pubtx, pubrx) = mpsc::channel(10);
		let worker = Worker::new(subrx, pubrx);
		let worker_handle = tokio::spawn(worker.run());
		{
			let (res_tx, mut res_rx) = mpsc::channel(10);
			let query = QueryAst {
				root: Expr::Condition(Condition {
					left: Box::new(Expr::Value(Value::String("msg".to_string()))),
					operator: puppylog::Operator::Equal,
					right: Box::new(Expr::Value(Value::String("test".to_string()))),
				}),
				..Default::default()
			};
			let req = SubscribeReq { res_tx, query };
			subtx.send(req).await.unwrap();
			sleep(Duration::from_millis(100)).await;
			let entry = LogEntry {
				msg: "This message will not match".to_string(),
				..Default::default()
			};
			pubtx.send(entry).await.unwrap();
			let result = timeout(Duration::from_millis(100), res_rx.recv()).await;
			assert!(result.is_err());
			drop(subtx);
			drop(pubtx);
		}
		worker_handle.await.unwrap();
	}
}
