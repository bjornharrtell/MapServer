//! Core geometry primitives.
//!
//! Ported from `src/mapprimitive.h` / `src/mapprimitive.cpp` (`rectObj`,
//! `pointObj`, `lineObj`, `shapeObj` and associated algorithms) and the
//! geospatial search helpers in `src/mapsearch.c` (rectangle relations,
//! point-in-polygon, segment intersection and distance computations).
//! Behavior is kept identical to the original C implementation so results
//! stay verifiable against the C code.

/// A rectangle/bounding box. Direct port of `rectObj`.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Rect {
    pub minx: f64,
    pub miny: f64,
    pub maxx: f64,
    pub maxy: f64,
}

/// A point with x, y, z and m (measure) values. Direct port of `pointObj`.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Point {
    pub x: f64,
    pub y: f64,
    pub z: f64,
    pub m: f64,
}

impl Point {
    pub fn new(x: f64, y: f64) -> Self {
        Self {
            x,
            y,
            z: 0.0,
            m: 0.0,
        }
    }
}

/// A line composed of one or more points. Direct port of `lineObj`.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Line {
    pub points: Vec<Point>,
}

impl Line {
    pub fn new(points: Vec<Point>) -> Self {
        Self { points }
    }
}

/// The geometry type of a [`Shape`], mirroring `MS_SHAPE_*` in
/// `src/mapserver.h`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ShapeType {
    #[default]
    Null,
    Point,
    Line,
    Polygon,
}

/// A single feature's geometry. Direct (partial) port of `shapeObj`,
/// covering the parts of the struct that don't depend on GDAL/OGR or GEOS
/// (`values`, `geometry`, `renderer_cache`, annotation and bookkeeping
/// fields are left for later ports that need them).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Shape {
    pub lines: Vec<Line>,
    pub bounds: Rect,
    pub shape_type: ShapeType,
}

impl Shape {
    /// Adds a copy of `line` to the shape. Direct port of `msAddLine()`.
    pub fn add_line(&mut self, line: Line) {
        self.lines.push(line);
    }
}

/// Returns `true` if rectangles `a` and `b` overlap.
///
/// Direct port of `msRectOverlap()` from `src/mapsearch.c`.
pub fn rect_overlap(a: &Rect, b: &Rect) -> bool {
    if a.minx > b.maxx {
        return false;
    }
    if a.maxx < b.minx {
        return false;
    }
    if a.miny > b.maxy {
        return false;
    }
    if a.maxy < b.miny {
        return false;
    }
    true
}

/// Computes the intersection of two rectangles, updating `a` to be only the
/// intersection of the two. Returns `false` if the intersection is empty.
///
/// Direct port of `msRectIntersect()` from `src/mapsearch.c`. Note the
/// emptiness check only inspects the x-axis (`maxx < minx`) of `a` and `b`,
/// not the y-axis; this mirrors the original C implementation exactly
/// (intentionally, for behavior parity) even though it looks asymmetric.
pub fn rect_intersect(a: &mut Rect, b: &Rect) -> bool {
    if a.maxx > b.maxx {
        a.maxx = b.maxx;
    }
    if a.minx < b.minx {
        a.minx = b.minx;
    }
    if a.maxy > b.maxy {
        a.maxy = b.maxy;
    }
    if a.miny < b.miny {
        a.miny = b.miny;
    }

    !(a.maxx < a.minx || b.maxx < b.minx)
}

/// Returns `true` if rectangle `a` is contained in rectangle `b`.
///
/// Direct port of `msRectContained()` from `src/mapsearch.c`.
pub fn rect_contained(a: &Rect, b: &Rect) -> bool {
    a.minx >= b.minx && a.maxx <= b.maxx && a.miny >= b.miny && a.maxy <= b.maxy
}

/// Merges rect `b` into rect `a`. `a` changes, `b` does not.
///
/// Direct port of `msMergeRect()` from `src/mapsearch.c`.
pub fn merge_rect(a: &mut Rect, b: &Rect) {
    a.minx = a.minx.min(b.minx);
    a.maxx = a.maxx.max(b.maxx);
    a.miny = a.miny.min(b.miny);
    a.maxy = a.maxy.max(b.maxy);
}

/// Returns `true` if point `p` lies within `rect` (inclusive of the edges).
///
/// Direct port of `msPointInRect()` from `src/mapsearch.c`.
pub fn point_in_rect(p: &Point, rect: &Rect) -> bool {
    !(p.x < rect.minx || p.x > rect.maxx || p.y < rect.miny || p.y > rect.maxy)
}

