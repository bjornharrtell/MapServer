//! Rendering backend abstraction.
//!
//! MapServer's C renderer abstraction (`rendererVTableObj` in
//! `src/mapserver.h`) is implemented by multiple backends (AGG, OGR, MVT,
//! ...). This crate follows the same shape but with a single Skia-backed
//! implementation ([`crate::skia_backend::SkiaRenderer`]), per the epic's
//! decision to replace AGG with Skia (bjornharrtell/MapServer#2) rather than
//! port AGG itself.
//!
//! The trait only covers the drawing primitives needed to render a
//! `shapeObj` styled by a `styleObj` (points/markers, lines, polygons),
//! mirroring the relevant parts of `msDrawShape()`/`msDrawMarkerSymbol()`/
//! `msDrawLineSymbol()`/`msDrawShadeSymbol()` in `src/mapdraw.c`. Label
//! rendering, symbol images/patterns, and other advanced styling
//! (`SYMBOL`, `PATTERN`, hatch fills, etc.) are out of scope for this first
//! cut and are left as follow-up work once the OWS layer (#13) drives
//! concrete requirements.

use mapserver_core::class::StyleObj;

/// A point in pixel space, as produced by
/// [`crate::view::MapView::geo_to_pixel`].
pub type PixelPoint = (f32, f32);

/// A rendering backend capable of drawing styled vector geometry onto an
/// image surface.
pub trait Renderer {
    /// Draws a point marker at `p`, styled by `style` (`color`/`size`).
    /// Analogous to `msDrawMarkerSymbol()`.
    fn draw_point(&mut self, p: PixelPoint, style: &StyleObj);

    /// Draws a polyline through `points`, styled by `style`
    /// (`color`/`width`). Analogous to `msDrawLineSymbol()`.
    fn draw_line(&mut self, points: &[PixelPoint], style: &StyleObj);

    /// Draws a filled (and optionally outlined) polygon made up of `rings`
    /// (exterior ring plus any interior/hole rings), styled by `style`
    /// (`color` for fill, `outline_color`/`outline_width` for the stroke).
    /// Analogous to `msDrawShadeSymbol()`.
    fn draw_polygon(&mut self, rings: &[Vec<PixelPoint>], style: &StyleObj);
}
