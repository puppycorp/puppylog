use chrono::Utc;
use criterion::{black_box, criterion_group, criterion_main, Criterion};
use puppylog::{check_expr, parse_log_query, LogEntry, LogLevel, Prop};

fn generate_logs(count: usize) -> Vec<LogEntry> {
	(0..count)
		.map(|i| LogEntry {
			random: i as u32,
			timestamp: Utc::now() + chrono::Duration::seconds(i as i64),
			level: if i % 2 == 0 {
				LogLevel::Info
			} else {
				LogLevel::Error
			},
			msg: format!("log {}", i),
			props: vec![Prop {
				key: "device".into(),
				value: (i % 10).to_string(),
			}],
			..Default::default()
		})
		.collect()
}

fn bench_log_search(c: &mut Criterion) {
	let logs = generate_logs(1_000);
	let query = parse_log_query("level = info and device = '5'").unwrap();
	c.bench_function("search_logs", |b| {
		b.iter(|| {
			let mut hits = 0usize;
			for entry in &logs {
				if check_expr(&query.root, entry).unwrap() {
					hits += 1;
				}
			}
			black_box(hits);
		})
	});
}

criterion_group!(benches, bench_log_search);
criterion_main!(benches);
