use axum::extract::DefaultBodyLimit;
use axum::routing::{delete, get, post};
use axum::Router;
use config::log_path;
use context::Context;
use log::LevelFilter;
use simple_logger::SimpleLogger;
use std::sync::Arc;
use tower_http::compression::CompressionLayer;
use tower_http::cors::{AllowMethods, Any, CorsLayer};
use tower_http::decompression::RequestDecompressionLayer;

mod cache;
mod cleanup;
mod config;
mod context;
mod controllers;
mod db;
mod dev_segment_merger;
mod device_segment_compactor;
mod logline;
mod schema;
mod segment;
mod settings;
mod slack;
mod subscribe_worker;
mod types;
mod upload;
mod upload_guard;
mod utility;
mod wal;

#[tokio::main]
async fn main() {
	SimpleLogger::new()
		.with_level(LevelFilter::Info)
		.init()
		.unwrap();
	let log_path = log_path();
	log::info!("checking if log path exists: {:?}", log_path);
	if !log_path.exists() {
		log::info!("does not exist, creating it");
		std::fs::create_dir_all(&log_path).unwrap();
	}
	let ctx = Context::new(log_path).await;
	if let Ok(val) = std::env::var("UPLOAD_FLUSH_THRESHOLD") {
		if let Ok(num) = val.parse::<usize>() {
			ctx.set_upload_flush_threshold(num);
		}
	}
	let ctx = Arc::new(ctx);

	tokio::spawn(upload::process_log_uploads(ctx.clone()));
	tokio::spawn(cleanup::run_disk_space_monitor(ctx.clone()));
	tokio::spawn(dev_segment_merger::run_dev_segment_merger(ctx.clone()));
	tokio::spawn(device_segment_compactor::run_device_segment_compactor(
		ctx.clone(),
	));

	let cors = CorsLayer::new()
		.allow_origin(Any) // Allow requests from any origin
		.allow_methods(AllowMethods::any()) // Allowed HTTP methods
		.allow_headers(Any);

	// build our application with a route
	let app = Router::new()
		.route("/", get(controllers::root))
		.route("/puppylog.js", get(controllers::js))
		.route("/puppylog.css", get(controllers::css))
		.route("/favicon.ico", get(controllers::favicon))
		.route("/favicon-192x192.png", get(controllers::favicon_192x192))
		.route("/favicon-512x512.png", get(controllers::favicon_512x512))
		.route("/manifest.json", get(controllers::manifest))
		.route("/api/logs", get(controllers::get_logs))
		.layer(CompressionLayer::new())
		.layer(cors.clone())
		.route("/api/logs/stream", get(controllers::stream_logs))
		.layer(cors.clone())
		.route(
			"/api/settings/query",
			post(controllers::post_settings_query),
		)
		.with_state(ctx.clone())
		.route("/api/settings/query", get(controllers::get_settings_query))
		.with_state(ctx.clone())
		.route("/api/segments", get(controllers::get_segments))
		.with_state(ctx.clone())
		.route(
			"/api/segment/metadata",
			get(controllers::get_segment_metadata),
		)
		.with_state(ctx.clone())
		.route("/api/v1/validate_query", get(controllers::validate_query))
		.route("/api/v1/logs/stream", get(controllers::stream_logs))
		.layer(cors.clone())
		.route("/api/v1/logs/histogram", get(controllers::get_histogram))
		.layer(cors.clone())
		.route(
			"/api/v1/device/{deviceId}/status",
			get(controllers::get_device_status),
		)
		.layer(cors.clone())
		.with_state(ctx.clone())
		.route("/api/v1/device/{deviceId}", get(controllers::get_device))
		.with_state(ctx.clone())
		.route(
			"/api/v1/device/{deviceId}/logs",
			post(controllers::upload_device_logs),
		)
		.layer(cors.clone())
		.layer(DefaultBodyLimit::max(1024 * 1024 * 1000))
		.layer(RequestDecompressionLayer::new().gzip(true).zstd(true))
		.with_state(ctx.clone())
		.route(
			"/api/v1/device/{deviceId}/metadata",
			post(controllers::update_device_metadata),
		)
		.with_state(ctx.clone())
		.route(
			"/api/v1/device/{deviceId}/settings",
			post(controllers::update_device_settings),
		)
		.with_state(ctx.clone())
		.route("/api/v1/device_bulkedit", post(controllers::bulk_edit))
		.with_state(ctx.clone())
		.route("/api/v1/settings", post(controllers::post_settings_query))
		.with_state(ctx.clone())
		.route("/api/v1/settings", get(controllers::get_settings_query))
		.with_state(ctx.clone())
		.route("/api/v1/devices", get(controllers::get_devices))
		.with_state(ctx.clone())
		.route("/api/v1/segments", get(controllers::get_segments))
		.with_state(ctx.clone())
		.route("/api/v1/buckets", get(controllers::list_buckets))
		.with_state(ctx.clone())
		.route("/api/v1/buckets", post(controllers::upsert_bucket))
		.with_state(ctx.clone())
		.route(
			"/api/v1/buckets/{bucketId}/logs",
			post(controllers::append_bucket_logs),
		)
		.with_state(ctx.clone())
		.route(
			"/api/v1/buckets/{bucketId}/clear",
			post(controllers::clear_bucket_logs),
		)
		.with_state(ctx.clone())
		.route(
			"/api/v1/buckets/{bucketId}",
			delete(controllers::delete_bucket),
		)
		.with_state(ctx.clone())
		.route("/api/v1/segment/{segmentId}", get(controllers::get_segment))
		.with_state(ctx.clone())
		.route(
			"/api/v1/segment/{segmentId}/props",
			get(controllers::get_segment_props),
		)
		.with_state(ctx.clone())
		.route(
			"/api/v1/segment/{segmentId}/download",
			get(controllers::download_segment),
		)
		.route(
			"/api/v1/segment/{segmentId}",
			delete(controllers::delete_segment),
		)
		.with_state(ctx.clone())
		.route("/api/v1/server_info", get(controllers::get_server_info))
		.with_state(ctx.clone())
		.fallback(get(controllers::root));

	// run our app with hyper, listening globally on port 3000
	let listener = tokio::net::TcpListener::bind("0.0.0.0:3337").await.unwrap();
	axum::serve(listener, app).await.unwrap();
}
