use puppylog::LogEntry;
use tokio::sync::mpsc;
use crate::log_query::QueryAst;
use crate::types::SubscribeReq;

struct Subscriber {
	res_tx: mpsc::Sender<LogEntry>,
	query: QueryAst,
}

pub struct Worker {
	subrx: mpsc::Receiver<SubscribeReq>,
	pubrx: mpsc::Receiver<LogEntry>,
	subs: Vec<Subscriber>,
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
		//log::info!("handle_entry {:?}", entry);
		let mut i = self.subs.len();
		while i > 0 {
			i -= 1;
			let m = match self.subs[i].query.matches(&entry) {
				Ok(v) => v,
				Err(_) => continue,
			};
			if !m { continue; }
			if let Err(_) = self.subs[i].res_tx.send(entry.clone()).await {
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
							self.subs.push(Subscriber {
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

pub struct WorkerManager {

}

impl WorkerManager {
	pub fn new() -> Self {
		WorkerManager {}
	}
}