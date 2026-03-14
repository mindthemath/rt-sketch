/// Binary protocol shared between rt-sketch and rt-viewer.
///
/// Wire format: 4-byte magic "RTSK" + u32 msg_type (LE) + u32 payload_len (LE) = 12-byte header.

pub const MAGIC: &[u8; 4] = b"RTSK";
pub const HEADER_SIZE: usize = 12;

// Messages sent from worker -> viewer
pub const MSG_HELLO: u32 = 0;
pub const MSG_LINE: u32 = 1;
pub const MSG_RESET: u32 = 2;
pub const MSG_STATE: u32 = 6; // payload: 1 byte (0 = paused, 1 = running)

// Commands sent from viewer -> worker
pub const CMD_PLAY: u32 = 3;
pub const CMD_PAUSE: u32 = 4;
pub const CMD_RESET_ALL: u32 = 5;

pub struct Header {
    pub msg_type: u32,
    pub payload_len: u32,
}

/// Parse a 12-byte buffer into a Header, validating the magic bytes.
pub fn parse_header(buf: &[u8; HEADER_SIZE]) -> Result<Header, &'static str> {
    if &buf[0..4] != MAGIC {
        return Err("invalid magic bytes");
    }
    Ok(Header {
        msg_type: u32::from_le_bytes([buf[4], buf[5], buf[6], buf[7]]),
        payload_len: u32::from_le_bytes([buf[8], buf[9], buf[10], buf[11]]),
    })
}

/// Build a 12-byte header for a message with the given type and payload length.
pub fn build_header(msg_type: u32, payload_len: u32) -> [u8; HEADER_SIZE] {
    let mut buf = [0u8; HEADER_SIZE];
    buf[0..4].copy_from_slice(MAGIC);
    buf[4..8].copy_from_slice(&msg_type.to_le_bytes());
    buf[8..12].copy_from_slice(&payload_len.to_le_bytes());
    buf
}

