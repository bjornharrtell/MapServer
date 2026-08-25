//! Projection support with PROJ integration.
//!
//! This module ports the practical core of `src/mapproject.[ch]`:
//! projection-object lifecycle semantics, loading CRS definitions from
//! strings, and point/shape/rectangle reprojection.
//!
//! The rectangle transformation implemented here samples key points and then
//! recomputes an output bounding box. The C implementation has additional
//! dateline/polar special-case logic (`msProjectRectAsPolygon`) that can be
//! added incrementally as needed.

use proj::Proj;

use crate::error::{MapServerError, Result};
use crate::primitive::{compute_bounds, Point, Rect, Shape};

pub const WKP_NONE: i32 = 0;
pub const WKP_LONLAT: i32 = 1;
pub const WKP_GMERC: i32 = 2;

/// Direct Rust analogue to `projectionObj` (subset used by the Rust port).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Projection {
    pub definition: Option<String>,
    pub generation_number: u16,
    pub wellknownprojection: i32,
}

impl Default for Projection {
    fn default() -> Self {
        Self {
            definition: None,
            generation_number: 0,
            wellknownprojection: WKP_NONE,
        }
    }
}

impl Projection {
    pub fn is_empty(&self) -> bool {
        self.definition
            .as_ref()
            .is_none_or(|definition| definition.trim().is_empty())
    }
}

/// Equivalent in spirit to `msLoadProjectionString()` + `msProcessProjection()`.
pub fn load_projection_string(proj: &mut Projection, value: &str) -> Result<()> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err(MapServerError::new(
            11,
            "msLoadProjectionString",
            "projection string is empty",
        ));
    }

    if trimmed.eq_ignore_ascii_case("GEOGRAPHIC") {
        return Err(MapServerError::new(
            11,
            "msProcessProjection",
            "PROJECTION 'GEOGRAPHIC' is not supported; provide an explicit CRS definition",
        ));
    }

    let normalized = trimmed.to_string();
    proj.wellknownprojection = detect_well_known_projection(&normalized);
    proj.definition = Some(normalized);
    proj.generation_number = proj.generation_number.wrapping_add(1);
    Ok(())
}

/// Equivalent in spirit to `msProjectionsDiffer()`.
pub fn projections_differ(a: &Projection, b: &Projection) -> bool {
    match (&a.definition, &b.definition) {
        (Some(da), Some(db)) => da != db,
        _ => false,
    }
}

/// Cache holder analogous to `reprojectionObj`.
pub struct Reprojector {
    transformer: Option<Proj>,
    pub generation_number_in: u16,
    pub generation_number_out: u16,
}

impl Reprojector {
    pub fn new(input: &Projection, output: &Projection) -> Result<Self> {
        let generation_number_in = input.generation_number;
        let generation_number_out = output.generation_number;

        if !projections_differ(input, output) {
            return Ok(Self {
                transformer: None,
                generation_number_in,
                generation_number_out,
            });
        }

        let src = definition_or_default_wgs84(input);
        let dst = definition_or_default_wgs84(output);

        let transformer =
            Proj::new_known_crs(src.as_str(), dst.as_str(), None).ok_or_else(|| {
                MapServerError::new(
                    11,
                    "msProjectCreateReprojector",
                    format!("failed to create PROJ transform from '{src}' to '{dst}'"),
                )
            })?;

        Ok(Self {
            transformer: Some(transformer),
            generation_number_in,
            generation_number_out,
        })
    }

    /// Equivalent in spirit to `msProjectPointEx()`.
    pub fn project_point(&self, point: &mut Point) -> Result<()> {
        let Some(transformer) = &self.transformer else {
            return Ok(());
        };

        let Some((x, y)) = transformer.convert((point.x, point.y)) else {
            return Err(MapServerError::new(
                11,
                "msProjectPointEx",
                "PROJ transformation failed for point",
            ));
        };

        point.x = x;
        point.y = y;
        Ok(())
    }

    /// Equivalent in spirit to `msProjectRect()` with simplified bbox
    /// computation by sampled vertices.
    pub fn project_rect(&self, rect: &mut Rect) -> Result<()> {
        let mut samples = [
            Point::new(rect.minx, rect.miny),
            Point::new(rect.maxx, rect.miny),
            Point::new(rect.maxx, rect.maxy),
            Point::new(rect.minx, rect.maxy),
            Point::new((rect.minx + rect.maxx) * 0.5, rect.miny),
            Point::new((rect.minx + rect.maxx) * 0.5, rect.maxy),
            Point::new(rect.minx, (rect.miny + rect.maxy) * 0.5),
            Point::new(rect.maxx, (rect.miny + rect.maxy) * 0.5),
            Point::new((rect.minx + rect.maxx) * 0.5, (rect.miny + rect.maxy) * 0.5),
        ];

        for point in &mut samples {
            self.project_point(point)?;
        }

        rect.minx = samples[0].x;
        rect.maxx = samples[0].x;
        rect.miny = samples[0].y;
        rect.maxy = samples[0].y;

        for point in &samples[1..] {
            rect.minx = rect.minx.min(point.x);
            rect.maxx = rect.maxx.max(point.x);
            rect.miny = rect.miny.min(point.y);
            rect.maxy = rect.maxy.max(point.y);
        }

        Ok(())
    }

