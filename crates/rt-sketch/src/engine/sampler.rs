use crate::engine::canvas::LineSegment;

/// A distribution that produces values in [0, 1].
#[derive(Debug, Clone)]
pub enum Distribution {
    Uniform,
    Beta { alpha: f64, beta: f64 },
}

impl Distribution {
    /// Sample a value in [0, 1].
    pub fn sample(&self) -> f64 {
        match self {
            Distribution::Uniform => fastrand::f64(),
            Distribution::Beta { alpha, beta } => {
                let u: f64 = fastrand::f64().max(1e-10);
                (1.0 - (1.0 - u.powf(1.0 / beta)).powf(1.0 / alpha)).clamp(0.0, 1.0)
            }
        }
    }

    /// Parse a distribution spec: preset name or "beta:a,b".
    pub fn parse(spec: &str) -> Result<Self, String> {
        match spec {
            "uniform" => Ok(Distribution::Uniform),
            "center" => Ok(Distribution::Beta {
                alpha: 2.0,
                beta: 2.0,
            }),
            "edges" => Ok(Distribution::Beta {
                alpha: 0.5,
                beta: 0.5,
            }),
            "low" => Ok(Distribution::Beta {
                alpha: 10.0,
                beta: 2.0,
            }),
            "high" => Ok(Distribution::Beta {
                alpha: 2.0,
                beta: 10.0,
            }),
            s if s.starts_with("beta:") => {
                let params = &s[5..];
                let parts: Vec<&str> = params.split(',').collect();
                if parts.len() != 2 {
                    return Err(format!("expected beta:a,b, got {}", s));
                }
                let a = parts[0]
                    .trim()
                    .parse::<f64>()
                    .map_err(|e| format!("bad alpha: {}", e))?;
                let b = parts[1]
                    .trim()
                    .parse::<f64>()
                    .map_err(|e| format!("bad beta: {}", e))?;
                Ok(Distribution::Beta { alpha: a, beta: b })
            }
            _ => Err(format!("unknown distribution: {}", spec)),
        }
    }
}

/// Sampler that generates line segments with independent distributions for x, y, and length.
pub struct LineSampler {
    pub x: Distribution,
    pub y: Distribution,
    pub length: Distribution,
}

impl LineSampler {
    pub fn new(x: Distribution, y: Distribution, length: Distribution) -> Self {
        Self { x, y, length }
    }

    pub fn sample(
        &self,
        canvas_width: f64,
        canvas_height: f64,
        stroke_width: f64,
        min_len: f64,
        max_len: f64,
    ) -> LineSegment {
        let x1 = self.x.sample() * canvas_width;
        let y1 = self.y.sample() * canvas_height;

        let angle = fastrand::f64() * std::f64::consts::TAU;
        let len = if (max_len - min_len).abs() < 1e-9 {
            min_len
        } else {
            min_len + self.length.sample() * (max_len - min_len)
        };

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
