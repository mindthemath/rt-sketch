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

    /// Rasterize the canvas to a grayscale buffer at the given pixel dimensions.
    /// Returns a Vec<u8> where each byte is a grayscale pixel value (0=black, 255=white).
    pub fn rasterize(&self, px_width: u32, px_height: u32) -> Vec<u8> {
        let mut pixmap = Pixmap::new(px_width, px_height).expect("valid pixmap dimensions");

        // Fill with white
        pixmap.fill(tiny_skia::Color::WHITE);

        let scale_x = px_width as f64 / self.width_cm;
        let scale_y = px_height as f64 / self.height_cm;

        let mut paint = Paint::default();
        paint.set_color_rgba8(0, 0, 0, 255);
        paint.anti_alias = true;

        for line in &self.lines {
            let stroke_px = (line.width * scale_x.min(scale_y)) as f32;
            let mut stroke = Stroke::default();
            stroke.width = stroke_px.max(0.5);
            stroke.line_cap = tiny_skia::LineCap::Round;

            let mut pb = PathBuilder::new();
            pb.move_to(
                (line.x1 * scale_x) as f32,
                (line.y1 * scale_y) as f32,
            );
            pb.line_to(
                (line.x2 * scale_x) as f32,
                (line.y2 * scale_y) as f32,
            );

            if let Some(path) = pb.finish() {
                pixmap.stroke_path(&path, &paint, &stroke, Transform::identity(), None);
            }
        }

        // Convert RGBA to grayscale
        let data = pixmap.data();
        let mut gray = Vec::with_capacity((px_width * px_height) as usize);
        for pixel in data.chunks_exact(4) {
            // Standard luminance conversion
            let g = (0.299 * pixel[0] as f64
                + 0.587 * pixel[1] as f64
                + 0.114 * pixel[2] as f64) as u8;
            gray.push(g);
        }
        gray
    }

    /// Rasterize to a PNG byte buffer (for sending over websocket).
    pub fn rasterize_png(&self, px_width: u32, px_height: u32) -> Vec<u8> {
        let mut pixmap = Pixmap::new(px_width, px_height).expect("valid pixmap dimensions");
        pixmap.fill(tiny_skia::Color::WHITE);

        let scale_x = px_width as f64 / self.width_cm;
        let scale_y = px_height as f64 / self.height_cm;

        let mut paint = Paint::default();
        paint.set_color_rgba8(0, 0, 0, 255);
        paint.anti_alias = true;

        for line in &self.lines {
            let stroke_px = (line.width * scale_x.min(scale_y)) as f32;
            let mut stroke = Stroke::default();
            stroke.width = stroke_px.max(0.5);
            stroke.line_cap = tiny_skia::LineCap::Round;

            let mut pb = PathBuilder::new();
            pb.move_to(
                (line.x1 * scale_x) as f32,
                (line.y1 * scale_y) as f32,
            );
            pb.line_to(
                (line.x2 * scale_x) as f32,
                (line.y2 * scale_y) as f32,
            );

            if let Some(path) = pb.finish() {
                pixmap.stroke_path(&path, &paint, &stroke, Transform::identity(), None);
            }
        }

        pixmap.encode_png().expect("PNG encoding should work")
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
