//! `poc2-capture` — the Hyprland item-capture daemon (ADR-0011).
//!
//! Overlay-style capture for the browser app: a compositor bind
//! (`CTRL+SHIFT+D` by default, see `examples/hyprland/poc2-capture.conf`)
//! hits the loopback trigger endpoint; the daemon injects the game's own
//! Ctrl+C, reads the item text from the Wayland clipboard, and pushes it to
//! the web app over a WebSocket. The browser can't read the clipboard
//! without focus — the push is what makes capture feel like Awakened PoE
//! Trade instead of "alt-tab and paste".
//!
//! Endpoints (loopback only):
//! - `GET /capture[?advanced=1][&mode=ocr]` — run one capture and broadcast.
//! - `GET /ws` — WebSocket event stream (`item-text` / `item-image` /
//!   `capture-error`), origin-checked to localhost.
//! - `GET /healthz` — liveness probe.
//!
//! Subcommands: `serve` (the daemon) and `trigger` (loopback curl
//! replacement for the Hyprland bind, no extra deps needed).

#![forbid(unsafe_code)]

mod capture;

use std::net::{Ipv4Addr, SocketAddr};
use std::sync::Arc;
use std::time::{Duration, Instant};

use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::{Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::IntoResponse;
use axum::routing::get;
use axum::Router;
use base64::Engine;
use clap::{Parser, Subcommand};
use serde::Deserialize;
use tokio::sync::{broadcast, Mutex};
use tracing::{info, warn};

#[derive(Parser, Debug)]
#[command(
    name = "poc2-capture",
    about = "Path of Crafting 2 — Hyprland item-capture daemon (hotkey → Ctrl+C → clipboard → web app)."
)]
struct Cli {
    #[command(subcommand)]
    command: Cmd,
}

#[derive(Subcommand, Debug)]
enum Cmd {
    /// Run the capture daemon (WebSocket + trigger endpoint on loopback).
    Serve {
        /// Port to bind on 127.0.0.1.
        #[arg(long, default_value_t = 17771, env = "POC2_CAPTURE_PORT")]
        port: u16,
    },
    /// Fire one capture against a running daemon (for the Hyprland bind).
    Trigger {
        #[arg(long, default_value_t = 17771, env = "POC2_CAPTURE_PORT")]
        port: u16,
        /// Request advanced mod descriptions (Ctrl+Alt+C).
        #[arg(long, default_value_t = false)]
        advanced: bool,
        /// Screenshot-OCR mode instead of clipboard copy.
        #[arg(long, default_value_t = false)]
        ocr: bool,
    },
}

struct AppState {
    events: broadcast::Sender<String>,
    /// Debounce: ignore triggers that arrive while a capture is in flight
    /// or within 250 ms of the previous one (key-repeat on the bind).
    last_trigger: Mutex<Option<Instant>>,
}

#[derive(Debug, Deserialize)]
struct CaptureParams {
    #[serde(default)]
    advanced: Option<u8>,
    #[serde(default)]
    mode: Option<String>,
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    match Cli::parse().command {
        Cmd::Serve { port } => serve(port).await,
        Cmd::Trigger {
            port,
            advanced,
            ocr,
        } => trigger(port, advanced, ocr).await,
    }
}

async fn serve(port: u16) -> anyhow::Result<()> {
    let (events, _) = broadcast::channel::<String>(16);
    let state = Arc::new(AppState {
        events,
        last_trigger: Mutex::new(None),
    });

    let app = Router::new()
        .route("/healthz", get(|| async { "ok" }))
        .route("/capture", get(capture_handler))
        .route("/ws", get(ws_handler))
        .with_state(state);

    let addr = SocketAddr::from((Ipv4Addr::LOCALHOST, port));
    info!(%addr, "poc2-capture daemon listening (loopback only)");
    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app)
        .with_graceful_shutdown(async {
            let _ = tokio::signal::ctrl_c().await;
        })
        .await?;
    Ok(())
}

/// Loopback origin gate for the WebSocket: browsers send an `Origin` header;
/// only local app origins may subscribe. Non-browser clients (no Origin)
/// are allowed — the socket is already loopback-bound.
fn origin_allowed(headers: &HeaderMap) -> bool {
    match headers.get(axum::http::header::ORIGIN) {
        None => true,
        Some(v) => {
            let Ok(origin) = v.to_str() else {
                return false;
            };
            // The future desktop shell serves the export over app://.
            if origin.starts_with("app://") {
                return true;
            }
            // Strict host check: scheme://HOST[:port] with HOST exactly
            // localhost or 127.0.0.1 (prefix matching would admit
            // `localhost.evil.example`).
            let rest = origin
                .strip_prefix("http://")
                .or_else(|| origin.strip_prefix("https://"));
            let Some(rest) = rest else { return false };
            let host = rest.split(':').next().unwrap_or(rest);
            host == "localhost" || host == "127.0.0.1"
        }
    }
}

async fn ws_handler(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    ws: WebSocketUpgrade,
) -> impl IntoResponse {
    if !origin_allowed(&headers) {
        warn!("rejected WebSocket subscriber with non-local Origin");
        return StatusCode::FORBIDDEN.into_response();
    }
    let rx = state.events.subscribe();
    ws.on_upgrade(move |socket| ws_pump(socket, rx))
}

