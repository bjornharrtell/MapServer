//! Binary entry point: starts the OWS HTTP service for a single mapfile.
//!
//! Usage: `mapserver-ows <mapfile> [bind-address]`
//! (defaults to `127.0.0.1:8080` when no bind address is given).

use std::env;
use std::fs;
use std::process::ExitCode;
use std::sync::Arc;

use actix_web::{web, App, HttpServer};
use mapserver_ows::{configure, parse_map_config, SharedMap};

#[actix_web::main]
async fn main() -> ExitCode {
    let mut args = env::args().skip(1);
    let Some(mapfile_path) = args.next() else {
        eprintln!("usage: mapserver-ows <mapfile> [bind-address]");
        return ExitCode::FAILURE;
    };
    let bind_address = args.next().unwrap_or_else(|| "127.0.0.1:8080".to_string());

    let mapfile_text = match fs::read_to_string(&mapfile_path) {
        Ok(text) => text,
        Err(e) => {
            eprintln!("failed to read mapfile {mapfile_path}: {e}");
            return ExitCode::FAILURE;
        }
    };

    let map_config = match parse_map_config(&mapfile_text) {
        Ok(config) => config,
        Err(e) => {
            eprintln!("failed to parse mapfile {mapfile_path}: {e}");
            return ExitCode::FAILURE;
        }
    };
    let shared: SharedMap = Arc::new(map_config);

    println!("mapserver-ows listening on http://{bind_address}/ows");

    let server = HttpServer::new(move || {
        App::new()
            .app_data(web::Data::new(shared.clone()))
            .configure(configure)
    })
    .bind(&bind_address);

    let server = match server {
        Ok(server) => server,
        Err(e) => {
            eprintln!("failed to bind {bind_address}: {e}");
            return ExitCode::FAILURE;
        }
    };

    match server.run().await {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("server error: {e}");
            ExitCode::FAILURE
        }
    }
}
