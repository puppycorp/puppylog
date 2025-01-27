use std::{thread::sleep, time::Duration};

use log::Log;
use puppylog::LoggerBuilder;

fn main() {
    let logger = LoggerBuilder::new()
		.stdout()
		.server("http://localhost:3337/api/logs")
		.build()
		.unwrap();

	log::info!("Hello, world!");
	log::warn!("Warning!");
	log::error!("Error!");
	log::debug!("Debug!");
	log::trace!("Trace!");
	logger.close();
}
