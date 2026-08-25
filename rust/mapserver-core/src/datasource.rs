//! Layer data-source abstraction.
//!
//! This is an initial Rust port step for the layer backend architecture used
//! throughout MapServer (`msLayer*` APIs): a trait for feature providers and a
//! reference in-memory implementation used by tests and higher-level modules.

use std::collections::BTreeMap;

use crate::primitive::{rect_overlap, Rect, Shape};

/// Simple attribute value model used by the first backend abstraction pass.
#[derive(Debug, Clone, PartialEq)]
pub enum AttributeValue {
    String(String),
    Number(f64),
    Bool(bool),
}

/// A feature returned by a layer data source.
#[derive(Debug, Clone, PartialEq)]
pub struct Feature {
    pub id: Option<u64>,
    pub geometry: Shape,
    pub attributes: BTreeMap<String, AttributeValue>,
}

impl Feature {
    pub fn attribute_str(&self, name: &str) -> Option<&str> {
        self.attributes.get(name).and_then(|value| match value {
            AttributeValue::String(string) => Some(string.as_str()),
            _ => None,
        })
    }
}

/// Basic query options shared across backends.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct QueryOptions {
    pub bbox: Option<Rect>,
    pub limit: Option<usize>,
    pub equals_filters: Vec<(String, String)>,
}

/// Abstraction for layer feature providers.
pub trait LayerDataSource {
    fn backend_name(&self) -> &'static str;
    fn query(&self, options: &QueryOptions) -> Vec<Feature>;
}

/// In-memory backend for tests and early integration.
#[derive(Debug, Clone, Default)]
pub struct MemoryDataSource {
    pub features: Vec<Feature>,
}

impl MemoryDataSource {
    pub fn new(features: Vec<Feature>) -> Self {
        Self { features }
    }
}

impl LayerDataSource for MemoryDataSource {
    fn backend_name(&self) -> &'static str {
        "memory"
    }

    fn query(&self, options: &QueryOptions) -> Vec<Feature> {
        let mut out = Vec::new();

        for feature in &self.features {
            if let Some(bbox) = options.bbox {
                if !rect_overlap(&feature.geometry.bounds, &bbox) {
                    continue;
                }
            }

            if !options.equals_filters.iter().all(|(name, expected)| {
                feature
                    .attribute_str(name)
                    .is_some_and(|actual| actual == expected)
            }) {
                continue;
            }

            out.push(feature.clone());
            if let Some(limit) = options.limit {
                if out.len() >= limit {
                    break;
                }
            }
        }

        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::primitive::{compute_bounds, Line, Point, Shape, ShapeType};

    fn make_box(minx: f64, miny: f64, maxx: f64, maxy: f64) -> Shape {
        let mut shape = Shape {
            lines: vec![Line::new(vec![
                Point::new(minx, miny),
                Point::new(maxx, miny),
                Point::new(maxx, maxy),
                Point::new(minx, maxy),
                Point::new(minx, miny),
            ])],
            shape_type: ShapeType::Polygon,
            ..Default::default()
        };
        compute_bounds(&mut shape);
        shape
    }

    fn feature(id: u64, kind: &str, bbox: (f64, f64, f64, f64)) -> Feature {
        let mut attributes = BTreeMap::new();
        attributes.insert("kind".to_string(), AttributeValue::String(kind.to_string()));

        Feature {
            id: Some(id),
            geometry: make_box(bbox.0, bbox.1, bbox.2, bbox.3),
            attributes,
        }
    }

    #[test]
    fn memory_backend_reports_name() {
        let ds = MemoryDataSource::default();
        assert_eq!(ds.backend_name(), "memory");
    }

    #[test]
    fn query_can_filter_by_bbox() {
        let ds = MemoryDataSource::new(vec![
            feature(1, "road", (0.0, 0.0, 10.0, 10.0)),
            feature(2, "water", (20.0, 20.0, 30.0, 30.0)),
        ]);

        let options = QueryOptions {
            bbox: Some(Rect {
                minx: -5.0,
                miny: -5.0,
                maxx: 12.0,
                maxy: 12.0,
            }),
            ..Default::default()
        };

        let result = ds.query(&options);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].id, Some(1));
    }

    #[test]
    fn query_can_filter_by_attribute_equals_and_limit() {
        let ds = MemoryDataSource::new(vec![
            feature(1, "road", (0.0, 0.0, 1.0, 1.0)),
            feature(2, "road", (2.0, 2.0, 3.0, 3.0)),
            feature(3, "water", (4.0, 4.0, 5.0, 5.0)),
        ]);

        let options = QueryOptions {
            equals_filters: vec![("kind".to_string(), "road".to_string())],
            limit: Some(1),
            ..Default::default()
        };

        let result = ds.query(&options);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].attribute_str("kind"), Some("road"));
    }
}
