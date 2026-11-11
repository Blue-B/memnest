pub mod api;
pub mod mcp;
pub mod operations;
pub mod task_cards;

use axum::{
    Router,
    extract::Request,
    http::{
        HeaderName, HeaderValue,
        header::{self},
    },
    middleware::{self, Next},
    response::Response,
    routing::{get, post},
};
use std::sync::Arc;
use tokio::sync::RwLock;

use crate::MemorySystem;

pub fn normalize_token(value: Option<String>) -> Option<String> {
    value
        .map(|token| token.trim().to_string())
        .filter(|token| !token.is_empty())
}

pub fn auth_token() -> Option<String> {
    normalize_token(std::env::var("MEMNEST_TOKEN").ok())
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
        .route("/recent", post(api::recent))
        .route("/reviewed", post(api::reviewed))
        .route("/delete", post(api::delete))
        .route("/restore", post(api::restore))
        .route("/purge", post(api::purge))
        .route("/retention", post(api::run_retention))
        .route("/prune", post(api::prune))
        .route("/secrets", get(api::list_secrets).post(api::set_secret))
        .route(
            "/secrets/{key}",
            get(api::get_secret).delete(api::delete_secret),
        )
        .route("/stats", get(api::guarded_stats))
        .route("/audit", get(crate::access::audit))
        // MCP over Streamable HTTP: same auth and security layers as every other
        // route, so one service covers the API and MCP clients.
        .route("/mcp", post(mcp::http_endpoint))
        .route("/task-card", post(task_cards::endpoint))
        .route("/task-card/hook", post(task_cards::hook_endpoint))
        .layer(middleware::from_fn(security_headers_middleware))
        .layer(middleware::from_fn_with_state(
            system.clone(),
            crate::access::middleware,
        ))
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

/// The real router on a loopback port, so the access middleware is under test too.
#[cfg(test)]
mod http_tests {
    use super::api::test_support::build_system;
    use serde_json::json;

    async fn serve() -> (tempfile::TempDir, String, String) {
        let (tmp, system) = build_system().await;
        let token = system.read().await.api_token.clone();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        tokio::spawn(async move {
            axum::serve(listener, super::create_router(system))
                .await
                .unwrap();
        });
        (tmp, base, token)
    }

    fn client() -> reqwest::Client {
        reqwest::Client::builder().no_proxy().build().unwrap()
    }

    #[tokio::test]
    async fn cross_site_requests_and_non_json_mcp_posts_are_rejected() {
        let (_tmp, base, _) = serve().await;
        let list = json!({"jsonrpc":"2.0","id":1,"method":"tools/list"}).to_string();
        let post = |content_type: &'static str, origin: Option<&'static str>| {
            let mut request = client()
                .post(format!("{base}/mcp"))
                .header("content-type", content_type)
                .body(list.clone());
            if let Some(origin) = origin {
                request = request.header("origin", origin);
            }
            async move { request.send().await.unwrap().status().as_u16() }
        };
        assert_eq!(post("application/json", None).await, 200);
        assert_eq!(post("application/json; charset=utf-8", None).await, 200);
        assert_eq!(post("text/plain", None).await, 415);
        for origin in ["http://203.0.113.7", "http://localhost:8080"] {
            assert_eq!(post("application/json", Some(origin)).await, 403);
            assert_eq!(post("text/plain", Some(origin)).await, 403);
        }
        // The other body-taking card routes only parse JSON.
        for path in ["/task-card", "/task-card/hook"] {
            let status = client()
                .post(format!("{base}{path}"))
                .header("content-type", "text/plain")
                .body(r#"{"action":"list"}"#)
                .send()
                .await
                .unwrap()
                .status();
            assert_eq!(status.as_u16(), 415, "{path}");
        }
    }

    #[tokio::test]
    async fn vault_needs_the_api_token_when_no_other_credential_is_configured() {
        let (_tmp, base, token) = serve().await;
        let secrets = |auth: Option<String>| {
            let mut request = client().get(format!("{base}/secrets"));
            if let Some(auth) = auth {
                request = request.header("authorization", auth);
            }
            async move { request.send().await.unwrap().status().as_u16() }
        };
        assert_eq!(secrets(None).await, 401);
        assert_eq!(secrets(Some("Bearer wrong".into())).await, 401);
        assert_eq!(secrets(Some(format!("Bearer {token}"))).await, 200);
        let one = client().get(format!("{base}/secrets/any"));
        assert_eq!(one.send().await.unwrap().status().as_u16(), 401);
        // Memory routes stay open to local clients.
        let health = client().get(format!("{base}/health")).send().await.unwrap();
        assert_eq!(health.status().as_u16(), 200);

        // The same rule for secret tools called through POST /mcp.
        let mcp = |method: &'static str, auth: Option<String>| {
            let mut request = client().post(format!("{base}/mcp")).json(&json!({
                "jsonrpc":"2.0","id":1,"method":method,
                "params":{"name":"secret_list","arguments":{}}
            }));
            if let Some(auth) = auth {
                request = request.header("authorization", auth);
            }
            async move {
                let body: serde_json::Value = request.send().await.unwrap().json().await.unwrap();
                body["result"].clone()
            }
        };
        let denied = mcp("tools/call", None).await;
        assert_eq!(denied["isError"], true);
        assert!(denied.to_string().contains("api-token"));
        let allowed = mcp("tools/call", Some(format!("Bearer {token}"))).await;
        assert!(allowed["isError"].is_null(), "{allowed}");
        let names = |result: serde_json::Value| result["tools"].to_string();
        assert!(!names(mcp("tools/list", None).await).contains("secret_get"));
        assert!(
            names(mcp("tools/list", Some(format!("Bearer {token}"))).await).contains("secret_get")
        );
    }
}
