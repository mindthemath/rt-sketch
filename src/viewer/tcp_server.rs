use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use serde::Serialize;
use tokio::io::AsyncReadExt;
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::broadcast;

const MAGIC: &[u8; 4] = b"RTSK";
const MSG_HELLO: u32 = 0;
const MSG_LINE: u32 = 1;
const MSG_RESET: u32 = 2;

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
}

/// Shared viewer state.
pub struct ViewerState {
    pub instances: Mutex<HashMap<String, InstanceInfo>>,
    pub event_tx: broadcast::Sender<ViewerEvent>,
}

pub async fn accept_loop(listener: TcpListener, state: Arc<ViewerState>) {
    loop {
        match listener.accept().await {
            Ok((stream, addr)) => {
                tracing::info!("TCP connection from {}", addr);
                let state = state.clone();
                tokio::spawn(async move {
                    if let Err(e) = handle_connection(stream, state).await {
                        tracing::warn!("TCP connection error: {}", e);
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

    tracing::info!(
        "instance \"{}\" connected: {:.1}x{:.1} cm, stroke {:.3} cm",
        name,
        canvas_width_cm,
        canvas_height_cm,
        stroke_width_cm
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
            },
        );
    }

    // Broadcast connect event
    let _ = state.event_tx.send(ViewerEvent::Connect {
        name: name.clone(),
        width_cm: canvas_width_cm,
        height_cm: canvas_height_cm,
        stroke_width_cm,
    });

    // Read messages in a loop
    let result = message_loop(&mut stream, &state, &name).await;

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
    stream: &mut TcpStream,
    state: &Arc<ViewerState>,
    name: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    loop {
        let header = read_header(stream).await?;

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

struct Header {
    msg_type: u32,
    payload_len: u32,
}

async fn read_header(stream: &mut TcpStream) -> Result<Header, Box<dyn std::error::Error>> {
    let mut buf = [0u8; 12];
    stream.read_exact(&mut buf).await?;

    if &buf[0..4] != MAGIC {
        return Err("invalid magic bytes".into());
    }

    Ok(Header {
        msg_type: u32::from_le_bytes(buf[4..8].try_into()?),
        payload_len: u32::from_le_bytes(buf[8..12].try_into()?),
    })
}
