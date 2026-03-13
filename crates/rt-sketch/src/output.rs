use crate::engine::canvas::LineSegment;

/// Trait for sending draw commands to a destination.
pub trait CommandSink: Send + Sync {
    fn send_line(&self, line: &LineSegment) -> Result<(), String>;
}

/// No-op sink for preview-only mode.
pub struct NoopSink;

impl CommandSink for NoopSink {
    fn send_line(&self, _line: &LineSegment) -> Result<(), String> {
        Ok(())
    }
}

/// HTTP sink that POSTs line commands to a robot server.
pub struct HttpSink {
    client: reqwest::blocking::Client,
    base_url: String,
}

impl HttpSink {
    pub fn new(base_url: &str) -> Self {
        Self {
            client: reqwest::blocking::Client::new(),
            base_url: base_url.trim_end_matches('/').to_string(),
        }
    }
}

impl CommandSink for HttpSink {
    fn send_line(&self, line: &LineSegment) -> Result<(), String> {
        let payload = serde_json::json!({
            "command": "line",
            "x1": line.x1,
            "y1": line.y1,
            "x2": line.x2,
            "y2": line.y2,
            "width": line.width,
        });

        self.client
            .post(format!("{}/draw", self.base_url))
            .json(&payload)
            .send()
            .map_err(|e| format!("failed to send draw command: {}", e))?;

        Ok(())
    }
}
