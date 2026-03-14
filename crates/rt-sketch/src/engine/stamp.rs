use std::fmt;
use std::io::Read;
use std::str::FromStr;

use regex::Regex;

use super::canvas::LineSegment;
use super::sampler::Distribution;

/// How to handle stamp lines that extend beyond canvas bounds.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum StampCrop {
    /// Clip lines to canvas bounds using line-segment intersection (preserves angles).
    Clip,
    /// Drop any line that has an endpoint outside the canvas.
    Drop,
    /// Leave endpoints as-is; tiny-skia handles rendering clipping.
    None,
}

impl Default for StampCrop {
    fn default() -> Self {
        StampCrop::Clip
    }
}

impl fmt::Display for StampCrop {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            StampCrop::Clip => write!(f, "clip"),
            StampCrop::Drop => write!(f, "drop"),
            StampCrop::None => write!(f, "none"),
        }
    }
}

impl FromStr for StampCrop {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "clip" => Ok(StampCrop::Clip),
            "drop" => Ok(StampCrop::Drop),
            "none" => Ok(StampCrop::None),
            _ => Err(format!(
                "invalid stamp-crop mode '{}': expected clip, drop, or none",
                s
            )),
        }
    }
}

/// A stamp: a collection of line segments with a bounding box.
#[derive(Debug, Clone)]
pub struct Stamp {
    /// Line segments in stamp-local coordinates (cm).
    pub lines: Vec<LineSegment>,
    /// Bounding box: (min_x, min_y, max_x, max_y) in cm.
    pub bbox: (f64, f64, f64, f64),
}

impl Stamp {
    /// Parse a stamp from SVG text containing `<line>` elements.
    /// `scale` multiplies all coordinates.
    /// `stroke_width` is the fixed stroke width applied to all lines (SVG stroke-width is ignored).
    pub fn from_svg(svg_text: &str, scale: f64, stroke_width: f64) -> Result<Self, String> {
        let line_re = Regex::new(
            r#"<line\s[^>]*?x1\s*=\s*"([^"]+)"[^>]*?y1\s*=\s*"([^"]+)"[^>]*?x2\s*=\s*"([^"]+)"[^>]*?y2\s*=\s*"([^"]+)"[^>]*?/?>"#,
        )
        .unwrap();

        let mut lines = Vec::new();

        for cap in line_re.captures_iter(svg_text) {
            let x1: f64 = cap[1].parse().map_err(|e| format!("bad x1: {}", e))?;
            let y1: f64 = cap[2].parse().map_err(|e| format!("bad y1: {}", e))?;
            let x2: f64 = cap[3].parse().map_err(|e| format!("bad x2: {}", e))?;
            let y2: f64 = cap[4].parse().map_err(|e| format!("bad y2: {}", e))?;

            lines.push(LineSegment {
                x1: x1 * scale,
                y1: y1 * scale,
                x2: x2 * scale,
                y2: y2 * scale,
                width: stroke_width,
            });
        }

        if lines.is_empty() {
            return Err("no <line> elements found in SVG".to_string());
        }

        let bbox = compute_bbox(&lines);
        Ok(Stamp { lines, bbox })
    }
}

/// A library of stamps loaded from a CSV file.
#[derive(Debug, Clone)]
pub struct StampLibrary {
    pub stamps: Vec<Stamp>,
}

