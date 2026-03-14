use std::io::Write;
use std::path::Path;
use std::process::{Child, Command, Stdio};

/// Streams preview frames to an FFmpeg subprocess for output to RTMP or a file.
pub struct StreamOutput {
    child: Child,
    width: u32,
    height: u32,
}

impl StreamOutput {
    /// Create a new stream output.
    ///
    /// - `width`/`height`: preview frame dimensions
    /// - `fps`: constant output framerate
    /// - `url`: RTMP URL (e.g. rtmp://...) — adds silent audio track
    /// - `path`: output file template (e.g. output.mkv) — timestamp is inserted
    ///   before the extension (e.g. output.2026-03-14T12:00:00Z.mp4)
    ///
    /// Exactly one of `url` or `path` should be Some.
    pub fn new(width: u32, height: u32, fps: f64, url: Option<&str>, path: Option<&str>) -> Self {
        let timestamped_path;
        let dest = if let Some(p) = path {
            timestamped_path = stamp_filename(p);
            timestamped_path.as_str()
        } else {
            url.expect("stream output requires a URL or path")
        };
        let is_rtmp = url.is_some();

        let mut cmd = Command::new("ffmpeg");
        cmd.arg("-hide_banner").arg("-loglevel").arg("warning");

        // Overwrite output file without asking
        cmd.arg("-y");

        // Video input: raw RGBA frames from pipe
        cmd.args(["-f", "rawvideo"]);
        cmd.args(["-pix_fmt", "rgba"]);
        cmd.args(["-s", &format!("{}x{}", width, height)]);
        cmd.args(["-r", &format!("{}", fps)]);
        cmd.args(["-i", "pipe:0"]);

        if is_rtmp {
            // Silent audio for RTMP (YouTube/Twitch require audio)
            cmd.args(["-f", "lavfi", "-i", "anullsrc=r=44100:cl=stereo"]);
        }

        // Video encoding
        cmd.args(["-c:v", "libx264"]);
        cmd.args(["-preset", "fast"]);
        cmd.args(["-pix_fmt", "yuv420p"]);
        // Ensure dimensions are divisible by 2 for x264
        cmd.args(["-vf", &format!("pad=ceil(iw/2)*2:ceil(ih/2)*2")]);

        if is_rtmp {
            // Audio encoding + RTMP output
            cmd.args(["-c:a", "aac"]);
            cmd.args(["-shortest"]);
            cmd.args(["-f", "flv"]);
        }

        cmd.arg(dest);

        cmd.stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::null());

        tracing::info!("spawning stream ffmpeg: {:?}", cmd);

        let child = cmd
            .spawn()
            .expect("failed to spawn ffmpeg for streaming — is it installed?");

        Self {
            child,
            width,
            height,
        }
    }

    /// Write a frame to the stream. `rgba_data` must be width*height*4 bytes.
    pub fn write_frame(&mut self, rgba_data: &[u8]) {
        let expected = (self.width * self.height * 4) as usize;
        debug_assert_eq!(
            rgba_data.len(),
            expected,
            "frame size mismatch: got {} expected {}",
            rgba_data.len(),
            expected
        );

        if let Some(ref mut stdin) = self.child.stdin {
            if let Err(e) = stdin.write_all(rgba_data) {
                tracing::warn!("stream output write failed: {}", e);
                // Close stdin so we don't keep trying
                self.child.stdin.take();
            }
        }
    }
}

/// Insert an ISO 8601 UTC timestamp before the file extension.
/// e.g. "output.mkv" → "output.2026-03-14T12:00:00Z.mkv"
fn stamp_filename(template: &str) -> String {
    let path = Path::new(template);
    let stem = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or(template);
    let ts = chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ");
    if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
        let parent = path.parent().and_then(|p| p.to_str()).unwrap_or("");
        if parent.is_empty() {
            format!("{}.{}.{}", stem, ts, ext)
        } else {
            format!("{}/{}.{}.{}", parent, stem, ts, ext)
        }
    } else {
        format!("{}.{}", template, ts)
    }
}

impl Drop for StreamOutput {
    fn drop(&mut self) {
        // Close stdin to signal EOF, then wait for ffmpeg to finish
        self.child.stdin.take();
        match self.child.wait() {
            Ok(status) => {
                if !status.success() {
                    tracing::warn!("stream ffmpeg exited with {}", status);
                }
            }
            Err(e) => {
                tracing::warn!("failed to wait for stream ffmpeg: {}", e);
                let _ = self.child.kill();
            }
        }
    }
}
