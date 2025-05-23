use crate::LogEntry;

pub struct Logfile {}

impl Logfile {
	pub fn new() -> Self {
		Logfile {}
	}

	pub fn write_log_entry(&self, entry: LogEntry) {}
}
