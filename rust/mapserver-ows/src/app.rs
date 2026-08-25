//! Actix-web application wiring: a single `/ows` endpoint that dispatches
//! on the `SERVICE`/`REQUEST` query parameters, mirroring the query-string
//! driven dispatch of the legacy CGI entry point (`msCGIDispatchRequest()`
//! in `src/mapserv.c`) but served directly over HTTP instead of CGI/FastCGI
//! (per the issue's explicit scope).

use std::collections::BTreeMap;
use std::sync::Arc;

use actix_web::{web, HttpResponse};

use crate::mapconfig::MapConfig;
use crate::wfs::{parse_get_feature_params, render_get_feature};
use crate::wms::{parse_get_map_params, render_get_map};

pub type SharedMap = Arc<MapConfig>;

fn get_param<'a>(params: &'a BTreeMap<String, String>, key: &str) -> Option<&'a str> {
    params
        .iter()
        .find(|(k, _)| k.eq_ignore_ascii_case(key))
        .map(|(_, v)| v.as_str())
}

/// `GET /ows` handler: dispatches to the WMS or WFS request handling based
/// on the `SERVICE`/`REQUEST` query parameters.
pub async fn ows_handler(
    map: web::Data<SharedMap>,
    query: web::Query<BTreeMap<String, String>>,
) -> HttpResponse {
    let params = query.into_inner();
    let service = get_param(&params, "SERVICE")
        .unwrap_or("")
        .to_ascii_uppercase();
    let request = get_param(&params, "REQUEST").unwrap_or("").to_string();

    match (service.as_str(), request.as_str()) {
        ("WMS", r) if r.eq_ignore_ascii_case("GetMap") => match parse_get_map_params(&params) {
            Ok(get_map) => {
                let png = render_get_map(&map, &get_map);
                HttpResponse::Ok().content_type("image/png").body(png)
            }
            Err(e) => HttpResponse::BadRequest().body(e.to_string()),
        },
        ("WFS", r) if r.eq_ignore_ascii_case("GetFeature") => {
            match parse_get_feature_params(&params) {
                Ok(get_feature) => {
                    let geojson = render_get_feature(&map, &get_feature);
                    HttpResponse::Ok()
                        .content_type("application/json")
                        .body(geojson)
                }
                Err(e) => HttpResponse::BadRequest().body(e.to_string()),
            }
        }
        _ => HttpResponse::BadRequest().body(format!(
            "unsupported SERVICE={service}/REQUEST={request} combination"
        )),
    }
}

pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg.route("/ows", web::get().to(ows_handler));
}

#[cfg(test)]
mod tests {
    use super::*;
    use actix_web::{test, App};

    fn test_map_config() -> MapConfig {
        crate::mapconfig::parse_map_config(
            r#"
            MAP
              NAME "test"
              LAYER
                NAME "roads"
                DATA "does-not-exist.fgb"
              END
            END
        "#,
        )
        .unwrap()
    }

    #[actix_rt::test]
    async fn unsupported_service_returns_bad_request() {
        let map: SharedMap = Arc::new(test_map_config());
        let app = test::init_service(
            App::new()
                .app_data(web::Data::new(map))
                .configure(configure),
        )
        .await;
        let req = test::TestRequest::get()
            .uri("/ows?SERVICE=FOO&REQUEST=Bar")
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), actix_web::http::StatusCode::BAD_REQUEST);
    }

    #[actix_rt::test]
    async fn get_map_missing_bbox_returns_bad_request() {
        let map: SharedMap = Arc::new(test_map_config());
        let app = test::init_service(
            App::new()
                .app_data(web::Data::new(map))
                .configure(configure),
        )
        .await;
        let req = test::TestRequest::get()
            .uri("/ows?SERVICE=WMS&REQUEST=GetMap&WIDTH=10&HEIGHT=10&LAYERS=roads")
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), actix_web::http::StatusCode::BAD_REQUEST);
    }

    #[actix_rt::test]
    async fn get_feature_missing_typename_returns_bad_request() {
        let map: SharedMap = Arc::new(test_map_config());
        let app = test::init_service(
            App::new()
                .app_data(web::Data::new(map))
                .configure(configure),
        )
        .await;
        let req = test::TestRequest::get()
            .uri("/ows?SERVICE=WFS&REQUEST=GetFeature")
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), actix_web::http::StatusCode::BAD_REQUEST);
    }
}
