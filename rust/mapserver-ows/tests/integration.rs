//! Integration test exercising the WMS `GetMap` and WFS `GetFeature`
//! handlers end-to-end against the real `africa.fgb` fixture used by
//! `mapserver-flatgeobuf`'s own tests (`msautotest/misc/data/africa.fgb`),
//! going through mapfile parsing, feature querying, classification, and
//! (for WMS) Skia rendering.

use std::collections::BTreeMap;
use std::path::PathBuf;

use mapserver_ows::mapconfig::parse_map_config;
use mapserver_ows::wfs::{parse_get_feature_params, render_get_feature};
use mapserver_ows::wms::{parse_get_map_params, render_get_map};

fn africa_fgb_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../msautotest/misc/data/africa.fgb")
}

fn test_mapfile() -> String {
    format!(
        r#"
        MAP
          NAME "africa_test"
          LAYER
            NAME "africa"
            DATA "{}"
            CLASS
              STYLE
                COLOR 0 128 0
                OUTLINECOLOR 0 0 0
              END
            END
          END
        END
        "#,
        africa_fgb_path().display()
    )
}

fn query(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
    pairs
        .iter()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect()
}

#[test]
fn get_map_renders_africa_layer_to_a_non_trivial_png() {
    let map = parse_map_config(&test_mapfile()).expect("mapfile should parse");

    let params = parse_get_map_params(&query(&[
        ("SERVICE", "WMS"),
        ("REQUEST", "GetMap"),
        ("BBOX", "-20,-35,55,40"),
        ("WIDTH", "200"),
        ("HEIGHT", "200"),
        ("LAYERS", "africa"),
        ("FORMAT", "image/png"),
    ]))
    .expect("valid GetMap params");

    let png = render_get_map(&map, &params);

    assert_eq!(
        &png[0..8],
        &[0x89, b'P', b'N', b'G', b'\r', b'\n', 0x1a, b'\n'],
        "output should be a valid PNG"
    );
    // A render of a whole continent onto a 200x200 canvas should produce
    // more than just a bare PNG header/footer.
    assert!(
        png.len() > 200,
        "expected non-trivial PNG content, got {} bytes",
        png.len()
    );
}

#[test]
fn get_feature_returns_geojson_features_from_africa_layer() {
    let map = parse_map_config(&test_mapfile()).expect("mapfile should parse");

    let params = parse_get_feature_params(&query(&[
        ("SERVICE", "WFS"),
        ("REQUEST", "GetFeature"),
        ("TYPENAME", "africa"),
    ]))
    .expect("valid GetFeature params");

    let geojson = render_get_feature(&map, &params);
    let value: serde_json::Value =
        serde_json::from_str(&geojson).expect("response should be valid JSON");

    assert_eq!(value["type"], "FeatureCollection");
    let features = value["features"].as_array().expect("features array");
    assert!(
        !features.is_empty(),
        "expected at least one feature from africa.fgb"
    );
    assert_eq!(features[0]["type"], "Feature");
    assert!(features[0]["geometry"]["type"].is_string());
}

#[test]
fn get_feature_respects_bbox_filter() {
    let map = parse_map_config(&test_mapfile()).expect("mapfile should parse");

    let all_params = parse_get_feature_params(&query(&[("TYPENAME", "africa")])).unwrap();
    let all_geojson = render_get_feature(&map, &all_params);
    let all_value: serde_json::Value = serde_json::from_str(&all_geojson).unwrap();
    let all_count = all_value["features"].as_array().unwrap().len();

    // A tiny bbox far from Africa should return no features.
    let empty_params =
        parse_get_feature_params(&query(&[("TYPENAME", "africa"), ("BBOX", "170,80,171,81")]))
            .unwrap();
    let empty_geojson = render_get_feature(&map, &empty_params);
    let empty_value: serde_json::Value = serde_json::from_str(&empty_geojson).unwrap();
    let empty_count = empty_value["features"].as_array().unwrap().len();

    assert!(all_count > 0);
    assert_eq!(empty_count, 0);
}
