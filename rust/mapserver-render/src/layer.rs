//! Layer rendering: classifies each feature (via `mapserver_core::class`)
//! and draws it through a [`Renderer`], mirroring the per-shape draw loop
//! in `msDrawLayer()`/`msDrawShape()` (`src/mapdraw.c`).

use mapserver_core::class::{get_class, ClassObj};
use mapserver_core::datasource::Feature;
use mapserver_core::primitive::{Point, ShapeType};

use crate::renderer::Renderer;
use crate::view::MapView;

/// Renders every feature in `features` that matches a class in `classes`,
/// styling it with the matched class's styles, in order.
///
/// Features that don't match any class (`get_class` returns `None`) are
/// skipped, matching `msDrawShape()`'s behavior of not rendering
/// unclassified shapes. Only the first style of the matched class is used
/// for now (`styleObj` supports layered styles per class, e.g. a cased
/// line, which is left as follow-up work).
pub fn render_layer<R: Renderer>(
    features: &[Feature],
    classes: &[ClassObj],
    classitem: Option<&str>,
    scaledenom: Option<f64>,
    view: &MapView,
    renderer: &mut R,
) {
    for feature in features {
        let Some(classindex) = get_class(
            classes,
            classitem,
            scaledenom,
            feature.geometry.shape_type,
            &feature.attributes,
        ) else {
            continue;
        };

        let Some(style) = classes[classindex].styles.first() else {
            continue;
        };

        match feature.geometry.shape_type {
            ShapeType::Point => {
                for line in &feature.geometry.lines {
                    for &p in &line.points {
                        renderer.draw_point(view.geo_to_pixel(p), style);
                    }
                }
            }
            ShapeType::Line => {
                for line in &feature.geometry.lines {
                    let pixels: Vec<_> =
                        line.points.iter().map(|&p| view.geo_to_pixel(p)).collect();
                    renderer.draw_line(&pixels, style);
                }
            }
            ShapeType::Polygon => {
                let rings: Vec<Vec<_>> = feature
                    .geometry
                    .lines
                    .iter()
                    .map(|line| line.points.iter().map(|&p| view.geo_to_pixel(p)).collect())
                    .collect();
                renderer.draw_polygon(&rings, style);
            }
            ShapeType::Null => {}
        }
    }
}

/// Convenience helper used mainly by tests: builds a single-point pixel
/// coordinate directly from geographic coordinates.
pub fn geo_point_to_pixel(view: &MapView, x: f64, y: f64) -> (f32, f32) {
    view.geo_to_pixel(Point::new(x, y))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::skia_backend::SkiaRenderer;
    use mapserver_core::class::{Expression, StyleObj};
    use mapserver_core::color::Color;
    use mapserver_core::primitive::{Line, Rect, Shape};
    use std::collections::BTreeMap;

    fn class_with_color(color: Color) -> ClassObj {
        ClassObj {
            expression: Expression::None,
            styles: vec![StyleObj {
                color: Some(color),
                ..Default::default()
            }],
            ..Default::default()
        }
    }

    #[test]
    fn render_layer_draws_matching_polygon_feature() {
        let view = MapView::new(
            Rect {
                minx: 0.0,
                miny: 0.0,
                maxx: 100.0,
                maxy: 100.0,
            },
            100,
            100,
        );
        let mut renderer = SkiaRenderer::new(
            100,
            100,
            Some(Color {
                red: 255,
                green: 255,
                blue: 255,
            }),
        );

        let mut shape = Shape {
            shape_type: ShapeType::Polygon,
            ..Default::default()
        };
        shape.add_line(Line::new(vec![
            Point::new(10.0, 10.0),
            Point::new(90.0, 10.0),
            Point::new(90.0, 90.0),
            Point::new(10.0, 90.0),
        ]));
        let feature = Feature {
            id: None,
            geometry: shape,
            attributes: BTreeMap::new(),
        };

        let classes = vec![class_with_color(Color {
            red: 0,
            green: 0,
            blue: 255,
        })];

        render_layer(&[feature], &classes, None, None, &view, &mut renderer);
        assert_eq!(renderer.pixel_at(50, 50), (0, 0, 255, 255));
        assert_eq!(renderer.pixel_at(1, 1), (255, 255, 255, 255));
    }

    #[test]
    fn render_layer_skips_features_with_no_matching_class() {
        let view = MapView::new(
            Rect {
                minx: 0.0,
                miny: 0.0,
                maxx: 100.0,
                maxy: 100.0,
            },
            100,
            100,
        );
        let mut renderer = SkiaRenderer::new(
            100,
            100,
            Some(Color {
                red: 255,
                green: 255,
                blue: 255,
            }),
        );

        let mut shape = Shape {
            shape_type: ShapeType::Polygon,
            ..Default::default()
        };
        shape.add_line(Line::new(vec![
            Point::new(10.0, 10.0),
            Point::new(90.0, 10.0),
            Point::new(90.0, 90.0),
            Point::new(10.0, 90.0),
        ]));
        let feature = Feature {
            id: None,
            geometry: shape,
            attributes: BTreeMap::new(),
        };

        // No classes at all => nothing should be drawn.
        render_layer(&[feature], &[], None, None, &view, &mut renderer);

        assert_eq!(renderer.pixel_at(50, 50), (255, 255, 255, 255));
    }
}
