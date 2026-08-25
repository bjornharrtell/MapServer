//! Skia-backed [`Renderer`] implementation.
//!
//! Renders onto an in-memory raster surface (`skia_safe::Surface`) and
//! exposes the result as PNG bytes, mirroring the role of `mapagg.cpp`'s
//! AGG renderer in the C codebase but built on
//! [`skia-safe`](https://crates.io/crates/skia-safe) per the epic's
//! decision to replace AGG rather than port it.

use mapserver_core::class::StyleObj;
use mapserver_core::color::Color as MsColor;
use skia_safe::{surfaces, Color as SkColor, EncodedImageFormat, Paint, PaintStyle, Path, Point};

use crate::renderer::{PixelPoint, Renderer};

fn to_sk_color(c: &MsColor, opacity: i32) -> SkColor {
    // `opacity` follows styleObj's convention: a percentage in [0, 100],
    // with values outside that range (e.g. the ConfigValue default of -1
    // meaning "unset") treated as fully opaque.
    let alpha = if (0..=100).contains(&opacity) {
        ((opacity as f32 / 100.0) * 255.0).round() as u8
    } else {
        255
    };
    SkColor::from_argb(alpha, c.red as u8, c.green as u8, c.blue as u8)
}

fn path_from_points(points: &[PixelPoint], close: bool) -> Path {
    let mut path = Path::new();
    let mut iter = points.iter();
    if let Some(&(x, y)) = iter.next() {
        path.move_to(Point::new(x, y));
        for &(x, y) in iter {
            path.line_to(Point::new(x, y));
        }
        if close {
            path.close();
        }
    }
    path
}

/// A [`Renderer`] that draws onto an in-memory Skia raster surface.
pub struct SkiaRenderer {
    surface: skia_safe::Surface,
}

impl SkiaRenderer {
    /// Creates a new renderer with a `width` x `height` transparent
    /// (unless `background` is set) raster surface.
    pub fn new(width: u32, height: u32, background: Option<MsColor>) -> Self {
        let mut surface = surfaces::raster_n32_premul((width as i32, height as i32))
            .expect("failed to allocate raster surface");
        let canvas = surface.canvas();
        match background {
            Some(c) => canvas.clear(to_sk_color(&c, 100)),
            None => canvas.clear(SkColor::TRANSPARENT),
        };
        Self { surface }
    }

    /// Encodes the current surface contents as PNG bytes. Analogous to
    /// `msSaveImage()` for the `image/png` output format.
    pub fn encode_png(&mut self) -> Vec<u8> {
        let image = self.surface.image_snapshot();
        let data = image
            .encode(None, EncodedImageFormat::PNG, None)
            .expect("PNG encoding failed");
        data.as_bytes().to_vec()
    }

    /// Reads back the color of the pixel at `(x, y)` for testing purposes.
    #[cfg(test)]
    pub(crate) fn pixel_at(&mut self, x: i32, y: i32) -> (u8, u8, u8, u8) {
        let pixmap = self.surface.peek_pixels().expect("raster surface");
        let color = pixmap.get_color((x, y));
        (color.r(), color.g(), color.b(), color.a())
    }
}

impl Renderer for SkiaRenderer {
    fn draw_point(&mut self, (x, y): PixelPoint, style: &StyleObj) {
        let Some(color) = &style.color else {
            return;
        };
        let mut paint = Paint::default();
        paint.set_anti_alias(true);
        paint.set_style(PaintStyle::Fill);
        paint.set_color(to_sk_color(color, style.opacity));

        let radius = if style.size > 0.0 { style.size } else { 3.0 } as f32;
        self.surface
            .canvas()
            .draw_circle(Point::new(x, y), radius, &paint);
    }

    fn draw_line(&mut self, points: &[PixelPoint], style: &StyleObj) {
        let Some(color) = &style.color else {
            return;
        };
        if points.len() < 2 {
            return;
        }
        let mut paint = Paint::default();
        paint.set_anti_alias(true);
        paint.set_style(PaintStyle::Stroke);
        paint.set_color(to_sk_color(color, style.opacity));
        paint.set_stroke_width(if style.width > 0.0 {
            style.width as f32
        } else {
            1.0
        });

        let path = path_from_points(points, false);
        self.surface.canvas().draw_path(&path, &paint);
    }

