use core::cmp::Ordering;

use crate::Vec2;
use crate::ops;

/// Relative tolerance used to group segments that touch the sweep line at
/// (approximately) the same height.
///
/// The sweep position `y_at` is computed by linear interpolation, which can be
/// off by a few ULPs even for exactly coincident points; ties must therefore
/// be detected with slack instead of exact equality.
const EPSILON: f32 = 1e-5;

/// Whether two heights are close enough to be treated as touching the sweep
/// line at the same point.
fn near(a: f32, b: f32) -> bool {
    ops::abs(a - b) <= EPSILON * (1.0 + ops::abs(a).max(ops::abs(b)))
}

/// Returns whether the point `q` lies on the (closed) segment `p1`-`p2`.
#[inline]
fn on_segment(p1: Vec2, p2: Vec2, q: Vec2) -> bool {
    q.x >= p1.x.min(p2.x) && q.x <= p1.x.max(p2.x) && q.y >= p1.y.min(p2.y) && q.y <= p1.y.max(p2.y)
}

/// Tests whether the two segments `a1`-`a2` and `b1`-`b2` intersect.
///
/// Returns `true` for a proper crossing as well as for touching (an endpoint
/// on the other segment) and overlapping collinear segments.
///
/// The cross products are compared with a tolerance: a true tangency is only
/// *approximately* zero in floating point (e.g. `3.6666667·(-1.6666666) -
/// (-3.6666666)·1.6666666` computes to `±1e-7` instead of `0`), so exact
/// `== 0.0` tests would silently miss vertices lying on edges.
#[inline]
fn segments_intersect(a1: Vec2, a2: Vec2, b1: Vec2, b2: Vec2) -> bool {
    let d1 = (a2 - a1).perp_dot(b1 - a1);
    let d2 = (a2 - a1).perp_dot(b2 - a1);
    let d3 = (b2 - b1).perp_dot(a1 - b1);
    let d4 = (b2 - b1).perp_dot(a2 - b1);

    let tol = EPSILON * (1.0 + ops::abs(d1) + ops::abs(d2) + ops::abs(d3) + ops::abs(d4));

    if ((d1 > tol && d2 < -tol) || (d1 < -tol && d2 > tol))
        && ((d3 > tol && d4 < -tol) || (d3 < -tol && d4 > tol))
    {
        return true;
    }
    if ops::abs(d1) <= tol && on_segment(a1, a2, b1) {
        return true;
    }
    if ops::abs(d2) <= tol && on_segment(a1, a2, b2) {
        return true;
    }
    if ops::abs(d3) <= tol && on_segment(b1, b2, a1) {
        return true;
    }
    if ops::abs(d4) <= tol && on_segment(b1, b2, a2) {
        return true;
    }
    false
}

/// Orders 2D points from -X to +X and then -Y to +Y.
fn xy_order(a: Vec2, b: Vec2) -> Ordering {
    a.x.total_cmp(&b.x).then_with(|| a.y.total_cmp(&b.y))
}

/// Type of the endpoint of an edge that generates a sweep line event.
#[derive(Debug, Clone, Copy)]
enum Endpoint {
    Left,
    Right,
}

/// A polygon edge with a normalized `(left, right)` orientation
/// (left.x <= right.x).
#[derive(Debug, Clone, Copy)]
struct Segment {
    edge_index: usize,
    left: Vec2,
    right: Vec2,
    /// Whether the segment is vertical (`left.x == right.x`); vertical
    /// segments span a whole y range at their x and are handled separately.
    vertical: bool,
    /// `dy/dx` of the segment; `f32::INFINITY` for vertical segments.
    slope: f32,
}

impl Segment {
    fn new(edge_index: usize, v1: Vec2, v2: Vec2) -> Self {
        let (left, right) = if xy_order(v1, v2) == Ordering::Less {
            (v1, v2)
        } else {
            (v2, v1)
        };
        let dx = right.x - left.x;
        let vertical = dx == 0.0;
        let slope = if vertical {
            f32::INFINITY
        } else {
            (right.y - left.y) / dx
        };
        Self {
            edge_index,
            left,
            right,
            vertical,
            slope,
        }
    }
}

/// An event in the [`EventQueue`] is either the left or right vertex of an
/// edge of the polygon.
///
/// Events are ordered so that any event `e1` which is to the left of another
/// event `e2` is less than that event.  At the same `x`, **left** endpoints
/// are processed before **right** endpoints, and ties are broken from bottom
/// to top.  Processing all left events before the right events at the same
/// point is what makes coincident vertices (shared by edges that end and edges
/// that start there) check against each other while both are still active.
#[derive(Debug, Clone, Copy)]
struct SweepLineEvent {
    segment: Segment,
    /// Type of the vertex (left or right)
    endpoint: Endpoint,
}

