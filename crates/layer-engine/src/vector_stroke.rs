use crate::input::{StrokeBuilder, to_stroke_point};
use crate::feedback::{PredictionState, PreviewTip, TipInput, finalized_count, tip_times};
use crate::{CanvasEngine, DabGenerator, InstantFeedbackConfig, PenEvent, PenPhase, PressureCurve, SampleFlags, ViewTransform};
use layer_core::authored::vector::{Anchor, AnchorKind, PathGeometry, Subpath, WidthKnot, WidthProfile};
use layer_core::{BrushSnapshot, RulerConstraint, StrokeId, StrokePoint};
use layer_render::{CanvasRenderer, Dab};
use std::collections::BTreeMap;

pub type Sample = ([f64; 2], f64);

pub struct VectorStroke {
    brush: BrushSnapshot,
    generator: DabGenerator,
    curve: PressureCurve,
    offset: [f32; 2],
    ruler: Option<RulerConstraint>,
    builder: StrokeBuilder,
    finalized: usize,
    estimates: BTreeMap<(u64, u64), (Vec<usize>, ViewTransform)>,
    tip: PreviewTip,
    feedback: InstantFeedbackConfig,
    surface: [f32; 6],
    dabs: Vec<Dab>,
    id: u64,
}
impl VectorStroke {
    fn new(id: u64, brush: BrushSnapshot, curve: PressureCurve, offset: [f64; 2], feedback: InstantFeedbackConfig, surface: [f32; 6]) -> Self {
        let mut generator = DabGenerator::new(layer_core::color::RgbSpace::Srgb);
        generator.reset_for_stroke(StrokeId(id), &brush);
        Self { brush, generator, curve, offset: offset.map(|v| v as f32), ruler: None, builder: StrokeBuilder::default(), finalized: 0, estimates: BTreeMap::new(),
            tip: PreviewTip::default(), feedback, surface, dabs: Vec::new(), id }
    }
    pub fn brush(&self) -> &BrushSnapshot { &self.brush }
    pub fn awaits_corrections(&self) -> bool { !self.estimates.is_empty() }
    pub fn corrects(&self, event: &PenEvent) -> bool {
        event.flags.contains(SampleFlags::CORRECTION) && self.estimates.contains_key(&(event.device_id, event.sequence))
    }
    fn push(&mut self, event: PenEvent, transform: ViewTransform) -> bool {
        let position = transform.map(event.surface_position);
        if !position.x.is_finite() || !position.y.is_finite() || self.builder.real_points().len() >= crate::canvas::MAX_CONTACT_POINTS { return false; }
        if event.phase == PenPhase::Down { self.builder.begin(event, transform, self.curve.clone()); } else { self.builder.push(event, transform, self.curve.clone()); }
        if event.flags.contains(SampleFlags::PREDICTED) { return !self.builder.predicted_points().is_empty(); }
        let Some(index) = self.builder.real_points().len().checked_sub(1) else { return false };
        if self.feedback.enabled { self.tip.prediction.observe(event); }
        if event.flags.contains(SampleFlags::ESTIMATED) {
            self.estimates.entry((event.device_id, event.sequence)).or_insert_with(|| (Vec::new(), transform)).0.push(index);
        }
        self.finalize();
        true
    }
    fn finalize(&mut self) {
        let real = self.builder.real_points();
        let count = if self.feedback.enabled { finalized_count(real, self.finalized) } else { real.len() };
        for point in &real[self.finalized..count] { self.generator.append(*point, &self.brush, &mut self.dabs); }
        self.finalized = count;
    }
    fn correct(&mut self, event: PenEvent) -> bool {
        let key = (event.device_id, event.sequence);
        let Some((indices, transform)) = self.estimates.get(&key) else { return false };
        let corrected = to_stroke_point(event, *transform, self.curve.clone(), 0);
        let mut changed = false;
        for &index in indices {
            let point = StrokePoint { elapsed_micros: self.builder.real_points()[index].elapsed_micros, ..corrected };
            changed |= self.builder.real_points()[index] != point;
            self.builder.replace_real(index, point);
        }
        if !event.flags.contains(SampleFlags::ESTIMATED) { self.estimates.remove(&key); }
        if changed {
            self.tip.prediction = PredictionState::default();
            self.generator.reset_for_stroke(StrokeId(self.id), &self.brush);
            self.dabs.clear();
            self.finalized = 0;
            self.finalize();
        }
        changed
    }
    pub fn preview(&mut self, timestamp_ns: Option<u64>, presentation_timestamp_ns: Option<u64>) -> Vec<Sample> {
        let mut tail = Vec::new();
        match self.builder.real_points().last().copied().filter(|_| self.feedback.enabled) {
            Some(latest) => {
                let builder = &self.builder;
                let (now, requested) = tip_times(self.feedback, latest, |timestamp| builder.elapsed_micros_at(timestamp).unwrap_or(latest.elapsed_micros),
                    timestamp_ns, presentation_timestamp_ns);
                let real = builder.real_points();
                let input = TipInput { real, pending: &real[self.finalized..], platform: builder.predicted_points(), brush: &self.brush, feedback: self.feedback,
                    transform: self.surface, now, requested, ending: None };
                self.tip.build(&self.generator, input, 0, &mut tail);
            }
            None => { self.generator.preview().flush_contact(&self.brush, &mut tail); }
        }
        samples(self.dabs.iter().chain(&tail))
    }
    pub fn samples(&self) -> Vec<Sample> {
        if self.brush.taper.end_distance_diameters > 0. && !self.builder.real_points().is_empty()
            && let Ok(stroke) = layer_core::Stroke::new(StrokeId(self.id), layer_core::SourceTarget::Paint(layer_core::authored::PaintHandle::from_index(0)),
                layer_core::StrokeTool::Brush, self.brush.clone(), self.builder.real_points().to_vec()) {
            let mut dabs = Vec::new();
            DabGenerator::generate(&stroke, layer_core::color::RgbSpace::Srgb, &mut dabs);
            return samples(dabs.iter());
        }
        let mut tail = Vec::new();
        let mut generator = self.generator.clone();
        for point in &self.builder.real_points()[self.finalized..] { generator.append(*point, &self.brush, &mut tail); }
        generator.finish(&self.brush, &mut tail);
        samples(self.dabs.iter().chain(&tail))
    }
}

