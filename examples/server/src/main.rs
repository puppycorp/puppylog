use std::{thread::sleep, time::Duration};
use puppylog::PuppylogBuilder;

fn main() {
    let logger = PuppylogBuilder::new()
		.stdout()
		.server("http://localhost:3337/api/logs").unwrap()
		.prop("app", "pupyapp")
		.build()
		.unwrap();

	log::info!("Hello, world!");
	log::warn!("Warning!");
	log::error!("Error!");
	log::debug!("Debug!");
	log::trace!("Trace!");

	for i in 0..10 {
		log::info!("Hello, world! {}", i);
		sleep(Duration::from_secs(1));
	}
	logger.close();
}
