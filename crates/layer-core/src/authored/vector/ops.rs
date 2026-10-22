use super::*;

#[derive(Clone, Copy)]
enum Span<'a> { Part { profile: Option<&'a WidthProfile>, scale: f64, segment: u32, from: f64, to: f64 }, Bridge { to: Option<usize> } }

impl Span<'_> {
    fn ends(&self) -> Option<[[f64; 2]; 2]> {
        let Self::Part { profile, scale, segment, from, to } = *self else { return None };
        let width = |t: f64| {
            let [l, r] = profile.map_or([1., 1.], |p| p.at(segment, t));
            let [l, r] = if from > to { [r, l] } else { [l, r] };
            [l * scale, r * scale]
        };
        Some([width(from), width(to)])
    }
}

fn whole<'a>(profile: Option<&'a WidthProfile>, scale: f64, segments: std::ops::Range<u32>) -> impl Iterator<Item = Span<'a>> {
    segments.map(move |segment| Span::Part { profile, scale, segment, from: 0., to: 1. })
}

fn simplified(knots: Vec<WidthKnot>) -> Vec<WidthKnot> {
    let at = |k: &WidthKnot| f64::from(k.segment()) + k.t();
    let mut out: Vec<WidthKnot> = Vec::with_capacity(knots.len());
    for knot in knots {
        if out.last().is_some_and(|last| (last.segment(), last.t()) >= (knot.segment(), knot.t())) { continue; }
        while let [.., a, b] = out[..] {
            let span = at(&knot) - at(&a);
            let u = if span > 0. { (at(&b) - at(&a)) / span } else { 0. };
            let on = |x: f64, y: f64, z: f64| (x + (z - x) * u - y).abs() < 1e-6;
            if !(on(a.left(), b.left(), knot.left()) && on(a.right(), b.right(), knot.right())) { break; }
            out.pop();
        }
        out.push(knot);
    }
    out
}

fn profile(spans: &[Span]) -> Option<WidthProfile> {
    let plain = |span: &Span| matches!(span, Span::Bridge { .. } | Span::Part { profile: None, .. }) && span.ends().is_none_or(|[a, b]| a == [1.; 2] && b == [1.; 2]);
    if spans.iter().all(plain) { return None; }
    let ends: Vec<Option<[[f64; 2]; 2]>> = spans.iter().map(Span::ends).collect();
    let mut knots = Vec::new();
    for (j, span) in spans.iter().enumerate() {
        let index = j as u32;
        match *span {
            Span::Part { profile, scale, segment, from, to } => {
                let [start, end] = ends[j].unwrap();
                knots.push(WidthKnot::new(index, 0., start[0], start[1]));
                if let Some(profile) = profile {
                    let (lo, hi) = (from.min(to), from.max(to));
                    let mut inner: Vec<WidthKnot> = profile.knots().iter().filter(|k| k.segment() == segment && k.t() > lo && k.t() < hi).map(|k| {
                        let [l, r] = if from > to { [k.right(), k.left()] } else { [k.left(), k.right()] };
                        WidthKnot::new(index, (k.t() - from) / (to - from), l * scale, r * scale)
                    }).collect();
                    if from > to { inner.reverse(); }
                    knots.extend(inner);
                }
                knots.push(WidthKnot::new(index, 1., end[0], end[1]));
            }
            Span::Bridge { to } => {
                let before = ends[..j].iter().rev().find_map(|e| e.map(|[_, end]| end));
                let after = match to { Some(k) => ends[k].map(|[start, _]| start), None => ends[j + 1..].iter().find_map(|e| e.map(|[start, _]| start)) };
                let [start, end] = [before.or(after), after.or(before)].map(|w| w.unwrap_or([1.; 2]));
                knots.extend([WidthKnot::new(index, 0., start[0], start[1]), WidthKnot::new(index, 1., end[0], end[1])]);
            }
        }
    }
    WidthProfile::new(simplified(knots)).ok()
}

fn rebased(width: f64, others: impl IntoIterator<Item = f64>) -> f64 {
    if width > 0. { width } else { others.into_iter().find(|w| *w > 0.).unwrap_or(width) }
}

