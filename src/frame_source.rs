use std::io::Read;
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex};
use std::thread;

/// Probe the source to get its native width and height in pixels.
/// Uses ffprobe to avoid decoding the whole stream.
pub fn probe_source_dimensions(source: &str) -> Option<(u32, u32)> {
    let spec = SourceSpec::parse(source);

    let input_path = match &spec {
        SourceSpec::Image(path) | SourceSpec::Video(path) => path.clone(),
        SourceSpec::Webcam(device) => {
            if cfg!(target_os = "linux") {
                if device.starts_with("/dev/") {
                    device.clone()
                } else {
                    format!("/dev/video{}", device)
                }
            } else if cfg!(target_os = "macos") {
                return probe_avfoundation_dimensions(device);
            } else {
                return None;
            }
        }
    };

    let mut cmd = Command::new("ffprobe");
    cmd.args([
        "-v", "error",
        "-select_streams", "v:0",
        "-show_entries", "stream=width,height",
        "-of", "csv=s=x:p=0",
    ]);

    // For webcam on linux, need to specify format
    if matches!(&spec, SourceSpec::Webcam(_)) && cfg!(target_os = "linux") {
        cmd.args(["-f", "v4l2"]);
    }

    cmd.arg(&input_path);
    cmd.stdout(Stdio::piped()).stderr(Stdio::piped());

    let output = cmd.output().ok()?;
    let text = String::from_utf8_lossy(&output.stdout);
    let parts: Vec<&str> = text.trim().split('x').collect();
    if parts.len() == 2 {
        let w = parts[0].parse::<u32>().ok()?;
        let h = parts[1].parse::<u32>().ok()?;
        Some((w, h))
    } else {
        None
    }
}

/// Probe macOS avfoundation webcam dimensions by running ffmpeg briefly.
fn probe_avfoundation_dimensions(device: &str) -> Option<(u32, u32)> {
    // Run ffmpeg for a single frame and parse resolution from stderr
    let output = Command::new("ffmpeg")
        .args([
            "-hide_banner",
            "-f", "avfoundation",
            "-framerate", "30",
            "-pixel_format", "nv12",
            "-video_size", "640x480",
            "-i", &format!("{}:", device),
            "-frames:v", "1",
            "-f", "null", "-",
        ])
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .output()
        .ok()?;

    let stderr = String::from_utf8_lossy(&output.stderr);
    // Look for pattern like "1280x720" in the stream info line
    for line in stderr.lines() {
        if line.contains("Video:") {
            // Match WxH pattern (e.g. "1280x720")
            for part in line.split([' ', ',']) {
                let dims: Vec<&str> = part.split('x').collect();
                if dims.len() == 2 {
                    if let (Ok(w), Ok(h)) = (dims[0].parse::<u32>(), dims[1].parse::<u32>()) {
                        if w > 0 && h > 0 && w < 10000 && h < 10000 {
                            return Some((w, h));
                        }
                    }
                }
            }
        }
    }
    None
}

/// Reads grayscale frames from an ffmpeg subprocess.
/// A background thread continuously reads frames and stores the latest one,
/// so the consumer always gets the most recent frame without lag.
pub struct FrameSource {
    latest: Arc<Mutex<Option<Vec<u8>>>>,
    _child: Arc<Mutex<Child>>,
    pub width: u32,
    pub height: u32,
}

/// Parsed source specification.
pub enum SourceSpec {
    Image(String),
    Webcam(String), // device index or path
    Video(String),
}

impl SourceSpec {
    pub fn parse(s: &str) -> Self {
        if s.starts_with("image:") {
            SourceSpec::Image(s.strip_prefix("image:").unwrap().to_string())
        } else if s.starts_with("video:") {
            SourceSpec::Video(s.strip_prefix("video:").unwrap().to_string())
        } else if s == "webcam" {
            SourceSpec::Webcam("0".to_string())
        } else if s.starts_with("webcam:") {
            SourceSpec::Webcam(s.strip_prefix("webcam:").unwrap().to_string())
        } else {
            // Guess: if it looks like a file path, treat as image
            SourceSpec::Image(s.to_string())
        }
    }
}

