use clap::Parser;
use serde::{Deserialize, Serialize};

#[derive(Parser, Debug, Clone)]
#[command(name = "rt-sketch", about = "Real-time video-to-SVG sketch engine")]
pub struct Args {
    /// Input source: "image:path.jpg", "webcam", "webcam:1", or "video:path.mp4"
    #[arg(long, default_value = "webcam")]
    pub source: String,

    /// Target frames per second
    #[arg(long, default_value_t = 6.0)]
    pub fps: f64,

    /// Processing resolution height in pixels
    #[arg(long, default_value_t = 256)]
    pub resolution: u32,

    /// Canvas width in cm
    #[arg(long, default_value_t = 10.0)]
    pub canvas_width: f64,

    /// Canvas height in cm
    #[arg(long, default_value_t = 10.0)]
    pub canvas_height: f64,

    /// Pixels per inch for web preview rendering
    #[arg(long, default_value_t = 72.0)]
    pub ppi: f64,

    /// Number of proposals per step
    #[arg(long, default_value_t = 200)]
    pub k: usize,

    /// X position distribution: uniform|center|edges|low|high|beta:a,b
    #[arg(long, default_value = "uniform")]
    pub x_sampler: String,

    /// Y position distribution: uniform|center|edges|low|high|beta:a,b
    #[arg(long, default_value = "uniform")]
    pub y_sampler: String,

    /// Line length distribution: uniform|center|edges|low|high|beta:a,b
    #[arg(long, default_value = "uniform")]
    pub length_sampler: String,

    /// Robot server address (omit for preview-only mode)
    #[arg(long)]
    pub robot_server: Option<String>,

    /// Web UI port (auto-selects if default 8080 is busy)
    #[arg(long)]
    pub web_port: Option<u16>,

    /// Pen stroke width in cm
    #[arg(long, default_value_t = 0.035)]
    pub stroke_width: f64,

    /// Minimum line length in cm
    #[arg(long, default_value_t = 2.0)]
    pub min_line_len: f64,

    /// Maximum line length in cm
    #[arg(long, default_value_t = 7.0)]
    pub max_line_len: f64,

    /// Overshoot penalty (asymmetric MSE alpha). 1.0 = standard MSE, >1 penalizes ink on whitespace.
    #[arg(long, default_value_t = 2.0)]
    pub alpha: f64,

    /// Gamma correction for target image. <1 brightens, >1 darkens, 1.0 = no change.
    #[arg(long, default_value_t = 1.0)]
    pub gamma: f64,

    /// Exposure compensation in EV stops. 0 = no change, +1 = 2x brighter, -1 = 2x darker.
    #[arg(long, default_value_t = 0.0)]
    pub exposure: f64,

    /// Contrast multiplier. 1.0 = no change, >1 increases contrast, <1 decreases.
    #[arg(long, default_value_t = 1.0)]
    pub contrast: f64,

    /// Stream preview to an RTMP URL (e.g. rtmp://a.rtmp.youtube.com/live2/KEY)
    #[arg(long)]
    pub stream_url: Option<String>,

    /// Stream preview to a file (e.g. output.[mkv|mp4] )
    #[arg(long)]
    pub stream_output: Option<String>,

    /// Stream lines to a TCP viewer server (e.g. 192.168.1.10:9900)
    #[arg(long)]
    pub stream_tcp: Option<String>,

    /// Instance name for TCP stream identification
    #[arg(long)]
    pub stream_name: Option<String>,

    /// Stamp library CSV (local path or HTTP URL). When set, proposals use stamps instead of random lines.
    #[arg(long)]
    pub stamp_library: Option<String>,

    /// How to handle stamp lines that extend beyond canvas bounds: clip, drop, or none.
    #[arg(long, default_value = "clip")]
    pub stamp_crop: String,

    /// Disable random rotation of stamps (place them axis-aligned).
    #[arg(long)]
    pub no_stamp_rotate: bool,

    /// Start drawing immediately without waiting for the web UI start button
    #[arg(long)]
    pub auto_start: bool,

    /// Wait for a successful viewer connection before starting (requires --stream-tcp)
    #[arg(long)]
    pub wait_for_viewer: bool,

    /// Number of threads for parallel proposal scoring (default: all cores)
    #[arg(long)]
    pub threads: Option<usize>,
}

/// Runtime configuration derived from CLI args. Can be updated from the web UI.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub fps: f64,
    pub resolution: u32,
    pub canvas_width_cm: f64,
    pub canvas_height_cm: f64,
    pub ppi: f64,
    pub k: usize,
    pub x_sampler: String,
    pub y_sampler: String,
    pub length_sampler: String,
    pub stroke_width_cm: f64,
    pub min_line_len_cm: f64,
    pub max_line_len_cm: f64,
    pub alpha: f64,
    pub gamma: f64,
    pub exposure: f64,
    pub contrast: f64,
}

impl Config {
    pub fn from_args(args: &Args) -> Self {
        Self {
            fps: args.fps,
            resolution: args.resolution,
            canvas_width_cm: args.canvas_width,
            canvas_height_cm: args.canvas_height,
            ppi: args.ppi,
            k: args.k,
            x_sampler: args.x_sampler.clone(),
            y_sampler: args.y_sampler.clone(),
            length_sampler: args.length_sampler.clone(),
            stroke_width_cm: args.stroke_width,
            min_line_len_cm: args.min_line_len,
            max_line_len_cm: args.max_line_len,
            alpha: args.alpha,
            gamma: args.gamma,
            exposure: args.exposure,
            contrast: args.contrast,
        }
    }

    /// Adjust canvas dimensions to fit the source aspect ratio within the
    /// current width/height bounding box.
    pub fn fit_to_source(&mut self, source_width: u32, source_height: u32) {
        let source_aspect = source_width as f64 / source_height as f64;
        let box_aspect = self.canvas_width_cm / self.canvas_height_cm;

        if source_aspect > box_aspect {
            // Source is wider — fit to width, shrink height
            self.canvas_height_cm = self.canvas_width_cm / source_aspect;
        } else {
            // Source is taller — fit to height, shrink width
            self.canvas_width_cm = self.canvas_height_cm * source_aspect;
        }
    }

    /// Processing resolution width, derived from height and canvas aspect ratio.
    pub fn processing_width(&self) -> u32 {
        let aspect = self.canvas_width_cm / self.canvas_height_cm;
        (self.resolution as f64 * aspect).round() as u32
    }

    /// Preview pixel dimensions from canvas cm and PPI.
    pub fn preview_width(&self) -> u32 {
        (self.canvas_width_cm * self.ppi / 2.54).round() as u32
    }

    pub fn preview_height(&self) -> u32 {
        (self.canvas_height_cm * self.ppi / 2.54).round() as u32
    }
}
