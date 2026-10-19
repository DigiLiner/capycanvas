use super::*;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Eraser { pub from: [f64; 2], pub to: [f64; 2], pub radius: f64 }

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum EraseMode { #[default] Touched, Whole, ToIntersection }

impl Eraser {
    pub fn bounds(&self) -> [f64; 4] {
        let r = self.radius;
        [self.from[0].min(self.to[0]) - r, self.from[1].min(self.to[1]) - r, self.from[0].max(self.to[0]) + r, self.from[1].max(self.to[1]) + r]
    }
    fn distance(&self, p: [f64; 2]) -> f64 {
        let d = [self.to[0] - self.from[0], self.to[1] - self.from[1]];
        let length = d[0] * d[0] + d[1] * d[1];
        let t = if length > 0. { (((p[0] - self.from[0]) * d[0] + (p[1] - self.from[1]) * d[1]) / length).clamp(0., 1.) } else { 0. };
        (self.from[0] + d[0] * t - p[0]).hypot(self.from[1] + d[1] * t - p[1])
    }
    fn covers(&self, p: [f64; 2], reach: f64) -> bool { self.distance(p) <= self.radius + reach }
    fn meets(&self, a: [f64; 2], b: [f64; 2], reach: f64) -> bool {
        let cross = |o: [f64; 2], p: [f64; 2], q: [f64; 2]| (p[0] - o[0]) * (q[1] - o[1]) - (p[1] - o[1]) * (q[0] - o[0]);
        let crossing = cross(a, b, self.from) * cross(a, b, self.to) < 0. && cross(self.from, self.to, a) * cross(self.from, self.to, b) < 0.;
        let chord = Eraser { from: a, to: b, radius: 0. };
        crossing || [a, b].iter().any(|p| self.covers(*p, reach)) || [self.from, self.to].iter().any(|p| chord.distance(*p) <= self.radius + reach)
    }
    fn cuts(&self, segment: &Segment) -> Vec<(f64, f64)> {
        let polygon = [segment.from, segment.control[0], segment.control[1], segment.to].windows(2).map(|w| (w[1][0] - w[0][0]).hypot(w[1][1] - w[0][1])).sum::<f64>();
        let count = ((polygon / (self.radius * 0.25).max(1e-3)).ceil() as usize).clamp(1, 4096);
        let inside = |t: f64| self.covers(segment.point(t), 0.);
        let edge = |mut outside: f64, mut within: f64| {
            for _ in 0..40 { let mid = (outside + within) * 0.5; if inside(mid) { within = mid } else { outside = mid } }
            within
        };
        let mut cuts = Vec::new();
        let mut start = inside(0.).then_some(0.);
        let mut last = 0.;
        for k in 1..=count {
            let t = k as f64 / count as f64;
            match (start, inside(t)) {
                (None, true) => start = Some(edge(last, t)),
                (Some(from), false) => { cuts.push((from, edge(t, last))); start = None; }
                _ => {}
            }
            last = t;
        }
        if let Some(from) = start { cuts.push((from, 1.)); }
        cuts
    }
}

#[derive(Clone, Copy)]
struct Origin { segment: u32, from: f64, to: f64 }
type Pieces = Vec<Vec<(usize, f64, f64)>>;

fn portion(segment: Segment, from: f64, to: f64) -> Segment {
    let right = if from > 0. { segment.split(from).1 } else { segment };
    if to < 1. { right.split((to - from) / (1. - from)).0 } else { right }
}

fn removed(subpath: &Subpath, eraser: &Eraser) -> Vec<(f64, f64)> {
    (0..subpath.segment_count()).flat_map(|index| eraser.cuts(&subpath.segment(index)).into_iter().map(move |(from, to)| (index as f64 + from, index as f64 + to))).collect()
}

fn reach_stops(removed: Vec<(f64, f64)>, stops: &[f64], length: f64, closed: bool) -> Vec<(f64, f64)> {
    if closed && stops.is_empty() && !removed.is_empty() { return vec![(0., length)]; }
    let mut out = Vec::new();
    for (a, b) in removed {
        let before = stops.iter().copied().filter(|s| *s <= a).fold(None, |m: Option<f64>, s| Some(m.map_or(s, |m| m.max(s))));
        let after = stops.iter().copied().filter(|s| *s >= b).fold(None, |m: Option<f64>, s| Some(m.map_or(s, |m| m.min(s))));
        let from = before.unwrap_or_else(|| if closed { stops.iter().copied().fold(f64::MIN, f64::max) - length } else { 0. });
        let to = after.unwrap_or_else(|| if closed { stops.iter().copied().fold(f64::MAX, f64::min) + length } else { length });
        if from < 0. { out.extend([(from + length, length), (0., to.min(length))]); }
        else if to > length { out.extend([(from, length), (0., to - length)]); }
        else { out.push((from, to)); }
    }
    out
}

fn kept(subpath: &Subpath, eraser: &Eraser, stops: Option<&[f64]>) -> Option<Pieces> {
    if subpath.segment_count() == 0 { return eraser.covers(subpath.anchor(0).point(), 0.).then(Vec::new); }
    let length = subpath.segment_count() as f64;
    let mut removed = removed(subpath, eraser);
    if removed.is_empty() { return None; }
    if let Some(stops) = stops { removed = reach_stops(removed, stops, length, subpath.closed); }
    removed.sort_by(|a, b| a.0.total_cmp(&b.0));
    let mut spans: Vec<(f64, f64)> = Vec::new();
    let mut at = 0.;
    for (a, b) in removed {
        if a > at + 1e-9 { spans.push((at, a)); }
        at = f64::max(at, b);
    }
    if at < length - 1e-9 { spans.push((at, length)); }
    let entries = |(u, v): (f64, f64)| -> Vec<(usize, f64, f64)> {
        let first = u.floor() as usize;
        let last = ((v.ceil() as usize).max(first + 1)).min(subpath.segment_count());
        (first..last).map(|i| (i, (u - i as f64).clamp(0., 1.), (v - i as f64).clamp(0., 1.))).filter(|(_, from, to)| to > from).collect()
    };
    let mut pieces: Pieces = spans.into_iter().map(entries).filter(|piece| !piece.is_empty()).collect();
    let wraps = pieces.first().is_some_and(|piece| piece[0].0 == 0 && piece[0].1 == 0.)
        && pieces.last().is_some_and(|piece| piece.last().is_some_and(|last| last.0 + 1 == subpath.segment_count() && last.2 == 1.));
    if subpath.closed && wraps && pieces.len() > 1 {
        let first = pieces.remove(0);
        pieces.last_mut().unwrap().extend(first);
    }
    Some(pieces)
}

fn polyline(subpath: &Subpath, tolerance: f64) -> Vec<([f64; 2], f64)> {
    let mut points = vec![(subpath.anchor(0).point(), 0.)];
    for (index, segment) in subpath.segments().enumerate() { segment.flatten(tolerance, |p, t| points.push((p, index as f64 + t))); }
    points
}

fn nearest_place(subpath: &Subpath, at: [f64; 2], from: f64, to: f64) -> f64 {
    let index = (from.floor() as usize).min(subpath.segment_count().saturating_sub(1));
    let segment = subpath.segment(index);
    let base = index as f64;
    let distance = |t: f64| { let p = segment.point(t - base); (p[0] - at[0]).hypot(p[1] - at[1]) };
    let (mut lo, mut hi) = (from.max(base), to.min(base + 1.));
    for _ in 0..60 {
        let (m1, m2) = (lo + (hi - lo) / 3., hi - (hi - lo) / 3.);
        if distance(m1) < distance(m2) { hi = m2 } else { lo = m1 }
    }
    (lo + hi) * 0.5
}

fn crossing(a: [f64; 2], b: [f64; 2], c: [f64; 2], d: [f64; 2]) -> Option<f64> {
    let r = [b[0] - a[0], b[1] - a[1]];
    let s = [d[0] - c[0], d[1] - c[1]];
    let denominator = r[0] * s[1] - r[1] * s[0];
    if denominator.abs() < 1e-12 { return None; }
    let q = [c[0] - a[0], c[1] - a[1]];
    let t = (q[0] * s[1] - q[1] * s[0]) / denominator;
    let u = (q[0] * r[1] - q[1] * r[0]) / denominator;
    ((0. ..=1.).contains(&t) && (0. ..=1.).contains(&u)).then_some(t)
}

impl PathItem {
    /// Where each subpath crosses `others` or another part of itself, in segment-plus-`t` places.
    fn crossings(&self, others: &[&PathItem], tolerance: f64) -> Vec<Vec<f64>> {
        let lines: Vec<Vec<([f64; 2], f64)>> = self.geometry.subpaths().iter().map(|s| polyline(s, tolerance)).collect();
        let foreign: Vec<Vec<[f64; 2]>> = others.iter().flat_map(|other| other.geometry.subpaths().iter().map(|s| polyline(s, tolerance).into_iter().map(|(p, _)| p).collect())).collect();
        lines.iter().enumerate().map(|(i, line)| {
            let mut stops = Vec::new();
            let closed = self.geometry.subpaths()[i].closed;
            let last = line.len().saturating_sub(2);
            for (k, edge) in line.windows(2).enumerate() {
                let ((a, ta), (b, tb)) = (edge[0], edge[1]);
                let subpath = &self.geometry.subpaths()[i];
                let mut found = |c: [f64; 2], d: [f64; 2]| if let Some(t) = crossing(a, b, c, d) {
                    let at = [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t];
                    stops.push(nearest_place(subpath, at, ta, tb));
                };
                for other in &foreign { for pair in other.windows(2) { found(pair[0], pair[1]); } }
                for (j, other) in lines.iter().enumerate() {
                    for (m, pair) in other.windows(2).enumerate() {
                        let adjacent = m + 1 >= k && m <= k + 1 || closed && (k == 0 && m == last || m == 0 && k == last);
                        if i == j && adjacent { continue; }
                        found(pair[0].0, pair[1].0);
                    }
                }
            }
            stops.sort_by(f64::total_cmp);
            stops
        }).collect()
    }
}

fn build(subpath: &Subpath, piece: &[(usize, f64, f64)]) -> Subpath {
    let segments: Vec<Segment> = piece.iter().map(|&(index, from, to)| portion(subpath.segment(index), from, to)).collect();
    let relative = |handle: [f64; 2], point: [f64; 2]| [handle[0] - point[0], handle[1] - point[1]];
    let anchors: Vec<Anchor> = (0..=segments.len()).map(|k| {
        let point = if k < segments.len() { segments[k].from } else { segments[k - 1].to };
        let incoming = if k > 0 { relative(segments[k - 1].control[1], point) } else { [0.; 2] };
        let outgoing = if k < segments.len() { relative(segments[k].control[0], point) } else { [0.; 2] };
        let cut = (k == 0 && piece[0].1 > 0.) || (k == segments.len() && piece[k - 1].2 < 1.);
        let original = if k < segments.len() { piece[k].0 } else { (piece[k - 1].0 + 1) % subpath.len() };
        let kind = match subpath.anchor(original).kind {
            _ if cut || k == 0 || k == segments.len() => AnchorKind::CORNER,
            AnchorKind::SYMMETRIC if (incoming[0].hypot(incoming[1]) - outgoing[0].hypot(outgoing[1])).abs() > 1e-6 => AnchorKind::SMOOTH,
            kind => kind,
        };
        Anchor::new(point, incoming, outgoing, kind)
    }).collect();
    Subpath::new(&anchors, false)
}

fn remap(profile: &WidthProfile, origins: &[Origin], pinned: &[bool]) -> Option<WidthProfile> {
    let mut knots: Vec<WidthKnot> = Vec::new();
    let mut push = |knot: WidthKnot| if knots.last().is_none_or(|last| (last.segment(), last.t()) < (knot.segment(), knot.t())) { knots.push(knot); };
    for (j, (origin, &pin)) in origins.iter().zip(pinned).enumerate() {
        let j = j as u32;
        let span = origin.to - origin.from;
        let pin = pin || origin.from > 0. || origin.to < 1.;
        if pin { let [l, r] = profile.at(origin.segment, origin.from); push(WidthKnot::new(j, 0., l, r)); }
        for knot in profile.knots().iter().filter(|k| k.segment() == origin.segment && k.t() >= origin.from && k.t() <= origin.to) {
            push(WidthKnot::new(j, ((knot.t() - origin.from) / span).clamp(0., 1.), knot.left(), knot.right()));
        }
        if pin { let [l, r] = profile.at(origin.segment, origin.to); push(WidthKnot::new(j, 1., l, r)); }
    }
    if knots.is_empty() { let [l, r] = profile.at(origins.first()?.segment, origins[0].from); knots.push(WidthKnot::new(0, 0., l, r)); }
    WidthProfile::new(knots).ok()
}

impl PathItem {
    pub fn touched_by(&self, eraser: &Eraser) -> bool {
        let [x0, y0, x1, y1] = self.bounds();
        let area = eraser.bounds();
        if self.hidden || area[2] < x0 || area[0] > x1 || area[3] < y0 || area[1] > y1 { return false; }
        let reach = self.style.strokes().map(|s| s.width * 0.5 * s.profile.as_ref().map_or(1., |p| p.max_factor())).fold(0., f64::max);
        let tolerance = (eraser.radius * 0.25).max(0.05);
        let filled = self.style.entries.iter().any(|e| matches!(e, PaintEntry::Fill(_)));
        self.geometry.subpaths().iter().any(|subpath| {
            let mut last = subpath.anchor(0).point();
            if eraser.covers(last, reach) { return true; }
            let mut found = false;
            for segment in subpath.segments() { segment.flatten(tolerance, |p, _| { found |= eraser.meets(last, p, reach); last = p; }); if found { return true; } }
            false
        }) || filled && self.hit(eraser.to, eraser.radius)
    }
    pub fn erased(&self, eraser: &Eraser, crossed: Option<&[&PathItem]>) -> Option<Vec<PathItem>> {
        let [x0, y0, x1, y1] = self.geometry.hull();
        let area = eraser.bounds();
        if area[2] < x0 || area[0] > x1 || area[3] < y0 || area[1] > y1 { return None; }
        let mut base = 0u32;
        let stops = crossed.map(|others| self.crossings(others, (eraser.radius * 0.25).max(0.05)));
        let results: Vec<Option<Pieces>> = self.geometry.subpaths().iter().enumerate()
            .map(|(i, subpath)| kept(subpath, eraser, stops.as_ref().map(|stops| stops[i].as_slice()))).collect();
        if results.iter().all(Option::is_none) { return None; }
        let compound = self.geometry.subpaths().len() > 1;
        let mut outputs: Vec<(Vec<Subpath>, Vec<Origin>, Vec<bool>)> = vec![Default::default()];
        for (subpath, result) in self.geometry.subpaths().iter().zip(results) {
            match result {
                None => {
                    let out = outputs.last_mut().unwrap();
                    out.0.push(subpath.clone());
                    for i in 0..subpath.segment_count() { out.1.push(Origin { segment: base + i as u32, from: 0., to: 1. }); out.2.push(i == 0 || i + 1 == subpath.segment_count()); }
                }
                Some(pieces) => for piece in pieces {
                    if !compound && !outputs.last().unwrap().0.is_empty() { outputs.push(Default::default()); }
                    let out = outputs.last_mut().unwrap();
                    out.0.push(build(subpath, &piece));
                    let last = piece.len() - 1;
                    for (k, &(index, from, to)) in piece.iter().enumerate() { out.1.push(Origin { segment: base + index as u32, from, to }); out.2.push(k == 0 || k == last); }
                },
            }
            base += subpath.segment_count() as u32;
        }
        Some(outputs.into_iter().filter(|(subpaths, _, _)| !subpaths.is_empty()).filter_map(|(subpaths, origins, pinned)| {
            let geometry = PathGeometry::new(subpaths).ok()?;
            let entries = self.style.entries.iter().map(|entry| match entry {
                PaintEntry::Stroke(stroke) => PaintEntry::Stroke(Stroke { profile: stroke.profile.as_ref().and_then(|p| remap(p, &origins, &pinned)).map(Arc::new), ..stroke.clone() }),
                other => other.clone(),
            }).collect();
            Some(PathItem { style: Arc::new(Style::new(entries)), ..self.with_geometry(Arc::new(geometry)) })
        }).collect())
    }
}

impl VectorObject {
    pub fn erase(&self, index: &mut VectorIndex, eraser: &Eraser, mode: EraseMode) -> Option<Self> {
        let mut changes = BTreeMap::new();
        let mut fresh = BTreeSet::new();
        for position in index.near(self, eraser.bounds()) {
            let Some(path) = self.record(position).and_then(Record::path).filter(|path| !path.locked && !path.hidden) else { continue };
            let replacement = match mode {
                EraseMode::Whole => path.touched_by(eraser).then(Vec::new),
                EraseMode::Touched => path.erased(eraser, None),
                EraseMode::ToIntersection => {
                    let others: Vec<&PathItem> = index.near(self, path.bounds()).into_iter()
                        .filter_map(|p| self.record(p).and_then(Record::path)).filter(|other| other.id != path.id && !other.hidden).collect();
                    path.erased(eraser, Some(&others))
                }
            };
            let Some(pieces) = replacement else { continue };
            let records = pieces.into_iter().enumerate().map(|(i, piece)| {
                let id = if i == 0 { path.id } else { loop { let id = self.allocate_id(); if fresh.insert(id) { break id; } } };
                Record::Path(PathItem { id, ..piece })
            }).collect();
            changes.insert(position, records);
        }
        (!changes.is_empty()).then(|| self.splice(changes))
    }
}