    /// Equivalent in spirit to `msProjectShapeEx()`.
    pub fn project_shape(&self, shape: &mut Shape) -> Result<()> {
        for line in &mut shape.lines {
            for point in &mut line.points {
                self.project_point(point)?;
            }
        }
        compute_bounds(shape);
        Ok(())
    }
}

fn definition_or_default_wgs84(proj: &Projection) -> String {
    proj.definition
        .clone()
        .filter(|definition| !definition.trim().is_empty())
        .unwrap_or_else(|| "EPSG:4326".to_string())
}

fn detect_well_known_projection(definition: &str) -> i32 {
    let lower = definition.to_ascii_lowercase();
    if lower.contains("epsg:4326") {
        WKP_LONLAT
    } else if lower.contains("epsg:3857") {
        WKP_GMERC
    } else {
        WKP_NONE
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::primitive::{Line, ShapeType};

    #[test]
    fn load_projection_sets_generation_and_wkp() {
        let mut projection = Projection::default();
        load_projection_string(&mut projection, "EPSG:4326").expect("valid epsg code");
        assert_eq!(projection.generation_number, 1);
        assert_eq!(projection.wellknownprojection, WKP_LONLAT);
    }

    #[test]
    fn projections_differ_matches_definition_comparison() {
        let mut a = Projection::default();
        let mut b = Projection::default();

        load_projection_string(&mut a, "EPSG:4326").expect("valid source projection");
        load_projection_string(&mut b, "EPSG:3857").expect("valid destination projection");

        assert!(projections_differ(&a, &b));
        assert!(!projections_differ(&a, &a));
    }

    #[test]
    fn project_point_wgs84_to_web_mercator() {
        let mut src = Projection::default();
        let mut dst = Projection::default();
        load_projection_string(&mut src, "EPSG:4326").expect("valid source projection");
        load_projection_string(&mut dst, "EPSG:3857").expect("valid destination projection");

        let reprojector = Reprojector::new(&src, &dst).expect("reprojector creation");

        let mut point = Point::new(0.0, 0.0);
        reprojector
            .project_point(&mut point)
            .expect("point transformation succeeds");

        assert!(point.x.abs() < 1e-9);
        assert!(point.y.abs() < 1e-9);
    }

    #[test]
    fn project_rect_computes_valid_bounds() {
        let mut src = Projection::default();
        let mut dst = Projection::default();
        load_projection_string(&mut src, "EPSG:4326").expect("valid source projection");
        load_projection_string(&mut dst, "EPSG:3857").expect("valid destination projection");

        let reprojector = Reprojector::new(&src, &dst).expect("reprojector creation");
        let mut rect = Rect {
            minx: -1.0,
            miny: -1.0,
            maxx: 1.0,
            maxy: 1.0,
        };

        reprojector
            .project_rect(&mut rect)
            .expect("rect transformation succeeds");

        assert!(rect.minx < 0.0);
        assert!(rect.maxx > 0.0);
        assert!(rect.miny < 0.0);
        assert!(rect.maxy > 0.0);
        assert!(rect.minx < rect.maxx);
        assert!(rect.miny < rect.maxy);
    }

    #[test]
    fn project_shape_updates_point_coordinates_and_bounds() {
        let mut src = Projection::default();
        let mut dst = Projection::default();
        load_projection_string(&mut src, "EPSG:4326").expect("valid source projection");
        load_projection_string(&mut dst, "EPSG:3857").expect("valid destination projection");

        let reprojector = Reprojector::new(&src, &dst).expect("reprojector creation");

        let mut shape = Shape {
            lines: vec![Line::new(vec![Point::new(0.0, 0.0), Point::new(1.0, 1.0)])],
            shape_type: ShapeType::Line,
            ..Default::default()
        };

        reprojector
            .project_shape(&mut shape)
            .expect("shape transformation succeeds");

        assert!(shape.lines[0].points[1].x > 100000.0);
        assert!(shape.lines[0].points[1].y > 100000.0);
        assert!(shape.bounds.maxx > shape.bounds.minx);
        assert!(shape.bounds.maxy > shape.bounds.miny);
    }

    #[test]
    fn empty_projection_falls_back_to_wgs84_like_mapserver_null_projection() {
        let src = Projection::default();
        let mut dst = Projection::default();
        load_projection_string(&mut dst, "EPSG:3857").expect("valid destination projection");

        let reprojector = Reprojector::new(&src, &dst).expect("reprojector creation");
        let mut point = Point::new(0.5, 0.5);

        reprojector
            .project_point(&mut point)
            .expect("point transformation succeeds");

        assert!(point.x > 0.0);
        assert!(point.y > 0.0);
    }
}
