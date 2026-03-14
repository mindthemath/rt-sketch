use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use serde::Serialize;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::broadcast;

use rt_protocol::{
    build_cmd, parse_header, CMD_PAUSE, CMD_PLAY, CMD_RESET_ALL, HEADER_SIZE, MSG_HELLO, MSG_LINE,
    MSG_RESET, MSG_STATE,
};

/// A line segment in canvas coordinates (cm).
#[derive(Clone, Serialize)]
pub struct Line {
    pub x1: f32,
    pub y1: f32,
    pub x2: f32,
    pub y2: f32,
    pub width: f32,
}

/// Info about a connected rt-sketch instance.
pub struct InstanceInfo {
    pub name: String,
    pub canvas_width_cm: f32,
    pub canvas_height_cm: f32,
    pub stroke_width_cm: f32,
    pub lines: Vec<Line>,
    pub paused: bool,
}

/// Events broadcast to WebSocket clients.
#[derive(Clone, Serialize)]
#[serde(tag = "type")]
pub enum ViewerEvent {
    #[serde(rename = "connect")]
    Connect {
        name: String,
        width_cm: f32,
        height_cm: f32,
        stroke_width_cm: f32,
        paused: bool,
    },
    #[serde(rename = "line")]
    Line {
        name: String,
        x1: f32,
        y1: f32,
        x2: f32,
        y2: f32,
        width: f32,
    },
    #[serde(rename = "reset")]
    Reset { name: String },
    #[serde(rename = "disconnect")]
    Disconnect { name: String },
    #[serde(rename = "state")]
    State { name: String, paused: bool },
}

/// Control commands sent from the viewer to workers.
/// `target` is None for global (all workers) or Some(name) for a specific instance.
#[derive(Clone, Debug)]
pub struct ControlCmd {
    pub command: ControlAction,
    pub target: Option<String>,
}

#[derive(Clone, Debug)]
pub enum ControlAction {
    Play,
    Pause,
    Reset,
}

/// Shared viewer state.
pub struct ViewerState {
    pub instances: Mutex<HashMap<String, InstanceInfo>>,
    pub event_tx: broadcast::Sender<ViewerEvent>,
    pub control_tx: broadcast::Sender<ControlCmd>,
    pub read_only: bool,
}

pub async fn accept_loop(listener: TcpListener, state: Arc<ViewerState>) {
    loop {
        match listener.accept().await {
            Ok((stream, addr)) => {
                tracing::info!("TCP connection from {}", addr);
                let state = state.clone();
                tokio::spawn(async move {
                    if let Err(e) = handle_connection(stream, state).await {
                        let msg = e.to_string();
                        if !msg.contains("eof") {
                            tracing::warn!("TCP connection error: {}", e);
                        }
                    }
                });
            }
            Err(e) => {
                tracing::warn!("TCP accept error: {}", e);
            }
        }
    }
}

async fn handle_connection(
    mut stream: TcpStream,
    state: Arc<ViewerState>,
) -> Result<(), Box<dyn std::error::Error>> {
    // Read HELLO message first
    let header = read_header(&mut stream).await?;
    if header.msg_type != MSG_HELLO {
        return Err("expected HELLO as first message".into());
    }

    let mut payload = vec![0u8; header.payload_len as usize];
    stream.read_exact(&mut payload).await?;

    let name_len = u16::from_le_bytes([payload[0], payload[1]]) as usize;
    let name = String::from_utf8_lossy(&payload[2..2 + name_len]).to_string();
    let offset = 2 + name_len;
    let canvas_width_cm = f32::from_le_bytes(payload[offset..offset + 4].try_into()?);
    let canvas_height_cm = f32::from_le_bytes(payload[offset + 4..offset + 8].try_into()?);
    let stroke_width_cm = f32::from_le_bytes(payload[offset + 8..offset + 12].try_into()?);
    // Running flag is optional for backwards compatibility with older workers
    let running = payload.get(offset + 12).copied().unwrap_or(0) != 0;

    tracing::info!(
        "instance \"{}\" connected: {:.1}x{:.1} cm, stroke {:.3} cm, running: {}",
        name,
        canvas_width_cm,
        canvas_height_cm,
        stroke_width_cm,
        running
    );

    // Register instance
    {
        let mut instances = state.instances.lock().unwrap();
        instances.insert(
            name.clone(),
            InstanceInfo {
                name: name.clone(),
                canvas_width_cm,
                canvas_height_cm,
                stroke_width_cm,
                lines: Vec::new(),
                paused: !running,
            },
        );
    }

    // Broadcast connect event
    let _ = state.event_tx.send(ViewerEvent::Connect {
        name: name.clone(),
        width_cm: canvas_width_cm,
        height_cm: canvas_height_cm,
        stroke_width_cm,
        paused: !running,
    });

    // Split stream for bidirectional communication
    let (mut read_half, mut write_half) = stream.into_split();

    // Subscribe to control commands
    let mut control_rx = state.control_tx.subscribe();

    // Run read loop and control forwarding concurrently
    let conn_name = name.clone();
    let result = tokio::select! {
        r = message_loop(&mut read_half, &state, &name) => r,
        _ = async {
            loop {
                match control_rx.recv().await {
                    Ok(cmd) => {
                        // Filter: global (None) or targeted at this instance
                        if let Some(ref target) = cmd.target {
                            if target != &conn_name {
                                continue;
                            }
                        }
                        let msg_type = match cmd.command {
                            ControlAction::Play => CMD_PLAY,
                            ControlAction::Pause => CMD_PAUSE,
                            ControlAction::Reset => CMD_RESET_ALL,
                        };
                        let buf = build_cmd(msg_type);
                        if write_half.write_all(&buf).await.is_err() {
                            break;
                        }
                    }
                    Err(broadcast::error::RecvError::Lagged(_)) => continue,
                    Err(broadcast::error::RecvError::Closed) => break,
                }
            }
        } => Ok(()),
    };

    // Clean up on disconnect
    tracing::info!("instance \"{}\" disconnected", name);
    {
        let mut instances = state.instances.lock().unwrap();
        instances.remove(&name);
    }
    let _ = state.event_tx.send(ViewerEvent::Disconnect { name });

    result
}