impl StampLibrary {
    /// Load a stamp library from a CSV file (local path or HTTP URL).
    /// The CSV must have a `path` column. Optional: `scale`.
    pub fn load(csv_source: &str, default_stroke_width: f64) -> Result<Self, String> {
        let csv_text = fetch_text(csv_source)?;

        let mut reader = csv::ReaderBuilder::new()
            .comment(Some(b'#'))
            .flexible(true)
            .from_reader(csv_text.as_bytes());
        let headers = reader
            .headers()
            .map_err(|e| format!("CSV header error: {}", e))?
            .clone();

        let path_idx = headers
            .iter()
            .position(|h| h.trim() == "path")
            .ok_or("CSV missing required 'path' column")?;
        let scale_idx = headers.iter().position(|h| h.trim() == "scale");

        let mut stamps = Vec::new();

        for (row_num, result) in reader.records().enumerate() {
            let record = result.map_err(|e| format!("CSV row {}: {}", row_num + 1, e))?;

            let path = record
                .get(path_idx)
                .ok_or_else(|| format!("row {}: missing path", row_num + 1))?
                .trim();
            if path.is_empty() {
                tracing::warn!("row {}: empty path, skipping", row_num + 1);
                continue;
            }

            let scale = scale_idx
                .and_then(|i| record.get(i))
                .and_then(|v| v.trim().parse::<f64>().ok())
                .unwrap_or(1.0);

            let svg_text = match fetch_text(path) {
                Ok(text) => text,
                Err(e) => {
                    tracing::warn!("row {}: failed to fetch '{}': {}", row_num + 1, path, e);
                    continue;
                }
            };

            match Stamp::from_svg(&svg_text, scale, default_stroke_width) {
                Ok(stamp) => {
                    tracing::info!(
                        "loaded stamp '{}': {} lines, bbox {:.2}x{:.2} cm",
                        path,
                        stamp.lines.len(),
                        stamp.bbox.2 - stamp.bbox.0,
                        stamp.bbox.3 - stamp.bbox.1,
                    );
                    stamps.push(stamp);
                }
                Err(e) => {
                    tracing::warn!("row {}: failed to parse '{}': {}", row_num + 1, path, e);
                }
            }
        }

        if stamps.is_empty() {
            return Err("stamp library is empty — no valid stamps loaded".to_string());
        }

        tracing::info!("stamp library loaded: {} stamps", stamps.len());
        Ok(StampLibrary { stamps })
    }

    /// Sample a random stamp, scaled randomly within [min_scale, max_scale],
    /// optionally rotated by a uniform random angle, then translated to a
    /// random position on the canvas.
    /// The runtime scale multiplies the stamp's base scale (from the CSV).
    /// Returns (translated line segments, runtime scale factor), cropped according to `crop` mode.
    pub fn sample(
        &self,
        canvas_w: f64,
        canvas_h: f64,
        x_dist: &Distribution,
        y_dist: &Distribution,
        crop: StampCrop,
        min_scale: f64,
        max_scale: f64,
        rotate: bool,
    ) -> (Vec<LineSegment>, f64) {
        // Pick a uniformly random stamp
        let idx = fastrand::usize(0..self.stamps.len());
        let stamp = &self.stamps[idx];

        // Random runtime scale within [min_scale, max_scale]
        let runtime_scale = if (max_scale - min_scale).abs() < 1e-9 {
            min_scale
        } else {
            min_scale + fastrand::f64() * (max_scale - min_scale)
        };

        // Random rotation angle (uniform over full circle)
        let (sin_t, cos_t) = if rotate {
            let theta = fastrand::f64() * std::f64::consts::TAU;
            (theta.sin(), theta.cos())
        } else {
            (0.0, 1.0)
        };

        // Stamp center (in base-scaled coordinates)
        let cx = (stamp.bbox.0 + stamp.bbox.2) / 2.0;
        let cy = (stamp.bbox.1 + stamp.bbox.3) / 2.0;

        // Random target position on canvas
        let tx = x_dist.sample() * canvas_w;
        let ty = y_dist.sample() * canvas_h;

        let mut result = Vec::with_capacity(stamp.lines.len());

        for line in &stamp.lines {
            // Center, scale, rotate, then translate to target position
            let dx1 = (line.x1 - cx) * runtime_scale;
            let dy1 = (line.y1 - cy) * runtime_scale;
            let dx2 = (line.x2 - cx) * runtime_scale;
            let dy2 = (line.y2 - cy) * runtime_scale;

            let x1 = dx1 * cos_t - dy1 * sin_t + tx;
            let y1 = dx1 * sin_t + dy1 * cos_t + ty;
            let x2 = dx2 * cos_t - dy2 * sin_t + tx;
            let y2 = dx2 * sin_t + dy2 * cos_t + ty;

            match crop {
                StampCrop::None => {
                    result.push(LineSegment {
                        x1,
                        y1,
                        x2,
                        y2,
                        width: line.width,
                    });
                }
                StampCrop::Drop => {
                    let in_bounds = x1 >= 0.0
                        && x1 <= canvas_w
                        && y1 >= 0.0
                        && y1 <= canvas_h
                        && x2 >= 0.0
                        && x2 <= canvas_w
                        && y2 >= 0.0
                        && y2 <= canvas_h;
                    if in_bounds {
                        result.push(LineSegment {
                            x1,
                            y1,
                            x2,
                            y2,
                            width: line.width,
                        });
                    }
                }
                StampCrop::Clip => {
                    if let Some(clipped) =
                        clip_line_to_rect(x1, y1, x2, y2, 0.0, 0.0, canvas_w, canvas_h)
                    {
                        result.push(LineSegment {
                            x1: clipped.0,
                            y1: clipped.1,
                            x2: clipped.2,
                            y2: clipped.3,
                            width: line.width,
                        });
                    }
                }
            }
        }

        (result, runtime_scale)
    }
}

