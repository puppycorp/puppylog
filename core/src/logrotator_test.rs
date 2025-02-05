use std::path::Path;

use puppylog::LogRotator;

fn main() {
	let mut logrotator = LogRotator::new(Path::new("./workdir/logs").to_path_buf(), 40, 3000);

	for i in 0..10_000 {
		logrotator.write(format!("[{}] Hello, world!\n", i).as_bytes());
	}
}