async fn ws_pump(mut socket: WebSocket, mut rx: broadcast::Receiver<String>) {
    let hello = serde_json::json!({"type": "hello", "version": env!("CARGO_PKG_VERSION")});
    if socket.send(Message::Text(hello.to_string())).await.is_err() {
        return;
    }
    loop {
        tokio::select! {
            ev = rx.recv() => match ev {
                Ok(json) => {
                    if socket.send(Message::Text(json)).await.is_err() {
                        return;
                    }
                }
                Err(broadcast::error::RecvError::Lagged(_)) => continue,
                Err(broadcast::error::RecvError::Closed) => return,
            },
            msg = socket.recv() => match msg {
                // Client pings / closes; we never expect payloads.
                Some(Ok(Message::Close(_))) | None => return,
                Some(Ok(_)) => continue,
                Some(Err(_)) => return,
            },
        }
    }
}

async fn capture_handler(
    State(state): State<Arc<AppState>>,
    Query(params): Query<CaptureParams>,
) -> impl IntoResponse {
    // Debounce key-repeat / double-binds.
    {
        let mut last = state.last_trigger.lock().await;
        let now = Instant::now();
        if let Some(prev) = *last {
            if now.duration_since(prev) < Duration::from_millis(250) {
                return (StatusCode::TOO_MANY_REQUESTS, "debounced\n").into_response();
            }
        }
        *last = Some(now);
    }

    let advanced = params.advanced.unwrap_or(0) == 1;
    let ocr = params.mode.as_deref() == Some("ocr");

    // Dev/E2E hook: `?mode=test` broadcasts a fixture item without touching
    // the keyboard or clipboard. Only honored when the daemon was started
    // with POC2_CAPTURE_TEST=1.
    if params.mode.as_deref() == Some("test") {
        if std::env::var("POC2_CAPTURE_TEST").as_deref() == Ok("1") {
            let fixture = "Item Class: Shields\nRarity: Normal\nEffigial Tower Shield\n--------\nBlock chance: 26%\nArmour: 76\n--------\nRequires: Level 21, 32 Str\n--------\nItem Level: 21\n--------\nGrants Skill: Raise Shield\n";
            let event =
                serde_json::json!({"type": "item-text", "text": fixture, "advanced": false});
            let _ = state.events.send(event.to_string());
            return (StatusCode::OK, "item-text (test fixture)\n").into_response();
        }
        return (StatusCode::FORBIDDEN, "test mode disabled\n").into_response();
    }

    let result = if ocr {
        capture::capture_cursor_region().await
    } else {
        capture::capture_item_text(advanced).await
    };

    let event = match result {
        Ok(capture::Captured::ItemText(text)) => {
            info!(bytes = text.len(), advanced, "item text captured");
            serde_json::json!({"type": "item-text", "text": text, "advanced": advanced})
        }
        Ok(capture::Captured::Image(png)) => {
            info!(
                bytes = png.len(),
                "cursor-region screenshot captured (OCR mode)"
            );
            serde_json::json!({
                "type": "item-image",
                "png_base64": base64::engine::general_purpose::STANDARD.encode(png),
            })
        }
        Err(e) => {
            warn!(error = %e, "capture failed");
            serde_json::json!({"type": "capture-error", "message": e.to_string()})
        }
    };

    let kind = event["type"].as_str().unwrap_or("?").to_string();
    let _ = state.events.send(event.to_string());
    (StatusCode::OK, format!("{kind}\n")).into_response()
}

/// Minimal loopback HTTP GET — keeps the Hyprland bind free of curl.
async fn trigger(port: u16, advanced: bool, ocr: bool) -> anyhow::Result<()> {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let mut path = String::from("/capture?");
    if advanced {
        path.push_str("advanced=1&");
    }
    if ocr {
        path.push_str("mode=ocr&");
    }
    let mut stream = tokio::net::TcpStream::connect((Ipv4Addr::LOCALHOST, port)).await?;
    let req = format!("GET {path} HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nConnection: close\r\n\r\n");
    stream.write_all(req.as_bytes()).await?;
    let mut buf = Vec::new();
    stream.read_to_end(&mut buf).await?;
    let reply = String::from_utf8_lossy(&buf);
    let body = reply.split("\r\n\r\n").nth(1).unwrap_or("").trim();
    println!("{body}");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn headers_with_origin(origin: &str) -> HeaderMap {
        let mut h = HeaderMap::new();
        h.insert(axum::http::header::ORIGIN, origin.parse().unwrap());
        h
    }

    #[test]
    fn ws_origin_gate() {
        assert!(origin_allowed(&HeaderMap::new())); // curl / native clients
        assert!(origin_allowed(&headers_with_origin(
            "http://localhost:3000"
        )));
        assert!(origin_allowed(&headers_with_origin(
            "http://127.0.0.1:3001"
        )));
        assert!(origin_allowed(&headers_with_origin("app://poc2")));
        assert!(!origin_allowed(&headers_with_origin(
            "https://evil.example"
        )));
        assert!(!origin_allowed(&headers_with_origin(
            "http://localhost.evil.example"
        )));
    }
}