/// Returns the winding direction of a closed ring: `1` for counter-clockwise,
/// `-1` for clockwise, `0` if degenerate/self-intersecting.
///
/// Direct port of `msPolygonDirection()` from `src/mapsearch.c`.
pub fn polygon_direction(c: &Line) -> i32 {
    let n = c.points.len();
    let last_vert = |v: usize| if v == 0 { n - 2 } else { v - 1 };
    let next_vert = |v: usize| if v == n - 2 { 0 } else { v + 1 };

    let mut mx = c.points[0].x;
    let mut my = c.points[0].y;
    let mut v = 0usize;

    for i in 0..n - 1 {
        if c.points[i].y < my || (c.points[i].y == my && c.points[i].x > mx) {
            v = i;
            mx = c.points[i].x;
            my = c.points[i].y;
        }
    }

    let lv = last_vert(v);
    let nv = next_vert(v);

    let area = c.points[lv].x * c.points[v].y - c.points[lv].y * c.points[v].x
        + c.points[lv].y * c.points[nv].x
        - c.points[lv].x * c.points[nv].y
        + c.points[v].x * c.points[nv].y
        - c.points[nv].x * c.points[v].y;

    if area > 0.0 {
        1
    } else if area < 0.0 {
        -1
    } else {
        0
    }
}

/// Returns `true` if point `p` is inside the ring `c` (even-odd rule).
///
/// Direct port of `msPointInPolygon()` from `src/mapsearch.c` (itself based
/// on the well known PNPOLY algorithm by W. Randolph Franklin).
pub fn point_in_polygon(p: &Point, c: &Line) -> bool {
    let n = c.points.len();
    let mut status = false;
    let mut j = n - 1;
    for i in 0..n {
        if ((c.points[i].y <= p.y && p.y < c.points[j].y)
            || (c.points[j].y <= p.y && p.y < c.points[i].y))
            && (p.x
                < (c.points[j].x - c.points[i].x) * (p.y - c.points[i].y)
                    / (c.points[j].y - c.points[i].y)
                    + c.points[i].x)
        {
            status = !status;
        }
        j = i;
    }
    status
}

/// Returns `true` if segment `ab` intersects segment `cd`.
///
/// Direct port of `msIntersectSegments()` from `src/mapsearch.c`.
pub fn intersect_segments(a: &Point, b: &Point, c: &Point, d: &Point) -> bool {
    let mut numerator = (a.y - c.y) * (d.x - c.x) - (a.x - c.x) * (d.y - c.y);
    let denominator = (b.x - a.x) * (d.y - c.y) - (b.y - a.y) * (d.x - c.x);

    if denominator == 0.0 && numerator == 0.0 {
        // Lines are coincident, intersection is a line segment if it exists.
        return if a.y == c.y {
            (a.x >= c.x.min(d.x) && a.x <= c.x.max(d.x))
                || (b.x >= c.x.min(d.x) && b.x <= c.x.max(d.x))
        } else {
            (a.y >= c.y.min(d.y) && a.y <= c.y.max(d.y))
                || (b.y >= c.y.min(d.y) && b.y <= c.y.max(d.y))
        };
    }

    if denominator == 0.0 {
        return false; // lines are parallel, can't intersect
    }

    let r = numerator / denominator;
    if !(0.0..=1.0).contains(&r) {
        return false;
    }

    numerator = (a.y - c.y) * (b.x - a.x) - (a.x - c.x) * (b.y - a.y);
    let s = numerator / denominator;

    (0.0..=1.0).contains(&s)
}

/// Returns `true` if `point` is inside `poly`, counting the number of rings
/// the point falls in (odd = inside, even/0 = in a hole or outside).
///
/// Direct port of `msIntersectPointPolygon()` from `src/mapsearch.c`.
pub fn intersect_point_polygon(point: &Point, poly: &Shape) -> bool {
    let mut status = false;
    for line in &poly.lines {
        if point_in_polygon(point, line) {
            status = !status;
        }
    }
    status
}

/// Squared distance between two points; avoids the `sqrt()` call needed by
/// [`distance_point_to_point`].
///
/// Direct port of `msSquareDistancePointToPoint()` from `src/mapsearch.c`.
pub fn square_distance_point_to_point(a: &Point, b: &Point) -> f64 {
    let dx = a.x - b.x;
    let dy = a.y - b.y;
    dx * dx + dy * dy
}