impl<B: CanvasRenderer> CanvasEngine<B> {
    pub fn begin_vector_stroke(&self, id: u64, event: PenEvent, offset: [f64; 2]) -> VectorStroke {
        let mut stroke = VectorStroke::new(id, self.stroke_brush(&event), self.configured_pressure_curve().clone(), offset,
            self.instant_feedback(), self.view().document_to_surface);
        self.push_vector_stroke(&mut stroke, event);
        stroke
    }
    pub fn push_vector_stroke(&self, stroke: &mut VectorStroke, event: PenEvent) -> bool {
        if event.flags.contains(SampleFlags::CORRECTION) { return stroke.correct(event); }
        if !matches!(event.phase, PenPhase::Down | PenPhase::Move | PenPhase::Up) { return false; }
        let mut transform = self.event_transform(&event);
        stroke.ruler = self.snap_to_ruler(&event, &mut transform, stroke.ruler);
        transform.surface_to_document[4] -= stroke.offset[0];
        transform.surface_to_document[5] -= stroke.offset[1];
        stroke.push(event, transform)
    }
}

pub(crate) fn samples<'a>(dabs: impl Iterator<Item = &'a Dab>) -> Vec<Sample> {
    let mut out: Vec<Sample> = Vec::new();
    for dab in dabs {
        let sample = ([f64::from(dab.center.x), f64::from(dab.center.y)], f64::from(dab.radii[0].max(0.)));
        if let Some(last) = out.last_mut() && distance(last.0, sample.0) < 1e-6 { *last = sample; continue; }
        out.push(sample);
    }
    out
}

fn sub(a: [f64; 2], b: [f64; 2]) -> [f64; 2] { [a[0] - b[0], a[1] - b[1]] }
fn add(a: [f64; 2], b: [f64; 2]) -> [f64; 2] { [a[0] + b[0], a[1] + b[1]] }
fn scale(a: [f64; 2], s: f64) -> [f64; 2] { [a[0] * s, a[1] * s] }
fn dot(a: [f64; 2], b: [f64; 2]) -> f64 { a[0] * b[0] + a[1] * b[1] }
fn distance(a: [f64; 2], b: [f64; 2]) -> f64 { let d = sub(a, b); dot(d, d).sqrt() }
fn unit(v: [f64; 2]) -> [f64; 2] { let l = dot(v, v).sqrt(); if l > 1e-12 { scale(v, 1. / l) } else { [0.; 2] } }

