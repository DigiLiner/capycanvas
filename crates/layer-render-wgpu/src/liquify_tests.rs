//! Direction oracles for Liquify modes: a marker's displacement under one dab.
use super::*;
use layer_core::{DefaultBrushPreset, LiquifyMode, Point, StrokeId};

const CENTER: [f32; 2] = [64., 64.];
const MARKER: [f32; 2] = [96., 64.];

fn marker_centroid(r: &mut WgpuRasterizer) -> [f32; 2] {
    let image = r.readback_srgb_rgba8().unwrap();
    let mut sum = [0.; 3];
    for (i, p) in image.chunks_exact(4).enumerate() {
        let weight = f32::from(p[3]) / 255.;
        sum[0] += weight * (i % 128) as f32;
        sum[1] += weight * (i / 128) as f32;
        sum[2] += weight;
    }
    assert!(sum[2] > 10., "the marker must stay visible");
    [sum[0] / sum[2] + 0.5, sum[1] / sum[2] + 0.5]
}

/// Paints a small marker right of the dab centre, applies one stationary
/// Liquify dab with the preset's mode, and returns the marker's displacement.
fn displacement(preset: DefaultBrushPreset) -> [f32; 2] {
    let mut r = WgpuRasterizer::new_native_headless(Default::default()).unwrap();
    let document = paint_document([128; 2], "marker");
    let mut marker = dab([1., 0., 0., 1.]);
    marker.center = Point { x: MARKER[0], y: MARKER[1] };
    marker.radii = [5.; 2];
    submit(&mut r, document.scene(), &[marker], &[batch(target(&document))], true);
    let before = marker_centroid(&mut r);
    let style = preset_style(preset);
    assert_eq!(style.execution, BrushExecution::Liquify);
    let mut deform = dab([0.; 4]);
    deform.center = Point { x: CENTER[0], y: CENTER[1] };
    deform.material = [0., 0., 0., 0.6];
    let liquify = DabBatch {
        stroke_id: StrokeId(2),
        ..crate::test_support::dab_batch(target(&document), style, batch(target(&document)).damage)
    };
    submit(&mut r, document.scene(), &[deform], &[liquify], false);
    let after = marker_centroid(&mut r);
    [after[0] - before[0], after[1] - before[1]]
}

#[test]
fn liquify_modes_move_a_marker_in_their_labelled_direction() {
    let pinch = displacement(DefaultBrushPreset::LiquifyPinch);
    assert!(pinch[0] < -3. && pinch[1].abs() < 0.5, "Pinch shrinks toward the centre: {pinch:?}");
    let expand = displacement(DefaultBrushPreset::LiquifyExpand);
    assert!(expand[0] > 3. && expand[1].abs() < 0.5, "Expand bulges away from the centre: {expand:?}");
    let counter_clockwise = displacement(DefaultBrushPreset::LiquifyTwirl);
    assert!(
        counter_clockwise[1] < -3.,
        "Twirl Counterclockwise lifts a marker right of the centre on the y-down canvas: {counter_clockwise:?}"
    );
    let clockwise = displacement(DefaultBrushPreset::LiquifyTwirlClockwise);
    assert!(
        clockwise[1] > 3.,
        "Twirl Clockwise lowers a marker right of the centre on the y-down canvas: {clockwise:?}"
    );
    assert_eq!(
        preset_style(DefaultBrushPreset::LiquifyTwirl).deform.mode,
        LiquifyMode::TwirlClockwise,
        "the shipped Twirl keeps its behaviour"
    );
}

/// Render and compute passes that one committed batch of `preset` adds on a
/// 4×4-page layer when its contact covers `radius` around the centre.
fn committed_batch_passes(preset: DefaultBrushPreset, radius: f32) -> u64 {
    let extent = [4 * PAGE_SIZE; 2];
    let mut r = WgpuRasterizer::new_native_headless(Default::default()).unwrap();
    let document = paint_document(extent, "material batch pages");
    let layer = target(&document);
    let mut base = dab([0.2, 0.4, 0.8, 1.]);
    base.center = Point { x: 512., y: 512. };
    base.radii = [500.; 2];
    let full = layer_core::Rect::from_extent(extent);
    let frame = |dabs, batches| FramePacket { dabs, dab_batches: batches, commit_rasters: false, ..packet(document.scene(), extent) };
    let paint = crate::test_support::dab_batch(layer, crate::tests::test_style(BrushExecution::Dry), full);
    r.submit(FramePacket { reset_layers: true, ..frame(std::slice::from_ref(&base), std::slice::from_ref(&paint)) }).unwrap();
    let mut contact = dab([0.; 4]);
    contact.center = Point { x: 400., y: 400. };
    contact.radii = [radius; 2];
    contact.motion = [48., 32.];
    contact.material = [0., 1., 1., 0.8];
    let stroke = DabBatch { stroke_id: StrokeId(2), ..crate::test_support::dab_batch(layer, preset_style(preset), contact.bounds()) };
    let before = r.metrics().command_passes;
    r.submit(frame(std::slice::from_ref(&contact), std::slice::from_ref(&stroke))).unwrap();
    r.wait_idle().unwrap();
    r.metrics().command_passes - before
}

#[test]
fn committed_destination_batches_encode_their_pages_in_one_pass() {
    for preset in [DefaultBrushPreset::LiquifyTwirl, DefaultBrushPreset::LiquifyPush, DefaultBrushPreset::Smudge, DefaultBrushPreset::NaturalBlender] {
        let passes = [40., 380.].map(|radius| committed_batch_passes(preset, radius));
        println!("{preset:?}: one page {}, sixteen pages {}", passes[0], passes[1]);
        assert_eq!(passes[0], passes[1], "{preset:?} adds passes per page");
    }
}