/// Cohen-Sutherland line clipping against an axis-aligned rectangle.
/// Returns Some((x1, y1, x2, y2)) if any portion of the line is inside,
/// or None if the line is entirely outside.
fn clip_line_to_rect(
    mut x1: f64,
    mut y1: f64,
    mut x2: f64,
    mut y2: f64,
    xmin: f64,
    ymin: f64,
    xmax: f64,
    ymax: f64,
) -> Option<(f64, f64, f64, f64)> {
    const INSIDE: u8 = 0;
    const LEFT: u8 = 1;
    const RIGHT: u8 = 2;
    const BOTTOM: u8 = 4;
    const TOP: u8 = 8;

    let outcode = |x: f64, y: f64| -> u8 {
        let mut code = INSIDE;
        if x < xmin {
            code |= LEFT;
        } else if x > xmax {
            code |= RIGHT;
        }
        if y < ymin {
            code |= BOTTOM;
        } else if y > ymax {
            code |= TOP;
        }
        code
    };

    let mut code1 = outcode(x1, y1);
    let mut code2 = outcode(x2, y2);

    loop {
        if (code1 | code2) == INSIDE {
            // Both inside
            return Some((x1, y1, x2, y2));
        }
        if (code1 & code2) != 0 {
            // Both on same outside side — entirely outside
            return None;
        }

        // Pick the endpoint that is outside
        let code_out = if code1 != INSIDE { code1 } else { code2 };

        let (x, y);
        if code_out & TOP != 0 {
            x = x1 + (x2 - x1) * (ymax - y1) / (y2 - y1);
            y = ymax;
        } else if code_out & BOTTOM != 0 {
            x = x1 + (x2 - x1) * (ymin - y1) / (y2 - y1);
            y = ymin;
        } else if code_out & RIGHT != 0 {
            y = y1 + (y2 - y1) * (xmax - x1) / (x2 - x1);
            x = xmax;
        } else {
            // LEFT
            y = y1 + (y2 - y1) * (xmin - x1) / (x2 - x1);
            x = xmin;
        }

        if code_out == code1 {
            x1 = x;
            y1 = y;
            code1 = outcode(x1, y1);
        } else {
            x2 = x;
            y2 = y;
            code2 = outcode(x2, y2);
        }
    }
}

fn compute_bbox(lines: &[LineSegment]) -> (f64, f64, f64, f64) {
    let mut min_x = f64::INFINITY;
    let mut min_y = f64::INFINITY;
    let mut max_x = f64::NEG_INFINITY;
    let mut max_y = f64::NEG_INFINITY;
    for line in lines {
        min_x = min_x.min(line.x1).min(line.x2);
        min_y = min_y.min(line.y1).min(line.y2);
        max_x = max_x.max(line.x1).max(line.x2);
        max_y = max_y.max(line.y1).max(line.y2);
    }
    (min_x, min_y, max_x, max_y)
}

