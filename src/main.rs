use axum::extract::DefaultBodyLimit;
use axum::response::Html;
use axum::routing::{delete, get, post};
use axum::Router;
use config::log_path;
use context::Context;
use log::LevelFilter;
use simple_logger::SimpleLogger;
use std::{collections::HashSet, sync::Arc};
use tokio::sync::Mutex;
use tokio::time::{interval, Duration};
use tower::ServiceBuilder;
use tower_http::compression::CompressionLayer;
use tower_http::cors::{AllowMethods, Any, CorsLayer};
use tower_http::decompression::RequestDecompressionLayer;
use wgui::{ClientEvent, Wgui, WguiHandle};

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
mod server_info;
mod settings;
mod slack;
mod subscribe_worker;
mod types;
mod ui;
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
	if std::env::var("DISK_SPACE_MONITOR").as_deref() == Ok("1") {
		tokio::spawn(cleanup::run_disk_space_monitor(ctx.clone()));
	}
	tokio::spawn(dev_segment_merger::run_dev_segment_merger(ctx.clone()));
	tokio::spawn(device_segment_compactor::run_device_segment_compactor(
		ctx.clone(),
	));

	let cors = CorsLayer::new()
		.allow_origin(Any) // Allow requests from any origin
		.allow_methods(AllowMethods::any()) // Allowed HTTP methods
		.allow_headers(Any);

	let mut wgui = Wgui::new_without_server();
	let wgui_router = wgui.router();
	let handle = wgui.handle();
	let clients = Arc::new(Mutex::new(HashSet::new()));

	let ctx_for_events = ctx.clone();
	let handle_for_events = handle.clone();
	let clients_for_events = clients.clone();
	tokio::spawn(async move {
		run_wgui_event_loop(wgui, handle_for_events, ctx_for_events, clients_for_events).await;
	});

	let ctx_for_refresh = ctx.clone();
	let handle_for_refresh = handle.clone();
	let clients_for_refresh = clients.clone();
	tokio::spawn(async move {
		let mut ticker = interval(Duration::from_secs(5));
		loop {
			ticker.tick().await;
			broadcast_snapshot(&handle_for_refresh, &ctx_for_refresh, &clients_for_refresh).await;
		}
	});

	let upload_router = Router::new()
		.route(
			"/api/v1/device/{deviceId}/logs",
			post(controllers::upload_device_logs),
		)
		.route_layer(
			ServiceBuilder::new()
				.layer(DefaultBodyLimit::max(1024 * 1024 * 1000))
				.layer(RequestDecompressionLayer::new().gzip(true).zstd(true)),
		);

	let api_router = Router::new()
		.route("/favicon.ico", get(controllers::favicon))
		.route("/favicon-192x192.png", get(controllers::favicon_192x192))
		.route("/favicon-512x512.png", get(controllers::favicon_512x512))
		.route("/manifest.json", get(controllers::manifest))
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
		.merge(upload_router)f
		.route("/api/v1/server_info", get(controllers::get_server_info))
		.with_state(ctx.clone())
		.layer(cors.clone());

	let app = Router::new()
		.merge(api_router)
		.merge(wgui_router)
		.fallback(get(wgui_index))
		.layer(CompressionLayer::new());

	// run our app with hyper, listening globally on port 3000
	let listener = tokio::net::TcpListener::bind("0.0.0.0:3337").await.unwrap();
	axum::serve(listener, app).await.unwrap();
}

async fn run_wgui_event_loop(
	mut wgui: Wgui,
	handle: WguiHandle,
	ctx: Arc<Context>,
	clients: Arc<Mutex<HashSet<usize>>>,
) {
	use ClientEvent::*;
	while let Some(event) = wgui.next().await {
		match event {
			Connected { id } => {
				{
					let mut guard = clients.lock().await;
					guard.insert(id);
				}
				render_snapshot_to_client(&handle, &ctx, id).await;
			}
			Disconnected { id } => {
				let mut guard = clients.lock().await;
				guard.remove(&id);
			}
			OnClick(click) => {
				if click.id == ui::REFRESH_BUTTON_ID {
					broadcast_snapshot(&handle, &ctx, &clients).await;
				}
			}
			_ => {}
		}
	}
}

async fn render_snapshot_to_client(handle: &WguiHandle, ctx: &Arc<Context>, client_id: usize) {
	let snapshot = ui::UiSnapshot::capture(ctx).await;
	handle.render(client_id, ui::render(&snapshot)).await;
}

async fn broadcast_snapshot(
	handle: &WguiHandle,
	ctx: &Arc<Context>,
	clients: &Mutex<HashSet<usize>>,
) {
	let ids = {
		let guard = clients.lock().await;
		guard.iter().copied().collect::<Vec<_>>()
	};
	if ids.is_empty() {
		return;
	}
	let snapshot = ui::UiSnapshot::capture(ctx).await;
	let view = ui::render(&snapshot);
	for id in ids {
		handle.render(id, view.clone()).await;
	}
}

async fn wgui_index() -> Html<&'static str> {
	Html(wgui::dist::index_html())
}