pub fn polyline(samples: &[Sample]) -> Option<(PathGeometry, Option<WidthProfile>, f64)> {
    let half = samples.iter().map(|s| s.1).fold(0., f64::max);
    if samples.is_empty() || half <= 0. { return None; }
    let anchors: Vec<Anchor> = samples.iter().map(|s| Anchor::corner(s.0)).collect();
    let geometry = PathGeometry::new(vec![Subpath::new(&anchors, false)]).ok()?;
    let segments = samples.len().saturating_sub(1).max(1);
    let knots = samples.iter().enumerate().map(|(i, s)| {
        let (segment, t) = if i < segments { (i as u32, 0.) } else { ((segments - 1) as u32, 1.) };
        WidthKnot::new(segment, t, s.1 / half, s.1 / half)
    }).collect();
    Some((geometry, WidthProfile::new(knots).ok(), half * 2.))
}

struct Fitted { cubics: Vec<[[f64; 2]; 4]>, params: Vec<(usize, f64)> }
struct Segment { cubic: [[f64; 2]; 4], u: Vec<f64>, error: f64 }

fn chord_parameters(points: &[[f64; 2]]) -> Vec<f64> {
    let mut u = vec![0.; points.len()];
    for i in 1..points.len() { u[i] = u[i - 1] + distance(points[i], points[i - 1]); }
    let total = *u.last().unwrap();
    if total > 0. { for v in &mut u { *v /= total; } }
    u
}
fn bezier(c: &[[f64; 2]; 4], t: f64) -> [f64; 2] {
    let s = 1. - t;
    let [a, b, c2, d] = [s * s * s, 3. * s * s * t, 3. * s * t * t, t * t * t];
    std::array::from_fn(|i| a * c[0][i] + b * c[1][i] + c2 * c[2][i] + d * c[3][i])
}
fn solve<const N: usize>(mut a: [[f64; N]; N], mut b: [f64; N]) -> Option<[f64; N]> {
    let scale = a.iter().enumerate().map(|(i, row)| row[i].abs()).fold(0., f64::max);
    for column in 0..N {
        let pivot = (column..N).max_by(|&i, &j| a[i][column].abs().total_cmp(&a[j][column].abs()))?;
        if a[pivot][column].abs() <= scale * 1e-12 { return None; }
        a.swap(column, pivot);
        b.swap(column, pivot);
        let (pivot_row, pivot_value) = (a[column], b[column]);
        for (row, value) in a.iter_mut().zip(b.iter_mut()).skip(column + 1) {
            let f = row[column] / pivot_row[column];
            for (entry, pivot) in row.iter_mut().zip(pivot_row).skip(column) { *entry -= f * pivot; }
            *value -= f * pivot_value;
        }
    }
    let mut x = [0.; N];
    for row in (0..N).rev() { x[row] = (b[row] - (row + 1..N).map(|k| a[row][k] * x[k]).sum::<f64>()) / a[row][row]; }
    Some(x)
}
fn least_squares(points: &[[f64; 2]], u: &[f64], left: Option<[f64; 2]>, right: Option<[f64; 2]>) -> Option<[[f64; 2]; 4]> {
    let (first, last) = (points[0], points[points.len() - 1]);
    let n = 2 + usize::from(left.is_none()) + usize::from(right.is_none());
    let mut normal = [[0.; 4]; 4];
    let mut rhs = [0.; 4];
    for (p, &t) in points.iter().zip(u) {
        let s = 1. - t;
        let (b1, b2) = (3. * s * s * t, 3. * s * t * t);
        let mut fixed = add(scale(first, s * s * s), scale(last, t * t * t));
        let mut columns = [[0.; 2]; 4];
        let mut k = 0;
        match left {
            Some(d) => { fixed = add(fixed, scale(first, b1)); columns[k] = scale(d, b1); k += 1; }
            None => { columns[k] = [b1, 0.]; columns[k + 1] = [0., b1]; k += 2; }
        }
        match right {
            Some(d) => { fixed = add(fixed, scale(last, b2)); columns[k] = scale(d, -b2); }
            None => { columns[k] = [b2, 0.]; columns[k + 1] = [0., b2]; }
        }
        let r = sub(*p, fixed);
        for i in 0..n {
            for j in 0..n { normal[i][j] += dot(columns[i], columns[j]); }
            rhs[i] += dot(columns[i], r);
        }
    }
    let x = match n {
        2 => solve([[normal[0][0], normal[0][1]], [normal[1][0], normal[1][1]]], [rhs[0], rhs[1]]).map(|x| [x[0], x[1], 0., 0.]),
        3 => solve(std::array::from_fn(|i| std::array::from_fn(|j| normal[i][j])), [rhs[0], rhs[1], rhs[2]]).map(|x| [x[0], x[1], x[2], 0.]),
        _ => solve(normal, rhs),
    }?;
    let (c1, k) = match left { Some(d) => { if x[0] <= 0. { return None; } (add(first, scale(d, x[0])), 1) } None => ([x[0], x[1]], 2) };
    let c2 = match right { Some(d) => { if x[k] <= 0. { return None; } sub(last, scale(d, x[k])) } None => [x[k], x[k + 1]] };
    let reach: f64 = points.windows(2).map(|w| distance(w[0], w[1])).sum::<f64>() * 2.;
    (distance(c1, first) <= reach && distance(c2, last) <= reach).then_some([first, c1, c2, last])
}
fn project(c: &[[f64; 2]; 4], points: &[[f64; 2]], u: &[f64]) -> Vec<f64> {
    let d1: [[f64; 2]; 3] = std::array::from_fn(|i| scale(sub(c[i + 1], c[i]), 3.));
    let d2: [[f64; 2]; 2] = std::array::from_fn(|i| scale(sub(d1[i + 1], d1[i]), 2.));
    let newton = |p: [f64; 2], t: f64| {
        let q = sub(bezier(c, t), p);
        let s = 1. - t;
        let q1 = add(add(scale(d1[0], s * s), scale(d1[1], 2. * s * t)), scale(d1[2], t * t));
        let q2 = add(scale(d2[0], s), scale(d2[1], t));
        let denominator = dot(q1, q1) + dot(q, q2);
        if denominator.abs() < 1e-12 { t } else { (t - dot(q, q1) / denominator).clamp(0., 1.) }
    };
    let mut next: Vec<f64> = points.iter().zip(u).map(|(p, t)| newton(*p, newton(*p, *t))).collect();
    (next[0], *next.last_mut().unwrap()) = (0., 1.);
    for i in 1..next.len() { next[i] = next[i].max(next[i - 1]); }
    next
}
fn chord_distance(p: [f64; 2], a: [f64; 2], b: [f64; 2]) -> f64 {
    let ab = sub(b, a);
    let length = dot(ab, ab);
    let t = if length > 0. { (dot(sub(p, a), ab) / length).clamp(0., 1.) } else { 0. };
    distance(p, add(a, scale(ab, t)))
}
fn worst(points: &[[f64; 2]], u: &[f64], c: &[[f64; 2]; 4]) -> f64 {
    let vertices = (1..points.len() - 1).map(|i| distance(bezier(c, u[i]), points[i]));
    let chords = (0..points.len() - 1).map(|i| chord_distance(bezier(c, (u[i] + u[i + 1]) * 0.5), points[i], points[i + 1]));
    vertices.chain(chords).fold(0., f64::max)
}
fn segment(points: &[[f64; 2]], left: Option<[f64; 2]>, right: Option<[f64; 2]>, tolerance: f64) -> Option<Segment> {
    let (first, last) = (points[0], points[points.len() - 1]);
    let mut u = chord_parameters(points);
    let mut best: Option<Segment> = None;
    for iteration in 0..6 {
        let Some(cubic) = least_squares(points, &u, left, right) else { break };
        u = project(&cubic, points, &u);
        let error = worst(points, &u, &cubic);
        if best.as_ref().is_none_or(|b| error < b.error) { best = Some(Segment { cubic, u: u.clone(), error }); }
        if error <= tolerance || (iteration > 0 && error > tolerance * 4.) { break; }
    }
    if best.is_none() && points.len() <= 3 {
        let span = distance(first, last) / 3.;
        let start = left.unwrap_or_else(|| unit(sub(points[1], first)));
        let end = right.unwrap_or_else(|| unit(sub(last, points[points.len() - 2])));
        let cubic = [first, add(first, scale(start, span)), sub(last, scale(end, span)), last];
        return Some(Segment { error: worst(points, &u, &cubic), cubic, u });
    }
    best
}
fn start_direction(c: &[[f64; 2]; 4]) -> [f64; 2] {
    unit([sub(c[1], c[0]), sub(c[2], c[0]), sub(c[3], c[0])].into_iter().find(|v| dot(*v, *v) > 1e-18).unwrap_or([1., 0.]))
}
fn end_direction(c: &[[f64; 2]; 4]) -> [f64; 2] {
    unit([sub(c[3], c[2]), sub(c[3], c[1]), sub(c[3], c[0])].into_iter().find(|v| dot(*v, *v) > 1e-18).unwrap_or([1., 0.]))
}
fn longest(points: &[[f64; 2]], start: usize, left: Option<[f64; 2]>, tolerance: f64) -> (usize, Segment) {
    let last = points.len() - 1;
    let fits = |end: usize| segment(&points[start..=end], left, None, tolerance).filter(|s| s.error <= tolerance);
    let mut good = (start + 1, segment(&points[start..=start + 1], left, None, tolerance).unwrap());
    let mut step = 2;
    let mut bad = None;
    while good.0 < last {
        let end = (start + step).min(last);
        match fits(end) { Some(s) => good = (end, s), None => { bad = Some(end); break; } }
        step *= 2;
    }
    if let Some(mut high) = bad {
        let mut low = good.0;
        while high - low > 1 {
            let middle = (low + high) / 2;
            match fits(middle) { Some(s) => { good = (middle, s); low = middle; } None => high = middle }
        }
    }
    good
}
fn fit(points: &[[f64; 2]], tolerance: f64, out: &mut Fitted, offset: usize) {
    let last = points.len() - 1;
    let mut pieces: Vec<(usize, Option<[f64; 2]>, Segment)> = Vec::new();
    let mut start = 0;
    while start < last {
        let (mut end, mut next) = longest(points, start, None, tolerance);
        if let Some((from, left, previous)) = pieces.last_mut() {
            let (incoming, outgoing) = (end_direction(&previous.cubic), start_direction(&next.cubic));
            let joined = [0., 0.25, 0.5, 0.75, 1.].into_iter().filter_map(|w| {
                let d = unit(add(scale(outgoing, 1. - w), scale(incoming, w)));
                let a = segment(&points[*from..=start], *left, Some(d), tolerance).filter(|a| a.error <= tolerance)?;
                let b = segment(&points[start..=end], Some(d), None, tolerance)?;
                Some((d, a, b))
            }).min_by(|x, y| x.2.error.total_cmp(&y.2.error));
            let d = match joined {
                Some((d, a, b)) => {
                    *previous = a;
                    if b.error <= tolerance { next = b } else { (end, next) = longest(points, start, Some(d), tolerance) }
                    d
                }
                None => { (end, next) = longest(points, start, Some(incoming), tolerance); incoming }
            };
            pieces.push((start, Some(d), next));
        } else {
            pieces.push((start, None, next));
        }
        start = end;
    }
    for (start, _, s) in pieces {
        out.cubics.push(s.cubic);
        for (i, t) in s.u.iter().enumerate().skip(1) { out.params[offset + start + i] = (out.cubics.len() - 1, *t); }
    }
}

