pub mod canvas;
pub mod sampler;
pub mod scorer;

use rayon::prelude::*;
use tiny_skia::Pixmap;

use crate::config::Config;
use canvas::{Canvas, LineSegment};
use sampler::{Distribution, LineSampler};

/// Result of a single engine step.
pub struct StepResult {
    /// The winning line that was added (if any improved the score).
    pub winning_line: Option<LineSegment>,
    /// MSE score after this step.
    pub score: f64,
}

/// The proposal engine: generates K proposals, scores them, picks the best.
pub struct ProposalEngine {
    pub canvas: Canvas,
    sampler: LineSampler,
    processing_width: u32,
    processing_height: u32,
    stroke_width: f64,
    pub min_line_len: f64,
    pub max_line_len: f64,
    pub alpha: f64,
    /// Cached rasterization of the canvas at processing resolution.
    /// Kept in sync incrementally — only the winning line is drawn each step.
    cached_pixmap: Pixmap,
    /// Cached rasterization at preview (full) resolution, also updated incrementally.
    preview_pixmap: Pixmap,
    preview_width: u32,
    preview_height: u32,
}

impl ProposalEngine {
    pub fn new(config: &Config) -> Self {
        let x = Distribution::parse(&config.x_sampler).expect("invalid x-sampler");
        let y = Distribution::parse(&config.y_sampler).expect("invalid y-sampler");
        let length = Distribution::parse(&config.length_sampler).expect("invalid length-sampler");
        let sampler = LineSampler::new(x, y, length);
        let pw = config.processing_width();
        let ph = config.resolution;
        let prev_w = config.preview_width();
        let prev_h = config.preview_height();
        let mut pixmap = Pixmap::new(pw, ph).expect("valid pixmap dimensions");
        pixmap.fill(tiny_skia::Color::WHITE);
        let mut preview_pixmap = Pixmap::new(prev_w, prev_h).expect("valid pixmap dimensions");
        preview_pixmap.fill(tiny_skia::Color::WHITE);
        Self {
            canvas: Canvas::new(config.canvas_width_cm, config.canvas_height_cm),
            sampler,
            processing_width: pw,
            processing_height: ph,
            stroke_width: config.stroke_width_cm,
            min_line_len: config.min_line_len_cm,
            max_line_len: config.max_line_len_cm,
            alpha: config.alpha,
            cached_pixmap: pixmap,
            preview_pixmap,
            preview_width: prev_w,
            preview_height: prev_h,
        }
    }

    pub fn set_x_sampler(&mut self, spec: &str) -> Result<(), String> {
        self.sampler.x = Distribution::parse(spec)?;
        Ok(())
    }

    pub fn set_y_sampler(&mut self, spec: &str) -> Result<(), String> {
        self.sampler.y = Distribution::parse(spec)?;
        Ok(())
    }

    pub fn set_length_sampler(&mut self, spec: &str) -> Result<(), String> {
        self.sampler.length = Distribution::parse(spec)?;
        Ok(())
    }

    pub fn reset(&mut self) {
        self.canvas.lines.clear();
        self.cached_pixmap.fill(tiny_skia::Color::WHITE);
        self.preview_pixmap.fill(tiny_skia::Color::WHITE);
    }

    /// Get the cached pixmap (current canvas at processing resolution).
    pub fn cached_pixmap(&self) -> &Pixmap {
        &self.cached_pixmap
    }

    /// Encode the cached preview pixmap as a PNG.
    pub fn preview_png(&self) -> Vec<u8> {
        self.preview_pixmap
            .encode_png()
            .expect("PNG encoding should work")
    }

    /// Run one step: generate K proposals, score each, keep the best.
    /// `target` is the grayscale target image at processing resolution.
    pub fn step(&mut self, target: &[u8], k: usize) -> StepResult {
        let pw = self.processing_width;
        let ph = self.processing_height;

        // Use the cached pixmap instead of re-rasterizing all lines
        let current_raster = Canvas::pixmap_to_gray(&self.cached_pixmap);
        let current_score = scorer::asymmetric_mse(&current_raster, target, self.alpha);

        let scale_x = pw as f64 / self.canvas.width_cm;
        let scale_y = ph as f64 / self.canvas.height_cm;

        // Generate K candidate lines
        let candidates: Vec<LineSegment> = (0..k)
            .map(|_| {
                self.sampler.sample(
                    self.canvas.width_cm,
                    self.canvas.height_cm,
                    self.stroke_width,
                    self.min_line_len,
                    self.max_line_len,
                )
            })
            .collect();

        // Score each candidate in parallel — clone the cached pixmap,
        // draw only the one new line, then score.
        let scores: Vec<f64> = candidates
            .par_iter()
            .map(|line| {
                let mut test_pixmap = self.cached_pixmap.clone();
                Canvas::rasterize_line_onto(&mut test_pixmap, line, scale_x, scale_y);
                let raster = Canvas::pixmap_to_gray(&test_pixmap);
                scorer::asymmetric_mse(&raster, target, self.alpha)
            })
            .collect();

        // Find the best
        let (best_idx, &best_score) = scores
            .iter()
            .enumerate()
            .min_by(|(_, a), (_, b)| a.partial_cmp(b).unwrap())
            .unwrap();

        if best_score < current_score {
            let winning_line = candidates[best_idx];
            self.canvas.add_line(winning_line);
            // Incrementally update cached pixmaps with just the new line
            Canvas::rasterize_line_onto(&mut self.cached_pixmap, &winning_line, scale_x, scale_y);
            let prev_sx = self.preview_width as f64 / self.canvas.width_cm;
            let prev_sy = self.preview_height as f64 / self.canvas.height_cm;
            Canvas::rasterize_line_onto(&mut self.preview_pixmap, &winning_line, prev_sx, prev_sy);
            StepResult {
                winning_line: Some(winning_line),
                score: best_score,
            }
        } else {
            StepResult {
                winning_line: None,
                score: current_score,
            }
        }
    }
}
