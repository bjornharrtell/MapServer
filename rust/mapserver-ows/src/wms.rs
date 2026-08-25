//! WMS `GetMap` handling: parses request parameters, queries and renders
//! the requested layers, and returns a PNG.
//!
//! Scoped to the fields needed to drive rendering (`BBOX`/`WIDTH`/`HEIGHT`/
//! `LAYERS`/`FORMAT`); `STYLES`, `CRS` reprojection, `TRANSPARENT`,
//! `BGCOLOR`, and exception handling are left as follow-up work (see
//! `rust/README.md`, "OWS services").

use std::collections::BTreeMap;

use mapserver_core::datasource::{LayerDataSource, QueryOptions};
use mapserver_core::primitive::Rect;
use mapserver_flatgeobuf::FlatgeobufDataSource;
use mapserver_render::{render_layer, MapView, SkiaRenderer};

use crate::mapconfig::MapConfig;

#[derive(Debug, Clone, PartialEq)]
pub struct GetMapParams {
    pub bbox: Rect,
    pub width: u32,
    pub height: u32,
    pub layers: Vec<String>,
    pub format: String,
}

#[derive(Debug, PartialEq, Eq)]
pub enum GetMapError {
    MissingParam(&'static str),
    InvalidParam(&'static str),
    UnsupportedFormat(String),
}

impl std::fmt::Display for GetMapError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            GetMapError::MissingParam(p) => write!(f, "missing required parameter {p}"),
            GetMapError::InvalidParam(p) => write!(f, "invalid value for parameter {p}"),
            GetMapError::UnsupportedFormat(fmt) => write!(f, "unsupported FORMAT {fmt}"),
        }
    }
}

/// Parses WMS `GetMap` parameters out of a case-insensitive query-string
/// parameter map (as produced by e.g. `actix_web::web::Query`).
pub fn parse_get_map_params(
    params: &BTreeMap<String, String>,
) -> Result<GetMapParams, GetMapError> {
    let get = |key: &str| -> Option<&String> {
        params
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(key))
            .map(|(_, v)| v)
    };

    let bbox_str = get("BBOX").ok_or(GetMapError::MissingParam("BBOX"))?;
    let parts: Vec<f64> = bbox_str
        .split(',')
        .map(|s| s.trim().parse::<f64>())
        .collect::<Result<_, _>>()
        .map_err(|_| GetMapError::InvalidParam("BBOX"))?;
    if parts.len() != 4 {
        return Err(GetMapError::InvalidParam("BBOX"));
    }
    let bbox = Rect {
        minx: parts[0],
        miny: parts[1],
        maxx: parts[2],
        maxy: parts[3],
    };

    let width = get("WIDTH")
        .ok_or(GetMapError::MissingParam("WIDTH"))?
        .parse::<u32>()
        .map_err(|_| GetMapError::InvalidParam("WIDTH"))?;
    let height = get("HEIGHT")
        .ok_or(GetMapError::MissingParam("HEIGHT"))?
        .parse::<u32>()
        .map_err(|_| GetMapError::InvalidParam("HEIGHT"))?;

    let layers = get("LAYERS")
        .ok_or(GetMapError::MissingParam("LAYERS"))?
        .split(',')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>();
    if layers.is_empty() {
        return Err(GetMapError::InvalidParam("LAYERS"));
    }

    let format = get("FORMAT")
        .cloned()
        .unwrap_or_else(|| "image/png".to_string());
    if format != "image/png" {
        return Err(GetMapError::UnsupportedFormat(format));
    }

    Ok(GetMapParams {
        bbox,
        width,
        height,
        layers,
        format,
    })
}

/// Renders a `GetMap` request against `map`, drawing the requested layers
/// (in request order, matching `msDrawMap()`'s layer draw order) and
/// returning encoded PNG bytes.
///
/// Only FlatGeobuf-backed layers (`DATA` pointing at a `.fgb` file) are
/// supported; unknown layer names are silently skipped, matching the
/// permissive style of the C implementation's layer name resolution
/// (`msDrawMap()` for layers it can't find in `LAYERS`).
pub fn render_get_map(map: &MapConfig, params: &GetMapParams) -> Vec<u8> {
    let view = MapView::new(params.bbox, params.width, params.height);
    let mut renderer = SkiaRenderer::new(params.width, params.height, None);

    for layer_name in &params.layers {
        let Some(layer) = map.layer(layer_name) else {
            continue;
        };
        let source = FlatgeobufDataSource::new(&layer.data);
        let options = QueryOptions {
            bbox: Some(params.bbox),
            limit: None,
            equals_filters: Vec::new(),
        };
        let features = source.query(&options);
        render_layer(
            &features,
            &layer.classes,
            layer.classitem.as_deref(),
            None,
            &view,
            &mut renderer,
        );
    }

    renderer.encode_png()
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
    fn parses_valid_get_map_request() {
        let p = params(&[
            ("SERVICE", "WMS"),
            ("REQUEST", "GetMap"),
            ("BBOX", "-10,-10,10,10"),
            ("WIDTH", "256"),
            ("HEIGHT", "256"),
            ("LAYERS", "roads,rivers"),
            ("FORMAT", "image/png"),
        ]);
        let parsed = parse_get_map_params(&p).unwrap();
        assert_eq!(
            parsed.bbox,
            Rect {
                minx: -10.0,
                miny: -10.0,
                maxx: 10.0,
                maxy: 10.0
            }
        );
        assert_eq!(parsed.width, 256);
        assert_eq!(parsed.height, 256);
        assert_eq!(parsed.layers, vec!["roads", "rivers"]);
    }

    #[test]
    fn rejects_missing_bbox() {
        let p = params(&[("WIDTH", "256"), ("HEIGHT", "256"), ("LAYERS", "roads")]);
        assert_eq!(
            parse_get_map_params(&p),
            Err(GetMapError::MissingParam("BBOX"))
        );
    }

    #[test]
    fn rejects_malformed_bbox() {
        let p = params(&[
            ("BBOX", "1,2,3"),
            ("WIDTH", "256"),
            ("HEIGHT", "256"),
            ("LAYERS", "roads"),
        ]);
        assert_eq!(
            parse_get_map_params(&p),
            Err(GetMapError::InvalidParam("BBOX"))
        );
    }

    #[test]
    fn rejects_unsupported_format() {
        let p = params(&[
            ("BBOX", "1,2,3,4"),
            ("WIDTH", "256"),
            ("HEIGHT", "256"),
            ("LAYERS", "roads"),
            ("FORMAT", "image/jpeg"),
        ]);
        assert_eq!(
            parse_get_map_params(&p),
            Err(GetMapError::UnsupportedFormat("image/jpeg".to_string()))
        );
    }

    #[test]
    fn parameter_lookup_is_case_insensitive() {
        let p = params(&[
            ("bbox", "1,2,3,4"),
            ("width", "10"),
            ("height", "10"),
            ("layers", "roads"),
        ]);
        assert!(parse_get_map_params(&p).is_ok());
    }
}
