pub mod api;
pub mod mcp;

use axum::{
    Router,
    extract::Request,
    http::{
        HeaderName, HeaderValue, StatusCode,
        header::{self},
    },
    middleware::{self, Next},
    response::Response,
    routing::{get, post, put},
};
use std::sync::Arc;
use tokio::sync::RwLock;
use tower_http::services::ServeDir;

use crate::MemorySystem;

async fn auth_middleware(request: Request, next: Next) -> Result<Response, StatusCode> {
    if let Ok(token) = std::env::var("MEMNEST_TOKEN") {
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
        .route("/neighbors", post(api::neighbors))
        .route("/context", post(api::context_pack))
        .route("/add", post(api::add))
        .route("/update", post(api::update))
        .route("/delete", post(api::delete))
        .route("/prune", post(api::prune))
        .route("/reproject", post(api::reproject))
        .route("/sessions/fork", post(api::fork_session))
        .route("/summary", post(api::add_summary))
        .route("/compact", post(api::compact))
        .route("/collections", get(api::list_collections))
        .route("/collection/{name}", get(api::collection_detail))
        .route("/collection/{name}/meta", put(api::set_collection_meta))
        .route("/sessions", get(api::list_sessions))
        .route("/facts", get(api::list_facts).post(api::add_fact))
        .route("/notes", get(api::list_notes).post(api::set_note))
        .route("/notes/{key}", get(api::get_note).delete(api::delete_note))
        .route("/servers", get(api::list_servers))
        .route("/secrets", get(api::list_secrets).post(api::set_secret))
        .route(
            "/secrets/{key}",
            get(api::get_secret).delete(api::delete_secret),
        )
        .route("/stats", get(api::stats))
        .nest_service("/assets", ServeDir::new("static"))
        .route("/", get(api::viewer_dashboard))
        .route("/viewer/collections", get(api::viewer_collections))
        .route("/viewer/search", get(api::viewer_search))
        .layer(middleware::from_fn(security_headers_middleware))
        .layer(middleware::from_fn(auth_middleware))
        .with_state(system)
}
