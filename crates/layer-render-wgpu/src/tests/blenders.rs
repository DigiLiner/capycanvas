//! Smudge blenders and the wet blenders on fixed scenes: flat and faint paint
//! stay put, a colour edge blends along its length, lines stop the
//! edge-keeping blender, the colour mixer carries one colour, the amount is
//! independent of spacing and the texture-keeping blenders keep fine detail.
use super::brush_blending::Canvas;
use super::*;
use layer_core::color::{DocumentColor, RgbSpace, SampleDepth};
use layer_core::{BlendSpace, BrushExecution, BrushSnapshot, DefaultBrushPreset as P, Stroke, StrokeId, StrokePoint, StrokeTool};

const RED: [f32; 4] = [0.8, 0.05, 0.03, 1.];
const BLUE: [f32; 4] = [0.03, 0.12, 0.8, 1.];

fn canvas() -> Canvas {
    Canvas::new(DocumentColor { depth: SampleDepth::F32, ..Default::default() }, BlendSpace::Linear, None)
}

fn brush(preset: P, diameter: f32) -> BrushSnapshot {
    let mut brush = layer_core::default_brush(preset);
    brush.diameter = diameter * brush.aspect.min(1.);
    brush.stabilization.strength = 0.;
    brush
}

fn contacts(brush: &BrushSnapshot, path: &[[f32; 2]]) -> Vec<Dab> {
    let points: Vec<_> = path.iter().enumerate().map(|(i, &[x, y])| StrokePoint {
        position: Point { x, y }, pressure: 1., tilt: [0.; 2], twist: 0., elapsed_micros: i as u32 * 4_000,
    }).collect();
    let stroke = Stroke::new(StrokeId(9), SourceTarget::Paint(layer_core::authored::PaintHandle::from_index(1)), StrokeTool::Brush,
        brush.clone(), points).unwrap();
    let mut dabs = Vec::new();
    layer_engine::DabGenerator::generate(&stroke, RgbSpace::Srgb, &mut dabs);
    dabs
}

fn line(from: [f32; 2], to: [f32; 2]) -> Vec<[f32; 2]> {
    (0..=64).map(|i| { let t = i as f32 / 64.; [from[0] + (to[0] - from[0]) * t, from[1] + (to[1] - from[1]) * t] }).collect()
}

/// Paint `left` on the whole layer and `right` from x = 66 rightwards.
fn halves(canvas: &mut Canvas, left: [f32; 4], right: [f32; 4]) {
    canvas.fill(canvas.base, left);
    let mut dab = test_dab([1066., 64.], right, 1.);
    dab.radii = [1000.; 2];
    canvas.paint(canvas.base, test_style(BrushExecution::Dry), dab);
}

fn pixel(pixels: &[[f64; 4]], x: usize, y: usize) -> [f64; 4] {
    pixels[y * 128 + x]
}

fn blenders() -> impl Iterator<Item = P> {
    P::ALL.into_iter().filter(|&p| layer_core::default_brush(p).execution == BrushExecution::Smudge).chain([P::WetBlender, P::ColorMixer])
}

#[test]
fn every_blender_leaves_flat_paint_unchanged() {
    for preset in blenders() {
        let mut canvas = canvas();
        canvas.fill(canvas.base, [0.4, 0.3, 0.2, 1.]);
        let before = canvas.composite();
        let brush = brush(preset, 48.);
        let path: Vec<_> = (0..=64).map(|i| { let t = i as f32 / 64.; [20. + 88. * t, 64. + 24. * (t * 6.).sin()] }).collect();
        canvas.stroke(1, &brush, &contacts(&brush, &path), 6, false);
        for (i, (a, b)) in canvas.composite().iter().zip(&before).enumerate() {
            assert!(a.iter().all(|v| v.is_finite()), "{preset:?} pixel {i} is finite: {a:?}");
            assert!(a.iter().zip(b).all(|(a, b)| (a - b).abs() < 0.0005), "{preset:?} pixel {i}: {a:?} != {b:?}");
        }
    }
}

#[test]
fn blenders_mix_an_edge_they_stroke_along() {
    for preset in [P::Smudge, P::NaturalBlender, P::SoftBlender, P::Blur] {
        let mut canvas = canvas();
        halves(&mut canvas, BLUE, RED);
        let before = canvas.composite();
        let brush = brush(preset, 48.);
        canvas.stroke(1, &brush, &contacts(&brush, &line([66., 4.], [66., 124.])), 6, false);
        let after = canvas.composite();
        for x in [60, 72] {
            let moved = (pixel(&after, x, 64)[0] - pixel(&before, x, 64)[0]).abs();
            assert!(moved > 0.04, "{preset:?} mixes {} px from the edge: {moved}", x.abs_diff(66));
        }
        assert_eq!(pixel(&after, 20, 64), pixel(&before, 20, 64), "{preset:?} leaves paint beyond its reach");
    }
}

