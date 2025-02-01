use std::sync::Arc;

use serde::Deserialize;
use serde::Serialize;
use std::fs::read_to_string;
use tokio::sync::Mutex;

use crate::config::settings_path;


#[derive(Debug, Default, Serialize, Deserialize)]
pub struct SettingsInner {
	pub collection_query: String,
}

impl SettingsInner {
	pub fn save(&self) -> anyhow::Result<()> {
		let text = serde_json::to_string(self)?;
		std::fs::write(settings_path(), text)?;
		Ok(())
	}
}

#[derive(Debug)]
pub struct Settings {
	inner: Arc<Mutex<SettingsInner>>,
}

impl Settings {
	pub fn load() -> anyhow::Result<Self> {
		let inner = match read_to_string(settings_path()) {
			Ok(text) => serde_json::from_str(&text)?,
			Err(_) => SettingsInner {
				collection_query: "qwert".to_string(),
				..Default::default()
			},
		};
		Ok(Self {
			inner: Arc::new(Mutex::new(inner)),
		})
	}

	pub fn new() -> Self {
		Self {
			inner: Arc::new(Mutex::new(SettingsInner::default())),
		}
	}

	pub async fn inner(&self) -> tokio::sync::MutexGuard<SettingsInner> {
		self.inner.lock().await
	}
}