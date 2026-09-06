//! Local notification intake.
//!
//! Why a webhook instead of reading OS notifications: on macOS, reading other
//! apps' notifications is effectively impossible post-SIP, and on Windows it
//! requires MSIX package identity. A localhost endpoint works identically on
//! both, needs no permissions, and lets anything push to the pet — a cron job,
//! a userscript, n8n, a CI pipeline's final `curl`.
//!
//!   curl -X POST http://127.0.0.1:7423/notify \
//!        -H "Authorization: Bearer $DESKPET_TOKEN" \
//!        -d '{"title":"Build passed","body":"main @ a1b2c3","source":"ci"}'
//!
//! Binds to 127.0.0.1 only, and requires a token even so: any process on the
//! machine can reach loopback, including a browser tab.

use anyhow::Result;
use axum::{
    extract::State,
    http::{HeaderMap, StatusCode},
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tauri::{AppHandle, Emitter};

pub const DEFAULT_PORT: u16 = 7423;
/// Event name the frontend listens on.
pub const EVENT: &str = "deskpet://notify";

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Notification {
    pub title: String,
    #[serde(default)]
    pub body: String,
    /// Free-form origin tag ("gmail", "ci", "slack"). Packs may map this to a
    /// specific reaction animation.
    #[serde(default)]
    pub source: String,
    /// Optional URL or file path opened when the user clicks the bubble.
    #[serde(default)]
    pub action_url: Option<String>,
    /// 0 = ambient, 1 = normal, 2 = important. Level 2 is the only one that
    /// pierces do-not-disturb, and even then only visually.
    #[serde(default = "default_priority")]
    pub priority: u8,
}

fn default_priority() -> u8 {
    1
}

struct Ctx {
    app: AppHandle,
    token: String,
}

pub async fn serve(app: AppHandle, token: String, port: u16) -> Result<()> {
    let ctx = Arc::new(Ctx { app, token });

    let router = Router::new()
        .route("/health", get(|| async { "ok" }))
        .route("/notify", post(notify))
        .with_state(ctx);

    let addr = std::net::SocketAddr::from(([127, 0, 0, 1], port));
    let listener = tokio::net::TcpListener::bind(addr).await?;
    tracing::info!("notification endpoint listening on http://{addr}");
    axum::serve(listener, router).await?;
    Ok(())
}

async fn notify(
    State(ctx): State<Arc<Ctx>>,
    headers: HeaderMap,
    Json(payload): Json<Notification>,
) -> StatusCode {
    let ok = headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .is_some_and(|t| constant_time_eq(t, &ctx.token));

    if !ok {
        return StatusCode::UNAUTHORIZED;
    }

    // The frontend decides how to react — whether to play an animation, queue
    // the bubble, or drop it because the pet is asleep. The transport layer
    // does not make behavioural decisions.
    if let Err(e) = ctx.app.emit(EVENT, &payload) {
        tracing::error!("failed to emit notification: {e}");
        return StatusCode::INTERNAL_SERVER_ERROR;
    }
    StatusCode::ACCEPTED
}

fn constant_time_eq(a: &str, b: &str) -> bool {
    let (a, b) = (a.as_bytes(), b.as_bytes());
    if a.len() != b.len() {
        return false;
    }
    a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

/// Generated once on first run and stored in the config directory.
pub fn generate_token() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let seed = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    // Placeholder. Replace with `rand::rngs::OsRng` before v0.1.0 — this is
    // predictable and only acceptable during local development.
    format!("{seed:032x}")
}
