pub fn log_path() -> std::path::PathBuf {
	match std::env::var("LOG_PATH") {
		Ok(val) => std::path::Path::new(&val).to_owned(),
		Err(_) => std::path::Path::new("./logs").to_owned(),
	}
}

pub fn db_path() -> std::path::PathBuf {
	match std::env::var("DB_PATH") {
		Ok(val) => std::path::Path::new(&val).to_owned(),
		Err(_) => std::path::Path::new("./puppylog.db").to_owned(),
	}
}

pub fn settings_path() -> std::path::PathBuf {
	match std::env::var("SETTINGS_PATH") {
		Ok(val) => std::path::Path::new(&val).to_owned(),
		Err(_) => std::path::Path::new("./settings.json").to_owned(),
	}
}

pub fn upload_path() -> std::path::PathBuf {
	match std::env::var("UPLOAD_PATH") {
		Ok(val) => std::path::Path::new(&val).to_owned(),
		Err(_) => std::path::Path::new("./uploads").to_owned(),
	}
}