fn relative(width: f64, base: f64) -> f64 { if base > 0. { width / base } else { 1. } }

fn stroke_width(path: &PathItem) -> Option<(f64, Option<&WidthProfile>)> {
    path.style.strokes().next().map(|stroke| (stroke.width, stroke.profile.as_deref()))
}

impl PathItem {
    fn with_spans<'a>(&'a self, geometry: PathGeometry, spans: impl Fn(Option<&'a WidthProfile>, f64) -> (f64, Vec<Span<'a>>)) -> Result<Self, &'static str> {
        let entries = self.style.entries.iter().map(|entry| match entry {
            PaintEntry::Stroke(stroke) => {
                let (width, spans) = spans(stroke.profile.as_deref(), stroke.width);
                PaintEntry::Stroke(Stroke { width, profile: profile(&spans).map(Arc::new), ..stroke.clone() })
            }
            other => other.clone(),
        }).collect();
        Ok(PathItem { style: Arc::new(Style::new(entries)), ..self.with_geometry(Arc::new(geometry)) })
    }
    fn segment_ranges(&self) -> Vec<std::ops::Range<u32>> {
        self.geometry.subpaths().iter().scan(0u32, |base, subpath| { let start = *base; *base += subpath.segment_count() as u32; Some(start..*base) }).collect()
    }
    pub fn reversed(&self) -> Result<Self, &'static str> {
        let ranges = self.segment_ranges();
        let subpaths: Vec<Subpath> = self.geometry.subpaths().iter().map(|subpath| {
            let anchors: Vec<Anchor> = subpath.anchors().rev().map(|a| Anchor::new(a.point(), a.out_handle(), a.in_handle(), a.kind)).collect();
            Subpath::new(&anchors, subpath.closed)
        }).collect();
        let closed: Vec<bool> = self.geometry.subpaths().iter().map(|s| s.closed && s.segment_count() > 0).collect();
        self.with_spans(PathGeometry::new(subpaths)?, |profile, width| (width, ranges.iter().zip(&closed).flat_map(|(range, &closed)| {
            let open = if closed { range.start..range.end - 1 } else { range.clone() };
            open.rev().chain(closed.then(|| range.end - 1)).map(move |segment| Span::Part { profile, scale: 1., segment, from: 1., to: 0. })
        }).collect()))
    }
    pub fn closed(&self) -> Result<Self, &'static str> {
        let ranges = self.segment_ranges();
        let closing: Vec<bool> = self.geometry.subpaths().iter().map(|s| !s.closed && s.len() >= 2).collect();
        let subpaths = self.geometry.subpaths().iter().zip(&closing).map(|(s, &close)| Subpath::new(&s.to_vec(), s.closed || close)).collect();
        self.with_spans(PathGeometry::new(subpaths)?, |profile, width| {
            let mut spans = Vec::new();
            for (range, &close) in ranges.iter().zip(&closing) {
                let first = spans.len();
                spans.extend(whole(profile, 1., range.clone()));
                if close { spans.push(Span::Bridge { to: Some(first) }); }
            }
            (width, spans)
        })
    }
    pub fn can_close(&self) -> bool { self.geometry.subpaths().iter().any(|s| !s.closed && s.len() >= 2) }
    pub fn cuts_at(&self, at: AnchorRef) -> bool {
        self.geometry.subpaths().get(at.subpath as usize).is_some_and(|subpath| {
            let (n, k) = (subpath.len(), at.anchor as usize);
            k < n && subpath.segment_count() > 0 && (subpath.closed || (k > 0 && k + 1 < n))
        })
    }
    pub fn split_at(&self, cuts: &BTreeSet<AnchorRef>) -> Result<Vec<Self>, &'static str> {
        let ranges = self.segment_ranges();
        let mut pieces: Vec<(usize, Vec<usize>)> = Vec::new();
        let mut cut = false;
        for (s, subpath) in self.geometry.subpaths().iter().enumerate() {
            let n = subpath.len();
            let at: Vec<usize> = cuts.iter().filter(|c| c.subpath as usize == s && self.cuts_at(**c)).map(|c| c.anchor as usize).collect();
            if at.is_empty() { pieces.push((s, (0..n).collect())); continue; }
            cut = true;
            if subpath.closed {
                for (i, &start) in at.iter().enumerate() {
                    let end = at.get(i + 1).copied().unwrap_or(at[0] + n);
                    pieces.push((s, (start..=end).map(|k| k % n).collect()));
                }
            } else {
                let bounds: Vec<usize> = std::iter::once(0).chain(at).chain([n - 1]).collect();
                pieces.extend(bounds.windows(2).map(|w| (s, (w[0]..=w[1]).collect())));
            }
        }
        if !cut { return Ok(vec![self.clone()]); }
        let build = |s: usize, indices: &[usize]| {
            let subpath = &self.geometry.subpaths()[s];
            let whole = indices.len() == subpath.len() && indices[0] == 0 && indices.windows(2).all(|w| w[1] == w[0] + 1);
            if whole { return (subpath.clone(), (0..subpath.segment_count() as u32).map(|i| ranges[s].start + i).collect::<Vec<_>>()); }
            let last = indices.len() - 1;
            let anchors: Vec<Anchor> = indices.iter().enumerate().map(|(i, &k)| {
                let a = subpath.anchor(k);
                if i == 0 { Anchor::new(a.point(), [0.; 2], a.out_handle(), AnchorKind::CORNER) }
                else if i == last { Anchor::new(a.point(), a.in_handle(), [0.; 2], AnchorKind::CORNER) }
                else { *a }
            }).collect();
            (Subpath::new(&anchors, false), indices[..last].iter().map(|&k| ranges[s].start + k as u32).collect())
        };
        let built: Vec<(Subpath, Vec<u32>)> = pieces.iter().map(|(s, indices)| build(*s, indices)).collect();
        let groups: Vec<Vec<usize>> = if self.geometry.subpaths().len() > 1 { vec![(0..built.len()).collect()] } else { (0..built.len()).map(|i| vec![i]).collect() };
        groups.into_iter().map(|group| {
            let geometry = PathGeometry::new(group.iter().map(|&i| built[i].0.clone()).collect())?;
            self.with_spans(geometry, |profile, width| (width, group.iter().flat_map(|&i| built[i].1.iter().map(move |&segment| Span::Part { profile, scale: 1., segment, from: 0., to: 1. })).collect()))
        }).collect()
    }
    pub fn open_ends(&self) -> Option<[[f64; 2]; 2]> {
        let [subpath] = self.geometry.subpaths() else { return None };
        (!subpath.closed).then(|| [subpath.anchor(0).point(), subpath.anchor(subpath.len() - 1).point()])
    }
    pub fn joinable(&self) -> bool { self.open_ends().is_some() }
    pub fn joined(&self, other: &PathItem, tolerance: f64) -> Result<Self, &'static str> {
        let ([a0, a1], [b0, b1]) = (self.open_ends().ok_or("Join open paths")?, other.open_ends().ok_or("Join open paths")?);
        let distance = |p: [f64; 2], q: [f64; 2]| (p[0] - q[0]).hypot(p[1] - q[1]);
        let (flip_a, flip_b, _) = [(false, false, distance(a1, b0)), (false, true, distance(a1, b1)), (true, false, distance(a0, b0)), (true, true, distance(a0, b1))]
            .into_iter().min_by(|x, y| x.2.total_cmp(&y.2)).unwrap();
        self.joined_ends(other, !flip_a, flip_b, tolerance)
    }
    pub fn joined_ends(&self, other: &PathItem, at_end: bool, other_end: bool, tolerance: f64) -> Result<Self, &'static str> {
        let (flip_a, flip_b) = (!at_end, other_end);
        let a = if flip_a { self.reversed()? } else { self.clone() };
        let b = if flip_b { other.reversed()? } else { other.clone() };
        let (first, second) = (&a.geometry.subpaths()[0], &b.geometry.subpaths()[0]);
        let [_, a1] = a.open_ends().ok_or("Join open paths")?;
        let [b0, _] = b.open_ends().ok_or("Join open paths")?;
        let gap = (a1[0] - b0[0]).hypot(a1[1] - b0[1]);
        let mut anchors = first.to_vec();
        let mut rest = second.to_vec();
        let touching = gap <= tolerance;
        if touching {
            let end = anchors.pop().unwrap();
            let start = rest.remove(0);
            anchors.push(Anchor::new(end.point(), end.in_handle(), start.out_handle(), AnchorKind::CORNER));
        } else {
            let end = anchors.last_mut().unwrap();
            *end = Anchor::new(end.point(), end.in_handle(), [0.; 2], AnchorKind::CORNER);
            rest[0] = Anchor::new(rest[0].point(), [0.; 2], rest[0].out_handle(), AnchorKind::CORNER);
        }
        anchors.extend(rest);
        let geometry = PathGeometry::new(vec![Subpath::new(&anchors, false)])?;
        let (count_a, count_b) = (first.segment_count() as u32, second.segment_count() as u32);
        let other_stroke = stroke_width(&b);
        a.with_spans(geometry, |profile, width| {
            let (other_width, other) = other_stroke.unwrap_or((width, None));
            let base = rebased(width, [other_width]);
            (base, whole(profile, relative(width, base), 0..count_a).chain((!touching).then_some(Span::Bridge { to: None })).chain(whole(other, relative(other_width, base), 0..count_b)).collect())
        })
    }
    pub fn compound(paths: &[PathItem]) -> Result<Self, &'static str> {
        let base = paths.first().ok_or("Select paths first")?;
        let geometry = PathGeometry::new(paths.iter().flat_map(|p| p.geometry.subpaths().iter().cloned()).collect())?;
        let combined = base.with_spans(geometry, |profile, width| {
            let widths: Vec<(f64, Option<&WidthProfile>)> = paths.iter().enumerate().map(|(i, path)| if i == 0 { (width, profile) } else { stroke_width(path).unwrap_or((width, None)) }).collect();
            let base = rebased(width, widths.iter().map(|(w, _)| *w));
            (base, paths.iter().zip(&widths).flat_map(|(path, &(w, other))| whole(other, relative(w, base), 0..path.geometry.segment_count() as u32)).collect())
        })?;
        Ok(PathItem { fill_rule: FillRule::EvenOdd, ..combined })
    }
    pub fn released(&self) -> Result<Vec<Self>, &'static str> {
        let ranges = self.segment_ranges();
        self.geometry.subpaths().iter().zip(ranges).map(|(subpath, range)| {
            self.with_spans(PathGeometry::new(vec![subpath.clone()])?, |profile, width| (width, whole(profile, 1., range.clone()).collect()))
        }).collect()
    }
}

