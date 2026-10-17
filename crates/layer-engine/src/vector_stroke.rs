use crate::input::to_stroke_point;
use crate::{DabGenerator, PenEvent, PenPhase, PressureCurve, SampleFlags, ViewTransform};
use layer_core::authored::vector::{Anchor, AnchorKind, PathGeometry, Subpath, WidthKnot, WidthProfile};
use layer_core::{BrushSnapshot, StrokeId};
use layer_render::Dab;

pub type Sample = ([f64; 2], f64);

pub struct VectorStroke {
    brush: BrushSnapshot,
    generator: DabGenerator,
    transform: ViewTransform,
    curve: PressureCurve,
    start_ns: u64,
    replay: Vec<layer_core::StrokePoint>,
    dabs: Vec<Dab>,
    id: u64,
}
impl VectorStroke {
    pub fn new(id: u64, brush: BrushSnapshot, transform: ViewTransform, curve: PressureCurve) -> Self {
        let mut generator = DabGenerator::new(layer_core::color::RgbSpace::Srgb);
        generator.reset_for_stroke(StrokeId(id), &brush);
        Self { brush, generator, transform, curve, start_ns: 0, replay: Vec::new(), dabs: Vec::new(), id }
    }
    pub fn brush(&self) -> &BrushSnapshot { &self.brush }
    pub fn push(&mut self, event: PenEvent) -> bool {
        if event.flags.contains(SampleFlags::PREDICTED) || event.flags.contains(SampleFlags::CORRECTION)
            || !matches!(event.phase, PenPhase::Down | PenPhase::Move | PenPhase::Up) { return false; }
        if event.phase == PenPhase::Down { self.start_ns = event.timestamp_ns; }
        let point = to_stroke_point(event, self.transform, self.curve.clone(), self.start_ns);
        if !point.position.x.is_finite() || !point.position.y.is_finite() || self.replay.len() >= crate::canvas::MAX_CONTACT_POINTS { return false; }
        self.replay.push(point);
        self.generator.append(point, &self.brush, &mut self.dabs);
        true
    }
    pub fn preview(&self) -> Vec<Sample> {
        let mut tail = Vec::new();
        self.generator.clone().finish(&self.brush, &mut tail);
        samples(self.dabs.iter().chain(&tail))
    }
    pub fn finish(mut self) -> Vec<Sample> {
        if self.brush.taper.end_distance_diameters > 0. && !self.replay.is_empty()
            && let Ok(stroke) = layer_core::Stroke::new(StrokeId(self.id), layer_core::SourceTarget::Paint(layer_core::authored::PaintHandle::from_index(0)),
                layer_core::StrokeTool::Brush, self.brush.clone(), std::mem::take(&mut self.replay)) {
            let mut dabs = Vec::new();
            let mut generator = DabGenerator::new(layer_core::color::RgbSpace::Srgb);
            generator.reset_for_replay(&stroke);
            for point in stroke.points.iter().copied() { generator.append(point, &stroke.brush, &mut dabs); }
            generator.finish(&stroke.brush, &mut dabs);
            return samples(dabs.iter());
        }
        let mut tail = Vec::new();
        self.generator.finish(&self.brush, &mut tail);
        samples(self.dabs.iter().chain(&tail))
    }
}
fn samples<'a>(dabs: impl Iterator<Item = &'a Dab>) -> Vec<Sample> {
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
        WidthKnot { segment, t, left: s.1 / half, right: s.1 / half }
    }).collect();
    Some((geometry, WidthProfile::new(knots).ok(), half * 2.))
}