/// Distance between two points.
///
/// Direct port of `msDistancePointToPoint()` from `src/mapsearch.c`.
pub fn distance_point_to_point(a: &Point, b: &Point) -> f64 {
    square_distance_point_to_point(a, b).sqrt()
}

/// Squared distance between point `p` and segment `ab`; avoids the `sqrt()`
/// call needed by [`distance_point_to_segment`].
///
/// Direct port of `msSquareDistancePointToSegment()` from `src/mapsearch.c`.
pub fn square_distance_point_to_segment(p: &Point, a: &Point, b: &Point) -> f64 {
    let l_squared = square_distance_point_to_point(a, b);

    if l_squared == 0.0 {
        // a == b
        return square_distance_point_to_point(a, p);
    }

    // Equivalent to the standard projection parameter
    // `dot(p - a, b - a) / l_squared`, just written (as in the C original)
    // with each term's subtraction order flipped in a way that cancels out.
    let r = ((a.y - p.y) * (a.y - b.y) - (a.x - p.x) * (b.x - a.x)) / l_squared;

    if !(0.0..=1.0).contains(&r) {
        // Perpendicular projection of p falls outside segment ab.
        return square_distance_point_to_point(p, b).min(square_distance_point_to_point(p, a));
    }

    let s = ((a.y - p.y) * (b.x - a.x) - (a.x - p.x) * (b.y - a.y)) / l_squared;

    (s * s * l_squared).abs()
}

/// Distance between point `p` and segment `ab`.
///
/// Direct port of `msDistancePointToSegment()` from `src/mapsearch.c`.
pub fn distance_point_to_segment(p: &Point, a: &Point, b: &Point) -> f64 {
    square_distance_point_to_segment(p, a, b).sqrt()
}

/// Distance between segment `ab` and segment `cd`.
///
/// Direct port of `msDistanceSegmentToSegment()` from `src/mapsearch.c`
/// (itself a modified version of the softSurfer segment-to-segment
/// distance algorithm).
pub fn distance_segment_to_segment(pa: &Point, pb: &Point, pc: &Point, pd: &Point) -> f64 {
    let u = (pb.x - pa.x, pb.y - pa.y);
    let v = (pd.x - pc.x, pd.y - pc.y);
    let w = (pa.x - pc.x, pa.y - pc.y);

    let dot = |u: (f64, f64), v: (f64, f64)| u.0 * v.0 + u.1 * v.1;

    let a = dot(u, u);
    let b = dot(u, v);
    let c = dot(v, v);
    let d = dot(u, w);
    let e = dot(v, w);

    let dd = a * c - b * b;

    const SMALL_NUMBER: f64 = 0.00000001;

    // Compute the line parameters of the two closest points.
    let mut sn;
    let mut sd = dd;
    let mut tn;
    let mut td = dd;

    if dd < SMALL_NUMBER {
        // Lines are parallel or almost parallel.
        sn = 0.0;
        sd = 1.0;
        tn = e;
        td = c;
    } else {
        // Get the closest points on the infinite lines.
        sn = b * e - c * d;
        tn = a * e - b * d;
        if sn < 0.0 {
            sn = 0.0;
            tn = e;
            td = c;
        } else if sn > sd {
            sn = sd;
            tn = e + b;
            td = c;
        }
    }

    if tn < 0.0 {
        tn = 0.0;
        if -d < 0.0 {
            sn = 0.0;
        } else if -d > a {
            sn = sd;
        } else {
            sn = -d;
            sd = a;
        }
    } else if tn > td {
        tn = td;
        if -d + b < 0.0 {
            sn = 0.0;
        } else if -d + b > a {
            sn = sd;
        } else {
            sn = -d + b;
            sd = a;
        }
    }

    let sc = sn / sd;
    let tc = tn / td;

    let dp = (w.0 + sc * u.0 - tc * v.0, w.1 + sc * u.1 - tc * v.1);

    (dp.0 * dp.0 + dp.1 * dp.1).sqrt()
}

