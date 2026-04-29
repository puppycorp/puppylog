use axum::body::Body;
use axum::extract::{Request, State};
use axum::http::{header, HeaderMap, HeaderValue, StatusCode};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde_json::json;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BearerAuthConfig {
	required: bool,
	allowed_api_keys: Vec<String>,
}

impl BearerAuthConfig {
	pub fn from_env() -> Self {
		let allowed_api_keys = std::env::var("PUPPYLOG_API_KEYS")
			.ok()
			.map(|value| parse_api_keys(&value))
			.unwrap_or_default();
		let required = std::env::var("PUPPYLOG_AUTH_REQUIRED")
			.ok()
			.and_then(|value| parse_bool(&value))
			.unwrap_or(!allowed_api_keys.is_empty());

		Self {
			required,
			allowed_api_keys,
		}
	}

	#[cfg(test)]
	pub fn disabled() -> Self {
		Self {
			required: false,
			allowed_api_keys: Vec::new(),
		}
	}

	#[cfg(test)]
	pub fn required<I, S>(allowed_api_keys: I) -> Self
	where
		I: IntoIterator<Item = S>,
		S: Into<String>,
	{
		Self {
			required: true,
			allowed_api_keys: allowed_api_keys.into_iter().map(Into::into).collect(),
		}
	}

	fn allows(&self, headers: &HeaderMap) -> bool {
		if !self.required {
			return true;
		}

		let Some(token) = bearer_token(headers) else {
			return false;
		};

		self.allowed_api_keys.iter().any(|key| key == token)
	}
}

pub async fn require_bearer_auth(
	State(config): State<BearerAuthConfig>,
	headers: HeaderMap,
	request: Request<Body>,
	next: Next,
) -> Response {
	if config.allows(&headers) {
		return next.run(request).await;
	}

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

fn parse_api_keys(value: &str) -> Vec<String> {
	value
		.split(',')
		.map(str::trim)
		.filter(|key| !key.is_empty())
		.map(str::to_string)
		.collect()
}

fn parse_bool(value: &str) -> Option<bool> {
	match value.trim().to_ascii_lowercase().as_str() {
		"1" | "true" | "yes" | "on" => Some(true),
		"0" | "false" | "no" | "off" => Some(false),
		_ => None,
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use axum::body::to_bytes;
	use axum::http::Request;
	use axum::middleware;
	use axum::routing::get;
	use axum::Router;
	use tower::ServiceExt;

	async fn ok() -> &'static str {
		"ok"
	}

	fn protected_app(config: BearerAuthConfig) -> Router {
		Router::new()
			.route("/protected", get(ok))
			.layer(middleware::from_fn_with_state(config, require_bearer_auth))
	}

	#[tokio::test]
	async fn disabled_auth_allows_missing_header() {
		let response = protected_app(BearerAuthConfig::disabled())
			.oneshot(
				Request::builder()
					.uri("/protected")
					.body(Body::empty())
					.unwrap(),
			)
			.await
			.unwrap();

		assert_eq!(response.status(), StatusCode::OK);
	}

	#[tokio::test]
	async fn required_auth_rejects_missing_header() {
		let response = protected_app(BearerAuthConfig::required(["secret"]))
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
	async fn required_auth_rejects_invalid_bearer_token() {
		let response = protected_app(BearerAuthConfig::required(["secret"]))
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
	async fn required_auth_accepts_valid_bearer_token() {
		let response = protected_app(BearerAuthConfig::required(["secret"]))
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

	#[test]
	fn api_keys_are_comma_separated_and_trimmed() {
		assert_eq!(
			parse_api_keys(" first,second ,, third "),
			vec!["first", "second", "third"]
		);
	}
}