struct Fitted { cubics: Vec<[[f64; 2]; 4]>, params: Vec<(usize, f64)> }

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
fn generate(points: &[[f64; 2]], u: &[f64], left: [f64; 2], right: [f64; 2]) -> [[f64; 2]; 4] {
    let (first, last) = (points[0], points[points.len() - 1]);
    let (mut c00, mut c01, mut c11, mut x0, mut x1) = (0., 0., 0., 0., 0.);
    for (p, &t) in points.iter().zip(u) {
        let s = 1. - t;
        let (b1, b2) = (3. * t * s * s, 3. * t * t * s);
        let (a1, a2) = (scale(left, b1), scale(right, b2));
        c00 += dot(a1, a1); c01 += dot(a1, a2); c11 += dot(a2, a2);
        let base = add(scale(first, s * s * s + b1), scale(last, b2 + t * t * t));
        let tmp = sub(*p, base);
        x0 += dot(a1, tmp); x1 += dot(a2, tmp);
    }
    let det = c00 * c11 - c01 * c01;
    let span = distance(first, last);
    let (mut alpha_l, mut alpha_r) = if det.abs() > 1e-12 { ((x0 * c11 - x1 * c01) / det, (c00 * x1 - c01 * x0) / det) } else { (span / 3., span / 3.) };
    if alpha_l < 1e-6 * span || alpha_r < 1e-6 * span || alpha_l > span * 2. || alpha_r > span * 2. { alpha_l = span / 3.; alpha_r = span / 3.; }
    [first, add(first, scale(left, alpha_l)), add(last, scale(right, alpha_r)), last]
}
fn worst(points: &[[f64; 2]], u: &[f64], c: &[[f64; 2]; 4]) -> (f64, usize) {
    let mut result = (0., points.len() / 2);
    for i in 1..points.len() - 1 {
        let d = sub(bezier(c, u[i]), points[i]);
        let e = dot(d, d);
        if e >= result.0 { result = (e, i); }
    }
    result
}
fn newton(c: &[[f64; 2]; 4], p: [f64; 2], t: f64) -> f64 {
    let d1: Vec<[f64; 2]> = (0..3).map(|i| scale(sub(c[i + 1], c[i]), 3.)).collect();
    let d2: Vec<[f64; 2]> = (0..2).map(|i| scale(sub(d1[i + 1], d1[i]), 2.)).collect();
    let q = sub(bezier(c, t), p);
    let s = 1. - t;
    let q1 = add(add(scale(d1[0], s * s), scale(d1[1], 2. * s * t)), scale(d1[2], t * t));
    let q2 = add(scale(d2[0], s), scale(d2[1], t));
    let denominator = dot(q1, q1) + dot(q, q2);
    if denominator.abs() < 1e-12 { t } else { (t - dot(q, q1) / denominator).clamp(0., 1.) }
}
fn fit(points: &[[f64; 2]], left: [f64; 2], right: [f64; 2], tolerance: f64, depth: u32, out: &mut Fitted, offset: usize) {
    let error = tolerance * tolerance;
    if points.len() == 2 || depth > 24 {
        let span = distance(points[0], points[points.len() - 1]) / 3.;
        let cubic = [points[0], add(points[0], scale(left, span)), add(points[points.len() - 1], scale(right, span)), points[points.len() - 1]];
        let u = chord_parameters(points);
        out.cubics.push(cubic);
        for (i, t) in u.iter().enumerate().skip(1) { out.params[offset + i] = (out.cubics.len() - 1, *t); }
        return;
    }
    let mut u = chord_parameters(points);
    let mut cubic = generate(points, &u, left, right);
    let (mut max, mut split) = worst(points, &u, &cubic);
    if max > error && max < error * 16. {
        for _ in 0..6 {
            let mut next: Vec<f64> = points.iter().zip(&u).map(|(p, t)| newton(&cubic, *p, *t)).collect();
            for i in 1..next.len() { if next[i] < next[i - 1] { next[i] = next[i - 1]; } }
            cubic = generate(points, &next, left, right);
            (max, split) = worst(points, &next, &cubic);
            u = next;
            if max <= error { break; }
        }
    }
    if max <= error {
        out.cubics.push(cubic);
        for (i, t) in u.iter().enumerate().skip(1) { out.params[offset + i] = (out.cubics.len() - 1, *t); }
        return;
    }
    let split = split.clamp(1, points.len() - 2);
    let center = unit(sub(points[split - 1], points[split + 1]));
    fit(&points[..=split], left, center, tolerance, depth + 1, out, offset);
    fit(&points[split..], scale(center, -1.), right, tolerance, depth + 1, out, offset + split);
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
    for i in range.start + 1..range.end - 1 {
        let t = if b[0] > a[0] { (values[i][0] - a[0]) / (b[0] - a[0]) } else { 0. };
        let error = (values[i][1] - (a[1] + (b[1] - a[1]) * t)).abs();
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
        if let Some(last) = kept.last() && distance(last.0, sample.0) < tolerance * 0.25 { continue; }
        kept.push(*sample);
    }
    if let Some(end) = samples.last() && kept.len() > 1 && distance(kept[kept.len() - 1].0, end.0) > 0. { *kept.last_mut().unwrap() = *end; }
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
        let run = &points[pair[0]..=pair[1]];
        let start = fitted.cubics.len();
        let left = tangent_at(run, 0, 1, reach);
        let right = tangent_at(run, run.len() - 1, -1, reach);
        fit(run, left, right, tolerance, 0, &mut fitted, pair[0]);
        kinds.push(AnchorKind::CORNER);
        kinds.extend(std::iter::repeat_n(AnchorKind::SMOOTH, fitted.cubics.len() - start - 1));
    }
    kinds.push(AnchorKind::CORNER);
    let mut anchors: Vec<Anchor> = Vec::with_capacity(fitted.cubics.len() + 1);
    for (i, cubic) in fitted.cubics.iter().enumerate() {
        if i == 0 { anchors.push(Anchor { point: cubic[0], in_handle: [0.; 2], out_handle: sub(cubic[1], cubic[0]), kind: kinds[0] }); }
        else { anchors.last_mut().unwrap().out_handle = sub(cubic[1], cubic[0]); }
        anchors.push(Anchor { point: cubic[3], in_handle: sub(cubic[2], cubic[3]), out_handle: [0.; 2], kind: kinds[i + 1] });
    }
    let geometry = PathGeometry::new(vec![Subpath::new(&anchors, false)]).ok()?;
    let widths: Vec<f64> = kept.iter().map(|s| s.1).collect();
    let (low, high) = widths.iter().fold((f64::INFINITY, 0f64), |(l, h), w| (l.min(*w), h.max(*w)));
    if high - low <= tolerance { return Some((geometry, None, high * 2.)); }
    let mut arc = vec![0.; points.len()];
    for i in 1..points.len() { arc[i] = arc[i - 1] + distance(points[i], points[i - 1]); }
    let values: Vec<[f64; 2]> = arc.iter().zip(&widths).map(|(a, w)| [*a, *w]).collect();
    let mut keep = vec![false; values.len()];
    keep[0] = true;
    *keep.last_mut().unwrap() = true;
    simplify(&values, tolerance, &mut keep, 0..values.len());
    let mut knots: Vec<WidthKnot> = Vec::new();
    for (i, w) in widths.iter().enumerate().filter(|(i, _)| keep[*i]) {
        let (segment, t) = fitted.params[i];
        let factor = w / high;
        if let Some(last) = knots.last() && (last.segment, last.t) >= (segment as u32, t) { continue; }
        knots.push(WidthKnot { segment: segment as u32, t: t.clamp(0., 1.), left: factor, right: factor });
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
    #[test]
    fn fitted_lines_stay_within_tolerance_with_few_anchors() {
        let samples = sample_curve(400, |t| 2. + 3. * (t * std::f64::consts::PI).sin());
        let (geometry, profile, width) = fit_line(&samples, 0.25).unwrap();
        let error = deviation(&geometry, &samples);
        eprintln!("anchors {} error {error:.3} knots {:?}", geometry.anchor_count(), profile.as_ref().map(|p| p.knots().len()));
        assert!(error < 0.3, "{error}");
        assert!(geometry.anchor_count() < 40);
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
        let corner = geometry.subpaths()[0].anchors().any(|a| a.kind == AnchorKind::CORNER && distance(a.point, [99., 0.]) < 1.5);
        assert!(corner, "{:?}", geometry.subpaths()[0].to_vec());
        assert!(deviation(&geometry, &samples) < 0.5);
        let (dot, _, width) = fit_line(&[([5., 5.], 2.)], 0.25).unwrap();
        assert_eq!((dot.anchor_count(), width), (1, 4.));
    }
}
