use std::io::Read;
use std::process::{Child, Command, Stdio};

/// Reads grayscale frames from an ffmpeg subprocess.
pub struct FrameSource {
    child: Child,
    frame_size: usize,
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
            .arg("error");

        match &spec {
            SourceSpec::Image(path) => {
                cmd.arg("-loop").arg("1").arg("-i").arg(path);
                // For static images, output at the target fps
                cmd.arg("-r").arg(format!("{}", fps));
            }
            SourceSpec::Webcam(device) => {
                if cfg!(target_os = "linux") {
                    let dev = if device.starts_with("/dev/") {
                        device.clone()
                    } else {
                        format!("/dev/video{}", device)
                    };
                    cmd.arg("-f").arg("v4l2").arg("-i").arg(dev);
                } else if cfg!(target_os = "macos") {
                    cmd.arg("-f").arg("avfoundation").arg("-i").arg(format!("{}:", device));
                }
            }
            SourceSpec::Video(path) => {
                cmd.arg("-i").arg(path);
            }
        }

        // Output: scaled grayscale raw frames
        cmd.arg("-vf")
            .arg(format!("scale={}:{}", target_width, target_height))
            .arg("-pix_fmt")
            .arg("gray")
            .arg("-f")
            .arg("rawvideo")
            .arg("pipe:1");

        cmd.stdout(Stdio::piped()).stderr(Stdio::piped());

        tracing::info!("spawning ffmpeg: {:?}", cmd);

        let child = cmd.spawn().expect("failed to spawn ffmpeg — is it installed?");

        Self {
            child,
            frame_size: (target_width * target_height) as usize,
            width: target_width,
            height: target_height,
        }
    }

    /// Read the next frame. Returns None if the stream has ended.
    pub fn next_frame(&mut self) -> Option<Vec<u8>> {
        let mut buf = vec![0u8; self.frame_size];
        let stdout = self.child.stdout.as_mut()?;

        match stdout.read_exact(&mut buf) {
            Ok(()) => Some(buf),
            Err(_) => None,
        }
    }
}

impl Drop for FrameSource {
    fn drop(&mut self) {
        let _ = self.child.kill();
    }
}
