//! FlatGeobuf layer backend for MapServer.
//!
//! Implements [`LayerDataSource`] for `.fgb` files using the [`flatgeobuf`]
//! crate.  Geometry and attribute values are mapped to the shared primitives
//! defined in [`mapserver_core`].

use std::collections::BTreeMap;
use std::fs::File;
use std::io::BufReader;
use std::path::{Path, PathBuf};

use flatgeobuf::{FallibleStreamingIterator, FgbReader, GeometryType};
use geozero::{ColumnValue, FeatureProperties};
use mapserver_core::datasource::{AttributeValue, Feature, LayerDataSource, QueryOptions};
use mapserver_core::primitive::{compute_bounds, Line, Point, Shape, ShapeType};

/// Layer data source backed by a FlatGeobuf file.
///
/// Supports bbox-filtered and unfiltered queries via the spatial index when
/// one is present in the file.
#[derive(Debug, Clone)]
pub struct FlatgeobufDataSource {
    path: PathBuf,
}

impl FlatgeobufDataSource {
    /// Create a new data source pointing at the given `.fgb` file path.
    pub fn new<P: AsRef<Path>>(path: P) -> Self {
        Self {
            path: path.as_ref().to_path_buf(),
        }
    }
}

// ---------------------------------------------------------------------------
// Geometry conversion helpers
// ---------------------------------------------------------------------------

/// Convert an interleaved xy flat array (accessed by length + index) into a
/// [`Line`].  Using a closure avoids a direct dependency on the `flatbuffers`
/// crate.
fn line_from_xy(len: usize, get: impl Fn(usize) -> f64) -> Line {
    let mut points = Vec::with_capacity(len / 2);
    let mut i = 0;
    while i + 1 < len {
        points.push(Point::new(get(i), get(i + 1)));
        i += 2;
    }
    Line::new(points)
}

/// Convert a FlatGeobuf `Geometry` representing a *single* point into a
/// [`Shape`].
fn shape_from_point(geom: &flatgeobuf::Geometry<'_>) -> Shape {
    let mut shape = Shape {
        shape_type: ShapeType::Point,
        ..Default::default()
    };
    if let Some(xy) = geom.xy() {
        if xy.len() >= 2 {
            shape.lines = vec![Line::new(vec![Point::new(xy.get(0), xy.get(1))])];
        }
    }
    compute_bounds(&mut shape);
    shape
}

/// Convert a FlatGeobuf `Geometry` representing a *line-string* into a
/// [`Shape`].
fn shape_from_linestring(geom: &flatgeobuf::Geometry<'_>) -> Shape {
    let mut shape = Shape {
        shape_type: ShapeType::Line,
        ..Default::default()
    };
    if let Some(xy) = geom.xy() {
        shape.lines = vec![line_from_xy(xy.len(), |i| xy.get(i))];
    }
    compute_bounds(&mut shape);
    shape
}

/// Convert a FlatGeobuf `Geometry` representing a *polygon* into a [`Shape`].
///
/// The polygon rings are stored as `parts`: the first is the exterior ring,
/// the rest are holes.  All rings are added as lines.
fn shape_from_polygon(geom: &flatgeobuf::Geometry<'_>) -> Shape {
    let mut shape = Shape {
        shape_type: ShapeType::Polygon,
        ..Default::default()
    };
    if let Some(parts) = geom.parts() {
        for i in 0..parts.len() {
            let part = parts.get(i);
            if let Some(xy) = part.xy() {
                shape.lines.push(line_from_xy(xy.len(), |j| xy.get(j)));
            }
        }
    } else if let Some(xy) = geom.xy() {
        // Polygon with a single ring stored directly in xy (no parts).
        shape.lines = vec![line_from_xy(xy.len(), |i| xy.get(i))];
    }
    compute_bounds(&mut shape);
    shape
}

/// Convert a FlatGeobuf `Geometry` representing a *multi-point* into a
/// [`Shape`].
fn shape_from_multipoint(geom: &flatgeobuf::Geometry<'_>) -> Shape {
    let mut shape = Shape {
        shape_type: ShapeType::Point,
        ..Default::default()
    };
    if let Some(parts) = geom.parts() {
        for i in 0..parts.len() {
            let part = parts.get(i);
            if let Some(xy) = part.xy() {
                if xy.len() >= 2 {
                    shape
                        .lines
                        .push(Line::new(vec![Point::new(xy.get(0), xy.get(1))]));
                }
            }
        }
    }
    compute_bounds(&mut shape);
    shape
}