/// Fetch text content from a local file path or HTTP URL.
fn fetch_text(source: &str) -> Result<String, String> {
    if source.starts_with("http://") || source.starts_with("https://") {
        let response = reqwest::blocking::get(source)
            .map_err(|e| format!("HTTP fetch failed for '{}': {}", source, e))?;
        if !response.status().is_success() {
            return Err(format!("HTTP {} for '{}'", response.status(), source));
        }
        response
            .text()
            .map_err(|e| format!("failed to read response body: {}", e))
    } else {
        let mut file = std::fs::File::open(source)
            .map_err(|e| format!("failed to open '{}': {}", source, e))?;
        let mut text = String::new();
        file.read_to_string(&mut text)
            .map_err(|e| format!("failed to read '{}': {}", source, e))?;
        Ok(text)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_simple_svg() {
        let svg = r#"<svg xmlns="http://www.w3.org/2000/svg">
  <line x1="0" y1="0" x2="1" y2="1" stroke-width="0.05"/>
  <line x1="1" y1="0" x2="0" y2="1" stroke-width="0.1"/>
</svg>"#;
        let stamp = Stamp::from_svg(svg, 1.0, 0.05).unwrap();
        assert_eq!(stamp.lines.len(), 2);
        assert!((stamp.lines[0].x2 - 1.0).abs() < 1e-9);
        // All lines use the fixed stroke width, SVG stroke-width is ignored
        assert!((stamp.lines[0].width - 0.05).abs() < 1e-9);
        assert!((stamp.lines[1].width - 0.05).abs() < 1e-9);
        assert!((stamp.bbox.0 - 0.0).abs() < 1e-9);
        assert!((stamp.bbox.2 - 1.0).abs() < 1e-9);
    }

    #[test]
    fn parse_svg_with_scale() {
        let svg = r#"<svg><line x1="0" y1="0" x2="2" y2="2" stroke-width="0.1"/></svg>"#;
        let stamp = Stamp::from_svg(svg, 0.5, 0.05).unwrap();
        assert_eq!(stamp.lines.len(), 1);
        assert!((stamp.lines[0].x2 - 1.0).abs() < 1e-9);
        // Fixed stroke width, not scaled
        assert!((stamp.lines[0].width - 0.05).abs() < 1e-9);
    }

    #[test]
    fn parse_svg_fixed_stroke_width() {
        let svg = r#"<svg><line x1="0" y1="0" x2="1" y2="1" stroke-width="999"/></svg>"#;
        let stamp = Stamp::from_svg(svg, 1.0, 0.03).unwrap();
        // SVG stroke-width is ignored, uses the fixed width
        assert!((stamp.lines[0].width - 0.03).abs() < 1e-9);
    }

    #[test]
    fn parse_empty_svg_fails() {
        let svg = r#"<svg><rect width="1" height="1"/></svg>"#;
        assert!(Stamp::from_svg(svg, 1.0, 0.05).is_err());
    }

    #[test]
    fn clip_line_fully_inside() {
        let r = clip_line_to_rect(1.0, 1.0, 3.0, 3.0, 0.0, 0.0, 5.0, 5.0);
        assert_eq!(r, Some((1.0, 1.0, 3.0, 3.0)));
    }

    #[test]
    fn clip_line_fully_outside() {
        let r = clip_line_to_rect(-3.0, -3.0, -1.0, -1.0, 0.0, 0.0, 5.0, 5.0);
        assert_eq!(r, None);
    }

    #[test]
    fn clip_line_partial() {
        // Diagonal from (-1, -1) to (1, 1), clipped to [0,0]-[5,5]
        let r = clip_line_to_rect(-1.0, -1.0, 1.0, 1.0, 0.0, 0.0, 5.0, 5.0).unwrap();
        assert!((r.0 - 0.0).abs() < 1e-9); // x1 clipped to 0
        assert!((r.1 - 0.0).abs() < 1e-9); // y1 clipped to 0
        assert!((r.2 - 1.0).abs() < 1e-9); // x2 unchanged
        assert!((r.3 - 1.0).abs() < 1e-9); // y2 unchanged
    }

    #[test]
    fn clip_line_crosses_right_edge() {
        // Horizontal line from (3, 2) to (7, 2), canvas 0-5
        let r = clip_line_to_rect(3.0, 2.0, 7.0, 2.0, 0.0, 0.0, 5.0, 5.0).unwrap();
        assert!((r.0 - 3.0).abs() < 1e-9);
        assert!((r.2 - 5.0).abs() < 1e-9); // clipped to right edge
        assert!((r.1 - 2.0).abs() < 1e-9);
        assert!((r.3 - 2.0).abs() < 1e-9); // y unchanged
    }
}
