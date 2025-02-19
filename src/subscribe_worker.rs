use puppylog::LogEntry;
use puppylog::QueryAst;
use tokio::sync::mpsc;
use tokio::sync::mpsc::error::TrySendError;

pub struct SubscribeReq {
	pub res_tx: mpsc::Sender<LogEntry>,
	pub query: QueryAst
}

#[derive(Debug)]
pub struct Subscriber {
	tx: mpsc::Sender<SubscribeReq>,
}

impl Subscriber {
	pub fn new(tx: mpsc::Sender<SubscribeReq>) -> Self {
		Self {
			tx
		}
	}

	pub async fn subscribe(&self, query: QueryAst) -> mpsc::Receiver<LogEntry> {
		let (res_tx, res_rx) = mpsc::channel(100);
		self.tx.send(SubscribeReq {
			res_tx,
			query,
		}).await;
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
			let m = match self.subs[i].query.matches(&entry) {
				Ok(v) => v,
				Err(_) => continue,
			};
			if !m { continue; }
			if let Err(e) = self.subs[i].res_tx.try_send(entry.clone()) {
				match e {
					TrySendError::Full(_) => continue,
					TrySendError::Closed(_) => {
						log::info!("subscriber closed");
					}
				}
				self.subs.remove(i);
			}
		}
	}

	pub async fn run(mut self) {
		loop {
			tokio::select! {
				req = self.subrx.recv() => {
					match req {
						Some(req) => {
							log::info!("subscribe {:?}", req.query);
							self.subs.push(SubscriberInfo {
								res_tx: req.res_tx,
								query: req.query,
							});
						}
						None => break,
					}
				}
				entry = self.pubrx.recv() => {
					match entry {
						Some(entry) => self.handle_entry(entry).await,
						None => break,
					}
				}
			}
		}
	}
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::sync::mpsc;
    use tokio::time::{timeout, Duration};
	use puppylog::{Condition, Expr, Value};

    #[tokio::test]
    async fn test_matching_subscription() {
        let (subtx, subrx) = mpsc::channel(10);
        let (pubtx, pubrx) = mpsc::channel(10);
        let worker = Worker::new(subrx, pubrx);
        let worker_handle = tokio::spawn(worker.run());
        let (res_tx, mut res_rx) = mpsc::channel(10);
        let query = QueryAst { 
			root: Expr::Condition(Condition {
				left: Box::new(Expr::Value(Value::String("msg".to_string()))),
				operator: puppylog::Operator::Equal,
				right: Box::new(Expr::Value(Value::String("This is a test message".to_string()))),
			}),
			..Default::default()
		};
        let req = SubscribeReq { res_tx, query };
        subtx.send(req).await.unwrap();
		let entry = LogEntry {
			msg: "This is a test message".to_string(),
			..Default::default()
		};
        pubtx.send(entry.clone()).await.unwrap();
        let received = res_rx.recv().await;
        assert_eq!(received, Some(entry));
        drop(subtx);
        drop(pubtx);
        worker_handle.await.unwrap();
    }

    #[tokio::test]
    async fn test_non_matching_subscription() {
        let (subtx, subrx) = mpsc::channel(10);
        let (pubtx, pubrx) = mpsc::channel(10);
        let worker = Worker::new(subrx, pubrx);
        let worker_handle = tokio::spawn(worker.run());
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
        let entry = LogEntry {
			msg: "This message will not match".to_string(),
			..Default::default()
		};
        pubtx.send(entry).await.unwrap();
        let result = timeout(Duration::from_millis(100), res_rx.recv()).await;
        assert!(result.is_err(), "Expected timeout since subscription should not match");
        drop(subtx);
        drop(pubtx);
        worker_handle.await.unwrap();
    }

    #[tokio::test]
    async fn test_closed_subscription() {
        let (subtx, subrx) = mpsc::channel(10);
        let (pubtx, pubrx) = mpsc::channel(10);
        let worker = Worker::new(subrx, pubrx);
        let worker_handle = tokio::spawn(worker.run());
        let (res_tx, res_rx) = mpsc::channel(10);
        let query = QueryAst {
			root: Expr::Empty,
			..Default::default()
		};
        let req = SubscribeReq { res_tx, query };
        subtx.send(req).await.unwrap();
        drop(res_rx);
		let entry = LogEntry {
			msg: "Test message for closed subscription".to_string(),
			..Default::default()
		};
        pubtx.send(entry).await.unwrap();
        drop(subtx);
        drop(pubtx);
        worker_handle.await.unwrap();
    }
}