/// Squared distance between `point` and `shape`; avoids expensive `sqrt()`
/// calls. Direct port of `msSquareDistancePointToShape()` from
/// `src/mapsearch.c`. Returns `-1.0` if `shape` has no geometry.
pub fn square_distance_point_to_shape(point: &Point, shape: &Shape) -> f64 {
    let mut min_dist = -1.0;

    let update = |min_dist: &mut f64, dist: f64| {
        if dist < *min_dist || *min_dist < 0.0 {
            *min_dist = dist;
        }
    };

    match shape.shape_type {
        ShapeType::Point => {
            for line in &shape.lines {
                for p in &line.points {
                    update(&mut min_dist, square_distance_point_to_point(point, p));
                }
            }
        }
        ShapeType::Line => {
            for line in &shape.lines {
                for i in 1..line.points.len() {
                    update(
                        &mut min_dist,
                        square_distance_point_to_segment(
                            point,
                            &line.points[i - 1],
                            &line.points[i],
                        ),
                    );
                }
            }
        }
        ShapeType::Polygon => {
            if intersect_point_polygon(point, shape) {
                min_dist = 0.0; // point is IN the shape
            } else {
                for line in &shape.lines {
                    for i in 1..line.points.len() {
                        update(
                            &mut min_dist,
                            square_distance_point_to_segment(
                                point,
                                &line.points[i - 1],
                                &line.points[i],
                            ),
                        );
                    }
                }
            }
        }
        ShapeType::Null => {}
    }

    min_dist
}

/// Distance between `point` and `shape`.
///
/// Direct port of `msDistancePointToShape()` from `src/mapsearch.c`.
pub fn distance_point_to_shape(point: &Point, shape: &Shape) -> f64 {
    square_distance_point_to_shape(point, shape).sqrt()
}

/// Recomputes `shape.bounds` from its line vertices.
///
/// Direct port of `msComputeBounds()` from `src/mapprimitive.cpp`.
pub fn compute_bounds(shape: &mut Shape) {
    if shape.lines.is_empty() {
        return;
    }

    let Some(first) = shape.lines.iter().find(|l| !l.points.is_empty()) else {
        return;
    };
    shape.bounds.minx = first.points[0].x;
    shape.bounds.maxx = first.points[0].x;
    shape.bounds.miny = first.points[0].y;
    shape.bounds.maxy = first.points[0].y;

    for line in &shape.lines {
        for p in &line.points {
            shape.bounds.minx = shape.bounds.minx.min(p.x);
            shape.bounds.maxx = shape.bounds.maxx.max(p.x);
            shape.bounds.miny = shape.bounds.miny.min(p.y);
            shape.bounds.maxy = shape.bounds.maxy.max(p.y);
        }
    }
}

/// Converts a rectangle into a closed, clockwise polygon (assuming a
/// Cartesian coordinate system with the y-origin at the bottom), appending
/// it to `poly`.
///
/// Direct port of `msRectToPolygon()` from `src/mapprimitive.cpp`.
pub fn rect_to_polygon(rect: Rect) -> Shape {
    let line = Line::new(vec![
        Point::new(rect.minx, rect.miny),
        Point::new(rect.minx, rect.maxy),
        Point::new(rect.maxx, rect.maxy),
        Point::new(rect.maxx, rect.miny),
        Point::new(rect.minx, rect.miny),
    ]);

    let mut poly = Shape {
        shape_type: ShapeType::Polygon,
        bounds: rect,
        ..Default::default()
    };
    poly.add_line(line);
    poly
}

#[derive(Clone, Copy, PartialEq)]
enum ClipState {
    Left,
    Middle,
    Right,
}

fn clip_check(min: f64, x: f64, max: f64) -> ClipState {
    if x < min {
        ClipState::Left
    } else if x > max {
        ClipState::Right
    } else {
        ClipState::Middle
    }
}

