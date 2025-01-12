
pub fn log_path() -> std::path::PathBuf {
    match std::env::var("LOG_PATH") {
        Ok(val) => std::path::Path::new(&val).to_owned(),
        Err(_) => std::path::Path::new("./logs").to_owned()
    }
}