use serde_json::json;

pub async fn notify(text: &str) {
	let webhook = match std::env::var("SLACK_WEBHOOK") {
		Ok(url) => url,
		Err(_) => return,
	};
	let client = reqwest::Client::new();
	let _ = client
		.post(&webhook)
		.json(&json!({ "text": text }))
		.send()
		.await;
}