impl SweepLineEvent {
    const fn position(&self) -> Vec2 {
        match self.endpoint {
            Endpoint::Left => self.segment.left,
            Endpoint::Right => self.segment.right,
        }
    }
}

/// The event queue holds an ordered list of all events the [`SweepLine`] will
/// encounter when checking the current polygon.
#[derive(Debug, Clone)]
struct EventQueue {
    events: Vec<SweepLineEvent>,
}

impl EventQueue {
    /// Initialize a new `EventQueue` with all events from the polygon
    /// represented by `vertices`.
    ///
    /// The events in the event queue will be ordered.
    fn new(vertices: &[Vec2]) -> Self {
        if vertices.is_empty() {
            return Self { events: Vec::new() };
        }

        let mut events = Vec::with_capacity(vertices.len() * 2);
        for i in 0..vertices.len() {
            let v1 = vertices[i];
            let v2 = *vertices.get(i + 1).unwrap_or(&vertices[0]);
            let segment = Segment::new(i, v1, v2);
            events.push(SweepLineEvent {
                segment,
                endpoint: Endpoint::Left,
            });
            events.push(SweepLineEvent {
                segment,
                endpoint: Endpoint::Right,
            });
        }

        events.sort_by(|a, b| {
            a.position()
                .x
                .total_cmp(&b.position().x)
                .then_with(|| match (a.endpoint, b.endpoint) {
                    (Endpoint::Left, Endpoint::Right) => Ordering::Less,
                    (Endpoint::Right, Endpoint::Left) => Ordering::Greater,
                    _ => a.position().y.total_cmp(&b.position().y),
                })
        });

        Self { events }
    }
}

/// A sweep line keeps the **active** segments ordered by their y coordinate at
/// the current sweep position `x`.
///
/// This is the classic Shamos-Hoey sweep: any two intersecting segments become
/// adjacent in the active list at some event, where they are tested.  The
/// ordering key is `(y at x, slope, edge index)`: two segments at the same
/// height at `x` pass through the same point, so their order just right of `x`
/// is determined by their slopes — using the slope as a tie-break keeps the
/// list sorted as the sweep advances past shared vertices where adjacent edges
/// diverge (their order would otherwise flip without an intersection).
#[derive(Debug, Clone)]
struct SweepLine<'a> {
    vertices: &'a [Vec2],
    /// Active non-vertical segments, sorted by [`SweepLine::key`].
    active: Vec<Segment>,
    /// Active vertical segments at the current `x`; a vertical segment spans a
    /// whole y range at its x, so its contacts are tested exactly instead of
    /// through the height-based ordering.
    verticals: Vec<Segment>,
    x: f32,
}

impl<'a> SweepLine<'a> {
    fn new(vertices: &'a [Vec2]) -> Self {
        Self {
            vertices,
            active: Vec::new(),
            verticals: Vec::new(),
            x: 0.0,
        }
    }

    /// The y of the segment at the current sweep position `x`.
    fn y_at(&self, s: &Segment) -> f32 {
        if s.vertical {
            s.left.y
        } else {
            s.left.y + s.slope * (self.x - s.left.x)
        }
    }

    /// The ordering key of `s` at the current sweep position.
    fn key(&self, s: &Segment) -> (f32, f32, usize) {
        (self.y_at(s), s.slope, s.edge_index)
    }

    /// Position of `s` in `active` (non-vertical only).
    ///
    /// Floating point interpolation can disorder segments that are within a
    /// few ULPs of each other, so the exact position is found by scanning the
    /// tolerance band around the binary-search result.
    fn find_pos(&self, s: &Segment) -> usize {
        let key = self.key(s);
        let pos = self.active.partition_point(|t| self.key(t) < key);
        if self
            .active
            .get(pos)
            .is_some_and(|t| t.edge_index == s.edge_index)
        {
            return pos;
        }
        let y = key.0;
        let mut i = pos;
        while i > 0 && near(self.y_at(&self.active[i - 1]), y) {
            i -= 1;
        }
        while i < self.active.len() && near(self.y_at(&self.active[i]), y) {
            if self.active[i].edge_index == s.edge_index {
                return i;
            }
            i += 1;
        }
        debug_assert!(false, "segment to remove must be present");
        pos
    }

