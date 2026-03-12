use std::sync::{Arc, Mutex};

use axum::{
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        State,
    },
    response::{Html, IntoResponse, Response},
    routing::get,
    Router,
};
use base64::Engine as _;
use serde::{Deserialize, Serialize};
use tokio::sync::broadcast;

use crate::config::Config;
use crate::engine::canvas::Canvas;

/// Shared application state.
pub struct AppState {
    pub config: Mutex<Config>,
    pub canvas: Mutex<Canvas>,
    pub target_frame: Mutex<Option<Vec<u8>>>,
    pub iteration: Mutex<u64>,
    pub current_score: Mutex<f64>,
    pub running: Mutex<bool>,
    pub update_tx: broadcast::Sender<UpdateMessage>,
    /// Channel for control commands from the web UI.
    pub control_tx: tokio::sync::mpsc::Sender<ControlCommand>,
}

#[derive(Debug, Clone, Serialize)]
pub struct UpdateMessage {
    #[serde(rename = "type")]
    pub msg_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub canvas_png: Option<String>, // base64
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target_png: Option<String>, // base64
    #[serde(skip_serializing_if = "Option::is_none")]
    pub preview_png: Option<String>, // base64, full-res preview
    #[serde(skip_serializing_if = "Option::is_none")]
    pub iteration: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub score: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fps: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub k: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub line_count: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub running: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_line_len: Option<f64>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ControlCommand {
    pub command: String,
    #[serde(default)]
    pub value: Option<serde_json::Value>,
}

/// Build the axum router.
pub fn build_router(state: Arc<AppState>) -> Router {
    Router::new()
        .route("/", get(index_handler))
        .route("/app.js", get(js_handler))
        .route("/style.css", get(css_handler))
        .route("/ws", get(ws_handler))
        .route("/svg", get(svg_handler))
        .with_state(state)
}

async fn index_handler() -> Html<&'static str> {
    Html(include_str!("static/index.html"))
}

async fn js_handler() -> Response {
    (
        [("content-type", "application/javascript")],
        include_str!("static/app.js"),
    )
        .into_response()
}

async fn css_handler() -> Response {
    (
        [("content-type", "text/css")],
        include_str!("static/style.css"),
    )
        .into_response()
}

async fn svg_handler(State(state): State<Arc<AppState>>) -> Response {
    let svg = state.canvas.lock().unwrap().to_svg();
    let line_count = state.canvas.lock().unwrap().lines.len();
    let mse = *state.current_score.lock().unwrap();
    let disposition = format!(
        "attachment; filename=\"rt-sketch_{}lines_{:.6}mse.svg\"",
        line_count, mse
    );
    (
        [
            ("content-type", "image/svg+xml".to_string()),
            ("content-disposition", disposition),
        ],
        svg,
    )
        .into_response()
}

async fn ws_handler(ws: WebSocketUpgrade, State(state): State<Arc<AppState>>) -> Response {
    ws.on_upgrade(|socket| handle_ws(socket, state))
}

async fn handle_ws(mut socket: WebSocket, state: Arc<AppState>) {
    // Send initial state — extract all data before awaiting
    let init_json = {
        let config = state.config.lock().unwrap();
        let running = *state.running.lock().unwrap();
        let iteration = *state.iteration.lock().unwrap();
        let score = *state.current_score.lock().unwrap();
        let line_count = state.canvas.lock().unwrap().lines.len();

        let target_png = state.target_frame.lock().unwrap().as_ref().map(|frame| {
            let pw = config.processing_width();
            let ph = config.resolution;
            gray_to_base64_png(frame, pw, ph)
        });

        let init = UpdateMessage {
            msg_type: "init".to_string(),
            canvas_png: None,
            target_png: target_png,
            preview_png: None,
            iteration: Some(iteration),
            score: Some(score),
            fps: Some(config.fps),
            k: Some(config.k),
            line_count: Some(line_count),
            running: Some(running),
            last_line_len: None,
        };

        serde_json::to_string(&init).ok()
    };

    if let Some(json) = init_json {
        let _ = socket.send(Message::Text(json.into())).await;
    }

    let mut update_rx = state.update_tx.subscribe();

    loop {
        tokio::select! {
            // Forward engine updates to the client
            Ok(update) = update_rx.recv() => {
                if let Ok(json) = serde_json::to_string(&update) {
                    if socket.send(Message::Text(json.into())).await.is_err() {
                        break;
                    }
                }
            }
            // Receive control commands from the client
            Some(Ok(msg)) = socket.recv() => {
                if let Message::Text(text) = msg {
                    if let Ok(cmd) = serde_json::from_str::<ControlCommand>(&text) {
                        let _ = state.control_tx.send(cmd).await;
                    }
                }
            }
            else => break,
        }
    }
}

/// Encode a grayscale buffer as a PNG, then base64-encode it.
pub fn gray_to_base64_png(data: &[u8], width: u32, height: u32) -> String {
    let img =
        image::GrayImage::from_raw(width, height, data.to_vec()).expect("valid image dimensions");
    let mut buf = std::io::Cursor::new(Vec::new());
    img.write_to(&mut buf, image::ImageFormat::Png)
        .expect("PNG encoding");
    base64::engine::general_purpose::STANDARD.encode(buf.into_inner())
}