/// Convert a FlatGeobuf `Geometry` representing a *multi-line-string* into a
/// [`Shape`].
fn shape_from_multilinestring(geom: &flatgeobuf::Geometry<'_>) -> Shape {
    let mut shape = Shape {
        shape_type: ShapeType::Line,
        ..Default::default()
    };
    if let Some(parts) = geom.parts() {
        for i in 0..parts.len() {
            let part = parts.get(i);
            if let Some(xy) = part.xy() {
                shape.lines.push(line_from_xy(xy.len(), |j| xy.get(j)));
            }
        }
    }
    compute_bounds(&mut shape);
    shape
}

/// Convert a FlatGeobuf `Geometry` representing a *multi-polygon* into a
/// [`Shape`].
///
/// Each polygon is stored as a `part`; its rings are stored as the polygon
/// part's own `parts`.
fn shape_from_multipolygon(geom: &flatgeobuf::Geometry<'_>) -> Shape {
    let mut shape = Shape {
        shape_type: ShapeType::Polygon,
        ..Default::default()
    };
    if let Some(polys) = geom.parts() {
        for p in 0..polys.len() {
            let poly = polys.get(p);
            if let Some(rings) = poly.parts() {
                for r in 0..rings.len() {
                    let ring = rings.get(r);
                    if let Some(xy) = ring.xy() {
                        shape.lines.push(line_from_xy(xy.len(), |j| xy.get(j)));
                    }
                }
            } else if let Some(xy) = poly.xy() {
                shape.lines.push(line_from_xy(xy.len(), |j| xy.get(j)));
            }
        }
    }
    compute_bounds(&mut shape);
    shape
}

/// Dispatch geometry conversion based on geometry type.
///
/// Returns `None` for geometry types that cannot be mapped to the current
/// [`ShapeType`] model.
fn shape_from_geometry(geom: &flatgeobuf::Geometry<'_>, geom_type: GeometryType) -> Option<Shape> {
    // If the feature geometry declares `Unknown` we fall back to the
    // feature-level type.
    let effective_type = if geom_type == GeometryType::Unknown {
        geom.type_()
    } else {
        geom_type
    };

    match effective_type {
        GeometryType::Point => Some(shape_from_point(geom)),
        GeometryType::MultiPoint => Some(shape_from_multipoint(geom)),
        GeometryType::LineString => Some(shape_from_linestring(geom)),
        GeometryType::MultiLineString => Some(shape_from_multilinestring(geom)),
        GeometryType::Polygon => Some(shape_from_polygon(geom)),
        GeometryType::MultiPolygon => Some(shape_from_multipolygon(geom)),
        // GeometryCollection and unsupported curve types are skipped.
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// Property / attribute conversion helpers
// ---------------------------------------------------------------------------

/// Collect all feature properties into an [`AttributeValue`] map.
fn collect_attributes(
    feature: &flatgeobuf::FgbFeature,
) -> Result<BTreeMap<String, AttributeValue>, Box<dyn std::error::Error>> {
    let mut map = BTreeMap::new();
    let mut collector = AttributeCollector { map: &mut map };
    let _ = feature.process_properties(&mut collector);
    Ok(map)
}

/// A `geozero` [`PropertyProcessor`] that accumulates column values into an
/// [`AttributeValue`] map.
struct AttributeCollector<'a> {
    map: &'a mut BTreeMap<String, AttributeValue>,
}

impl geozero::PropertyProcessor for AttributeCollector<'_> {
    fn property(
        &mut self,
        _idx: usize,
        name: &str,
        value: &ColumnValue,
    ) -> geozero::error::Result<bool> {
        let attr = match value {
            ColumnValue::String(s) | ColumnValue::Json(s) | ColumnValue::DateTime(s) => {
                AttributeValue::String((*s).to_string())
            }
            ColumnValue::Bool(b) => AttributeValue::Bool(*b),
            ColumnValue::Byte(n) => AttributeValue::Number(*n as f64),
            ColumnValue::UByte(n) => AttributeValue::Number(*n as f64),
            ColumnValue::Short(n) => AttributeValue::Number(*n as f64),
            ColumnValue::UShort(n) => AttributeValue::Number(*n as f64),
            ColumnValue::Int(n) => AttributeValue::Number(*n as f64),
            ColumnValue::UInt(n) => AttributeValue::Number(*n as f64),
            ColumnValue::Long(n) => AttributeValue::Number(*n as f64),
            ColumnValue::ULong(n) => AttributeValue::Number(*n as f64),
            ColumnValue::Float(n) => AttributeValue::Number(*n as f64),
            ColumnValue::Double(n) => AttributeValue::Number(*n),
            // Binary values are skipped – they have no AttributeValue mapping.
            ColumnValue::Binary(_) => return Ok(false),
        };
        self.map.insert(name.to_string(), attr);
        Ok(false)
    }
}

