use std::io::{Read, Write};
use std::net::{TcpStream, ToSocketAddrs};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use crate::engine::canvas::LineSegment;

use rt_protocol::{
    build_cmd, build_header, parse_header, CMD_PAUSE, CMD_PLAY, CMD_RESET_ALL, HEADER_SIZE,
    MSG_HELLO, MSG_LINE, MSG_RESET, MSG_STATE,
};

/// Commands received from the viewer.
#[derive(Debug, Clone, PartialEq)]
pub enum ViewerCommand {
    Play,
    Pause,
    Reset,
}

/// Streams line segments to a TCP viewer server.
pub struct TcpOutput {
    stream: Option<TcpStream>,
    addr: String,
    name: String,
    canvas_width_cm: f32,
    canvas_height_cm: f32,
    stroke_width_cm: f32,
    running: Arc<AtomicBool>,
    shutdown: Arc<AtomicBool>,
    last_connect_attempt: Option<Instant>,
    reconnect_interval: Duration,
}

const MIN_RECONNECT_INTERVAL: Duration = Duration::from_millis(500);
const MAX_RECONNECT_INTERVAL: Duration = Duration::from_secs(3);

impl TcpOutput {
    pub fn new(
        addr: &str,
        name: &str,
        canvas_width_cm: f64,
        canvas_height_cm: f64,
        stroke_width_cm: f64,
        running: Arc<AtomicBool>,
        shutdown: Arc<AtomicBool>,
    ) -> Self {
        let mut out = Self {
            stream: None,
            addr: addr.to_string(),
            name: name.to_string(),
            canvas_width_cm: canvas_width_cm as f32,
            canvas_height_cm: canvas_height_cm as f32,
            stroke_width_cm: stroke_width_cm as f32,
            running,
            shutdown,
            last_connect_attempt: None,
            reconnect_interval: MIN_RECONNECT_INTERVAL,
        };
        out.try_connect();
        out
    }

    /// Attempt reconnect only if enough time has passed since the last attempt.
    fn try_reconnect(&mut self) {
        if let Some(last) = self.last_connect_attempt {
            if last.elapsed() < self.reconnect_interval {
                return;
            }
        }
        self.try_connect();
    }

    fn try_connect(&mut self) {
        self.last_connect_attempt = Some(Instant::now());

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
                        self.reconnect_interval = MIN_RECONNECT_INTERVAL;
                    }
                    return;
                }
                Err(_) => continue,
            }
        }
        // Back off on failure
        tracing::warn!(
            "could not connect to viewer at {} (retrying in {:.1}s)",
            self.addr,
            self.reconnect_interval.as_secs_f64()
        );
        self.reconnect_interval = (self.reconnect_interval * 2).min(MAX_RECONNECT_INTERVAL);
    }

    fn send_hello(&mut self) -> std::io::Result<()> {
        let name_bytes = self.name.as_bytes();
        let payload_len = 2 + name_bytes.len() + 12 + 1; // u16 name_len + name + 3x f32 + u8 running

        let stream = self.stream.as_mut().unwrap();
        stream.write_all(&build_header(MSG_HELLO, payload_len as u32))?;
        // Payload
        stream.write_all(&(name_bytes.len() as u16).to_le_bytes())?;
        stream.write_all(name_bytes)?;
        stream.write_all(&self.canvas_width_cm.to_le_bytes())?;
        stream.write_all(&self.canvas_height_cm.to_le_bytes())?;
        stream.write_all(&self.stroke_width_cm.to_le_bytes())?;
        let running = if self.running.load(Ordering::Relaxed) {
            1u8
        } else {
            0u8
        };
        stream.write_all(&[running])?;
        stream.flush()?;
        Ok(())
    }

    pub fn is_connected(&self) -> bool {
        self.stream.is_some()
    }

    /// Block until a connection to the viewer is established, retrying every second.
    /// Returns false if shutdown was requested before connecting.
    pub fn wait_for_connection(&mut self) -> bool {
        while self.stream.is_none() {
            if self.shutdown.load(Ordering::Relaxed) {
                return false;
            }
            std::thread::sleep(Duration::from_millis(500));
            self.try_connect();
        }
        true
    }

    pub fn send_line(&mut self, line: &LineSegment) {
        if self.stream.is_none() {
            self.try_reconnect();
        }
        let Some(ref mut stream) = self.stream else {
            return;
        };

        let mut buf = [0u8; HEADER_SIZE + 20]; // header + 5x f32
        buf[0..HEADER_SIZE].copy_from_slice(&build_header(MSG_LINE, 20));
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

    /// Non-blocking poll for commands from the viewer.
    /// Also attempts to reconnect if disconnected (with backoff).
    pub fn poll_commands(&mut self) -> Vec<ViewerCommand> {
        let mut cmds = Vec::new();
        if self.stream.is_none() {
            self.try_reconnect();
        }
        let Some(ref mut stream) = self.stream else {
            return cmds;
        };

        // Temporarily set non-blocking to poll
        stream.set_nonblocking(true).ok();
        let mut buf = [0u8; HEADER_SIZE];
        loop {
            match stream.read_exact(&mut buf) {
                Ok(()) => {
                    let header = match parse_header(&buf) {
                        Ok(h) => h,
                        Err(_) => continue,
                    };
                    if header.payload_len > 0 {
                        let mut discard = vec![0u8; header.payload_len as usize];
                        let _ = stream.read_exact(&mut discard);
                    }
                    match header.msg_type {
                        CMD_PLAY => cmds.push(ViewerCommand::Play),
                        CMD_PAUSE => cmds.push(ViewerCommand::Pause),
                        CMD_RESET_ALL => cmds.push(ViewerCommand::Reset),
                        _ => {}
                    }
                }
                Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => break,
                Err(_) => {
                    self.stream = None;
                    break;
                }
            }
        }
        // Restore blocking mode
        if let Some(ref stream) = self.stream {
            stream.set_nonblocking(false).ok();
        }
        cmds
    }

    pub fn send_state(&mut self, running: bool) {
        if let Some(ref mut stream) = self.stream {
            let mut buf = [0u8; HEADER_SIZE + 1];
            buf[0..HEADER_SIZE].copy_from_slice(&build_header(MSG_STATE, 1));
            buf[HEADER_SIZE] = if running { 1 } else { 0 };
            if let Err(e) = stream.write_all(&buf) {
                tracing::warn!("TCP write failed: {}, will reconnect", e);
                self.stream = None;
            }
        }
    }

    pub fn send_reset(&mut self) {
        if let Some(ref mut stream) = self.stream {
            if let Err(e) = stream.write_all(&build_cmd(MSG_RESET)) {
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
