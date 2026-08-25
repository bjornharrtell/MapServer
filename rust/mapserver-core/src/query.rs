//! Query engine (spatial and attribute queries).
//!
//! Incremental Rust port of the per-layer query execution logic in
//! `src/mapquery.cpp` (`msQueryByPoint()`, `msQueryByRect()`,
//! `msQueryByAttributes()`, `msQueryByShape()`), scoped to the parts that
//! operate on a single [`LayerDataSource`] and its features.
//!
//! The C implementation interleaves this logic with `mapObj`/`layerObj`
//! concerns that don't have a Rust equivalent yet (pixel/georeferenced
//! tolerance conversion via `map->cellsize`, per-layer scale/geowidth
//! gating, raster layers, result-set persistence, and template-driven
//! rendering). Those are left to the future rendering-pipeline/OWS-service
//! ports; this module focuses on the feature-matching core: given a data
//! source and a query, which features match and in what order.

use crate::class::{get_class, ClassObj, Expression};
use crate::datasource::{Feature, LayerDataSource, QueryOptions};
use crate::primitive::{
    distance_point_to_shape, intersect_point_polygon, intersect_segments, rect_overlap, Point,
    Rect, Shape, ShapeType,
};

/// A single matched feature, with the classification result (mirroring
/// `resultCacheMemberObj.classindex`, computed via
/// [`crate::class::get_class`]) and, for point queries, the distance from
/// the query point (mirroring the distance-based ordering in
/// `msQueryByPoint()`).
#[derive(Debug, Clone, PartialEq)]
pub struct QueryResult {
    pub feature: Feature,
    pub classindex: Option<usize>,
    pub distance: Option<f64>,
}

/// Shared classification context for query functions that need to resolve
/// each matched feature's class, mirroring `layer->classitem`/
/// `layer->class[]` and the scale denominator used by
/// `msShapeGetClass()`.
#[derive(Debug, Clone, Copy, Default)]
pub struct ClassificationContext<'a> {
    pub classes: &'a [ClassObj],
    pub classitem: Option<&'a str>,
    pub scaledenom: Option<f64>,
}

impl<'a> ClassificationContext<'a> {
    fn classify(&self, feature: &Feature) -> Option<usize> {
        if self.classes.is_empty() {
            return None;
        }
        get_class(
            self.classes,
            self.classitem,
            self.scaledenom,
            feature.geometry.shape_type,
            &feature.attributes,
        )
    }
}

fn to_result(feature: Feature, ctx: &ClassificationContext, distance: Option<f64>) -> QueryResult {
    let classindex = ctx.classify(&feature);
    QueryResult {
        feature,
        classindex,
        distance,
    }
}

/// Port of `msQueryByRect()`: return every feature in `source` whose
/// geometry bounds overlap `rect`.
///
/// `max_results` mirrors `layer->maxfeatures`/`map->query.maxfeatures`.
pub fn query_by_rect(
    source: &dyn LayerDataSource,
    rect: Rect,
    ctx: &ClassificationContext,
    max_results: Option<usize>,
) -> Vec<QueryResult> {
    let options = QueryOptions {
        bbox: Some(rect),
        limit: max_results,
        ..Default::default()
    };
    source
        .query(&options)
        .into_iter()
        .map(|f| to_result(f, ctx, None))
        .collect()
}