impl PathItem {
    pub fn area_around(&self, point: [f64; 2], tolerance: f64) -> f64 {
        self.geometry.subpaths().iter().filter_map(|subpath| {
            let mut ring = vec![subpath.anchor(0).point()];
            for segment in subpath.segments() { segment.flatten(tolerance, |p, _| ring.push(p)); }
            let (mut winding, mut twice) = (0i32, 0.);
            for (a, b) in ring.iter().zip(ring.iter().cycle().skip(1)) {
                twice += a[0] * b[1] - b[0] * a[1];
                if (a[1] <= point[1]) != (b[1] <= point[1]) && a[0] + (point[1] - a[1]) / (b[1] - a[1]) * (b[0] - a[0]) > point[0] {
                    winding += if b[1] > a[1] { 1 } else { -1 };
                }
            }
            (winding != 0).then_some(twice.abs() * 0.5)
        }).fold(f64::INFINITY, f64::min)
    }
    pub fn is_known(&self) -> bool {
        let known = |paint: &Paint| matches!(paint, Paint::Color { unknown: None, .. });
        self.unknown.is_none() && self.style.entries.iter().all(|entry| match entry {
            PaintEntry::Fill(fill) => fill.unknown.is_none() && known(&fill.paint),
            PaintEntry::Stroke(stroke) => stroke.unknown.is_none() && known(&stroke.paint),
            PaintEntry::Opaque(_) => false,
        })
    }
}