async fn message_loop(
    stream: &mut tokio::net::tcp::OwnedReadHalf,
    state: &Arc<ViewerState>,
    name: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    loop {
        let header = read_header_from(stream).await?;

        match header.msg_type {
            MSG_LINE => {
                let mut payload = [0u8; 20];
                stream.read_exact(&mut payload).await?;

                let x1 = f32::from_le_bytes(payload[0..4].try_into()?);
                let y1 = f32::from_le_bytes(payload[4..8].try_into()?);
                let x2 = f32::from_le_bytes(payload[8..12].try_into()?);
                let y2 = f32::from_le_bytes(payload[12..16].try_into()?);
                let width = f32::from_le_bytes(payload[16..20].try_into()?);

                // Store line for replay
                {
                    let mut instances = state.instances.lock().unwrap();
                    if let Some(instance) = instances.get_mut(name) {
                        instance.lines.push(Line {
                            x1,
                            y1,
                            x2,
                            y2,
                            width,
                        });
                    }
                }

                let _ = state.event_tx.send(ViewerEvent::Line {
                    name: name.to_string(),
                    x1,
                    y1,
                    x2,
                    y2,
                    width,
                });
            }
            MSG_RESET => {
                if header.payload_len > 0 {
                    let mut discard = vec![0u8; header.payload_len as usize];
                    stream.read_exact(&mut discard).await?;
                }

                // Clear stored lines
                {
                    let mut instances = state.instances.lock().unwrap();
                    if let Some(instance) = instances.get_mut(name) {
                        instance.lines.clear();
                    }
                }

                let _ = state.event_tx.send(ViewerEvent::Reset {
                    name: name.to_string(),
                });
            }
            MSG_STATE => {
                let mut payload = [0u8; 1];
                stream.read_exact(&mut payload).await?;
                let paused = payload[0] == 0;

                {
                    let mut instances = state.instances.lock().unwrap();
                    if let Some(instance) = instances.get_mut(name) {
                        instance.paused = paused;
                    }
                }

                let _ = state.event_tx.send(ViewerEvent::State {
                    name: name.to_string(),
                    paused,
                });
            }
            _ => {
                // Skip unknown message types
                if header.payload_len > 0 {
                    let mut discard = vec![0u8; header.payload_len as usize];
                    stream.read_exact(&mut discard).await?;
                }
            }
        }
    }
}

use rt_protocol::Header;

async fn read_header(stream: &mut TcpStream) -> Result<Header, Box<dyn std::error::Error>> {
    let mut buf = [0u8; HEADER_SIZE];
    stream.read_exact(&mut buf).await?;
    parse_header(&buf).map_err(|e| e.into())
}

async fn read_header_from(
    stream: &mut tokio::net::tcp::OwnedReadHalf,
) -> Result<Header, Box<dyn std::error::Error>> {
    let mut buf = [0u8; HEADER_SIZE];
    stream.read_exact(&mut buf).await?;
    parse_header(&buf).map_err(|e| e.into())
}
