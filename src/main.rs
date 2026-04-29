use axum::extract::DefaultBodyLimit;
use axum::middleware;
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

mod auth;
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
mod search;
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
	auth::bootstrap_admin_from_env(&ctx.db)
		.await
		.expect("failed to bootstrap admin user");

	tokio::spawn(upload::process_log_uploads(ctx.clone()));
	if std::env::var("DISK_SPACE_MONITOR").as_deref() == Ok("1") {
		tokio::spawn(cleanup::run_disk_space_monitor(ctx.clone()));
	}
	tokio::spawn(dev_segment_merger::run_dev_segment_merger(ctx.clone()));
	tokio::spawn(device_segment_compactor::run_device_segment_compactor(
		ctx.clone(),
	));

	let app = build_router(ctx);

	// run our app with hyper, listening globally on port 3000
	let listener = tokio::net::TcpListener::bind("0.0.0.0:3337").await.unwrap();
	axum::serve(listener, app).await.unwrap();
}

fn build_router(ctx: Arc<Context>) -> Router {
	let cors = CorsLayer::new()
		.allow_origin(Any)
		.allow_methods(AllowMethods::any())
		.allow_headers(Any);

	let protected_routes = Router::new()
		.route("/api/logs", get(controllers::get_logs))
		.route("/api/logs/stream", get(controllers::stream_logs))
		.route(
			"/api/settings/query",
			post(controllers::post_settings_query),
		)
		.route("/api/settings/query", get(controllers::get_settings_query))
		.route("/api/segments", get(controllers::get_segments))
		.route(
			"/api/segment/metadata",
			get(controllers::get_segment_metadata),
		)
		.route("/api/v1/validate_query", get(controllers::validate_query))
		.route("/api/v1/logs/stream", get(controllers::stream_logs))
		.route("/api/v1/logs/histogram", get(controllers::get_histogram))
		.route(
			"/api/v1/device/{deviceId}/status",
			get(controllers::get_device_status),
		)
		.route("/api/v1/device/{deviceId}", get(controllers::get_device))
		.route(
			"/api/v1/device/{deviceId}/logs",
			post(controllers::upload_device_logs),
		)
		.layer(DefaultBodyLimit::max(1024 * 1024 * 1000))
		.layer(RequestDecompressionLayer::new().gzip(true).zstd(true))
		.route(
			"/api/v1/device/{deviceId}/metadata",
			post(controllers::update_device_metadata),
		)
		.route(
			"/api/v1/device/{deviceId}/settings",
			post(controllers::update_device_settings),
		)
		.route("/api/v1/device_bulkedit", post(controllers::bulk_edit))
		.route("/api/v1/settings", post(controllers::post_settings_query))
		.route("/api/v1/settings", get(controllers::get_settings_query))
		.route("/api/v1/devices", get(controllers::get_devices))
		.route("/api/v1/segments", get(controllers::get_segments))
		.route("/api/v1/segment/{segmentId}", get(controllers::get_segment))
		.route(
			"/api/v1/segment/{segmentId}/logs.txt",
			get(controllers::download_segment_text),
		)
		.route(
			"/api/v1/segment/{segmentId}/props",
			get(controllers::get_segment_props),
		)
		.route(
			"/api/v1/segment/{segmentId}/download",
			get(controllers::download_segment),
		)
		.route(
			"/api/v1/segment/{segmentId}",
			delete(controllers::delete_segment),
		)
		.route("/api/v1/server/cleanup", post(controllers::start_cleanup))
		.route_layer(middleware::from_fn_with_state(
			ctx.clone(),
			auth::require_bearer_auth,
		))
		.with_state(ctx);

	Router::new()
		.route("/", get(controllers::root))
		.route("/puppylog.js", get(controllers::js))
		.route("/puppylog.css", get(controllers::css))
		.route("/favicon.ico", get(controllers::favicon))
		.route("/favicon-192x192.png", get(controllers::favicon_192x192))
		.route("/favicon-512x512.png", get(controllers::favicon_512x512))
		.route("/manifest.json", get(controllers::manifest))
		.route("/api/v1/server_info", get(controllers::get_server_info))
		.merge(protected_routes)
		.fallback(get(controllers::root))
		.layer(CompressionLayer::new())
		.layer(cors)
}

#[cfg(test)]
mod tests {
	use super::*;
	use axum::body::Body;
	use axum::http::{header, Request, StatusCode};
	use serial_test::serial;
	use tempfile::TempDir;
	use tower::ServiceExt;

	async fn test_app() -> (TempDir, Arc<Context>, Router) {
		let dir = TempDir::new().unwrap();
		let log_dir = dir.path().join("logs");
		std::fs::create_dir_all(&log_dir).unwrap();
		std::env::set_var("LOG_PATH", &log_dir);
		std::env::set_var("DB_PATH", dir.path().join("db.sqlite"));
		std::env::set_var("SETTINGS_PATH", dir.path().join("settings.json"));
		std::env::set_var("UPLOAD_PATH", dir.path().join("uploads"));
		std::fs::write(
			dir.path().join("settings.json"),
			"{\"collection_query\":\"\"}",
		)
		.unwrap();

		let ctx = Arc::new(Context::new(log_dir).await);
		let app = build_router(ctx.clone());
		(dir, ctx, app)
	}

	#[tokio::test]
	#[serial]
	async fn protected_api_rejects_missing_bearer_token() {
		let (_dir, _ctx, app) = test_app().await;

		let response = app
			.oneshot(
				Request::builder()
					.uri("/api/v1/validate_query?query=level%20=%20info")
					.body(Body::empty())
					.unwrap(),
			)
			.await
			.unwrap();

		assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
		assert_eq!(
			response.headers().get(header::WWW_AUTHENTICATE).unwrap(),
			"Bearer"
		);
	}

	#[tokio::test]
	#[serial]
	async fn protected_api_accepts_valid_bearer_token() {
		let (_dir, ctx, app) = test_app().await;
		ctx.db
			.create_user_with_api_key("admin", true, Some("test"), "secret")
			.await
			.unwrap();

		let response = app
			.oneshot(
				Request::builder()
					.uri("/api/v1/validate_query?query=level%20=%20info")
					.header(header::AUTHORIZATION, "Bearer secret")
					.body(Body::empty())
					.unwrap(),
			)
			.await
			.unwrap();

		assert_eq!(response.status(), StatusCode::OK);
	}

	#[tokio::test]
	#[serial]
	async fn server_info_stays_public_with_auth_required() {
		let (_dir, _ctx, app) = test_app().await;

		let response = app
			.oneshot(
				Request::builder()
					.uri("/api/v1/server_info")
					.body(Body::empty())
					.unwrap(),
			)
			.await
			.unwrap();

		assert_eq!(response.status(), StatusCode::OK);
	}
}
