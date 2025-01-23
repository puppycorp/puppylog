use puppylog::LoggerBuilder;

fn main() {
    LoggerBuilder::new()
		.with_stdout()
		.with_http("http://localhost:8080")
		.build();
}