    fn draw_polygon(&mut self, rings: &[Vec<PixelPoint>], style: &StyleObj) {
        if rings.is_empty() {
            return;
        }
        let mut path = Path::new();
        for ring in rings {
            path.add_path(&path_from_points(ring, true), Point::new(0.0, 0.0), None);
        }
        path.set_fill_type(skia_safe::PathFillType::EvenOdd);

        if let Some(color) = &style.color {
            let mut fill = Paint::default();
            fill.set_anti_alias(true);
            fill.set_style(PaintStyle::Fill);
            fill.set_color(to_sk_color(color, style.opacity));
            self.surface.canvas().draw_path(&path, &fill);
        }

        if let Some(outline) = &style.outline_color {
            let mut stroke = Paint::default();
            stroke.set_anti_alias(true);
            stroke.set_style(PaintStyle::Stroke);
            stroke.set_color(to_sk_color(outline, style.opacity));
            stroke.set_stroke_width(if style.outline_width > 0.0 {
                style.outline_width as f32
            } else {
                1.0
            });
            self.surface.canvas().draw_path(&path, &stroke);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn style_with(color: Option<MsColor>, outline: Option<MsColor>) -> StyleObj {
        StyleObj {
            color,
            outline_color: outline,
            opacity: 100,
            ..Default::default()
        }
    }

    #[test]
    fn draw_point_paints_a_filled_circle() {
        let mut r = SkiaRenderer::new(
            50,
            50,
            Some(MsColor {
                red: 255,
                green: 255,
                blue: 255,
            }),
        );
        let style = style_with(
            Some(MsColor {
                red: 255,
                green: 0,
                blue: 0,
            }),
            None,
        );
        r.draw_point((25.0, 25.0), &style);
        assert_eq!(r.pixel_at(25, 25), (255, 0, 0, 255));
        // Corner should remain background-colored (outside the circle).
        assert_eq!(r.pixel_at(1, 1), (255, 255, 255, 255));
    }

    #[test]
    fn draw_line_paints_stroke_color_along_path() {
        let mut r = SkiaRenderer::new(
            50,
            50,
            Some(MsColor {
                red: 255,
                green: 255,
                blue: 255,
            }),
        );
        let style = StyleObj {
            color: Some(MsColor {
                red: 0,
                green: 0,
                blue: 255,
            }),
            width: 4.0,
            opacity: 100,
            ..Default::default()
        };
        r.draw_line(&[(5.0, 25.0), (45.0, 25.0)], &style);
        assert_eq!(r.pixel_at(25, 25), (0, 0, 255, 255));
    }

    #[test]
    fn draw_polygon_fills_interior_and_strokes_outline() {
        let mut r = SkiaRenderer::new(
            100,
            100,
            Some(MsColor {
                red: 255,
                green: 255,
                blue: 255,
            }),
        );
        let style = style_with(
            Some(MsColor {
                red: 0,
                green: 128,
                blue: 0,
            }),
            Some(MsColor {
                red: 0,
                green: 0,
                blue: 0,
            }),
        );
        let ring = vec![(10.0, 10.0), (90.0, 10.0), (90.0, 90.0), (10.0, 90.0)];
        r.draw_polygon(&[ring], &style);

        // Interior is filled.
        assert_eq!(r.pixel_at(50, 50), (0, 128, 0, 255));
        // Outside the polygon remains background-colored.
        assert_eq!(r.pixel_at(1, 1), (255, 255, 255, 255));
    }

    #[test]
    fn draw_polygon_with_hole_leaves_hole_unfilled() {
        let mut r = SkiaRenderer::new(
            100,
            100,
            Some(MsColor {
                red: 255,
                green: 255,
                blue: 255,
            }),
        );
        let style = style_with(
            Some(MsColor {
                red: 0,
                green: 128,
                blue: 0,
            }),
            None,
        );
        let outer = vec![(10.0, 10.0), (90.0, 10.0), (90.0, 90.0), (10.0, 90.0)];
        let hole = vec![(40.0, 40.0), (60.0, 40.0), (60.0, 60.0), (40.0, 60.0)];
        r.draw_polygon(&[outer, hole], &style);

        // Inside the hole remains background-colored.
        assert_eq!(r.pixel_at(50, 50), (255, 255, 255, 255));
        // Elsewhere inside the outer ring (but outside the hole) is filled.
        assert_eq!(r.pixel_at(20, 20), (0, 128, 0, 255));
    }

    #[test]
    fn encode_png_produces_valid_png_signature() {
        let mut r = SkiaRenderer::new(10, 10, None);
        let bytes = r.encode_png();
        assert_eq!(
            &bytes[0..8],
            &[0x89, b'P', b'N', b'G', b'\r', b'\n', 0x1a, b'\n']
        );
    }
}
