//! WFS `GetFeature` handling: parses request parameters, queries the
//! requested layer(s), and returns a GeoJSON `FeatureCollection`.
//!
//! MapServer's WFS output is GML by default (`mapwfs.cpp`); this first cut
//! returns GeoJSON instead (simpler to produce correctly without a GML/XSD
//! schema layer, and directly consumable by common GIS clients) -- see
//! `rust/README.md` ("OWS services") for the rationale and scope.

use std::collections::BTreeMap;

use mapserver_core::datasource::{AttributeValue, Feature, LayerDataSource, QueryOptions};
use mapserver_core::primitive::{Line, Rect, Shape, ShapeType};
use mapserver_flatgeobuf::FlatgeobufDataSource;
use serde_json::{json, Value};

use crate::mapconfig::MapConfig;

#[derive(Debug, Clone, PartialEq)]
pub struct GetFeatureParams {
    pub type_names: Vec<String>,
    pub bbox: Option<Rect>,
    pub max_features: Option<usize>,
}

#[derive(Debug, PartialEq, Eq)]
pub enum GetFeatureError {
    MissingParam(&'static str),
    InvalidParam(&'static str),
}

impl std::fmt::Display for GetFeatureError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            GetFeatureError::MissingParam(p) => write!(f, "missing required parameter {p}"),
            GetFeatureError::InvalidParam(p) => write!(f, "invalid value for parameter {p}"),
        }
    }
}

/// Parses WFS `GetFeature` parameters, accepting both the WFS 1.x
/// `TYPENAME` and WFS 2.0 `TYPENAMES` spellings.
pub fn parse_get_feature_params(
    params: &BTreeMap<String, String>,
) -> Result<GetFeatureParams, GetFeatureError> {
    let get = |key: &str| -> Option<&String> {
        params
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(key))
            .map(|(_, v)| v)
    };

    let type_names_str = get("TYPENAMES")
        .or_else(|| get("TYPENAME"))
        .ok_or(GetFeatureError::MissingParam("TYPENAME"))?;
    let type_names: Vec<String> = type_names_str
        .split(',')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();
    if type_names.is_empty() {
        return Err(GetFeatureError::InvalidParam("TYPENAME"));
    }

    let bbox = match get("BBOX") {
        None => None,
        Some(raw) => {
            let parts: Vec<f64> = raw
                .split(',')
                .map(|s| s.trim().parse::<f64>())
                .collect::<Result<_, _>>()
                .map_err(|_| GetFeatureError::InvalidParam("BBOX"))?;
            if parts.len() != 4 {
                return Err(GetFeatureError::InvalidParam("BBOX"));
            }
            Some(Rect {
                minx: parts[0],
                miny: parts[1],
                maxx: parts[2],
                maxy: parts[3],
            })
        }
    };

    let max_features = match get("COUNT").or_else(|| get("MAXFEATURES")) {
        None => None,
        Some(raw) => Some(
            raw.parse::<usize>()
                .map_err(|_| GetFeatureError::InvalidParam("COUNT"))?,
        ),
    };

    Ok(GetFeatureParams {
        type_names,
        bbox,
        max_features,
    })
}

fn attribute_value_to_json(value: &AttributeValue) -> Value {
    match value {
        AttributeValue::String(s) => json!(s),
        AttributeValue::Number(n) => json!(n),
        AttributeValue::Bool(b) => json!(b),
    }
}

fn line_to_json_coords(line: &Line) -> Value {
    json!(line
        .points
        .iter()
        .map(|p| json!([p.x, p.y]))
        .collect::<Vec<_>>())
}

fn geometry_to_geojson(shape: &Shape) -> Value {
    match shape.shape_type {
        ShapeType::Point => {
            let point = shape
                .lines
                .first()
                .and_then(|l| l.points.first())
                .map(|p| json!([p.x, p.y]))
                .unwrap_or(Value::Null);
            json!({ "type": "Point", "coordinates": point })
        }
        ShapeType::Line => {
            if shape.lines.len() == 1 {
                json!({
                    "type": "LineString",
                    "coordinates": line_to_json_coords(&shape.lines[0]),
                })
            } else {
                json!({
                    "type": "MultiLineString",
                    "coordinates": shape.lines.iter().map(line_to_json_coords).collect::<Vec<_>>(),
                })
            }
        }
        ShapeType::Polygon => json!({
            "type": "Polygon",
            "coordinates": shape.lines.iter().map(line_to_json_coords).collect::<Vec<_>>(),
        }),
        ShapeType::Null => Value::Null,
    }
}

fn feature_to_geojson(feature: &Feature) -> Value {
    let properties: serde_json::Map<String, Value> = feature
        .attributes
        .iter()
        .map(|(k, v)| (k.clone(), attribute_value_to_json(v)))
        .collect();

    json!({
        "type": "Feature",
        "id": feature.id,
        "geometry": geometry_to_geojson(&feature.geometry),
        "properties": properties,
    })
}

/// Runs a `GetFeature` request against `map` and returns a GeoJSON
/// `FeatureCollection` as a serialized string.
///
/// Only FlatGeobuf-backed layers are supported; unknown `TYPENAME`s are
/// silently skipped (their features simply don't appear in the result).
pub fn render_get_feature(map: &MapConfig, params: &GetFeatureParams) -> String {
    let mut features_json = Vec::new();

    for type_name in &params.type_names {
        let Some(layer) = map.layer(type_name) else {
            continue;
        };
        let source = FlatgeobufDataSource::new(&layer.data);
        let options = QueryOptions {
            bbox: params.bbox,
            limit: params.max_features,
            equals_filters: Vec::new(),
        };
        for feature in source.query(&options) {
            features_json.push(feature_to_geojson(&feature));
        }
    }

    let collection = json!({
        "type": "FeatureCollection",
        "features": features_json,
    });
    collection.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn params(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    }

    #[test]
    fn parses_typename_and_bbox() {
        let p = params(&[("TYPENAME", "roads"), ("BBOX", "1,2,3,4"), ("COUNT", "10")]);
        let parsed = parse_get_feature_params(&p).unwrap();
        assert_eq!(parsed.type_names, vec!["roads"]);
        assert_eq!(
            parsed.bbox,
            Some(Rect {
                minx: 1.0,
                miny: 2.0,
                maxx: 3.0,
                maxy: 4.0
            })
        );
        assert_eq!(parsed.max_features, Some(10));
    }

    #[test]
    fn accepts_wfs2_typenames_spelling() {
        let p = params(&[("TYPENAMES", "roads,rivers")]);
        let parsed = parse_get_feature_params(&p).unwrap();
        assert_eq!(parsed.type_names, vec!["roads", "rivers"]);
    }

    #[test]
    fn rejects_missing_typename() {
        let p = params(&[]);
        assert_eq!(
            parse_get_feature_params(&p),
            Err(GetFeatureError::MissingParam("TYPENAME"))
        );
    }

    #[test]
    fn point_geometry_serializes_to_geojson_point() {
        use mapserver_core::primitive::Point;
        let mut shape = Shape {
            shape_type: ShapeType::Point,
            ..Default::default()
        };
        shape.add_line(Line::new(vec![Point::new(1.5, 2.5)]));
        let feature = Feature {
            id: Some(1),
            geometry: shape,
            attributes: BTreeMap::new(),
        };
        let geo = feature_to_geojson(&feature);
        assert_eq!(geo["geometry"]["type"], "Point");
        assert_eq!(geo["geometry"]["coordinates"], json!([1.5, 2.5]));
    }
}