impl VectorObject {
    pub fn holds_only_known_paths(&self) -> bool {
        self.slots().is_empty() && self.records().all(|(_, record)| record.path().is_some_and(PathItem::is_known))
    }
    pub fn map_paths(&self, ids: &BTreeSet<ItemId>, map: impl Fn(&PathItem) -> Result<Option<PathItem>, &'static str>) -> Result<Option<Self>, &'static str> {
        let mut changes = Vec::new();
        for (position, path) in self.selected_paths(ids) {
            if let Some(changed) = map(path)? { changes.push((position, changed)); }
        }
        (!changes.is_empty()).then(|| self.with_paths(changes)).transpose()
    }
    fn with_fresh_ids(pieces: Vec<PathItem>, ids: &mut impl Iterator<Item = ItemId>) -> Vec<Record> {
        pieces.into_iter().enumerate().map(|(i, piece)| {
            if i == 0 { return Record::Path(piece); }
            Record::Path(PathItem { id: ids.next().unwrap_or_default(), ..piece })
        }).collect()
    }
    pub fn split_paths(&self, cuts: &BTreeMap<ItemId, BTreeSet<AnchorRef>>) -> Result<Option<(Self, BTreeSet<ItemId>)>, &'static str> {
        let ids: BTreeSet<ItemId> = cuts.keys().copied().collect();
        let mut fresh = self.allocate_ids();
        let mut changes = BTreeMap::new();
        for (position, path) in self.selected_paths(&ids) {
            let pieces = path.split_at(&cuts[&path.id])?;
            if pieces.len() == 1 && pieces[0].geometry == path.geometry { continue; }
            changes.insert(position, Self::with_fresh_ids(pieces, &mut fresh));
        }
        if changes.is_empty() { return Ok(None); }
        let selected = changes.values().flatten().filter_map(Record::id).collect();
        Ok(Some((self.splice(changes), selected)))
    }
    pub fn join_paths(&self, ids: &BTreeSet<ItemId>, tolerance: f64) -> Result<Option<(Self, ItemId)>, &'static str> {
        let found = self.selected_paths(ids);
        let joined = match found[..] {
            [(position, path)] if path.can_close() => (path.closed()?, vec![position]),
            [(front, a), (back, b)] if a.joinable() && b.joinable() => (a.joined(b, tolerance)?, vec![front, back]),
            _ => return Ok(None),
        };
        let (path, positions) = joined;
        let id = path.id;
        let mut changes: BTreeMap<Position, Vec<Record>> = positions.iter().skip(1).map(|p| (*p, Vec::new())).collect();
        changes.insert(positions[0], vec![Record::Path(path)]);
        Ok(Some((self.splice(changes), id)))
    }
    pub fn compound_paths(&self, ids: &BTreeSet<ItemId>) -> Result<Option<(Self, ItemId)>, &'static str> {
        let found = self.selected_paths(ids);
        if found.len() < 2 { return Ok(None); }
        let back = found.iter().rev().map(|(_, path)| (*path).clone()).collect::<Vec<_>>();
        let combined = PathItem::compound(&back)?;
        let id = combined.id;
        let mut changes: BTreeMap<Position, Vec<Record>> = found.iter().map(|(p, _)| (*p, Vec::new())).collect();
        changes.insert(found.last().unwrap().0, vec![Record::Path(combined)]);
        Ok(Some((self.splice(changes), id)))
    }
    pub fn release_paths(&self, ids: &BTreeSet<ItemId>) -> Result<Option<(Self, BTreeSet<ItemId>)>, &'static str> {
        let mut fresh = self.allocate_ids();
        let mut changes = BTreeMap::new();
        for (position, path) in self.selected_paths(ids).into_iter().filter(|(_, path)| path.geometry.subpaths().len() > 1) {
            changes.insert(position, Self::with_fresh_ids(path.released()?, &mut fresh));
        }
        if changes.is_empty() { return Ok(None); }
        let selected = changes.values().flatten().filter_map(Record::id).collect();
        Ok(Some((self.splice(changes), selected)))
    }
}