fn tangent_at(points: &[[f64; 2]], from: usize, toward: isize, reach: f64) -> [f64; 2] {
    let mut j = from as isize;
    loop {
        let next = j + toward;
        if next < 0 || next as usize >= points.len() { break; }
        j = next;
        if distance(points[j as usize], points[from]) >= reach { break; }
    }
    unit(sub(points[j as usize], points[from]))
}
fn corners(points: &[[f64; 2]], reach: f64) -> Vec<usize> {
    let mut result = vec![0];
    let angles: Vec<f64> = (0..points.len()).map(|i| {
        if i == 0 || i + 1 == points.len() { return 0.; }
        let a = tangent_at(points, i, -1, reach);
        let b = tangent_at(points, i, 1, reach);
        (-dot(a, b)).clamp(-1., 1.).acos()
    }).collect();
    let limit = 55f64.to_radians();
    for i in 1..points.len().saturating_sub(1) {
        if angles[i] > limit && angles[i] >= angles[i - 1] && angles[i] > angles[i + 1] && distance(points[i], points[*result.last().unwrap()]) > reach * 0.5 { result.push(i); }
    }
    result.push(points.len() - 1);
    result.dedup();
    result
}

fn simplify(values: &[[f64; 2]], tolerance: f64, keep: &mut [bool], range: std::ops::Range<usize>) {
    if range.len() < 3 { return; }
    let (a, b) = (values[range.start], values[range.end - 1]);
    let mut worst = (0., range.start);
    for (i, value) in values.iter().enumerate().take(range.end - 1).skip(range.start + 1) {
        let t = if b[0] > a[0] { (value[0] - a[0]) / (b[0] - a[0]) } else { 0. };
        let error = (value[1] - (a[1] + (b[1] - a[1]) * t)).abs();
        if error > worst.0 { worst = (error, i); }
    }
    if worst.0 > tolerance {
        keep[worst.1] = true;
        simplify(values, tolerance, keep, range.start..worst.1 + 1);
        simplify(values, tolerance, keep, worst.1..range.end);
    }
}

