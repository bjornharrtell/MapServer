//! A minimal, self-contained "map document" model assembled from a parsed
//! mapfile ([`mapserver_core::mapfile::parse_mapfile`]), just enough to
//! drive the OWS handlers in this crate (extent/size defaults, layers with
//! a data source path, classification item, and classes).
//!
//! This intentionally does not attempt to be a full port of `mapObj`er
//! `layerObj` (`src/mapserver.h`) -- only the fields needed to answer
//! WMS `GetMap` / WFS `GetFeature` requests for FlatGeobuf-backed layers
//! are modeled. See `rust/README.md` ("OWS services") for the scope of
//! this first cut.

use mapserver_core::class::{parse_class, ClassObj};
use mapserver_core::config::ConfigBlock;
use mapserver_core::error::{MapServerError, Result};
use mapserver_core::mapfile::parse_mapfile;
use mapserver_core::primitive::Rect;

/// A single `LAYER` block, reduced to what's needed to query and render it.
#[derive(Debug, Clone)]
pub struct LayerConfig {
    pub name: String,
    /// Path to the layer's data source, from the `DATA` property. Only
    /// FlatGeobuf (`.fgb`) sources are supported in this first cut (see
    /// issue #9/#12).
    pub data: String,
    pub classitem: Option<String>,
    pub classes: Vec<ClassObj>,
}

/// A reduced `MAP` block: default extent/size plus its layers.
#[derive(Debug, Clone)]
pub struct MapConfig {
    pub extent: Option<Rect>,
    pub width: u32,
    pub height: u32,
    pub layers: Vec<LayerConfig>,
}

fn parse_extent(block: &ConfigBlock) -> Option<Rect> {
    let values = block.properties.get("EXTENT")?;
    if values.len() < 4 {
        return None;
    }
    let n = |i: usize| values.get(i).and_then(|v| v.as_f64());
    Some(Rect {
        minx: n(0)?,
        miny: n(1)?,
        maxx: n(2)?,
        maxy: n(3)?,
    })
}

fn parse_size(block: &ConfigBlock) -> (u32, u32) {
    let values = block.properties.get("SIZE");
    let w = values
        .and_then(|v| v.first())
        .and_then(|v| v.as_f64())
        .map(|n| n as u32)
        .unwrap_or(400);
    let h = values
        .and_then(|v| v.get(1))
        .and_then(|v| v.as_f64())
        .map(|n| n as u32)
        .unwrap_or(300);
    (w, h)
}

fn parse_layer(block: &ConfigBlock) -> Result<LayerConfig> {
    let name = block
        .first_str("NAME")
        .ok_or_else(|| MapServerError::new(12, "parse_layer", "LAYER is missing required NAME"))?
        .to_string();
    let data = block
        .first_str("DATA")
        .ok_or_else(|| {
            MapServerError::new(
                12,
                "parse_layer",
                format!("LAYER {name} is missing required DATA"),
            )
        })?
        .to_string();
    let classitem = block.first_str("CLASSITEM").map(str::to_string);
    let classes = block
        .children_of_kind("CLASS")
        .map(parse_class)
        .collect::<Result<Vec<_>>>()?;

    Ok(LayerConfig {
        name,
        data,
        classitem,
        classes,
    })
}

/// Parses a mapfile's text into a [`MapConfig`].
pub fn parse_map_config(input: &str) -> Result<MapConfig> {
    let root = parse_mapfile(input)?;
    let extent = parse_extent(&root);
    let (width, height) = parse_size(&root);
    let layers = root
        .children_of_kind("LAYER")
        .map(parse_layer)
        .collect::<Result<Vec<_>>>()?;

    Ok(MapConfig {
        extent,
        width,
        height,
        layers,
    })
}

impl MapConfig {
    pub fn layer(&self, name: &str) -> Option<&LayerConfig> {
        self.layers
            .iter()
            .find(|l| l.name.eq_ignore_ascii_case(name))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_extent_size_and_layers() {
        let mapfile = r#"
            MAP
              NAME "test"
              EXTENT 0 0 100 100
              SIZE 640 480
              LAYER
                NAME "roads"
                DATA "roads.fgb"
                CLASSITEM "type"
                CLASS
                  EXPRESSION "primary"
                  STYLE
                    COLOR 255 0 0
                  END
                END
              END
            END
        "#;

        let config = parse_map_config(mapfile).unwrap();
        assert_eq!(
            config.extent,
            Some(Rect {
                minx: 0.0,
                miny: 0.0,
                maxx: 100.0,
                maxy: 100.0,
            })
        );
        assert_eq!(config.width, 640);
        assert_eq!(config.height, 480);
        assert_eq!(config.layers.len(), 1);

        let layer = config.layer("roads").unwrap();
        assert_eq!(layer.data, "roads.fgb");
        assert_eq!(layer.classitem.as_deref(), Some("type"));
        assert_eq!(layer.classes.len(), 1);
    }

    #[test]
    fn layer_lookup_is_case_insensitive() {
        let mapfile = r#"
            MAP
              LAYER
                NAME "Roads"
                DATA "roads.fgb"
              END
            END
        "#;
        let config = parse_map_config(mapfile).unwrap();
        assert!(config.layer("roads").is_some());
        assert!(config.layer("ROADS").is_some());
        assert!(config.layer("rivers").is_none());
    }

    #[test]
    fn layer_missing_data_is_an_error() {
        let mapfile = r#"
            MAP
              LAYER
                NAME "roads"
              END
            END
        "#;
        assert!(parse_map_config(mapfile).is_err());
    }
}