    /// Determine whether the given edges of the polygon intersect.
    fn intersects(&self, edge1: usize, edge2: usize) -> bool {
        // All adjacent edges intersect at their shared vertex, and a segment
        // always intersects itself / an identical edge; those do not count.
        if edge1 == edge2
            || (edge1 + 1) % self.vertices.len() == edge2
            || (edge2 + 1) % self.vertices.len() == edge1
        {
            return false;
        }

        let s11 = self.vertices[edge1];
        let s12 = *self.vertices.get(edge1 + 1).unwrap_or(&self.vertices[0]);
        let s21 = self.vertices[edge2];
        let s22 = *self.vertices.get(edge2 + 1).unwrap_or(&self.vertices[0]);

        segments_intersect(s11, s12, s21, s22)
    }

    /// Check `s` (at position `pos` in `active`) against every segment it
    /// could touch at the current sweep position: the band of segments at
    /// (approximately) the same height, the band's immediate neighbours, and
    /// all active vertical segments.
    fn check_contacts(&self, s: &Segment, pos: usize) -> bool {
        let y = self.y_at(s);

        let mut start = pos;
        while start > 0 && near(self.y_at(&self.active[start - 1]), y) {
            start -= 1;
        }
        let mut end = pos;
        while end + 1 < self.active.len() && near(self.y_at(&self.active[end + 1]), y) {
            end += 1;
        }

        for i in start..=end {
            if i != pos && self.intersects(s.edge_index, self.active[i].edge_index) {
                return true;
            }
        }
        if start > 0 && self.intersects(s.edge_index, self.active[start - 1].edge_index) {
            return true;
        }
        if end + 1 < self.active.len()
            && self.intersects(s.edge_index, self.active[end + 1].edge_index)
        {
            return true;
        }

        // Vertical segments span a whole y range at their x; test them
        // exactly since their contacts are not captured by the height band.
        for v in &self.verticals {
            if self.intersects(s.edge_index, v.edge_index) {
                return true;
            }
        }

        false
    }

    /// Check the vertical segment `s` against every segment it could touch:
    /// all active non-vertical segments (which cross its x) and the other
    /// active vertical segments.
    fn check_vertical_contacts(&self, s: &Segment) -> bool {
        for t in &self.active {
            if self.intersects(s.edge_index, t.edge_index) {
                return true;
            }
        }
        for v in &self.verticals {
            if v.edge_index != s.edge_index && self.intersects(s.edge_index, v.edge_index) {
                return true;
            }
        }
        false
    }

    /// Insert `s` and check it against every segment it could touch.
    fn insert(&mut self, s: Segment) -> bool {
        if s.vertical {
            self.verticals.push(s);
            return self.check_vertical_contacts(&s);
        }
        let pos = self.active.partition_point(|t| self.key(t) < self.key(&s));
        self.active.insert(pos, s);
        self.check_contacts(&s, pos)
    }

    /// Remove `s`; before doing so check it against everything it could touch
    /// (its right endpoint may lie on another edge), and afterwards check the
    /// segments that become neighbours against each other.
    fn remove(&mut self, s: &Segment) -> bool {
        if s.vertical {
            if self.check_vertical_contacts(s) {
                return true;
            }
            self.verticals.retain(|v| v.edge_index != s.edge_index);
            return false;
        }

        let pos = self.find_pos(s);
        if self.check_contacts(s, pos) {
            return true;
        }
        // After the removal the neighbours become adjacent; a crossing pair
        // must be adjacent at some moment, so check them against each other.
        if pos > 0 && pos + 1 < self.active.len() {
            let a = self.active[pos - 1].edge_index;
            let b = self.active[pos + 1].edge_index;
            if self.intersects(a, b) {
                return true;
            }
        }
        self.active.remove(pos);
        false
    }
}

/// Tests whether the `vertices` describe a simple polygon.
/// The last vertex must not be equal to the first vertex.
///
/// A polygon is simple if it is not self intersecting and not self tangent.
/// As such, no two edges of the polygon may cross each other and each vertex must not lie on another edge.
///
/// Any 'polygon' with less than three vertices is simple.
///
/// The algorithm used is the Shamos-Hoey algorithm, a version of the Bentley-Ottman algorithm adapted to only detect whether any intersections exist.
/// This function will run in O(n * log n)
pub(crate) fn is_polygon_simple(vertices: &[Vec2]) -> bool {
    if vertices.len() < 3 {
        return true;
    }
    if vertices.len() == 3 {
        // A triangle is simple iff it is not degenerate (zero area).
        let [a, b, c] = [vertices[0], vertices[1], vertices[2]];
        return 0.5 * ops::abs((b - a).perp_dot(c - a)) > 0.0;
    }

    let event_queue = EventQueue::new(vertices);
    let mut sweep_line = SweepLine::new(vertices);

    for e in event_queue.events {
        sweep_line.x = e.position().x;
        let intersects = match e.endpoint {
            Endpoint::Left => sweep_line.insert(e.segment),
            Endpoint::Right => sweep_line.remove(&e.segment),
        };
        if intersects {
            return false;
        }
    }

    true
}

