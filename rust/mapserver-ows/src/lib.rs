//! Self-hosted HTTP OWS (WMS/WFS) service for the MapServer Rust port.
//!
//! Replaces the legacy CGI/FastCGI entry point (`src/mapserv.c`) with a
//! standalone `actix-web` server exposing the same OWS request/response
//! logic directly over HTTP. See `rust/README.md` ("OWS services") for the
//! scope of this first cut and known limitations.

pub mod app;
pub mod mapconfig;
pub mod wfs;
pub mod wms;

pub use app::{configure, SharedMap};
pub use mapconfig::{parse_map_config, LayerConfig, MapConfig};