/// Clips the segment `(x1, y1)`-`(x2, y2)` against `rect`, returning `false`
/// if the (clipped) segment lies entirely outside the rectangle.
///
/// Private implementation of the Sutherland-Cohen algorithm, ported from the
/// static `clipLine()` helper in `src/mapprimitive.cpp`.
fn clip_line(x1: &mut f64, y1: &mut f64, x2: &mut f64, y2: &mut f64, rect: &Rect) -> bool {
    if *x1 < rect.minx && *x2 < rect.minx {
        return false;
    }
    if *x1 > rect.maxx && *x2 > rect.maxx {
        return false;
    }

    let check1 = clip_check(rect.minx, *x1, rect.maxx);
    let check2 = clip_check(rect.minx, *x2, rect.maxx);
    if check1 == ClipState::Left || check2 == ClipState::Left {
        let slope = (*y2 - *y1) / (*x2 - *x1);
        let y = *y1 + (rect.minx - *x1) * slope;
        if check1 == ClipState::Left {
            *x1 = rect.minx;
            *y1 = y;
        } else {
            *x2 = rect.minx;
            *y2 = y;
        }
    }
    if check1 == ClipState::Right || check2 == ClipState::Right {
        let slope = (*y2 - *y1) / (*x2 - *x1);
        let y = *y1 + (rect.maxx - *x1) * slope;
        if check1 == ClipState::Right {
            *x1 = rect.maxx;
            *y1 = y;
        } else {
            *x2 = rect.maxx;
            *y2 = y;
        }
    }

    if *y1 < rect.miny && *y2 < rect.miny {
        return false;
    }
    if *y1 > rect.maxy && *y2 > rect.maxy {
        return false;
    }

    let check1 = clip_check(rect.miny, *y1, rect.maxy);
    let check2 = clip_check(rect.miny, *y2, rect.maxy);
    if check1 == ClipState::Left || check2 == ClipState::Left {
        let slope = (*x2 - *x1) / (*y2 - *y1);
        let x = *x1 + (rect.miny - *y1) * slope;
        if check1 == ClipState::Left {
            *x1 = x;
            *y1 = rect.miny;
        } else {
            *x2 = x;
            *y2 = rect.miny;
        }
    }
    if check1 == ClipState::Right || check2 == ClipState::Right {
        let slope = (*x2 - *x1) / (*y2 - *y1);
        let x = *x1 + (rect.maxy - *y1) * slope;
        if check1 == ClipState::Right {
            *x1 = x;
            *y1 = rect.maxy;
        } else {
            *x2 = x;
            *y2 = rect.maxy;
        }
    }

    true
}

/// Clips a polyline shape against `rect`, discarding the parts outside of
/// it.
///
/// Direct port of `msClipPolylineRect()` from `src/mapprimitive.cpp`.
pub fn clip_polyline_rect(shape: &mut Shape, rect: &Rect) {
    if shape.lines.is_empty() {
        return;
    }

    // Skip clipping if the shape is already fully contained in `rect`.
    if shape.bounds.maxx <= rect.maxx
        && shape.bounds.minx >= rect.minx
        && shape.bounds.maxy <= rect.maxy
        && shape.bounds.miny >= rect.miny
    {
        return;
    }

    let mut result: Vec<Line> = Vec::new();

    for line in &shape.lines {
        if line.points.is_empty() {
            continue;
        }
        let mut points: Vec<Point> = Vec::new();

        let mut x1 = line.points[0].x;
        let mut y1 = line.points[0].y;
        for j in 1..line.points.len() {
            let mut x2 = line.points[j].x;
            let mut y2 = line.points[j].y;

            if clip_line(&mut x1, &mut y1, &mut x2, &mut y2, rect) {
                if points.is_empty() {
                    points.push(Point::new(x1, y1));
                    points.push(Point::new(x2, y2));
                } else {
                    points.push(Point::new(x2, y2));
                }

                if x2 != line.points[j].x || y2 != line.points[j].y {
                    result.push(Line::new(std::mem::take(&mut points)));
                }
            }

            x1 = line.points[j].x;
            y1 = line.points[j].y;
        }

        if !points.is_empty() {
            result.push(Line::new(points));
        }
    }

    shape.lines = result;
    compute_bounds(shape);
}

const NEARZERO: f64 = 0.000001;