// ---------------------------------------------------------------------------
// LayerDataSource implementation
// ---------------------------------------------------------------------------

impl LayerDataSource for FlatgeobufDataSource {
    fn backend_name(&self) -> &'static str {
        "flatgeobuf"
    }

    fn query(&self, options: &QueryOptions) -> Vec<Feature> {
        match self.query_inner(options) {
            Ok(features) => features,
            Err(e) => {
                log::error!("flatgeobuf: failed to query {:?}: {}", self.path, e);
                Vec::new()
            }
        }
    }
}

impl FlatgeobufDataSource {
    fn query_inner(
        &self,
        options: &QueryOptions,
    ) -> Result<Vec<Feature>, Box<dyn std::error::Error>> {
        let file = File::open(&self.path)?;
        let mut reader = BufReader::new(file);
        let fgb = FgbReader::open(&mut reader)?;
        let header_geom_type = fgb.header().geometry_type();

        let mut out = Vec::new();

        if let Some(bbox) = options.bbox {
            let mut iter = fgb.select_bbox(bbox.minx, bbox.miny, bbox.maxx, bbox.maxy)?;
            while let Some(feature) = iter.next()? {
                if let Some(f) = convert_feature(feature, header_geom_type, options)? {
                    out.push(f);
                    if options.limit.is_some_and(|l| out.len() >= l) {
                        break;
                    }
                }
            }
        } else {
            let mut iter = fgb.select_all()?;
            while let Some(feature) = iter.next()? {
                if let Some(f) = convert_feature(feature, header_geom_type, options)? {
                    out.push(f);
                    if options.limit.is_some_and(|l| out.len() >= l) {
                        break;
                    }
                }
            }
        }

        Ok(out)
    }
}

