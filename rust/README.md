# MapServer Rust port

This directory hosts the in-progress Rust port of MapServer, tracked by the
project goal in bjornharrtell/MapServer#2. The end goal is a fully
verifiable Rust implementation of MapServer that:

- Replaces the GDAL/OGR interop with appropriate Rust crates.
- Replaces the flatgeobuf C++ reader (`src/flatgeobuf`) with the
  [`flatgeobuf`](https://crates.io/crates/flatgeobuf) Rust crate.
- Replaces the legacy AGG 2D rendering engine (`mapagg.cpp`) with rendering
  built on the [`skia-safe`](https://crates.io/crates/skia-safe) crate,
  taking inspiration from the abandoned Skia effort in
  [MapServer/MapServer#6574](https://github.com/MapServer/MapServer/pull/6574).

## Strategy

Given the size of the existing C/C++ codebase (100k+ lines), the port is
being done incrementally:

1. Stand up a Cargo workspace (`rust/Cargo.toml`) alongside the existing
   C/C++ build, so both can coexist during the transition.
2. Port small, self-contained, easily-testable modules first (e.g. string
   and color parsing utilities), each with unit tests that verify behavior
   matches the original C implementation.
3. Progressively work up to larger subsystems (map/layer parsing, query
   engine, rendering) once their dependencies have been ported.
4. Replace third-party interop (GDAL, flatgeobuf) with native Rust crates
   as the corresponding subsystems are ported.
5. Replace the AGG rendering backend with a Skia-based renderer once the
   symbology and rendering pipeline are ported.

## Layout

- `mapserver-core` - core utilities being ported first (currently: color
  parsing, ported from `msHexToInt()`/`msSLDSetColorObject()`).

## Building and testing

```sh
cd rust
cargo build
cargo test
```
