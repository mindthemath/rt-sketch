use std::io::Write;
use std::net::{TcpStream, ToSocketAddrs};
use std::time::Duration;

use crate::engine::canvas::LineSegment;

const MAGIC: &[u8; 4] = b"RTSK";
const MSG_HELLO: u32 = 0;
const MSG_LINE: u32 = 1;
const MSG_RESET: u32 = 2;

/// Streams line segments to a TCP viewer server.
pub struct TcpOutput {
    stream: Option<TcpStream>,
    addr: String,
    name: String,
    canvas_width_cm: f32,
    canvas_height_cm: f32,
    stroke_width_cm: f32,
}

impl TcpOutput {
    pub fn new(
        addr: &str,
        name: &str,
        canvas_width_cm: f64,
        canvas_height_cm: f64,
        stroke_width_cm: f64,
    ) -> Self {
        let mut out = Self {
            stream: None,
            addr: addr.to_string(),
            name: name.to_string(),
            canvas_width_cm: canvas_width_cm as f32,
            canvas_height_cm: canvas_height_cm as f32,
            stroke_width_cm: stroke_width_cm as f32,
        };
        out.try_connect();
        out
    }

    fn try_connect(&mut self) {
        // Resolve hostname — may return multiple addresses (IPv6 + IPv4)
        let addrs: Vec<_> = match self.addr.to_socket_addrs() {
            Ok(addrs) => addrs.collect(),
            Err(e) => {
                tracing::warn!("invalid viewer address '{}': {}", self.addr, e);
                return;
            }
        };
        if addrs.is_empty() {
            tracing::warn!("could not resolve viewer address: {}", self.addr);
            return;
        }

        // Try each resolved address (IPv4 usually works when server binds 0.0.0.0)
        for addr in &addrs {
            match TcpStream::connect_timeout(addr, Duration::from_millis(500)) {
                Ok(stream) => {
                    stream.set_nodelay(true).ok();
                    self.stream = Some(stream);
                    if let Err(e) = self.send_hello() {
                        tracing::warn!("failed to send HELLO: {}", e);
                        self.stream = None;
                    } else {
                        tracing::info!("connected to viewer at {} ({})", self.addr, addr);
                    }
                    return;
                }
                Err(_) => continue,
            }
        }
        tracing::warn!(
            "could not connect to viewer at {} (tried {} addrs)",
            self.addr,
            addrs.len()
        );
    }

    fn send_hello(&mut self) -> std::io::Result<()> {
        let name_bytes = self.name.as_bytes();
        let payload_len = 2 + name_bytes.len() + 12; // u16 name_len + name + 3x f32

        let stream = self.stream.as_mut().unwrap();
        // Header
        stream.write_all(MAGIC)?;
        stream.write_all(&MSG_HELLO.to_le_bytes())?;
        stream.write_all(&(payload_len as u32).to_le_bytes())?;
        // Payload
        stream.write_all(&(name_bytes.len() as u16).to_le_bytes())?;
        stream.write_all(name_bytes)?;
        stream.write_all(&self.canvas_width_cm.to_le_bytes())?;
        stream.write_all(&self.canvas_height_cm.to_le_bytes())?;
        stream.write_all(&self.stroke_width_cm.to_le_bytes())?;
        stream.flush()?;
        Ok(())
    }

    pub fn is_connected(&self) -> bool {
        self.stream.is_some()
    }

    /// Block until a connection to the viewer is established, retrying every second.
    pub fn wait_for_connection(&mut self) {
        while self.stream.is_none() {
            std::thread::sleep(Duration::from_secs(1));
            self.try_connect();
        }
    }

    pub fn send_line(&mut self, line: &LineSegment) {
        if self.stream.is_none() {
            self.try_connect();
        }
        let Some(ref mut stream) = self.stream else {
            return;
        };

        let mut buf = [0u8; 12 + 20]; // header + 5x f32
        buf[0..4].copy_from_slice(MAGIC);
        buf[4..8].copy_from_slice(&MSG_LINE.to_le_bytes());
        buf[8..12].copy_from_slice(&20u32.to_le_bytes());
        buf[12..16].copy_from_slice(&(line.x1 as f32).to_le_bytes());
        buf[16..20].copy_from_slice(&(line.y1 as f32).to_le_bytes());
        buf[20..24].copy_from_slice(&(line.x2 as f32).to_le_bytes());
        buf[24..28].copy_from_slice(&(line.y2 as f32).to_le_bytes());
        buf[28..32].copy_from_slice(&(line.width as f32).to_le_bytes());

        if let Err(e) = stream.write_all(&buf) {
            tracing::warn!("TCP write failed: {}, will reconnect", e);
            self.stream = None;
        }
    }

    pub fn send_reset(&mut self) {
        if let Some(ref mut stream) = self.stream {
            let mut buf = [0u8; 12];
            buf[0..4].copy_from_slice(MAGIC);
            buf[4..8].copy_from_slice(&MSG_RESET.to_le_bytes());
            buf[8..12].copy_from_slice(&0u32.to_le_bytes());

            if let Err(e) = stream.write_all(&buf) {
                tracing::warn!("TCP write failed: {}, will reconnect", e);
                self.stream = None;
            }
        }
    }
}

impl Drop for TcpOutput {
    fn drop(&mut self) {
        if let Some(ref mut stream) = self.stream {
            stream.flush().ok();
        }
    }
}