pub fn fit_line(samples: &[Sample], tolerance: f64) -> Option<(PathGeometry, Option<WidthProfile>, f64)> {
    let half = samples.iter().map(|s| s.1).fold(0., f64::max);
    if samples.is_empty() || half <= 0. { return None; }
    let mut kept: Vec<Sample> = Vec::with_capacity(samples.len());
    for sample in samples {
        if let Some(last) = kept.last_mut() && distance(last.0, sample.0) < tolerance * 0.25 { last.1 = last.1.max(sample.1); continue; }
        kept.push(*sample);
    }
    if let Some(end) = samples.last() && kept.len() > 1 && let Some(last) = kept.last_mut() { last.0 = end.0; }
    let points: Vec<[f64; 2]> = kept.iter().map(|s| s.0).collect();
    if points.len() == 1 {
        let geometry = PathGeometry::new(vec![Subpath::new(&[Anchor::corner(points[0])], false)]).ok()?;
        return Some((geometry, None, half * 2.));
    }
    let reach = (tolerance * 8.).max(half);
    let corners = corners(&points, reach);
    let mut fitted = Fitted { cubics: Vec::new(), params: vec![(0, 0.); points.len()] };
    let mut kinds = Vec::new();
    for pair in corners.windows(2) {
        let start = fitted.cubics.len();
        fit(&points[pair[0]..=pair[1]], tolerance, &mut fitted, pair[0]);
        kinds.push(AnchorKind::CORNER);
        kinds.extend(std::iter::repeat_n(AnchorKind::SMOOTH, fitted.cubics.len() - start - 1));
    }
    kinds.push(AnchorKind::CORNER);
    let mut anchors: Vec<Anchor> = Vec::with_capacity(fitted.cubics.len() + 1);
    for (i, cubic) in fitted.cubics.iter().enumerate() {
        if i == 0 { anchors.push(Anchor::new(cubic[0], [0.; 2], sub(cubic[1], cubic[0]), kinds[0])); }
        else { let last = anchors.last_mut().unwrap(); *last = last.with_out_handle(sub(cubic[1], cubic[0])); }
        anchors.push(Anchor::new(cubic[3], sub(cubic[2], cubic[3]), [0.; 2], kinds[i + 1]));
    }
    let geometry = PathGeometry::new(vec![Subpath::new(&anchors, false)]).ok()?;
    let widths: Vec<f64> = kept.iter().map(|s| s.1).collect();
    let (low, high) = widths.iter().fold((f64::INFINITY, 0f64), |(l, h), w| (l.min(*w), h.max(*w)));
    if high - low <= tolerance { return Some((geometry, None, high * 2.)); }
    let values: Vec<[f64; 2]> = fitted.params.iter().zip(&widths).map(|((segment, t), w)| [*segment as f64 + t, *w]).collect();
    let mut keep = vec![false; values.len()];
    keep[0] = true;
    *keep.last_mut().unwrap() = true;
    simplify(&values, tolerance, &mut keep, 0..values.len());
    let mut knots: Vec<WidthKnot> = Vec::new();
    for (i, w) in widths.iter().enumerate().filter(|(i, _)| keep[*i]) {
        let (segment, t) = fitted.params[i];
        let knot = WidthKnot::new(segment as u32, t.clamp(0., 1.), w / high, w / high);
        match knots.last_mut() {
            Some(last) if (last.segment(), last.t()) >= (knot.segment(), knot.t()) => if knot.left() > last.left() { *last = WidthKnot::new(last.segment(), last.t(), knot.left(), knot.right()) },
            _ => knots.push(knot),
        }
    }
    Some((geometry, WidthProfile::new(knots).ok(), high * 2.))
}