/// Build a 12-byte command message (zero-length payload).
pub fn build_cmd(msg_type: u32) -> [u8; HEADER_SIZE] {
    build_header(msg_type, 0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_header() {
        let header = build_header(MSG_LINE, 42);
        let parsed = parse_header(&header).unwrap();
        assert_eq!(parsed.msg_type, MSG_LINE);
        assert_eq!(parsed.payload_len, 42);
    }

    #[test]
    fn build_cmd_has_zero_payload() {
        let header = build_cmd(CMD_PAUSE);
        let parsed = parse_header(&header).unwrap();
        assert_eq!(parsed.msg_type, CMD_PAUSE);
        assert_eq!(parsed.payload_len, 0);
    }

    #[test]
    fn invalid_magic_rejected() {
        let mut buf = build_header(MSG_HELLO, 0);
        buf[0] = b'X';
        assert!(parse_header(&buf).is_err());
    }

    #[test]
    fn all_message_types_round_trip() {
        for &msg in &[MSG_HELLO, MSG_LINE, MSG_RESET, MSG_STATE, CMD_PLAY, CMD_PAUSE, CMD_RESET_ALL] {
            let header = build_header(msg, 100);
            let parsed = parse_header(&header).unwrap();
            assert_eq!(parsed.msg_type, msg);
        }
    }

    /// Serialize a MSG_LINE payload the same way tcp_output.rs does.
    fn encode_line(x1: f32, y1: f32, x2: f32, y2: f32, width: f32) -> [u8; HEADER_SIZE + 20] {
        let mut buf = [0u8; HEADER_SIZE + 20];
        buf[0..HEADER_SIZE].copy_from_slice(&build_header(MSG_LINE, 20));
        buf[12..16].copy_from_slice(&x1.to_le_bytes());
        buf[16..20].copy_from_slice(&y1.to_le_bytes());
        buf[20..24].copy_from_slice(&x2.to_le_bytes());
        buf[24..28].copy_from_slice(&y2.to_le_bytes());
        buf[28..32].copy_from_slice(&width.to_le_bytes());
        buf
    }

    /// Deserialize a MSG_LINE payload the same way tcp_server.rs does.
    fn decode_line(payload: &[u8; 20]) -> (f32, f32, f32, f32, f32) {
        let x1 = f32::from_le_bytes(payload[0..4].try_into().unwrap());
        let y1 = f32::from_le_bytes(payload[4..8].try_into().unwrap());
        let x2 = f32::from_le_bytes(payload[8..12].try_into().unwrap());
        let y2 = f32::from_le_bytes(payload[12..16].try_into().unwrap());
        let width = f32::from_le_bytes(payload[16..20].try_into().unwrap());
        (x1, y1, x2, y2, width)
    }

    #[test]
    fn line_payload_round_trip() {
        let buf = encode_line(1.5, -2.25, 3.0, 4.75, 0.1);

        // Verify header
        let header = parse_header(buf[0..12].try_into().unwrap()).unwrap();
        assert_eq!(header.msg_type, MSG_LINE);
        assert_eq!(header.payload_len, 20);

        // Verify payload round-trips
        let payload: &[u8; 20] = buf[12..32].try_into().unwrap();
        let (x1, y1, x2, y2, width) = decode_line(payload);
        assert_eq!(x1, 1.5);
        assert_eq!(y1, -2.25);
        assert_eq!(x2, 3.0);
        assert_eq!(y2, 4.75);
        assert_eq!(width, 0.1);
    }

    /// Serialize a MSG_HELLO payload the same way tcp_output.rs does.
    fn encode_hello(name: &str, canvas_w: f32, canvas_h: f32, stroke_w: f32, running: bool) -> Vec<u8> {
        let name_bytes = name.as_bytes();
        let payload_len = 2 + name_bytes.len() + 12 + 1;
        let mut buf = Vec::with_capacity(HEADER_SIZE + payload_len);
        buf.extend_from_slice(&build_header(MSG_HELLO, payload_len as u32));
        buf.extend_from_slice(&(name_bytes.len() as u16).to_le_bytes());
        buf.extend_from_slice(name_bytes);
        buf.extend_from_slice(&canvas_w.to_le_bytes());
        buf.extend_from_slice(&canvas_h.to_le_bytes());
        buf.extend_from_slice(&stroke_w.to_le_bytes());
        buf.push(if running { 1 } else { 0 });
        buf
    }

    /// Deserialize a MSG_HELLO payload the same way tcp_server.rs does.
    fn decode_hello(payload: &[u8]) -> (String, f32, f32, f32, bool) {
        let name_len = u16::from_le_bytes([payload[0], payload[1]]) as usize;
        let name = String::from_utf8_lossy(&payload[2..2 + name_len]).to_string();
        let offset = 2 + name_len;
        let canvas_w = f32::from_le_bytes(payload[offset..offset + 4].try_into().unwrap());
        let canvas_h = f32::from_le_bytes(payload[offset + 4..offset + 8].try_into().unwrap());
        let stroke_w = f32::from_le_bytes(payload[offset + 8..offset + 12].try_into().unwrap());
        let running = payload.get(offset + 12).copied().unwrap_or(0) != 0;
        (name, canvas_w, canvas_h, stroke_w, running)
    }

    #[test]
    fn hello_payload_round_trip() {
        let buf = encode_hello("robot-1", 21.0, 14.8, 0.035, true);

        let header = parse_header(buf[0..12].try_into().unwrap()).unwrap();
        assert_eq!(header.msg_type, MSG_HELLO);

        let payload = &buf[12..];
        assert_eq!(payload.len(), header.payload_len as usize);

        let (name, w, h, s, running) = decode_hello(payload);
        assert_eq!(name, "robot-1");
        assert_eq!(w, 21.0);
        assert_eq!(h, 14.8);
        assert_eq!(s, 0.035);
        assert!(running);
    }

    #[test]
    fn hello_empty_name() {
        let buf = encode_hello("", 10.0, 10.0, 0.5, false);
        let payload = &buf[12..];
        let (name, w, h, s, running) = decode_hello(payload);
        assert_eq!(name, "");
        assert_eq!(w, 10.0);
        assert_eq!(h, 10.0);
        assert_eq!(s, 0.5);
        assert!(!running);
    }

    #[test]
    fn hello_without_running_flag_defaults_paused() {
        // Simulate an older worker that doesn't send the running byte
        let name = "old-worker";
        let name_bytes = name.as_bytes();
        let payload_len = 2 + name_bytes.len() + 12; // no +1 for running
        let mut buf = Vec::new();
        buf.extend_from_slice(&build_header(MSG_HELLO, payload_len as u32));
        buf.extend_from_slice(&(name_bytes.len() as u16).to_le_bytes());
        buf.extend_from_slice(name_bytes);
        buf.extend_from_slice(&15.0_f32.to_le_bytes());
        buf.extend_from_slice(&10.0_f32.to_le_bytes());
        buf.extend_from_slice(&0.05_f32.to_le_bytes());
        // No running byte

        let payload = &buf[12..];
        let (name_out, _, _, _, running) = decode_hello(payload);
        assert_eq!(name_out, "old-worker");
        assert!(!running); // defaults to paused
    }
}