#[cfg(test)]
mod tests {
    use super::{is_polygon_simple, segments_intersect};
    use crate::Vec2;

    #[test]
    fn complex_polygon() {
        // A square with one side punching through the opposite side.
        let verts = [Vec2::ZERO, Vec2::X, Vec2::ONE, Vec2::Y, Vec2::new(2.0, 0.5)];
        assert!(!is_polygon_simple(&verts));

        // A square with a vertex from one side touching the opposite side.
        let verts = [Vec2::ZERO, Vec2::X, Vec2::ONE, Vec2::Y, Vec2::new(1.0, 0.5)];
        assert!(!is_polygon_simple(&verts));

        // A square with one side touching the opposite side.
        let verts = [
            Vec2::ZERO,
            Vec2::X,
            Vec2::ONE,
            Vec2::Y,
            Vec2::new(1.0, 0.6),
            Vec2::new(1.0, 0.4),
        ];
        assert!(!is_polygon_simple(&verts));

        // Four points lying on a line
        let verts = [Vec2::ONE, Vec2::new(3., 2.), Vec2::new(5., 3.), Vec2::NEG_X];
        assert!(!is_polygon_simple(&verts));

        // Three points lying on a line
        let verts = [Vec2::ONE, Vec2::new(3., 2.), Vec2::NEG_X];
        assert!(!is_polygon_simple(&verts));

        // Two identical points and one other point
        let verts = [Vec2::ONE, Vec2::ONE, Vec2::NEG_X];
        assert!(!is_polygon_simple(&verts));

        // Two triangles with one shared side
        let verts = [Vec2::ZERO, Vec2::X, Vec2::Y, Vec2::ONE, Vec2::X, Vec2::Y];
        assert!(!is_polygon_simple(&verts));
    }

    #[test]
    fn simple_polygon() {
        // A square
        let verts = [Vec2::ZERO, Vec2::X, Vec2::ONE, Vec2::Y];
        assert!(is_polygon_simple(&verts));

        let verts = [];
        assert!(is_polygon_simple(&verts));
    }

    #[test]
    fn floating_point_precision() {
        let verts = [
            Vec2::new(0.0, 0.0), // A
            Vec2::new(1.0, 0.5), // B ← DE
            Vec2::new(2.0, 1.0), // C
            Vec2::new(2.0, 0.0), // D
            Vec2::new(0.0, 1.0), // E
        ];

        assert!(!is_polygon_simple(&verts));
    }

    #[test]
    fn triangle_with_duplicate_vertex() {
        let verts = [
            Vec2::new(0.0, 0.0), // A
            Vec2::new(0.0, 0.0), // B ← A
            Vec2::new(1.0, 0.0), // C
        ];

        assert!(!is_polygon_simple(&verts));
    }

    // ── Edge-case regression tests ──

    #[test]
    fn bowtie_self_intersecting() {
        // Classic self-intersecting "bow tie": e0 and e2 cross in their interiors
        let verts = [
            Vec2::new(0., 0.),
            Vec2::new(1., 1.),
            Vec2::new(1., 0.),
            Vec2::new(0., 1.),
        ];
        assert!(!is_polygon_simple(&verts));
    }

    #[test]
    fn edges_with_same_sweep_key() {
        // e0 (0,0)→(2,1) and e2 (1,1)→(1,0) cross at (1, 0.5); e0/e2 also share
        // the same (left.y, right.y) sweep key (= (0,1)), which used to rely on
        // BTreeMap key deduplication behavior.
        let verts = [
            Vec2::new(0., 0.),
            Vec2::new(2., 1.),
            Vec2::new(1., 1.),
            Vec2::new(1., 0.),
        ];
        assert!(!is_polygon_simple(&verts));
    }

    #[test]
    fn vertex_on_edge() {
        // A vertex lies exactly on the interior of another edge (self-tangency)
        let verts = [
            Vec2::new(0., 0.),
            Vec2::new(2., 0.),
            Vec2::new(1., 0.),
            Vec2::new(1., 1.),
        ];
        assert!(!is_polygon_simple(&verts));
    }