impl FrameSource {
    /// Create a new FrameSource from the given source spec.
    /// `target_width` and `target_height` are the processing resolution.
    pub fn new(source: &str, target_width: u32, target_height: u32, fps: f64) -> Self {
        let spec = SourceSpec::parse(source);

        let mut cmd = Command::new("ffmpeg");
        cmd.arg("-hide_banner")
            .arg("-loglevel")
            .arg("fatal");

        match &spec {
            SourceSpec::Image(path) => {
                cmd.arg("-loop").arg("1").arg("-i").arg(path);
                // For static images, output at the target fps
                cmd.arg("-r").arg(format!("{}", fps));
            }
            SourceSpec::Webcam(device) => {
                // Request 640x480 capture — close to typical processing
                // resolution and much cheaper than 1280x720.
                if cfg!(target_os = "linux") {
                    let dev = if device.starts_with("/dev/") {
                        device.clone()
                    } else {
                        format!("/dev/video{}", device)
                    };
                    cmd.arg("-f").arg("v4l2")
                        .arg("-video_size").arg("640x480")
                        .arg("-i").arg(dev);
                } else if cfg!(target_os = "macos") {
                    cmd.arg("-f").arg("avfoundation")
                        .arg("-framerate").arg("30")
                        .arg("-pixel_format").arg("nv12")
                        .arg("-video_size").arg("640x480")
                        .arg("-i").arg(format!("{}:", device));
                }
            }
            SourceSpec::Video(path) => {
                cmd.arg("-i").arg(path);
            }
        }

        // Output: scaled grayscale raw frames
        // For live sources, limit output fps to reduce CPU overhead —
        // the algorithm doesn't need 30 target frames per second.
        let vf = match &spec {
            SourceSpec::Image(_) => format!("scale={}:{}", target_width, target_height),
            _ => format!("fps={},scale={}:{}", fps, target_width, target_height),
        };
        cmd.arg("-vf")
            .arg(vf)
            .arg("-pix_fmt")
            .arg("gray")
            .arg("-f")
            .arg("rawvideo")
            .arg("pipe:1");

        cmd.stdout(Stdio::piped()).stderr(Stdio::inherit());

        tracing::info!("spawning ffmpeg: {:?}", cmd);

        let mut child = cmd.spawn().expect("failed to spawn ffmpeg — is it installed?");

        let frame_size = (target_width * target_height) as usize;
        let latest: Arc<Mutex<Option<Vec<u8>>>> = Arc::new(Mutex::new(None));

        // Take stdout from child before moving it into the Arc
        let mut stdout = child.stdout.take().expect("ffmpeg stdout not captured");

        let child = Arc::new(Mutex::new(child));

        // Background thread: continuously read frames, keeping only the latest
        let latest_writer = Arc::clone(&latest);
        thread::Builder::new()
            .name("frame-reader".into())
            .spawn(move || {
                let mut buf = vec![0u8; frame_size];
                loop {
                    match stdout.read_exact(&mut buf) {
                        Ok(()) => {
                            let mut slot = latest_writer.lock().unwrap();
                            *slot = Some(buf.clone());
                        }
                        Err(_) => break,
                    }
                }
            })
            .expect("failed to spawn frame reader thread");

        Self {
            latest,
            _child: child,
            width: target_width,
            height: target_height,
        }
    }

    /// Get the most recent frame. Returns None if no frame has been read yet.
    pub fn next_frame(&mut self) -> Option<Vec<u8>> {
        self.latest.lock().unwrap().take()
    }
}

impl Drop for FrameSource {
    fn drop(&mut self) {
        let _ = self._child.lock().unwrap().kill();
    }
}
