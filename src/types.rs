use chrono::DateTime;
use chrono::Utc;
use serde::Deserialize;

#[derive(Deserialize, Debug)]
#[serde(rename_all = "lowercase")]
pub enum SortDir {
	Asc,
	Desc,
}

#[derive(Deserialize, Debug, Default)]
pub struct GetSegmentsQuery {
	pub start: Option<DateTime<Utc>>,
	pub end: Option<DateTime<Utc>>,
	pub device_ids: Option<Vec<String>>,
	pub count: Option<usize>,
	pub sort: Option<SortDir>,
}

#[derive(Deserialize, Debug, Default)]
pub struct GetDevicesQuery {
	pub search: Option<String>,
	pub page: Option<usize>,
	pub count: Option<usize>,
}
