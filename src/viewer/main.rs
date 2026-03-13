mod tcp_server;

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use axum::extract::ws::{Message, WebSocket};
use axum::extract::{State, WebSocketUpgrade};
use axum::response::Html;
use axum::routing::get;
use axum::Router;
use clap::Parser;
use tokio::sync::broadcast;

use tcp_server::{ViewerEvent, ViewerState};

#[derive(Parser, Debug)]
#[command(name = "rt-viewer", about = "Multi-instance viewer for rt-sketch")]
struct Args {
    /// TCP port for rt-sketch instances to connect to
    #[arg(long, default_value_t = 9900)]
    tcp_port: u16,

    /// Web UI port for the viewer page
    #[arg(long, default_value_t = 9901)]
    web_port: u16,
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "rt_viewer=info".parse().unwrap()),
        )
        .init();

    let args = Args::parse();

    let (event_tx, _) = broadcast::channel::<ViewerEvent>(256);

    let state = Arc::new(ViewerState {
        instances: Mutex::new(HashMap::new()),
        event_tx,
    });

    // Start TCP listener for rt-sketch instances
    let tcp_listener = tokio::net::TcpListener::bind(format!("0.0.0.0:{}", args.tcp_port))
        .await
        .unwrap_or_else(|e| {
            eprintln!("error: cannot bind TCP port {}: {}", args.tcp_port, e);
            std::process::exit(1);
        });
    tracing::info!("TCP listener on port {}", args.tcp_port);

    let tcp_state = state.clone();
    tokio::spawn(async move {
        tcp_server::accept_loop(tcp_listener, tcp_state).await;
    });

    // Start web server
    let web_listener = tokio::net::TcpListener::bind(format!("0.0.0.0:{}", args.web_port))
        .await
        .unwrap_or_else(|e| {
            eprintln!("error: cannot bind web port {}: {}", args.web_port, e);
            std::process::exit(1);
        });
    tracing::info!("viewer UI: http://localhost:{}", args.web_port);

    let app = Router::new()
        .route("/", get(serve_html))
        .route("/viewer.js", get(serve_js))
        .route("/viewer.css", get(serve_css))
        .route("/ws", get(ws_handler))
        .with_state(state);

    axum::serve(web_listener, app).await.unwrap();
}

async fn serve_html() -> Html<&'static str> {
    Html(include_str!("static/viewer.html"))
}

async fn serve_js() -> ([(axum::http::header::HeaderName, &'static str); 1], &'static str) {
    (
        [(axum::http::header::CONTENT_TYPE, "application/javascript")],
        include_str!("static/viewer.js"),
    )
}

async fn serve_css() -> ([(axum::http::header::HeaderName, &'static str); 1], &'static str) {
    (
        [(axum::http::header::CONTENT_TYPE, "text/css")],
        include_str!("static/viewer.css"),
    )
}

async fn ws_handler(
    ws: WebSocketUpgrade,
    State(state): State<Arc<ViewerState>>,
) -> axum::response::Response {
    ws.on_upgrade(move |socket| handle_ws(socket, state))
}

async fn handle_ws(mut socket: WebSocket, state: Arc<ViewerState>) {
    // Send init message with all current instances and their lines
    let init_data = {
        let instances = state.instances.lock().unwrap();
        let init: Vec<serde_json::Value> = instances
            .values()
            .map(|inst| {
                let lines: Vec<serde_json::Value> = inst
                    .lines
                    .iter()
                    .map(|l| {
                        serde_json::json!({
                            "x1": l.x1, "y1": l.y1,
                            "x2": l.x2, "y2": l.y2,
                            "width": l.width
                        })
                    })
                    .collect();
                serde_json::json!({
                    "name": inst.name,
                    "width_cm": inst.canvas_width_cm,
                    "height_cm": inst.canvas_height_cm,
                    "stroke_width_cm": inst.stroke_width_cm,
                    "lines": lines
                })
            })
            .collect();
        serde_json::json!({ "type": "init", "instances": init })
    };

    if socket
        .send(Message::Text(init_data.to_string().into()))
        .await
        .is_err()
    {
        return;
    }

    // Forward events to this WebSocket client
    let mut event_rx = state.event_tx.subscribe();
    loop {
        match event_rx.recv().await {
            Ok(event) => {
                let json = serde_json::to_string(&event).unwrap();
                if socket.send(Message::Text(json.into())).await.is_err() {
                    break;
                }
            }
            Err(broadcast::error::RecvError::Lagged(n)) => {
                tracing::warn!("WebSocket client lagged, skipped {} events", n);
            }
            Err(broadcast::error::RecvError::Closed) => break,
        }
    }
}