#[cfg(test)]
mod tests {
    use super::*;
    fn sample_curve(n: usize, radius: impl Fn(f64) -> f64) -> Vec<Sample> {
        (0..n).map(|i| { let t = i as f64 / (n - 1) as f64; ([t * 300., 80. * (t * 6.).sin() + t * t * 40.], radius(t)) }).collect()
    }
    fn deviation(geometry: &PathGeometry, samples: &[Sample]) -> f64 {
        let curve: Vec<[f64; 2]> = geometry.segments().flat_map(|s| (0..=1024).map(move |i| s.point(i as f64 / 1024.))).collect();
        samples.iter().map(|(p, _)| curve.iter().map(|q| distance(*p, *q)).fold(f64::INFINITY, f64::min)).fold(0., f64::max)
    }
    fn hand(kind: usize) -> Vec<PenEvent> {
        let mut seed = 0x2545_f491_4f6c_dd1du64 + kind as u64;
        let mut noise = move || { seed ^= seed << 13; seed ^= seed >> 7; seed ^= seed << 17; (seed % 10_000) as f64 / 10_000. - 0.5 };
        let (seconds, path): (f64, Box<dyn Fn(f64) -> [f64; 2]>) = match kind {
            0 => (1.1, Box::new(|t: f64| [100. + 360. * t, 300. + 110. * (t * std::f64::consts::TAU).sin()])),
            1 => (1.3, Box::new(|t: f64| { let a = t * 5.9; [400. + 130. * a.cos() * (1. + 0.08 * t), 300. + 110. * a.sin()] })),
            2 => (0.14, Box::new(|t: f64| [200. + 160. * t, 200. + 30. * t + 6. * (t * 3.1).sin()])),
            _ => (1.2, Box::new(|t: f64| { let k = (t * 4.).min(3.9999); let up = k.floor() as i32 % 2 == 0; [100. + 80. * k, if up { 100. + 120. * k.fract() } else { 220. - 120. * k.fract() }] })),
        };
        let n = (seconds * 240.) as usize;
        (0..=n).map(|i| {
            let t = i as f64 / n as f64;
            let [x, y] = path(t);
            let pressure = ((t * std::f64::consts::PI).sin().powf(0.3) * 0.75 + 0.05 * noise()).clamp(0.02, 1.);
            PenEvent {
                device_id: 1, sequence: i as u64, timestamp_ns: (t * seconds * 1e9) as u64 + 1, view_revision: 0,
                surface_position: layer_core::Point { x: (x + 0.3 * noise()) as f32, y: (y + 0.3 * noise()) as f32 },
                pressure: pressure as f32, tilt_radians: [0.; 2], twist_radians: 0., distance: 0.,
                phase: if i == 0 { PenPhase::Down } else if i == n { PenPhase::Up } else { PenPhase::Move },
                tool: crate::ToolKind::Pen, flags: SampleFlags::PRIMARY,
            }
        }).collect()
    }
    fn drawn(events: &[PenEvent], strength: f32) -> VectorStroke {
        let mut brush = layer_core::default_brush(layer_core::DefaultBrushPreset::GPen);
        brush.stabilization.strength = strength;
        let mut stroke = VectorStroke::new(1, brush, PressureCurve::default(), [0.; 2], InstantFeedbackConfig::default(), [1., 0., 0., 1., 0., 0.]);
        for event in events {
            if event.flags.contains(SampleFlags::CORRECTION) { stroke.correct(*event); } else { stroke.push(*event, ViewTransform::IDENTITY); }
        }
        stroke
    }
    fn width_error(geometry: &PathGeometry, profile: Option<&WidthProfile>, width: f64, samples: &[Sample]) -> f64 {
        let fitted: Vec<([f64; 2], f64)> = geometry.segments().enumerate()
            .flat_map(|(s, seg)| { let n = 4096; (0..=n).map(move |i| (s, i as f64 / n as f64, seg.point(i as f64 / n as f64))) })
            .map(|(s, t, p)| (p, profile.map_or(1., |profile| profile.at(s as u32, t)[0]) * width * 0.5)).collect();
        let ends = [samples[0].0, samples[samples.len() - 1].0];
        let widest = |points: &mut dyn Iterator<Item = ([f64; 2], f64)>, at: [f64; 2]| points.filter(|(p, _)| distance(*p, at) <= 1.).map(|(_, r)| r).fold(0., f64::max);
        let tips = ends.map(|at| (widest(&mut samples.iter().copied(), at) - widest(&mut fitted.iter().copied(), at)).abs());
        samples.iter().filter(|(p, _)| ends.iter().all(|end| distance(*p, *end) > 1.)).map(|(p, r)| {
            let nearest = fitted.iter().min_by(|a, b| distance(a.0, *p).total_cmp(&distance(b.0, *p))).unwrap();
            (nearest.1 - r).abs()
        }).chain(tips).fold(0., f64::max)
    }
    #[test]
    fn hand_drawn_lines_keep_few_anchors_within_tolerance() {
        let tolerance = 0.5;
        for (kind, most) in [(0, 7), (1, 5), (2, 2), (3, 12)] {
            for strength in [0., 0.06, 0.4] {
                let samples = drawn(&hand(kind), strength).samples();
                let (geometry, profile, width) = fit_line(&samples, tolerance).unwrap();
                let knots = profile.as_ref().map_or(0, |p| p.knots().len());
                let label = format!("stroke {kind}, smoothing {strength}: {} anchors, {knots} knots, error {:.3}, width error {:.3}", geometry.anchor_count(), deviation(&geometry, &samples), width_error(&geometry, profile.as_ref(), width, &samples));
                assert!(geometry.anchor_count() <= most, "{label}");
                assert!(knots <= 12, "{label}");
                assert!(deviation(&geometry, &samples) <= tolerance * 1.02, "{label}");
                assert!(width_error(&geometry, profile.as_ref(), width, &samples) <= tolerance * 1.2, "{label}");
            }
        }
    }
    #[test]
    fn late_pressure_corrections_replay_the_line() {
        let events = hand(0);
        let estimated: Vec<PenEvent> = events.iter().map(|e| PenEvent { pressure: e.pressure * 0.3, flags: SampleFlags(SampleFlags::PRIMARY.0 | SampleFlags::ESTIMATED.0), ..*e }).collect();
        let mut corrections: Vec<PenEvent> = events.iter().map(|e| PenEvent { flags: SampleFlags(SampleFlags::PRIMARY.0 | SampleFlags::CORRECTION.0), ..*e }).collect();
        let half = corrections.len() / 2;
        let mut stroke = drawn(&estimated, 0.06);
        assert!(stroke.awaits_corrections());
        let unknown = PenEvent { sequence: 10_000, ..corrections[0] };
        assert!(!stroke.corrects(&unknown) && !stroke.correct(unknown));
        for event in corrections.drain(..half) { assert!(stroke.corrects(&event)); stroke.correct(event); }
        assert!(stroke.awaits_corrections());
        for event in corrections { stroke.correct(event); }
        assert!(!stroke.awaits_corrections());
        assert_eq!(stroke.samples(), drawn(&events, 0.06).samples());
        assert_ne!(drawn(&estimated, 0.06).samples(), drawn(&events, 0.06).samples());
    }
    #[test]
    fn fitted_lines_stay_within_tolerance_with_few_anchors() {
        let samples = sample_curve(400, |t| 2. + 3. * (t * std::f64::consts::PI).sin());
        let (geometry, profile, width) = fit_line(&samples, 0.25).unwrap();
        let error = deviation(&geometry, &samples);
        assert!(error < 0.26, "{error}");
        assert!(geometry.anchor_count() <= 8, "{}", geometry.anchor_count());
        assert!((width - 10.).abs() < 0.2);
        let profile = profile.unwrap();
        assert!(profile.knots().len() < 60);
        for (p, r) in samples.iter().step_by(23) {
            let nearest = geometry.segments().enumerate().flat_map(|(s, seg)| (0..=64).map(move |i| (s, i as f64 / 64., seg.point(i as f64 / 64.))))
                .min_by(|a, b| distance(a.2, *p).total_cmp(&distance(b.2, *p))).unwrap();
            let fitted = profile.at(nearest.0 as u32, nearest.1)[0] * width * 0.5;
            assert!((fitted - r).abs() < 0.6, "width {fitted} vs {r}");
        }
    }
    #[test]
    fn corners_stay_sharp_and_constant_widths_drop_the_profile() {
        let mut samples: Vec<Sample> = (0..100).map(|i| ([i as f64, 0.], 3.)).collect();
        samples.extend((1..100).map(|i| ([99., i as f64], 3.)));
        let (geometry, profile, _) = fit_line(&samples, 0.25).unwrap();
        assert!(profile.is_none());
        let corner = geometry.subpaths()[0].anchors().any(|a| a.kind == AnchorKind::CORNER && distance(a.point(), [99., 0.]) < 1.5);
        assert!(corner, "{:?}", geometry.subpaths()[0].to_vec());
        assert!(deviation(&geometry, &samples) < 0.5);
        let (dot, _, width) = fit_line(&[([5., 5.], 2.)], 0.25).unwrap();
        assert_eq!((dot.anchor_count(), width), (1, 4.));
    }
}
