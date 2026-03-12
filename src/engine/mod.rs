pub mod canvas;
pub mod sampler;
pub mod scorer;

use rayon::prelude::*;

use crate::config::Config;
use canvas::{Canvas, LineSegment};
use sampler::SamplingStrategy;

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
    sampler: Box<dyn SamplingStrategy>,
    processing_width: u32,
    processing_height: u32,
    stroke_width: f64,
    pub min_line_len: f64,
    pub max_line_len: f64,
    pub alpha: f64,
}

impl ProposalEngine {
    pub fn new(config: &Config) -> Self {
        let sampler = sampler::create_sampler(&config.sampler);
        Self {
            canvas: Canvas::new(config.canvas_width_cm, config.canvas_height_cm),
            sampler,
            processing_width: config.processing_width(),
            processing_height: config.resolution,
            stroke_width: config.stroke_width_cm,
            min_line_len: config.min_line_len_cm,
            max_line_len: config.max_line_len_cm,
            alpha: config.alpha,
        }
    }

    pub fn set_sampler(&mut self, name: &str) {
        self.sampler = sampler::create_sampler(name);
    }

    pub fn reset(&mut self) {
        self.canvas.lines.clear();
    }

    /// Run one step: generate K proposals, score each, keep the best.
    /// `target` is the grayscale target image at processing resolution.
    pub fn step(&mut self, target: &[u8], k: usize) -> StepResult {
        let pw = self.processing_width;
        let ph = self.processing_height;

        // Rasterize current canvas once (shared across all candidates)
        let base_pixmap = self.canvas.rasterize_pixmap(pw, ph);
        let current_raster = Canvas::pixmap_to_gray(&base_pixmap);
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

        // Score each candidate in parallel — clone the pixmap (not the canvas),
        // draw only the one new line, then score.
        let scores: Vec<f64> = candidates
            .par_iter()
            .map(|line| {
                let mut test_pixmap = base_pixmap.clone();
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
