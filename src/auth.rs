use axum::body::Body;
use axum::extract::{Request, State};
use axum::http::{header, HeaderMap, HeaderValue, StatusCode};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde_json::json;
use std::sync::Arc;

use crate::context::Context;
use crate::db::DB;

const BOOTSTRAP_ADMIN_NAME: &str = "PUPPYLOG_BOOTSTRAP_ADMIN_NAME";
const BOOTSTRAP_ADMIN_API_KEY: &str = "PUPPYLOG_BOOTSTRAP_ADMIN_API_KEY";

pub async fn require_bearer_auth(
	State(ctx): State<Arc<Context>>,
	headers: HeaderMap,
	mut request: Request<Body>,
	next: Next,
) -> Response {
	let Some(token) = bearer_token(&headers) else {
		return unauthorized_response();
	};

	match ctx.db.authenticate_api_key(token).await {
		Ok(Some(user)) => {
			request.extensions_mut().insert(user);
			next.run(request).await
		}
		Ok(None) => unauthorized_response(),
		Err(err) => {
			log::error!("failed to authenticate bearer token: {err}");
			(
				StatusCode::INTERNAL_SERVER_ERROR,
				Json(json!({
					"error": "failed to authenticate bearer token"
				})),
			)
				.into_response()
		}
	}
}

pub async fn bootstrap_admin_from_env(db: &DB) -> anyhow::Result<()> {
	let name = std::env::var(BOOTSTRAP_ADMIN_NAME).ok();
	let api_key = std::env::var(BOOTSTRAP_ADMIN_API_KEY).ok();

	let (Some(name), Some(api_key)) = (name, api_key) else {
		if std::env::var(BOOTSTRAP_ADMIN_NAME).is_ok()
			|| std::env::var(BOOTSTRAP_ADMIN_API_KEY).is_ok()
		{
			anyhow::bail!(
				"{BOOTSTRAP_ADMIN_NAME} and {BOOTSTRAP_ADMIN_API_KEY} must be set together"
			);
		}
		return Ok(());
	};

	let name = name.trim();
	let api_key = api_key.trim();
	if name.is_empty() || api_key.is_empty() {
		anyhow::bail!("{BOOTSTRAP_ADMIN_NAME} and {BOOTSTRAP_ADMIN_API_KEY} must not be empty");
	}

	match db.bootstrap_admin(name, api_key).await? {
		Some(user) => log::info!("created bootstrap admin user '{}'", user.name),
		None => log::info!("bootstrap admin skipped because users already exist"),
	}
	Ok(())
}

fn unauthorized_response() -> Response {
	let mut response = (
		StatusCode::UNAUTHORIZED,
		Json(json!({
			"error": "missing or invalid bearer token"
		})),
	)
		.into_response();
	response
		.headers_mut()
		.insert(header::WWW_AUTHENTICATE, HeaderValue::from_static("Bearer"));
	response
}

fn bearer_token(headers: &HeaderMap) -> Option<&str> {
	let value = headers.get(header::AUTHORIZATION)?.to_str().ok()?.trim();
	let (scheme, token) = value.split_once(' ')?;
	if !scheme.eq_ignore_ascii_case("Bearer") {
		return None;
	}
	let token = token.trim();
	if token.is_empty() {
		None
	} else {
		Some(token)
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::db::User;
	use axum::body::to_bytes;
	use axum::http::Request;
	use axum::middleware;
	use axum::routing::get;
	use axum::Extension;
	use axum::Router;
	use tempfile::TempDir;
	use tower::ServiceExt;

	async fn ok() -> &'static str {
		"ok"
	}

	async fn current_user(Extension(user): Extension<User>) -> Json<serde_json::Value> {
		Json(json!({
			"id": user.id,
			"name": user.name,
			"isAdmin": user.is_admin
		}))
	}

	async fn test_context() -> (TempDir, Arc<Context>) {
		let dir = TempDir::new().unwrap();
		let log_dir = dir.path().join("logs");
		std::fs::create_dir_all(&log_dir).unwrap();
		(dir, Arc::new(Context::new(log_dir).await))
	}

	fn protected_app(ctx: Arc<Context>) -> Router {
		Router::new()
			.route("/protected", get(ok))
			.route("/whoami", get(current_user))
			.layer(middleware::from_fn_with_state(ctx, require_bearer_auth))
	}

	#[tokio::test]
	async fn rejects_missing_header() {
		let (_dir, ctx) = test_context().await;
		let response = protected_app(ctx)
			.oneshot(
				Request::builder()
					.uri("/protected")
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
	async fn rejects_invalid_bearer_token() {
		let (_dir, ctx) = test_context().await;
		ctx.db
			.create_user_with_api_key("admin", true, Some("test"), "secret")
			.await
			.unwrap();

		let response = protected_app(ctx)
			.oneshot(
				Request::builder()
					.uri("/protected")
					.header(header::AUTHORIZATION, "Bearer wrong")
					.body(Body::empty())
					.unwrap(),
			)
			.await
			.unwrap();

		assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
	}

	#[tokio::test]
	async fn accepts_valid_bearer_token() {
		let (_dir, ctx) = test_context().await;
		ctx.db
			.create_user_with_api_key("admin", true, Some("test"), "secret")
			.await
			.unwrap();

		let response = protected_app(ctx)
			.oneshot(
				Request::builder()
					.uri("/protected")
					.header(header::AUTHORIZATION, "Bearer secret")
					.body(Body::empty())
					.unwrap(),
			)
			.await
			.unwrap();

		assert_eq!(response.status(), StatusCode::OK);
		let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
		assert_eq!(&body[..], b"ok");
	}

	#[tokio::test]
	async fn exposes_authenticated_user_to_handlers() {
		let (_dir, ctx) = test_context().await;
		let user = ctx
			.db
			.create_user_with_api_key("admin", true, Some("test"), "secret")
			.await
			.unwrap();

		let response = protected_app(ctx)
			.oneshot(
				Request::builder()
					.uri("/whoami")
					.header(header::AUTHORIZATION, "Bearer secret")
					.body(Body::empty())
					.unwrap(),
			)
			.await
			.unwrap();

		assert_eq!(response.status(), StatusCode::OK);
		let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
		let payload: serde_json::Value = serde_json::from_slice(&body).unwrap();
		assert_eq!(payload["id"], user.id);
		assert_eq!(payload["name"], "admin");
		assert_eq!(payload["isAdmin"], true);
	}
}