/// Port of `msQueryByPoint()` (`MS_MULTIPLE`/`MS_SINGLE` result modes
/// combined): return every feature within `tolerance` (a georeferenced-unit
/// distance — the caller is responsible for any pixel-to-georeferenced-unit
/// conversion normally done via `map->cellsize`, since this module has no
/// map/pixel context) of `point`, nearest first.
///
/// When `single` is `true`, only the single nearest match is returned
/// (mirroring `MS_QUERY_SINGLE`).
pub fn query_by_point(
    source: &dyn LayerDataSource,
    point: Point,
    tolerance: f64,
    single: bool,
    ctx: &ClassificationContext,
    max_results: Option<usize>,
) -> Vec<QueryResult> {
    let search_rect = Rect {
        minx: point.x - tolerance,
        miny: point.y - tolerance,
        maxx: point.x + tolerance,
        maxy: point.y + tolerance,
    };

    let mut matches: Vec<QueryResult> = source
        .query(&QueryOptions {
            bbox: Some(search_rect),
            ..Default::default()
        })
        .into_iter()
        .filter_map(|feature| {
            let distance = distance_point_to_shape(&point, &feature.geometry);
            if distance <= tolerance {
                Some(to_result(feature, ctx, Some(distance)))
            } else {
                None
            }
        })
        .collect();

    matches.sort_by(|a, b| {
        a.distance
            .partial_cmp(&b.distance)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    if single {
        matches.truncate(1);
    } else if let Some(limit) = max_results {
        matches.truncate(limit);
    }

    matches
}

/// Port of `msQueryByAttributes()`: return every feature whose `item`
/// attribute matches `expression` (mirroring `msEvalExpression()` via
/// [`Expression::evaluate`]).
pub fn query_by_attributes(
    source: &dyn LayerDataSource,
    item: &str,
    expression: &Expression,
    ctx: &ClassificationContext,
    max_results: Option<usize>,
) -> Vec<QueryResult> {
    let mut out = Vec::new();
    for feature in source.query(&QueryOptions::default()) {
        if expression.evaluate(Some(item), &feature.attributes) {
            out.push(to_result(feature, ctx, None));
            if max_results.is_some_and(|limit| out.len() >= limit) {
                break;
            }
        }
    }
    out
}

/// Port of `msQueryByShape()`: return every feature whose geometry
/// intersects `query_shape` (a point, line, or polygon), narrowing
/// candidates with a bounding-box pre-filter first (as the C
/// implementation does via `msComputeBounds()` + layer `WhichShapes`).
pub fn query_by_shape(
    source: &dyn LayerDataSource,
    query_shape: &Shape,
    ctx: &ClassificationContext,
    max_results: Option<usize>,
) -> Vec<QueryResult> {
    let mut out = Vec::new();
    let options = QueryOptions {
        bbox: Some(query_shape.bounds),
        ..Default::default()
    };
    for feature in source.query(&options) {
        if shapes_intersect(query_shape, &feature.geometry) {
            out.push(to_result(feature, ctx, None));
            if max_results.is_some_and(|limit| out.len() >= limit) {
                break;
            }
        }
    }
    out
}

/// Returns `true` if geometries `a` and `b` intersect.
///
/// This is a general-purpose replacement for the C code's per-shape-type
/// dispatch (`msIntersectMultiPolygons`/segment intersection/point-in-ring
/// helpers spread across `mapsearch.c`), implemented in terms of the shared
/// primitives ([`intersect_point_polygon`], [`intersect_segments`]):
/// bounding-box overlap, vertex-in-polygon containment (either direction),
/// segment-to-segment crossing, and point coincidence.
pub fn shapes_intersect(a: &Shape, b: &Shape) -> bool {
    if !rect_overlap(&a.bounds, &b.bounds) {
        return false;
    }

    if a.shape_type == ShapeType::Polygon && any_vertex_inside(b, a) {
        return true;
    }
    if b.shape_type == ShapeType::Polygon && any_vertex_inside(a, b) {
        return true;
    }

    for line_a in &a.lines {
        for window_a in line_a.points.windows(2) {
            let [p1, p2] = window_a else { continue };
            for line_b in &b.lines {
                for window_b in line_b.points.windows(2) {
                    let [p3, p4] = window_b else { continue };
                    if intersect_segments(p1, p2, p3, p4) {
                        return true;
                    }
                }
            }
        }
    }

    if a.shape_type == ShapeType::Point || b.shape_type == ShapeType::Point {
        for line_a in &a.lines {
            for pa in &line_a.points {
                for line_b in &b.lines {
                    for pb in &line_b.points {
                        if points_coincide(pa, pb) {
                            return true;
                        }
                    }
                }
            }
        }
    }

    false
}

fn any_vertex_inside(candidate: &Shape, polygon: &Shape) -> bool {
    candidate.lines.iter().any(|line| {
        line.points
            .iter()
            .any(|p| intersect_point_polygon(p, polygon))
    })
}

fn points_coincide(a: &Point, b: &Point) -> bool {
    (a.x - b.x).abs() < f64::EPSILON && (a.y - b.y).abs() < f64::EPSILON
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::datasource::{AttributeValue, MemoryDataSource};
    use crate::primitive::{compute_bounds, Line, Point as P};
    use std::collections::BTreeMap;

    fn make_polygon(ring: &[(f64, f64)]) -> Shape {
        let points = ring.iter().map(|&(x, y)| P::new(x, y)).collect();
        let mut shape = Shape {
            lines: vec![Line::new(points)],
            shape_type: ShapeType::Polygon,
            ..Default::default()
        };
        compute_bounds(&mut shape);
        shape
    }

    fn make_point(x: f64, y: f64) -> Shape {
        let mut shape = Shape {
            lines: vec![Line::new(vec![P::new(x, y)])],
            shape_type: ShapeType::Point,
            ..Default::default()
        };
        compute_bounds(&mut shape);
        shape
    }

    fn make_line(points: &[(f64, f64)]) -> Shape {
        let pts = points.iter().map(|&(x, y)| P::new(x, y)).collect();
        let mut shape = Shape {
            lines: vec![Line::new(pts)],
            shape_type: ShapeType::Line,
            ..Default::default()
        };
        compute_bounds(&mut shape);
        shape
    }

    fn feature(id: u64, geometry: Shape, kind: &str) -> Feature {
        let mut attributes = BTreeMap::new();
        attributes.insert("kind".to_string(), AttributeValue::String(kind.to_string()));
        Feature {
            id: Some(id),
            geometry,
            attributes,
        }
    }

    fn square(minx: f64, miny: f64, maxx: f64, maxy: f64) -> Shape {
        make_polygon(&[
            (minx, miny),
            (maxx, miny),
            (maxx, maxy),
            (minx, maxy),
            (minx, miny),
        ])
    }

    const NO_CLASSES: ClassificationContext = ClassificationContext {
        classes: &[],
        classitem: None,
        scaledenom: None,
    };

    #[test]
    fn query_by_rect_returns_overlapping_features_only() {
        let ds = MemoryDataSource::new(vec![
            feature(1, square(0.0, 0.0, 10.0, 10.0), "a"),
            feature(2, square(100.0, 100.0, 110.0, 110.0), "b"),
        ]);

        let results = query_by_rect(
            &ds,
            Rect {
                minx: -5.0,
                miny: -5.0,
                maxx: 5.0,
                maxy: 5.0,
            },
            &NO_CLASSES,
            None,
        );

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].feature.id, Some(1));
    }

    #[test]
    fn query_by_point_orders_by_distance_and_respects_tolerance() {
        let ds = MemoryDataSource::new(vec![
            feature(1, make_point(10.0, 0.0), "near"),
            feature(2, make_point(2.0, 0.0), "nearest"),
            feature(3, make_point(1000.0, 0.0), "far"),
        ]);

        let results = query_by_point(&ds, P::new(0.0, 0.0), 20.0, false, &NO_CLASSES, None);

        assert_eq!(results.len(), 2);
        assert_eq!(results[0].feature.id, Some(2));
        assert_eq!(results[1].feature.id, Some(1));
        assert!(results[0].distance.unwrap() < results[1].distance.unwrap());
    }

    #[test]
    fn query_by_point_single_returns_only_nearest() {
        let ds = MemoryDataSource::new(vec![
            feature(1, make_point(10.0, 0.0), "near"),
            feature(2, make_point(2.0, 0.0), "nearest"),
        ]);

        let results = query_by_point(&ds, P::new(0.0, 0.0), 20.0, true, &NO_CLASSES, None);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].feature.id, Some(2));
    }

    #[test]
    fn query_by_attributes_filters_by_expression() {
        let ds = MemoryDataSource::new(vec![
            feature(1, make_point(0.0, 0.0), "road"),
            feature(2, make_point(1.0, 1.0), "water"),
        ]);

        let expr = Expression::String {
            value: "water".to_string(),
            case_insensitive: false,
        };
        let results = query_by_attributes(&ds, "kind", &expr, &NO_CLASSES, None);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].feature.id, Some(2));
    }

    #[test]
    fn query_by_shape_matches_overlapping_polygons() {
        let ds = MemoryDataSource::new(vec![
            feature(1, square(0.0, 0.0, 10.0, 10.0), "overlap"),
            feature(2, square(100.0, 100.0, 110.0, 110.0), "far"),
        ]);

        let query_shape = square(5.0, 5.0, 15.0, 15.0);
        let results = query_by_shape(&ds, &query_shape, &NO_CLASSES, None);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].feature.id, Some(1));
    }

    #[test]
    fn shapes_intersect_detects_point_in_polygon() {
        let poly = square(0.0, 0.0, 10.0, 10.0);
        let inside = make_point(5.0, 5.0);
        let outside = make_point(50.0, 50.0);
        assert!(shapes_intersect(&poly, &inside));
        assert!(!shapes_intersect(&poly, &outside));
    }

    #[test]
    fn shapes_intersect_detects_crossing_lines_with_no_shared_vertex() {
        let a = make_line(&[(0.0, 0.0), (10.0, 10.0)]);
        let b = make_line(&[(0.0, 10.0), (10.0, 0.0)]);
        assert!(shapes_intersect(&a, &b));

        let c = make_line(&[(100.0, 100.0), (110.0, 110.0)]);
        assert!(!shapes_intersect(&a, &c));
    }

    #[test]
    fn shapes_intersect_detects_overlapping_polygons_without_contained_vertices() {
        // A plus-sign-like overlap: two rectangles that cross but neither
        // has a vertex inside the other.
        let horizontal = square(0.0, 4.0, 10.0, 6.0);
        let vertical = square(4.0, 0.0, 6.0, 10.0);
        assert!(shapes_intersect(&horizontal, &vertical));
    }

    #[test]
    fn classification_context_assigns_matching_class() {
        let mut classes_map = crate::mapfile::parse_mapfile(
            r#"
MAP
  LAYER
    CLASS
      NAME "roads"
      EXPRESSION "road"
    END
  END
END
"#,
        )
        .unwrap();
        let layer = classes_map
            .children_of_kind("LAYER")
            .next()
            .unwrap()
            .clone();
        let classes: Vec<ClassObj> = layer
            .children_of_kind("CLASS")
            .map(|c| crate::class::parse_class(c).unwrap())
            .collect();
        classes_map.children.clear();

        let ctx = ClassificationContext {
            classes: &classes,
            classitem: Some("kind"),
            scaledenom: None,
        };

        let ds = MemoryDataSource::new(vec![feature(1, make_point(0.0, 0.0), "road")]);
        let results = query_by_rect(
            &ds,
            Rect {
                minx: -1.0,
                miny: -1.0,
                maxx: 1.0,
                maxy: 1.0,
            },
            &ctx,
            None,
        );

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].classindex, Some(0));
    }
}
