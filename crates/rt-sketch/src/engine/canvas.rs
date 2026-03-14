use serde::{Deserialize, Serialize};
use tiny_skia::{Paint, PathBuilder, Pixmap, Stroke, Transform};

/// A single line segment in canvas coordinates (cm).
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct LineSegment {
    pub x1: f64,
    pub y1: f64,
    pub x2: f64,
    pub y2: f64,
    pub width: f64,
}

impl LineSegment {
    pub fn length(&self) -> f64 {
        let dx = self.x2 - self.x1;
        let dy = self.y2 - self.y1;
        (dx * dx + dy * dy).sqrt()
    }
}

/// The canvas: a collection of line segments drawn on a white background.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Canvas {
    pub width_cm: f64,
    pub height_cm: f64,
    pub lines: Vec<LineSegment>,
}

impl Canvas {
    pub fn new(width_cm: f64, height_cm: f64) -> Self {
        Self {
            width_cm,
            height_cm,
            lines: Vec::new(),
        }
    }

    pub fn add_line(&mut self, line: LineSegment) {
        self.lines.push(line);
    }

    /// Rasterize a single line onto an existing RGBA pixmap.
    pub fn rasterize_line_onto(
        pixmap: &mut Pixmap,
        line: &LineSegment,
        scale_x: f64,
        scale_y: f64,
    ) {
        let mut paint = Paint::default();
        paint.set_color_rgba8(0, 0, 0, 255);
        paint.anti_alias = true;

        let stroke_px = (line.width * scale_x.min(scale_y)) as f32;
        let mut stroke = Stroke::default();
        stroke.width = stroke_px.max(0.5);
        stroke.line_cap = tiny_skia::LineCap::Round;

        let mut pb = PathBuilder::new();
        pb.move_to((line.x1 * scale_x) as f32, (line.y1 * scale_y) as f32);
        pb.line_to((line.x2 * scale_x) as f32, (line.y2 * scale_y) as f32);

        if let Some(path) = pb.finish() {
            pixmap.stroke_path(&path, &paint, &stroke, Transform::identity(), None);
        }
    }

    /// Rasterize multiple lines onto an existing RGBA pixmap.
    pub fn rasterize_lines_onto(
        pixmap: &mut Pixmap,
        lines: &[LineSegment],
        scale_x: f64,
        scale_y: f64,
    ) {
        for line in lines {
            Self::rasterize_line_onto(pixmap, line, scale_x, scale_y);
        }
    }

    /// Convert an RGBA Pixmap to a grayscale buffer.
    pub fn pixmap_to_gray(pixmap: &Pixmap) -> Vec<u8> {
        let data = pixmap.data();
        let mut gray = Vec::with_capacity(data.len() / 4);
        for pixel in data.chunks_exact(4) {
            let g =
                (0.299 * pixel[0] as f64 + 0.587 * pixel[1] as f64 + 0.114 * pixel[2] as f64) as u8;
            gray.push(g);
        }
        gray
    }

    /// Export as SVG string.
    pub fn to_svg(&self) -> String {
        let mut svg = format!(
            r#"<svg xmlns="http://www.w3.org/2000/svg" width="{}cm" height="{}cm" viewBox="0 0 {} {}">"#,
            self.width_cm, self.height_cm, self.width_cm, self.height_cm
        );
        svg.push('\n');
        svg.push_str(&format!(
            r#"  <rect width="{}" height="{}" fill="white"/>"#,
            self.width_cm, self.height_cm
        ));
        svg.push('\n');

        for line in &self.lines {
            svg.push_str(&format!(
                r#"  <line x1="{:.4}" y1="{:.4}" x2="{:.4}" y2="{:.4}" stroke="black" stroke-width="{:.4}" stroke-linecap="round"/>"#,
                line.x1, line.y1, line.x2, line.y2, line.width
            ));
            svg.push('\n');
        }

        svg.push_str("</svg>");
        svg
    }
}