    #[test]
    fn zero_length_edge() {
        // 4+ vertices with a zero-length edge (adjacent duplicate vertices)
        let verts = [
            Vec2::new(0., 0.),
            Vec2::new(1., 1.),
            Vec2::new(1., 1.),
            Vec2::new(1., 0.),
            Vec2::new(0., 1.),
        ];
        assert!(!is_polygon_simple(&verts));
    }

    #[test]
    fn shared_left_vertex_with_different_slopes() {
        // e1 and e2 share the left vertex (-2,2) but have different slopes
        // (1/4 vs 1/5); past that vertex the true y order flips to [e2, e1].
        // The old static key (left.y, right.y) ordered them [e1, e2], so e3's
        // below-neighbor was picked wrong and (2,3)∈e3 was missed. The slope
        // tie-break fixes this.
        let verts = [
            Vec2::new(-1., 3.),
            Vec2::new(2., 3.),
            Vec2::new(-2., 2.),
            Vec2::new(3., 3.),
        ];
        assert!(!is_polygon_simple(&verts));
    }

    #[test]
    fn vertex_on_vertical_edge() {
        // Vertex (0,1) lies on the interior of the vertical edge (0,0)-(0,2);
        // a vertical edge spans a y range at its x, so it needs exact tests
        // rather than the height-band scan.
        let verts = [
            Vec2::new(0., 0.),
            Vec2::new(0., 2.),
            Vec2::new(2., 0.),
            Vec2::new(0., 1.),
        ];
        assert!(!is_polygon_simple(&verts));
    }

    #[test]
    fn coincident_non_adjacent_vertices() {
        // The polygon visits the same point (2,0) twice (non-adjacently); the
        // edges sharing that vertex must be checked against each other.
        let verts = [
            Vec2::new(0., 0.),
            Vec2::new(2., 0.),
            Vec2::new(3., 1.),
            Vec2::new(2., 2.),
            Vec2::new(2., 0.),
            Vec2::new(0., 1.),
        ];
        assert!(!is_polygon_simple(&verts));
    }

    // ── Reference implementation and fuzzing ──

    /// Brute-force reference: checks whether any two non-adjacent edges
    /// intersect (crossing, tangency, or overlap all count).
    /// Shares the same `segments_intersect` primitive as the main implementation.
    fn is_simple_bruteforce(verts: &[Vec2]) -> bool {
        let n = verts.len();
        if n < 3 {
            return true;
        }
        for i in 0..n {
            let a1 = verts[i];
            let a2 = verts[(i + 1) % n];
            for j in (i + 1)..n {
                if j == i + 1 || (i == 0 && j == n - 1) {
                    continue; // Adjacent edges share a vertex; ignore
                }
                let b1 = verts[j];
                let b2 = verts[(j + 1) % n];
                if segments_intersect(a1, a2, b1, b2) {
                    return false;
                }
            }
        }
        true
    }

    /// Generates `count` random polygons with an LCG and asserts that the sweep
    /// agrees with the brute-force reference.
    /// Coordinates are integers in `[min, max]` (or fractions with `denom`).
    fn fuzz_against_bruteforce_impl(
        count: usize,
        n_min: usize,
        n_max: usize,
        min: i64,
        max: i64,
        denom: i64,
    ) {
        let mut state: u64 = 0x9e37_79b9_7f4a_7c15;
        let mut rng = move || {
            state = state
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            state >> 33
        };

        for _ in 0..count {
            let n = n_min + (rng() % (n_max - n_min + 1) as u64) as usize;
            let span = (max - min + 1) as u64;
            let verts: Vec<Vec2> = (0..n)
                .map(|_| {
                    let x = (min + (rng() % span) as i64) as f32 / denom as f32;
                    let y = (min + (rng() % span) as i64) as f32 / denom as f32;
                    Vec2::new(x, y)
                })
                .collect();

            let sweep = is_polygon_simple(&verts);
            let brute = is_simple_bruteforce(&verts);
            assert_eq!(
                sweep, brute,
                "mismatch: sweep={sweep} brute={brute}, verts={verts:?}"
            );
        }
    }

    #[test]
    fn fuzz_against_bruteforce() {
        fuzz_against_bruteforce_impl(200_000, 4, 8, -2, 3, 1);
    }

    #[test]
    fn fuzz_against_bruteforce_wide() {
        fuzz_against_bruteforce_impl(100_000, 4, 12, -10, 10, 1);
    }

    #[test]
    fn fuzz_against_bruteforce_fractional() {
        // Coordinates with denominator 3: slopes dy/dx cannot be represented
        // exactly in f32, exercising the interpolation precision (ULP error in
        // y_at) and the tolerance-band scans.
        fuzz_against_bruteforce_impl(100_000, 4, 8, -9, 9, 3);
    }
}
