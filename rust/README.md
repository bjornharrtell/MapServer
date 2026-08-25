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
   parsing, bit-array helpers, string/scalar helpers, error model, debug
   scaffolding, core geometry primitives: rect/point/line/shape, rect
   relations, point-in-polygon, segment intersection, distance, bounds
   computation, rect clipping, and initial PROJ-backed projection support
   for loading CRS strings and transforming point/shape/rect geometries,
   plus initial mapfile configuration object model and parser for nested
   `MAP/LAYER/CLASS/STYLE ... END` blocks, a layer data-source abstraction
   trait with an in-memory backend for feature querying, and a
   symbology/style/class/label object model with `expressionObj`-equivalent
   parsing (string/list/regex/logical expressions) and class-matching logic
   mirroring `msShapeGetNextClass()`, and a query engine
   (`query_by_rect`/`query_by_point`/`query_by_attributes`/`query_by_shape`)
   mirroring the per-feature matching core of `msQueryBy*()`
   (`src/mapquery.cpp`)).
- `mapserver-flatgeobuf` - a `LayerDataSource` implementation backed by the
   `flatgeobuf` crate, for `.fgb` layers.
- `mapserver-parity-tests` - parity checks between the Rust port and the
   original C implementation, see "Parity testing against the C
   implementation" below.
- `mapserver-render` - a rendering pipeline abstraction (`Renderer` trait)
   with a Skia-backed implementation (`SkiaRenderer`), plus a
   `render_layer` function that classifies features (via
   `mapserver-core`'s `class` module) and draws points/lines/polygons
   styled by the matched class, mirroring the per-shape draw loop in
   `msDrawLayer()`/`msDrawShape()` (`src/mapdraw.c`) — see "Rendering" below.

## Building and testing

```sh
cd rust
cargo build
cargo test
```

## Parity testing against the C implementation

Unit tests within each crate assert that ported functions behave like their
documented C counterparts, but for a subset of small, easily-isolated
functions we go one step further and check parity directly against the
original C source at test time, rather than relying only on hand-derived
expectations.

Building and linking the full C/C++ MapServer codebase (and its GDAL/PROJ
dependencies) is out of scope for this check — instead, `mapserver-parity-tests`
takes verbatim copies of a handful of small, dependency-free reference
functions from `src/` (e.g. `msHexToInt()`, `msEncodeChar()`,
`msGetBitArraySize()`), stores them under `mapserver-parity-tests/c_ref/`,
compiles them via a `build.rs` using the [`cc`](https://crates.io/crates/cc)
crate, and calls them through FFI in `tests/parity.rs` to compare their
output directly against the corresponding Rust port function across the
full (or a representative) input domain.

This intentionally only covers "core utility" style functions that have no
transitive dependency on the rest of the C codebase (`mapserver.h`, GDAL,
etc.) and therefore compile standalone. As more of the port lands, the same
pattern can be extended to additional pure functions: add the verbatim
extract to `c_ref/`, list it in `build.rs`, declare its FFI binding and a
safe wrapper in `src/lib.rs`, and add a comparison test in
`tests/parity.rs`. Each reference file carries a comment instructing not to
"fix" it to make tests pass — a mismatch means the Rust port has a bug (or
the reference copy has drifted and needs re-syncing from `src/`), not the
other way around.

Run just the parity suite with:

```sh
cd rust
cargo test -p mapserver-parity-tests
```

## Rendering

`mapserver-render` replaces the legacy AGG-based rendering engine
(`mapagg.cpp`) with a small `Renderer` trait (`draw_point`/`draw_line`/
`draw_polygon`, styled by `mapserver-core`'s `StyleObj`) and a Skia-backed
implementation (`SkiaRenderer`) built on
[`skia-safe`](https://crates.io/crates/skia-safe), taking inspiration from
the abandoned Skia effort in
[MapServer/MapServer#6574](https://github.com/MapServer/MapServer/pull/6574).

`view::MapView` converts georeferenced coordinates to pixel coordinates
given an extent and image size (mirroring `MS_MAP2IMAGE_X`/`_Y` in
`src/mapserver.h`), and `layer::render_layer` ties this together with
`mapserver-core::class::get_class` to classify and draw a slice of
`Feature`s (as produced by a `LayerDataSource` query) onto a `Renderer`.

This first cut intentionally covers only the core per-feature drawing path:
`SYMBOL`/`PATTERN` styling, multiple layered styles per class (e.g. cased
lines), and label rendering are left as follow-up work, to be driven by
concrete needs once the OWS service layer (issue tracked by the epic) is
built on top of this.

Render a PNG and inspect it, e.g. in a test:

```rust
use mapserver_core::primitive::Rect;
use mapserver_render::{render_layer, MapView, SkiaRenderer};

let view = MapView::new(Rect { minx: 0.0, miny: 0.0, maxx: 100.0, maxy: 100.0 }, 256, 256);
let mut renderer = SkiaRenderer::new(256, 256, None);
render_layer(&features, &classes, None, None, &view, &mut renderer);
let png_bytes = renderer.encode_png();
```
