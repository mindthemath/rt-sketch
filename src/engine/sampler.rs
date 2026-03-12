use crate::engine::canvas::LineSegment;

/// Trait for line sampling strategies.
pub trait SamplingStrategy: Send + Sync {
    /// Generate a random line segment within the given canvas bounds.
    fn sample(
        &self,
        canvas_width: f64,
        canvas_height: f64,
        stroke_width: f64,
        min_len: f64,
        max_len: f64,
    ) -> LineSegment;
}

/// Uniform random sampling: all positions and angles equally likely.
pub struct UniformSampler;

impl SamplingStrategy for UniformSampler {
    fn sample(
        &self,
        canvas_width: f64,
        canvas_height: f64,
        stroke_width: f64,
        min_len: f64,
        max_len: f64,
    ) -> LineSegment {
        let x1 = fastrand::f64() * canvas_width;
        let y1 = fastrand::f64() * canvas_height;

        let angle = fastrand::f64() * std::f64::consts::TAU;
        let len = min_len + fastrand::f64() * (max_len - min_len);

        let x2 = (x1 + angle.cos() * len).clamp(0.0, canvas_width);
        let y2 = (y1 + angle.sin() * len).clamp(0.0, canvas_height);

        LineSegment {
            x1,
            y1,
            x2,
            y2,
            width: stroke_width,
        }
    }
}

/// Beta distribution sampling: biases toward configurable regions.
/// Uses the Kumaraswamy distribution as an approximation (no external dep needed).
/// alpha < 1, beta < 1 → biased toward edges
/// alpha > 1, beta > 1 → biased toward center
pub struct BetaSampler {
    pub alpha: f64,
    pub beta: f64,
}

impl BetaSampler {
    pub fn centered() -> Self {
        Self {
            alpha: 2.0,
            beta: 2.0,
        }
    }

    /// Kumaraswamy sample approximating Beta(alpha, beta).
    fn kumaraswamy_sample(&self) -> f64 {
        let u: f64 = fastrand::f64().max(1e-10);
        (1.0 - (1.0 - u.powf(1.0 / self.beta)).powf(1.0 / self.alpha)).clamp(0.0, 1.0)
    }
}

impl SamplingStrategy for BetaSampler {
    fn sample(
        &self,
        canvas_width: f64,
        canvas_height: f64,
        stroke_width: f64,
        min_len: f64,
        max_len: f64,
    ) -> LineSegment {
        let x1 = self.kumaraswamy_sample() * canvas_width;
        let y1 = self.kumaraswamy_sample() * canvas_height;

        let angle = fastrand::f64() * std::f64::consts::TAU;
        let len = min_len + fastrand::f64() * (max_len - min_len);

        let x2 = (x1 + angle.cos() * len).clamp(0.0, canvas_width);
        let y2 = (y1 + angle.sin() * len).clamp(0.0, canvas_height);

        LineSegment {
            x1,
            y1,
            x2,
            y2,
            width: stroke_width,
        }
    }
}

/// Create a sampler from a strategy name.
pub fn create_sampler(name: &str) -> Box<dyn SamplingStrategy> {
    match name {
        "beta" => Box::new(BetaSampler::centered()),
        _ => Box::new(UniformSampler),
    }
}
