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
    #[arg(long, default_value_t = 50)]
    pub k: usize,

    /// Sampling strategy: "uniform" or "beta"
    #[arg(long, default_value = "uniform")]
    pub sampler: String,

    /// Robot server address (omit for preview-only mode)
    #[arg(long)]
    pub robot_server: Option<String>,

    /// Web UI port
    #[arg(long, default_value_t = 8080)]
    pub web_port: u16,

    /// Pen stroke width in cm
    #[arg(long, default_value_t = 0.05)]
    pub stroke_width: f64,

    /// Minimum line length in cm
    #[arg(long, default_value_t = 0.2)]
    pub min_line_len: f64,

    /// Maximum line length in cm
    #[arg(long, default_value_t = 5.0)]
    pub max_line_len: f64,

    /// Overshoot penalty (asymmetric MSE alpha). 1.0 = standard MSE, >1 penalizes ink on whitespace.
    #[arg(long, default_value_t = 2.0)]
    pub alpha: f64,

    /// Gamma correction for target image. <1 brightens, >1 darkens, 1.0 = no change.
    #[arg(long, default_value_t = 1.0)]
    pub gamma: f64,
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
    pub sampler: String,
    pub stroke_width_cm: f64,
    pub min_line_len_cm: f64,
    pub max_line_len_cm: f64,
    pub alpha: f64,
    pub gamma: f64,
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
            sampler: args.sampler.clone(),
            stroke_width_cm: args.stroke_width,
            min_line_len_cm: args.min_line_len,
            max_line_len_cm: args.max_line_len,
            alpha: args.alpha,
            gamma: args.gamma,
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
