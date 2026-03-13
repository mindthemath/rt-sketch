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
}