/// Clips a polygon shape against `rect`, discarding the parts outside of it.
///
/// Direct port of `msClipPolygonRect()` from `src/mapprimitive.cpp`, using a
/// slightly modified version of the Liang-Barsky polygon clipping algorithm.
pub fn clip_polygon_rect(shape: &mut Shape, rect: &Rect) {
    if shape.lines.is_empty() {
        return;
    }

    // Skip clipping if the shape is already fully contained in `rect`.
    if shape.bounds.maxx <= rect.maxx
        && shape.bounds.minx >= rect.minx
        && shape.bounds.maxy <= rect.maxy
        && shape.bounds.miny >= rect.miny
    {
        return;
    }

    let mut result: Vec<Line> = Vec::new();

    for line in &shape.lines {
        if line.points.len() < 2 {
            continue;
        }
        let mut points: Vec<Point> = Vec::new();

        for i in 0..line.points.len() - 1 {
            let x1 = line.points[i].x;
            let y1 = line.points[i].y;
            let x2 = line.points[i + 1].x;
            let y2 = line.points[i + 1].y;

            let mut deltax = x2 - x1;
            if deltax == 0.0 {
                deltax = if x1 > rect.minx { -NEARZERO } else { NEARZERO };
            }
            let mut deltay = y2 - y1;
            if deltay == 0.0 {
                deltay = if y1 > rect.miny { -NEARZERO } else { NEARZERO };
            }

            let (xin, xout) = if deltax > 0.0 {
                (rect.minx, rect.maxx)
            } else {
                (rect.maxx, rect.minx)
            };
            let (yin, yout) = if deltay > 0.0 {
                (rect.miny, rect.maxy)
            } else {
                (rect.maxy, rect.miny)
            };

            let tinx = (xin - x1) / deltax;
            let tiny = (yin - y1) / deltay;

            let (tin1, tin2) = if tinx < tiny {
                (tinx, tiny)
            } else {
                (tiny, tinx)
            };

            if 1.0 >= tin1 {
                if 0.0 < tin1 {
                    points.push(Point::new(xin, yin));
                }
                if 1.0 >= tin2 {
                    let toutx = (xout - x1) / deltax;
                    let touty = (yout - y1) / deltay;
                    let tout = if toutx < touty { toutx } else { touty };

                    if 0.0 < tin2 || 0.0 < tout {
                        if tin2 <= tout {
                            if 0.0 < tin2 {
                                if tinx > tiny {
                                    points.push(Point::new(xin, y1 + tinx * deltay));
                                } else {
                                    points.push(Point::new(x1 + tiny * deltax, yin));
                                }
                            }
                            if 1.0 > tout {
                                if toutx < touty {
                                    points.push(Point::new(xout, y1 + toutx * deltay));
                                } else {
                                    points.push(Point::new(x1 + touty * deltax, yout));
                                }
                            } else {
                                points.push(Point::new(x2, y2));
                            }
                        } else if tinx > tiny {
                            points.push(Point::new(xin, yout));
                        } else {
                            points.push(Point::new(xout, yin));
                        }
                    }
                }
            }
        }

        if !points.is_empty() {
            let first = points[0];
            points.push(first); // force closure
            result.push(Line::new(points));
        }
    }

    shape.lines = result;
    compute_bounds(shape);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rect_overlap_matches_c() {
        let a = Rect {
            minx: 0.0,
            miny: 0.0,
            maxx: 10.0,
            maxy: 10.0,
        };
        let b = Rect {
            minx: 5.0,
            miny: 5.0,
            maxx: 15.0,
            maxy: 15.0,
        };
        let c = Rect {
            minx: 20.0,
            miny: 20.0,
            maxx: 30.0,
            maxy: 30.0,
        };
        assert!(rect_overlap(&a, &b));
        assert!(!rect_overlap(&a, &c));
    }

    #[test]
    fn rect_intersect_updates_a() {
        let mut a = Rect {
            minx: 0.0,
            miny: 0.0,
            maxx: 10.0,
            maxy: 10.0,
        };
        let b = Rect {
            minx: 5.0,
            miny: 5.0,
            maxx: 15.0,
            maxy: 15.0,
        };
        assert!(rect_intersect(&mut a, &b));
        assert_eq!(
            a,
            Rect {
                minx: 5.0,
                miny: 5.0,
                maxx: 10.0,
                maxy: 10.0
            }
        );
    }

    #[test]
    fn rect_contained_matches_c() {
        let outer = Rect {
            minx: 0.0,
            miny: 0.0,
            maxx: 10.0,
            maxy: 10.0,
        };
        let inner = Rect {
            minx: 2.0,
            miny: 2.0,
            maxx: 8.0,
            maxy: 8.0,
        };
        assert!(rect_contained(&inner, &outer));
        assert!(!rect_contained(&outer, &inner));
    }

    #[test]
    fn merge_rect_grows_a() {
        let mut a = Rect {
            minx: 0.0,
            miny: 0.0,
            maxx: 5.0,
            maxy: 5.0,
        };
        let b = Rect {
            minx: -1.0,
            miny: 2.0,
            maxx: 6.0,
            maxy: 3.0,
        };
        merge_rect(&mut a, &b);
        assert_eq!(
            a,
            Rect {
                minx: -1.0,
                miny: 0.0,
                maxx: 6.0,
                maxy: 5.0
            }
        );
    }

    #[test]
    fn point_in_rect_inclusive_of_edges() {
        let rect = Rect {
            minx: 0.0,
            miny: 0.0,
            maxx: 10.0,
            maxy: 10.0,
        };
        assert!(point_in_rect(&Point::new(0.0, 0.0), &rect));
        assert!(point_in_rect(&Point::new(10.0, 10.0), &rect));
        assert!(!point_in_rect(&Point::new(10.1, 5.0), &rect));
    }

    fn square(side: f64) -> Line {
        Line::new(vec![
            Point::new(0.0, 0.0),
            Point::new(side, 0.0),
            Point::new(side, side),
            Point::new(0.0, side),
            Point::new(0.0, 0.0),
        ])
    }

    #[test]
    fn polygon_direction_counter_clockwise() {
        // The square above is wound counter-clockwise.
        assert_eq!(polygon_direction(&square(10.0)), 1);
    }

    #[test]
    fn polygon_direction_clockwise() {
        let mut cw = square(10.0);
        cw.points.reverse();
        assert_eq!(polygon_direction(&cw), -1);
    }

    #[test]
    fn point_in_polygon_basic_square() {
        let ring = square(10.0);
        assert!(point_in_polygon(&Point::new(5.0, 5.0), &ring));
        assert!(!point_in_polygon(&Point::new(15.0, 5.0), &ring));
    }

    #[test]
    fn intersect_segments_crossing() {
        let a = Point::new(0.0, 0.0);
        let b = Point::new(10.0, 10.0);
        let c = Point::new(0.0, 10.0);
        let d = Point::new(10.0, 0.0);
        assert!(intersect_segments(&a, &b, &c, &d));
    }

    #[test]
    fn intersect_segments_parallel_no_cross() {
        let a = Point::new(0.0, 0.0);
        let b = Point::new(10.0, 0.0);
        let c = Point::new(0.0, 5.0);
        let d = Point::new(10.0, 5.0);
        assert!(!intersect_segments(&a, &b, &c, &d));
    }

    #[test]
    fn intersect_point_polygon_hole() {
        let mut outer = square(10.0);
        outer.points.reverse(); // outer ring
        let mut hole = Line::new(vec![
            Point::new(2.0, 2.0),
            Point::new(2.0, 8.0),
            Point::new(8.0, 8.0),
            Point::new(8.0, 2.0),
            Point::new(2.0, 2.0),
        ]);
        hole.points.reverse();
        let shape = Shape {
            lines: vec![outer, hole],
            shape_type: ShapeType::Polygon,
            bounds: Rect {
                minx: 0.0,
                miny: 0.0,
                maxx: 10.0,
                maxy: 10.0,
            },
        };
        assert!(intersect_point_polygon(&Point::new(1.0, 1.0), &shape)); // in the ring, outside hole
        assert!(!intersect_point_polygon(&Point::new(5.0, 5.0), &shape)); // inside the hole
    }

    #[test]
    fn distance_point_to_point_matches_c() {
        let a = Point::new(0.0, 0.0);
        let b = Point::new(3.0, 4.0);
        assert_eq!(distance_point_to_point(&a, &b), 5.0);
    }

    #[test]
    fn distance_point_to_segment_perpendicular_and_endpoints() {
        let a = Point::new(0.0, 0.0);
        let b = Point::new(10.0, 0.0);
        assert_eq!(
            distance_point_to_segment(&Point::new(5.0, 5.0), &a, &b),
            5.0
        );
        assert_eq!(
            distance_point_to_segment(&Point::new(-5.0, 0.0), &a, &b),
            5.0
        );
        assert_eq!(distance_point_to_segment(&a, &a, &a), 0.0);
    }

    #[test]
    fn distance_segment_to_segment_intersecting_is_zero() {
        let pa = Point::new(0.0, 0.0);
        let pb = Point::new(10.0, 10.0);
        let pc = Point::new(0.0, 10.0);
        let pd = Point::new(10.0, 0.0);
        assert!(distance_segment_to_segment(&pa, &pb, &pc, &pd) < 1e-9);
    }

    #[test]
    fn distance_segment_to_segment_parallel() {
        let pa = Point::new(0.0, 0.0);
        let pb = Point::new(10.0, 0.0);
        let pc = Point::new(0.0, 5.0);
        let pd = Point::new(10.0, 5.0);
        assert!((distance_segment_to_segment(&pa, &pb, &pc, &pd) - 5.0).abs() < 1e-9);
    }

    #[test]
    fn distance_point_to_shape_polygon_inside_is_zero() {
        let mut shape = Shape {
            lines: vec![square(10.0)],
            shape_type: ShapeType::Polygon,
            bounds: Rect {
                minx: 0.0,
                miny: 0.0,
                maxx: 10.0,
                maxy: 10.0,
            },
        };
        compute_bounds(&mut shape);
        assert_eq!(distance_point_to_shape(&Point::new(5.0, 5.0), &shape), 0.0);
        assert!((distance_point_to_shape(&Point::new(15.0, 5.0), &shape) - 5.0).abs() < 1e-9);
    }

    #[test]
    fn compute_bounds_matches_c() {
        let mut shape = Shape {
            lines: vec![Line::new(vec![
                Point::new(1.0, 2.0),
                Point::new(-3.0, 4.0),
                Point::new(5.0, -6.0),
            ])],
            ..Default::default()
        };
        compute_bounds(&mut shape);
        assert_eq!(
            shape.bounds,
            Rect {
                minx: -3.0,
                miny: -6.0,
                maxx: 5.0,
                maxy: 4.0
            }
        );
    }

    #[test]
    fn rect_to_polygon_builds_closed_ring() {
        let rect = Rect {
            minx: 0.0,
            miny: 0.0,
            maxx: 10.0,
            maxy: 5.0,
        };
        let poly = rect_to_polygon(rect);
        assert_eq!(poly.shape_type, ShapeType::Polygon);
        assert_eq!(poly.bounds, rect);
        assert_eq!(poly.lines.len(), 1);
        assert_eq!(poly.lines[0].points.len(), 5);
        assert_eq!(poly.lines[0].points[0], poly.lines[0].points[4]);
    }

    #[test]
    fn clip_polyline_rect_trims_to_bounds() {
        let mut shape = Shape {
            lines: vec![Line::new(vec![
                Point::new(-5.0, 5.0),
                Point::new(15.0, 5.0),
            ])],
            shape_type: ShapeType::Line,
            ..Default::default()
        };
        compute_bounds(&mut shape);
        let rect = Rect {
            minx: 0.0,
            miny: 0.0,
            maxx: 10.0,
            maxy: 10.0,
        };
        clip_polyline_rect(&mut shape, &rect);
        assert_eq!(shape.lines.len(), 1);
        assert_eq!(shape.lines[0].points[0], Point::new(0.0, 5.0));
        assert_eq!(shape.lines[0].points[1], Point::new(10.0, 5.0));
    }

    #[test]
    fn clip_polyline_rect_skips_fully_contained_shape() {
        let mut shape = Shape {
            lines: vec![Line::new(vec![Point::new(1.0, 1.0), Point::new(2.0, 2.0)])],
            shape_type: ShapeType::Line,
            bounds: Rect {
                minx: 1.0,
                miny: 1.0,
                maxx: 2.0,
                maxy: 2.0,
            },
        };
        let original = shape.clone();
        let rect = Rect {
            minx: 0.0,
            miny: 0.0,
            maxx: 10.0,
            maxy: 10.0,
        };
        clip_polyline_rect(&mut shape, &rect);
        assert_eq!(shape, original);
    }

    #[test]
    fn clip_polygon_rect_clips_square_overhang() {
        // A square from (-5,-5) to (5,5) clipped against the unit rect
        // [0,0]-[10,10] should produce the quarter square [0,0]-[5,5].
        let mut shape = Shape {
            lines: vec![Line::new(vec![
                Point::new(-5.0, -5.0),
                Point::new(5.0, -5.0),
                Point::new(5.0, 5.0),
                Point::new(-5.0, 5.0),
                Point::new(-5.0, -5.0),
            ])],
            shape_type: ShapeType::Polygon,
            ..Default::default()
        };
        compute_bounds(&mut shape);
        let rect = Rect {
            minx: 0.0,
            miny: 0.0,
            maxx: 10.0,
            maxy: 10.0,
        };
        clip_polygon_rect(&mut shape, &rect);
        assert_eq!(shape.bounds.minx, 0.0);
        assert_eq!(shape.bounds.miny, 0.0);
        assert_eq!(shape.bounds.maxx, 5.0);
        assert_eq!(shape.bounds.maxy, 5.0);
    }
}
