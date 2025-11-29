use std::sync::Arc;

use axum::extract::{FromRef, FromRequestParts};
use axum::http::request::Parts;
use axum::http::{header, HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use serde::{Deserialize, Serialize};

use crate::context::Context;

#[derive(Debug, Clone)]
pub struct GoogleAuth {
	client_id: String,
	allowed_domains: Option<Vec<String>>,
	http: reqwest::Client,
}

#[derive(Debug, Clone, Serialize)]
pub struct GoogleAuthConfig {
	pub client_id: String,
	#[serde(skip_serializing_if = "Option::is_none")]
	pub allowed_domains: Option<Vec<String>>,
}

impl GoogleAuth {
	pub fn from_env() -> Option<Self> {
		let client_id = std::env::var("GOOGLE_OAUTH_CLIENT_ID").ok()?;
		if client_id.trim().is_empty() {
			return None;
		}
		let allowed_domains = std::env::var("GOOGLE_OAUTH_ALLOWED_DOMAINS")
			.ok()
			.map(|value| {
				value
					.split(',')
					.map(|domain| domain.trim().to_lowercase())
					.filter(|domain| !domain.is_empty())
					.collect::<Vec<_>>()
			})
			.filter(|domains| !domains.is_empty());
		Some(Self {
			client_id,
			allowed_domains,
			http: reqwest::Client::new(),
		})
	}

	pub fn config(&self) -> GoogleAuthConfig {
		GoogleAuthConfig {
			client_id: self.client_id.clone(),
			allowed_domains: self.allowed_domains.clone(),
		}
	}

	pub async fn verify_token(&self, token: &str) -> Result<GoogleUser, AuthError> {
		let response = self
			.http
			.get("https://oauth2.googleapis.com/tokeninfo")
			.query(&[("id_token", token)])
			.send()
			.await
			.map_err(|err| AuthError::Upstream(format!("google request failed: {err}")))?;
		if !response.status().is_success() {
			return Err(AuthError::Unauthorized(
				"token rejected by google".to_string(),
			));
		}
		let token_info: GoogleTokenInfo = response
			.json()
			.await
			.map_err(|err| AuthError::Upstream(format!("invalid google response: {err}")))?;
		if token_info.aud != self.client_id {
			return Err(AuthError::Unauthorized(
				"token audience mismatch".to_string(),
			));
		}
		if token_info.email.is_none() {
			return Err(AuthError::Unauthorized("token missing email".to_string()));
		}
		if !token_info.email_verified() {
			return Err(AuthError::Unauthorized("email is not verified".to_string()));
		}
		if let Some(allowed) = &self.allowed_domains {
			if let Some(email) = &token_info.email {
				let domain = email
					.split('@')
					.nth(1)
					.map(|s| s.to_lowercase())
					.unwrap_or_default();
				if !allowed.iter().any(|d| d == &domain) {
					return Err(AuthError::Forbidden(format!(
						"email domain `{domain}` is not allowed"
					)));
				}
			}
		}
		Ok(GoogleUser {
			email: token_info.email.unwrap(),
			name: token_info.name,
			picture: token_info.picture,
		})
	}
}

#[derive(Debug, Clone)]
pub struct GoogleUser {
	pub email: String,
	pub name: Option<String>,
	pub picture: Option<String>,
}

#[derive(Debug, Deserialize)]
struct GoogleTokenInfo {
	aud: String,
	email: Option<String>,
	email_verified: Option<String>,
	name: Option<String>,
	picture: Option<String>,
}

impl GoogleTokenInfo {
	fn email_verified(&self) -> bool {
		match &self.email_verified {
			Some(value) => matches!(value.as_str(), "true" | "1"),
			None => false,
		}
	}
}

#[derive(Debug)]
pub enum AuthError {
	Unauthorized(String),
	Forbidden(String),
	Upstream(String),
}

impl AuthError {
	fn status_code(&self) -> StatusCode {
		match self {
			AuthError::Unauthorized(_) => StatusCode::UNAUTHORIZED,
			AuthError::Forbidden(_) => StatusCode::FORBIDDEN,
			AuthError::Upstream(_) => StatusCode::BAD_GATEWAY,
		}
	}

	fn message(&self) -> &str {
		match self {
			AuthError::Unauthorized(msg) | AuthError::Forbidden(msg) | AuthError::Upstream(msg) => {
				msg
			}
		}
	}
}

impl IntoResponse for AuthError {
	fn into_response(self) -> Response {
		let status = self.status_code();
		(status, self.message().to_string()).into_response()
	}
}

#[derive(Debug, Clone)]
pub struct MaybeAuthUser(pub Option<GoogleUser>);

#[derive(Debug)]
pub struct AuthRejection(AuthError);

impl IntoResponse for AuthRejection {
	fn into_response(self) -> Response {
		self.0.into_response()
	}
}

fn hex_value(byte: u8) -> Option<u8> {
	match byte {
		b'0'..=b'9' => Some(byte - b'0'),
		b'a'..=b'f' => Some(byte - b'a' + 10),
		b'A'..=b'F' => Some(byte - b'A' + 10),
		_ => None,
	}
}

fn decode_query_value(value: &str) -> Option<String> {
	let mut bytes = Vec::with_capacity(value.len());
	let raw = value.as_bytes();
	let mut i = 0;
	while i < raw.len() {
		match raw[i] {
			b'%' => {
				if i + 2 >= raw.len() {
					return None;
				}
				let hi = hex_value(raw[i + 1])?;
				let lo = hex_value(raw[i + 2])?;
				bytes.push((hi << 4) | lo);
				i += 3;
			}
			b'+' => {
				bytes.push(b' ');
				i += 1;
			}
			byte => {
				bytes.push(byte);
				i += 1;
			}
		}
	}
	String::from_utf8(bytes).ok()
}

fn extract_token(headers: &HeaderMap, parts: &Parts) -> Option<String> {
	if let Some(value) = headers.get(header::AUTHORIZATION) {
		if let Ok(value) = value.to_str() {
			if let Some(token) = value.strip_prefix("Bearer ") {
				return Some(token.trim().to_string());
			}
		}
	}
	if let Some(query) = parts.uri.query() {
		for pair in query.split('&') {
			let mut segments = pair.splitn(2, '=');
			if let Some(key) = segments.next() {
				if key == "token" {
					let value = segments.next().unwrap_or("");
					let decoded =
						decode_query_value(value).unwrap_or_else(|| value.replace('+', " "));
					return Some(decoded);
				}
			}
		}
	}
	None
}

impl<S> FromRequestParts<S> for MaybeAuthUser
where
	Arc<Context>: FromRef<S>,
	S: Sync,
{
	type Rejection = AuthRejection;

	async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
		let ctx: Arc<Context> = Arc::from_ref(state);
		let Some(auth) = ctx.google_auth() else {
			return Ok(MaybeAuthUser(None));
		};
		let token = extract_token(&parts.headers, parts);
		let Some(token) = token else {
			return Err(AuthRejection(AuthError::Unauthorized(
				"missing bearer token".to_string(),
			)));
		};
		match auth.verify_token(&token).await {
			Ok(user) => Ok(MaybeAuthUser(Some(user))),
			Err(err) => Err(AuthRejection(err)),
		}
	}
}
