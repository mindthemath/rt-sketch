use std::io::Read;
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

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
                // We capture at 640x480, so just return that directly.
                // Avoids grabbing the camera during probe (which can
                // interfere with the main capture).
                return Some((640, 480));
            } else {
                return None;
            }
        }
    };

    let mut cmd = Command::new("ffprobe");
    cmd.args([
        "-v",
        "error",
        "-select_streams",
        "v:0",
        "-show_entries",
        "stream=width,height",
        "-of",
        "csv=s=x:p=0",
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

/// Reads grayscale frames from an ffmpeg subprocess.
/// A background thread continuously reads frames and stores the latest one,
/// so the consumer always gets the most recent frame without lag.
/// If ffmpeg exits (e.g. source unavailable), the reader thread automatically
/// respawns it after a short delay.
pub struct FrameSource {
    latest: Arc<Mutex<Option<Vec<u8>>>>,
    shutdown: Arc<AtomicBool>,
    _reader_thread: thread::JoinHandle<()>,
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

/// Build the ffmpeg Command for a given source spec.
fn build_ffmpeg_cmd(source: &str, target_width: u32, target_height: u32, fps: f64) -> Command {
    let spec = SourceSpec::parse(source);

    let mut cmd = Command::new("ffmpeg");
    cmd.arg("-hide_banner").arg("-loglevel").arg("fatal");

    match &spec {
        SourceSpec::Image(path) => {
            cmd.arg("-i").arg(path);
            cmd.arg("-frames:v").arg("1");
        }
        SourceSpec::Webcam(device) => {
            if cfg!(target_os = "linux") {
                let dev = if device.starts_with("/dev/") {
                    device.clone()
                } else {
                    format!("/dev/video{}", device)
                };
                cmd.arg("-f")
                    .arg("v4l2")
                    .arg("-video_size")
                    .arg("640x480")
                    .arg("-i")
                    .arg(dev);
            } else if cfg!(target_os = "macos") {
                cmd.arg("-f")
                    .arg("avfoundation")
                    .arg("-framerate")
                    .arg("30")
                    .arg("-pixel_format")
                    .arg("nv12")
                    .arg("-video_size")
                    .arg("640x480")
                    .arg("-i")
                    .arg(format!("{}:", device));
            }
        }
        SourceSpec::Video(path) => {
            cmd.arg("-i").arg(path);
        }
    }

    let vf = match &spec {
        SourceSpec::Image(_) => format!(
            "color=white:s={}x{},format=rgba[bg];[0]scale={}:{},format=rgba[fg];[bg][fg]overlay",
            target_width, target_height, target_width, target_height
        ),
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
    cmd
}

/// Spawn ffmpeg and return the child process (or None on failure).
fn spawn_ffmpeg(source: &str, target_width: u32, target_height: u32, fps: f64) -> Option<Child> {
    let mut cmd = build_ffmpeg_cmd(source, target_width, target_height, fps);
    match cmd.spawn() {
        Ok(child) => Some(child),
        Err(e) => {
            tracing::warn!("failed to spawn ffmpeg: {}", e);
            None
        }
    }
}

impl FrameSource {
    /// Create a new FrameSource from the given source spec.
    /// `target_width` and `target_height` are the processing resolution.
    /// For image sources, decodes the image once with ffmpeg and stops.
    /// For video/webcam sources, runs ffmpeg continuously in a background
    /// thread with automatic retry on failure.
    pub fn new(source: &str, target_width: u32, target_height: u32, fps: f64) -> Self {
        let frame_size = (target_width * target_height) as usize;
        let latest: Arc<Mutex<Option<Vec<u8>>>> = Arc::new(Mutex::new(None));
        let shutdown = Arc::new(AtomicBool::new(false));

        let spec = SourceSpec::parse(source);
        let is_image = matches!(spec, SourceSpec::Image(_));

        let latest_writer = Arc::clone(&latest);
        let shutdown_flag = Arc::clone(&shutdown);
        let source_owned = source.to_string();

        let handle = thread::Builder::new()
            .name("frame-reader".into())
            .spawn(move || {
                if is_image {
                    // Static image: decode once with ffmpeg (no -loop), set the
                    // frame, and exit. The algorithm reuses the last frame when
                    // next_frame() returns None, so no further work is needed.
                    let mut child =
                        match spawn_ffmpeg(&source_owned, target_width, target_height, fps) {
                            Some(c) => c,
                            None => {
                                tracing::error!("failed to spawn ffmpeg for image");
                                return;
                            }
                        };
                    let mut stdout = match child.stdout.take() {
                        Some(s) => s,
                        None => {
                            tracing::error!("ffmpeg stdout not captured for image");
                            let _ = child.kill();
                            return;
                        }
                    };
                    let mut buf = vec![0u8; frame_size];
                    if stdout.read_exact(&mut buf).is_ok() {
                        let mut slot = latest_writer.lock().unwrap();
                        *slot = Some(buf);
                    }
                    let _ = child.kill();
                    let _ = child.wait();
                    return;
                }

                // Video/webcam: continuously read frames
                while !shutdown_flag.load(Ordering::Relaxed) {
                    // Spawn ffmpeg
                    let mut child =
                        match spawn_ffmpeg(&source_owned, target_width, target_height, fps) {
                            Some(c) => c,
                            None => {
                                tracing::warn!("retrying ffmpeg in 2s...");
                                thread::sleep(Duration::from_secs(2));
                                continue;
                            }
                        };

                    let mut stdout = match child.stdout.take() {
                        Some(s) => s,
                        None => {
                            tracing::warn!("ffmpeg stdout not captured, retrying in 2s...");
                            let _ = child.kill();
                            thread::sleep(Duration::from_secs(2));
                            continue;
                        }
                    };

                    // Read frames until EOF or error
                    let mut buf = vec![0u8; frame_size];
                    loop {
                        if shutdown_flag.load(Ordering::Relaxed) {
                            let _ = child.kill();
                            return;
                        }
                        match stdout.read_exact(&mut buf) {
                            Ok(()) => {
                                let mut slot = latest_writer.lock().unwrap();
                                *slot = Some(buf.clone());
                            }
                            Err(_) => break,
                        }
                    }

                    // ffmpeg exited — clean up and retry
                    let _ = child.wait();
                    if !shutdown_flag.load(Ordering::Relaxed) {
                        tracing::warn!("ffmpeg exited, restarting in 2s...");
                        thread::sleep(Duration::from_secs(2));
                    }
                }
            })
            .expect("failed to spawn frame reader thread");

        Self {
            latest,
            shutdown,
            _reader_thread: handle,
        }
    }

    /// Get the most recent frame. Returns None if no frame has been read yet.
    pub fn next_frame(&mut self) -> Option<Vec<u8>> {
        self.latest.lock().unwrap().take()
    }
}

impl Drop for FrameSource {
    fn drop(&mut self) {
        self.shutdown.store(true, Ordering::Relaxed);
    }
}
