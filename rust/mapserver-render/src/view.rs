//! Geographic-to-pixel coordinate transform for rendering, mirroring the
//! `MS_MAP2IMAGE_X`/`MS_MAP2IMAGE_Y` macros in `src/mapserver.h` (used by
//! `msDrawMap`/`msDrawLayer` in `src/mapdraw.c`).

use mapserver_core::primitive::{Point, Rect};

/// Describes how a georeferenced [`Rect`] extent maps onto a
/// `width` x `height` pixel image.
///
/// Uses the "OGC pixel outside edge-to-pixel outside edge" convention
/// (`MS_OWS_CELLSIZE` in `src/mapserver.h`, i.e. `cellsize = extent / size`)
/// rather than the pixel-center convention used by `mapObj`'s own
/// `cellsize` field (`MS_CELLSIZE`, `extent / (size - 1)`). This matches
/// what OWS services (WMS `GetMap` etc.) expect and is the natural choice
/// for a standalone renderer that isn't otherwise tied to `mapObj` state.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MapView {
    pub extent: Rect,
    pub width: u32,
    pub height: u32,
}

impl MapView {
    pub fn new(extent: Rect, width: u32, height: u32) -> Self {
        Self {
            extent,
            width,
            height,
        }
    }

    /// Georeferenced width of one pixel column.
    pub fn cellsize_x(&self) -> f64 {
        (self.extent.maxx - self.extent.minx) / self.width as f64
    }

    /// Georeferenced height of one pixel row.
    pub fn cellsize_y(&self) -> f64 {
        (self.extent.maxy - self.extent.miny) / self.height as f64
    }

    /// Converts a georeferenced [`Point`] to floating-point pixel
    /// coordinates. Direct analogue of `MS_MAP2IMAGE_X`/`_Y`, without the
    /// rounding to nearest integer pixel (`MS_NINT`) so the caller/renderer
    /// backend can decide on its own anti-aliasing/snapping behavior.
    pub fn geo_to_pixel(&self, p: Point) -> (f32, f32) {
        let cx = self.cellsize_x();
        let cy = self.cellsize_y();
        let px = (p.x - self.extent.minx) / cx;
        let py = (self.extent.maxy - p.y) / cy;
        (px as f32, py as f32)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn geo_to_pixel_maps_corners_to_image_edges() {
        let view = MapView::new(
            Rect {
                minx: 0.0,
                miny: 0.0,
                maxx: 100.0,
                maxy: 50.0,
            },
            200,
            100,
        );

        let (x, y) = view.geo_to_pixel(Point::new(0.0, 50.0));
        assert!((x - 0.0).abs() < 1e-6);
        assert!((y - 0.0).abs() < 1e-6);

        let (x, y) = view.geo_to_pixel(Point::new(100.0, 0.0));
        assert!((x - 200.0).abs() < 1e-6);
        assert!((y - 100.0).abs() < 1e-6);

        let (x, y) = view.geo_to_pixel(Point::new(50.0, 25.0));
        assert!((x - 100.0).abs() < 1e-6);
        assert!((y - 50.0).abs() < 1e-6);
    }

    #[test]
    fn cellsize_matches_extent_over_size() {
        let view = MapView::new(
            Rect {
                minx: 0.0,
                miny: 0.0,
                maxx: 10.0,
                maxy: 20.0,
            },
            100,
            200,
        );
        assert!((view.cellsize_x() - 0.1).abs() < 1e-9);
        assert!((view.cellsize_y() - 0.1).abs() < 1e-9);
    }
}
