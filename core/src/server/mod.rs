pub mod api;
pub mod mcp;
pub mod operations;

use axum::{
    Router,
    extract::Request,
    http::{
        HeaderName, HeaderValue, StatusCode,
        header::{self},
    },
    middleware::{self, Next},
    response::Response,
    routing::{get, post},
};
use std::sync::Arc;
use tokio::sync::RwLock;
use tower_http::services::ServeDir;

use crate::MemorySystem;

pub fn normalize_token(value: Option<String>) -> Option<String> {
    value
        .map(|token| token.trim().to_string())
        .filter(|token| !token.is_empty())
}

pub fn auth_token() -> Option<String> {
    normalize_token(std::env::var("MEMNEST_TOKEN").ok())
}

async fn auth_middleware(request: Request, next: Next) -> Result<Response, StatusCode> {
    if let Some(token) = auth_token() {
        let auth_header = request
            .headers()
            .get("authorization")
            .and_then(|h| h.to_str().ok())
            .unwrap_or("");
        let expected = format!("Bearer {}", token);
        if auth_header != expected {
            return Err(StatusCode::UNAUTHORIZED);
        }
    }
    Ok(next.run(request).await)
}

async fn security_headers_middleware(request: Request, next: Next) -> Response {
    let mut response = next.run(request).await;
    let headers = response.headers_mut();
    headers.insert(
        header::X_CONTENT_TYPE_OPTIONS,
        HeaderValue::from_static("nosniff"),
    );
    headers.insert(
        HeaderName::from_static("referrer-policy"),
        HeaderValue::from_static("no-referrer"),
    );
    headers.insert(
        HeaderName::from_static("content-security-policy"),
        HeaderValue::from_static(
            "default-src 'self'; img-src 'self' data:; style-src 'self' 'unsafe-inline'; script-src 'self' 'unsafe-inline'; connect-src 'self'; frame-ancestors 'none'; base-uri 'self'; form-action 'self'",
        ),
    );
    headers.insert(
        HeaderName::from_static("permissions-policy"),
        HeaderValue::from_static("camera=(), microphone=(), geolocation=()"),
    );
    headers.insert(
        HeaderName::from_static("cross-origin-resource-policy"),
        HeaderValue::from_static("same-origin"),
    );
    headers
        .entry(header::CACHE_CONTROL)
        .or_insert(HeaderValue::from_static("no-store"));
    response
}

pub fn create_router(system: Arc<RwLock<MemorySystem>>) -> Router {
    Router::new()
        .route("/health", get(api::health))
        .route("/search", post(api::search))
        .route("/chunk/{id}", get(api::get_chunk_full))
        .route("/context", post(api::context_pack))
        .route("/add", post(api::add))
        .route("/update", post(api::update))
        .route("/delete", post(api::delete))
        .route("/restore", post(api::restore))
        .route("/prune", post(api::prune))
        .route("/secrets", get(api::list_secrets).post(api::set_secret))
        .route(
            "/secrets/{key}",
            get(api::get_secret).delete(api::delete_secret),
        )
        .route("/stats", get(api::stats))
        // MCP over Streamable HTTP: same auth and security layers as every other
        // route, so one service covers the API and MCP clients.
        .route("/mcp", post(mcp::http_endpoint))
        // No page is served from this process anymore, but the mount stays:
        // the install scripts and preflight checks ship `static/` and assume
        // it is reachable.
        .nest_service("/assets", ServeDir::new("static"))
        .layer(middleware::from_fn(security_headers_middleware))
        .layer(middleware::from_fn(auth_middleware))
        .with_state(system)
}

#[cfg(test)]
mod auth_tests {
    #[test]
    fn empty_and_whitespace_tokens_are_disabled() {
        assert_eq!(super::normalize_token(None), None);
        assert_eq!(super::normalize_token(Some("  ".into())), None);
        assert_eq!(
            super::normalize_token(Some(" token ".into())).as_deref(),
            Some("token")
        );
    }
}