/// Convert a single [`flatgeobuf::FgbFeature`] into a [`Feature`].
///
/// Returns `None` when the geometry cannot be mapped or attribute filters
/// exclude the feature.
fn convert_feature(
    fgb_feature: &flatgeobuf::FgbFeature,
    header_geom_type: GeometryType,
    options: &QueryOptions,
) -> Result<Option<Feature>, Box<dyn std::error::Error>> {
    let geom = match fgb_feature.geometry() {
        Some(g) => g,
        None => return Ok(None),
    };

    let shape = match shape_from_geometry(&geom, header_geom_type) {
        Some(s) => s,
        None => return Ok(None),
    };

    let attributes = collect_attributes(fgb_feature)?;

    // Apply attribute equality filters.
    for (name, expected) in &options.equals_filters {
        match attributes.get(name) {
            Some(AttributeValue::String(v)) if v == expected => {}
            _ => return Ok(None),
        }
    }

    Ok(Some(Feature {
        id: None,
        geometry: shape,
        attributes,
    }))
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use mapserver_core::datasource::QueryOptions;
    use mapserver_core::primitive::{Rect, ShapeType};

    /// Path to the africa fixture relative to the workspace root.
    fn africa_path() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../msautotest/misc/data/africa.fgb")
    }

    /// Path to the ne_110m_land fixture.
    fn ne_land_path() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../msautotest/config/data/ne_110m_land.fgb")
    }

    #[test]
    fn backend_name_is_flatgeobuf() {
        let ds = FlatgeobufDataSource::new(africa_path());
        assert_eq!(ds.backend_name(), "flatgeobuf");
    }

    #[test]
    fn query_all_returns_features() {
        let ds = FlatgeobufDataSource::new(africa_path());
        let features = ds.query(&QueryOptions::default());
        assert!(
            !features.is_empty(),
            "expected at least one feature from africa.fgb"
        );
    }

    #[test]
    fn query_all_features_have_polygon_geometry() {
        let ds = FlatgeobufDataSource::new(africa_path());
        let features = ds.query(&QueryOptions::default());
        for f in &features {
            assert_eq!(
                f.geometry.shape_type,
                ShapeType::Polygon,
                "all africa features should be polygons"
            );
        }
    }

    #[test]
    fn query_all_features_have_non_empty_bounds() {
        let ds = FlatgeobufDataSource::new(africa_path());
        let features = ds.query(&QueryOptions::default());
        for f in &features {
            let b = f.geometry.bounds;
            assert!(b.maxx > b.minx, "bounds maxx should exceed minx");
            assert!(b.maxy > b.miny, "bounds maxy should exceed miny");
        }
    }

    #[test]
    fn query_bbox_filters_features() {
        let ds = FlatgeobufDataSource::new(africa_path());
        let all = ds.query(&QueryOptions::default());

        let options = QueryOptions {
            bbox: Some(Rect {
                minx: 20.0,
                miny: -35.0,
                maxx: 35.0,
                maxy: -20.0,
            }),
            ..Default::default()
        };
        let filtered = ds.query(&options);
        assert!(
            filtered.len() <= all.len(),
            "bbox filter must not return more features than an unfiltered query"
        );
        assert!(
            !filtered.is_empty(),
            "expected at least one southern-Africa feature"
        );
    }

    #[test]
    fn query_limit_is_respected() {
        let ds = FlatgeobufDataSource::new(africa_path());
        let options = QueryOptions {
            limit: Some(3),
            ..Default::default()
        };
        let features = ds.query(&options);
        assert_eq!(
            features.len(),
            3,
            "limit=3 should return exactly 3 features"
        );
    }

    #[test]
    fn query_returns_attributes() {
        let ds = FlatgeobufDataSource::new(africa_path());
        let features = ds.query(&QueryOptions {
            limit: Some(1),
            ..Default::default()
        });
        assert_eq!(features.len(), 1);
        // africa.fgb has a "name" or "NAME" attribute – verify some attribute
        // is present.
        assert!(
            !features[0].attributes.is_empty(),
            "feature should have at least one attribute"
        );
    }

    #[test]
    fn query_equals_filter_selects_correct_features() {
        let ds = FlatgeobufDataSource::new(africa_path());
        // Read all features to discover an attribute name/value pair.
        let all = ds.query(&QueryOptions::default());
        assert!(!all.is_empty());

        // Find the first string attribute in the first feature.
        let (key, value) = all[0]
            .attributes
            .iter()
            .find_map(|(k, v)| {
                if let AttributeValue::String(s) = v {
                    Some((k.clone(), s.clone()))
                } else {
                    None
                }
            })
            .expect("first feature should have at least one string attribute");

        let options = QueryOptions {
            equals_filters: vec![(key.clone(), value.clone())],
            ..Default::default()
        };
        let filtered = ds.query(&options);

        assert!(
            !filtered.is_empty(),
            "filter on the first feature's attribute should return at least one feature"
        );
        for f in &filtered {
            assert_eq!(
                f.attributes.get(&key),
                Some(&AttributeValue::String(value.clone())),
                "every returned feature must match the filter"
            );
        }
    }

    #[test]
    fn ne_land_query_all_returns_features() {
        let ds = FlatgeobufDataSource::new(ne_land_path());
        let features = ds.query(&QueryOptions::default());
        assert!(
            !features.is_empty(),
            "expected at least one feature from ne_110m_land.fgb"
        );
    }

    #[test]
    fn ne_land_features_have_geometry() {
        let ds = FlatgeobufDataSource::new(ne_land_path());
        let features = ds.query(&QueryOptions {
            limit: Some(5),
            ..Default::default()
        });
        for f in &features {
            assert_ne!(
                f.geometry.shape_type,
                ShapeType::Null,
                "ne_land features should have non-null geometry"
            );
        }
    }
}
