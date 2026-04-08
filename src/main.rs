use anyhow::Error;
use axum::body::Body;
use axum::extract::{
	ws::{Message, WebSocket, WebSocketUpgrade},
	DefaultBodyLimit, Path, State,
};
use axum::http::{header, StatusCode};
use axum::response::{Html, IntoResponse, Response};
use axum::routing::{delete, get, post};
use axum::Router;
use config::log_path;
use context::Context;
use futures_util::Stream;
use log::LevelFilter;
use simple_logger::SimpleLogger;
use std::{
	path::{Path as FsPath, PathBuf},
	pin::Pin,
	sync::Arc,
	task::{Context as TaskContext, Poll},
};
use tower::ServiceBuilder;
use tower_http::compression::CompressionLayer;
use tower_http::cors::{AllowMethods, Any, CorsLayer};
use tower_http::decompression::RequestDecompressionLayer;
use wgui::{Wgui, WguiHandle, WsMessage};

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
	wgui.set_ctx_state(ui::UiContext::new(ctx.clone()));
	wgui.add_component::<ui::Ui>("/");
	let handle = wgui.handle();
	tokio::spawn(async move {
		wgui.run().await;
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
		.merge(upload_router)
		.route("/api/v1/server_info", get(controllers::get_server_info))
		.with_state(ctx.clone())
		.layer(cors.clone());

	let ui_router = Router::new()
		.route("/", get(wgui_index))
		.route("/ws", get(wgui_ws))
		.route("/index.js", get(wgui_js))
		.route("/index.css", get(wgui_css))
		.route("/assets/{*path}", get(wgui_asset))
		.fallback(get(wgui_index))
		.with_state(handle);

	let app = Router::new()
		.merge(api_router)
		.merge(ui_router)
		.layer(CompressionLayer::new());

	// run our app with hyper, listening globally on port 3000
	let listener = tokio::net::TcpListener::bind("0.0.0.0:3337").await.unwrap();
	axum::serve(listener, app).await.unwrap();
}

struct AxumWs {
	inner: WebSocket,
}

impl AxumWs {
	fn new(inner: WebSocket) -> Self {
		Self { inner }
	}
}

impl Stream for AxumWs {
	type Item = Result<WsMessage, Error>;

	fn poll_next(mut self: Pin<&mut Self>, cx: &mut TaskContext<'_>) -> Poll<Option<Self::Item>> {
		match Pin::new(&mut self.inner).poll_next(cx) {
			Poll::Ready(Some(Ok(message))) => Poll::Ready(Some(Ok(match message {
				Message::Text(text) => WsMessage::Text(text.to_string()),
				Message::Binary(bytes) => WsMessage::Binary(bytes.to_vec()),
				Message::Ping(bytes) => WsMessage::Ping(bytes.to_vec()),
				Message::Pong(bytes) => WsMessage::Pong(bytes.to_vec()),
				Message::Close(_) => WsMessage::Close,
			}))),
			Poll::Ready(Some(Err(err))) => Poll::Ready(Some(Err(err.into()))),
			Poll::Ready(None) => Poll::Ready(None),
			Poll::Pending => Poll::Pending,
		}
	}
}

impl futures_util::Sink<WsMessage> for AxumWs {
	type Error = Error;

	fn poll_ready(
		mut self: Pin<&mut Self>,
		cx: &mut TaskContext<'_>,
	) -> Poll<Result<(), Self::Error>> {
		Pin::new(&mut self.inner)
			.poll_ready(cx)
			.map_err(Error::from)
	}

	fn start_send(mut self: Pin<&mut Self>, item: WsMessage) -> Result<(), Self::Error> {
		let message = match item {
			WsMessage::Text(text) => Message::Text(text.into()),
			WsMessage::Binary(bytes) => Message::Binary(bytes.into()),
			WsMessage::Ping(bytes) => Message::Ping(bytes.into()),
			WsMessage::Pong(bytes) => Message::Pong(bytes.into()),
			WsMessage::Close => Message::Close(None),
		};
		Pin::new(&mut self.inner)
			.start_send(message)
			.map_err(Error::from)
	}

	fn poll_flush(
		mut self: Pin<&mut Self>,
		cx: &mut TaskContext<'_>,
	) -> Poll<Result<(), Self::Error>> {
		Pin::new(&mut self.inner)
			.poll_flush(cx)
			.map_err(Error::from)
	}

	fn poll_close(
		mut self: Pin<&mut Self>,
		cx: &mut TaskContext<'_>,
	) -> Poll<Result<(), Self::Error>> {
		Pin::new(&mut self.inner)
			.poll_close(cx)
			.map_err(Error::from)
	}
}

async fn wgui_ws(State(handle): State<WguiHandle>, ws: WebSocketUpgrade) -> impl IntoResponse {
	ws.on_upgrade(move |socket| async move {
		handle.handle_ws(AxumWs::new(socket)).await;
	})
}

async fn wgui_index() -> Html<&'static str> {
	Html(wgui::dist::index_html())
}

async fn wgui_js() -> Response {
	Response::builder()
		.header(header::CONTENT_TYPE, "text/javascript")
		.header(header::CACHE_CONTROL, "no-store")
		.body(Body::from(wgui::dist::index_js()))
		.unwrap()
}

async fn wgui_css() -> Response {
	Response::builder()
		.header(header::CONTENT_TYPE, "text/css")
		.header(header::CACHE_CONTROL, "no-store")
		.body(Body::from(wgui::dist::index_css()))
		.unwrap()
}

async fn wgui_asset(Path(path): Path<String>) -> Response {
	let Some(asset_path) = sanitize_asset_path(&path) else {
		return Response::builder()
			.status(StatusCode::BAD_REQUEST)
			.body(Body::from("bad asset path"))
			.unwrap();
	};

	match tokio::fs::read(&asset_path).await {
		Ok(bytes) => Response::builder()
			.header(header::CONTENT_TYPE, content_type_for(&asset_path))
			.body(Body::from(bytes))
			.unwrap(),
		Err(_) => Response::builder()
			.status(StatusCode::NOT_FOUND)
			.body(Body::from("asset not found"))
			.unwrap(),
	}
}

fn sanitize_asset_path(uri_path: &str) -> Option<PathBuf> {
	if uri_path.is_empty() {
		return None;
	}
	let mut out = PathBuf::from("assets");
	for part in uri_path.split('/') {
		if part.is_empty() || part == "." || part == ".." {
			return None;
		}
		out.push(part);
	}
	Some(out)
}

fn content_type_for(path: &FsPath) -> &'static str {
	match path
		.extension()
		.and_then(|ext| ext.to_str())
		.unwrap_or_default()
	{
		"css" => "text/css",
		"html" => "text/html",
		"ico" => "image/x-icon",
		"js" => "text/javascript",
		"json" => "application/json",
		"jpg" | "jpeg" => "image/jpeg",
		"png" => "image/png",
		"svg" => "image/svg+xml",
		_ => "application/octet-stream",
	}
}