#[test]
fn edge_keeping_stops_at_a_thin_line_that_soft_blending_crosses() {
    let line_change = |preset| {
        let mut canvas = canvas();
        canvas.fill(canvas.base, [0.8, 0.55, 0.45, 1.]);
        for y in 0..128 {
            let mut dab = test_dab([64., y as f32 + 0.5], [0.01, 0.01, 0.01, 1.], 1.);
            dab.radii = [1.2; 2];
            dab.hardness = 1.;
            canvas.paint(canvas.base, test_style(BrushExecution::Dry), dab);
        }
        let before = pixel(&canvas.composite(), 64, 64)[0];
        let brush = brush(preset, 40.);
        canvas.stroke(1, &brush, &contacts(&brush, &line([30., 64.], [98., 64.])), 6, false);
        (pixel(&canvas.composite(), 64, 64)[0] - before).abs()
    };
    assert!(line_change(P::EdgeKeepingBlender) < 0.01, "the line survives edge-keeping blending");
    assert!(line_change(P::SoftBlender) > 0.1, "soft blending crosses the line");
}

#[test]
fn color_mixer_lays_one_averaged_color_where_the_wet_blender_keeps_both() {
    let spread = |preset| {
        let mut canvas = canvas();
        halves(&mut canvas, BLUE, RED);
        let brush = brush(preset, 40.);
        canvas.stroke(1, &brush, &contacts(&brush, &line([66., 4.], [66., 124.])), 3, false);
        let after = canvas.composite();
        let [a, b] = [pixel(&after, 58, 96), pixel(&after, 74, 96)];
        (0..3).map(|c| (a[c] - b[c]).abs()).fold(0., f64::max)
    };
    assert!(spread(P::ColorMixer) < 0.1, "the color mixer carries one color across the tip");
    assert!(spread(P::WetBlender) > 0.3, "the wet blender keeps each side's color");
}

#[test]
fn blending_amount_follows_distance_not_spacing() {
    let mixed = |spacing| {
        let mut canvas = canvas();
        halves(&mut canvas, BLUE, RED);
        let before = canvas.composite();
        let mut brush = brush(P::SoftBlender, 48.);
        brush.spacing = spacing;
        canvas.stroke(1, &brush, &contacts(&brush, &line([66., 4.], [66., 124.])), 6, false);
        (pixel(&canvas.composite(), 60, 64)[0] - pixel(&before, 60, 64)[0]).abs()
    };
    let [dense, sparse] = [mixed(0.02), mixed(0.06)];
    assert!((dense - sparse).abs() < 0.3 * dense.max(sparse), "{dense} vs {sparse}");
}

#[test]
fn blender_samples_stay_inside_a_page_neighbourhood() {
    assert!(layer_core::SMUDGE_CHUNK_TRAVEL_LIMIT + layer_core::BLENDER_REACH_LIMIT + 4. <= PAGE_SIZE as f32);
    let source = include_str!("../material_blend.wgsl");
    assert!(source.contains(&format!("const BLEND_REACH_LIMIT: f32 = {:.1};", layer_core::BLENDER_REACH_LIMIT)));
    assert!(source.contains(&format!("const SMUDGE_TRAVEL_LIMIT: f32 = {:.1};", layer_core::SMUDGE_CHUNK_TRAVEL_LIMIT)));
    for (index, kind) in layer_core::BlenderKind::ALL.into_iter().enumerate() {
        assert_eq!(kind as usize, index);
    }
}

#[test]
fn blenders_stay_finite_over_faint_paint() {
    for alpha in [1e-6, 1e-39] {
        for preset in blenders() {
            let mut canvas = canvas();
            canvas.fill(canvas.base, [0.4, 0.3, 0.2, alpha]);
            let brush = brush(preset, 40.);
            canvas.stroke(1, &brush, &contacts(&brush, &line([30., 64.], [98., 64.])), 6, false);
            for (i, p) in canvas.composite().iter().enumerate() {
                assert!(p.iter().all(|v| v.is_finite() && (0. ..=1.).contains(v)), "{preset:?} over alpha {alpha}: pixel {i} {p:?}");
            }
        }
    }
}

#[test]
fn texture_keeping_blenders_keep_fine_detail_that_soft_blending_removes() {
    let kept = |preset| {
        let mut canvas = canvas();
        canvas.fill(canvas.base, [0.6, 0.45, 0.3, 1.]);
        for y in (34..96).step_by(5) {
            for x in (22..108).step_by(5) {
                let mut dab = test_dab([x as f32 + 0.5, y as f32 + 0.5], [0.3, 0.22, 0.15, 1.], 1.);
                dab.radii = [1.; 2];
                dab.hardness = 1.;
                canvas.paint(canvas.base, test_style(BrushExecution::Dry), dab);
            }
        }
        let detail = |pixels: &[[f64; 4]]| {
            let mut sum = 0.;
            for y in 50..78 {
                for x in 44..86 {
                    let mean = (0..25).map(|k| pixel(pixels, x + k % 5 - 2, y + k / 5 - 2)[0]).sum::<f64>() / 25.;
                    sum += (pixel(pixels, x, y)[0] - mean).powi(2);
                }
            }
            sum.sqrt()
        };
        let before = detail(&canvas.composite());
        let brush = brush(preset, 48.);
        for pass in 0..4 {
            let [from, to] = if pass % 2 == 0 { [[30., 64.], [98., 64.]] } else { [[98., 64.], [30., 64.]] };
            canvas.stroke(pass + 1, &brush, &contacts(&brush, &line(from, to)), 6, false);
        }
        detail(&canvas.composite()) / before
    };
    let soft = kept(P::SoftBlender);
    let texture = kept(P::TextureBlender);
    assert!(texture > 1.5 * soft, "texture-keeping keeps {texture} of the detail, soft {soft}");
    assert!(kept(P::HueBlender) > 0.9, "hue blending keeps lightness detail");
}
