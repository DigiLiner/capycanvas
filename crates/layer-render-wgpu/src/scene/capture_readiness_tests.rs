use super::*;
use crate::{artwork_sample_tests::{insert_effect, refresh}, test_support::{packet, float_pixels, staged_renderer}};
use layer_core::{Document, DocumentNames, EffectInstance, EffectValue, authored::{OccurrenceHandle, PortableId}};
use layer_core::color::{RgbColor, RgbSpace};

const EXTENT: [u32; 2] = [17, 19];

fn fixture(hidden_separator: bool) -> (Document, Vec<OccurrenceHandle>, Vec<OccurrenceHandle>) {
    let mut doc = Document::new(PortableId::random(), EXTENT[0], EXTENT[1],
        DocumentNames {paint: "Ink".into(), paper: "Paper".into()});
    for handle in doc.scene().children(None).to_vec() {
        doc.artwork.occurrences.get_mut(handle).unwrap().visible = false;
    }
    let mut fill = EffectInstance::new(crate::tests::fixture("solid_color").program());
    fill.set("color", EffectValue::Color(RgbColor::from_linear(RgbSpace::Srgb, [0., 0., 0., 1.]).unwrap())).unwrap();
    insert_effect(&mut doc, fill, 0);
    let a = insert_effect(&mut doc, EffectInstance::new(crate::tests::fixture("invert").program()), 0);
    let b = insert_effect(&mut doc, EffectInstance::new(crate::tests::fixture("invert").program()), 0);
    if hidden_separator {
        let x = insert_effect(&mut doc, EffectInstance::new(crate::tests::fixture("invert").program()), 0);
        let c = insert_effect(&mut doc, EffectInstance::new(crate::tests::fixture("invert").program()), 0);
        let d = insert_effect(&mut doc, EffectInstance::new(crate::tests::fixture("invert").program()), 0);
        doc.artwork.occurrences.get_mut(x).unwrap().visible = false;
        refresh(&mut doc);
        (doc, vec![a, b, c, d], vec![a, b])
    } else {
        let mut curves = EffectInstance::new(crate::tests::fixture("curves").program());
        for key in ["red", "green", "blue"] {
            curves.set(key, EffectValue::Curve(vec![[0., 1.], [1., 0.]])).unwrap();
        }
        let c = insert_effect(&mut doc, curves, 0);
        let application = doc.scene().effect_handle(c).unwrap();
        let program = &mut doc.artwork.effects.get_mut(application).unwrap().program;
        let program = Arc::make_mut(program);
        program.entry = format!("{}_channels", program.entry).into();
        (doc, vec![a, b, c], vec![a, b])
    }
}

fn prepared_scene(doc: &Document, broad: &[OccurrenceHandle], missing: &[OccurrenceHandle]) -> (WgpuRasterizer, Box<Scene>) {
    let reference = WgpuRasterizer::new_native_headless(doc.composition().color).unwrap();
    assert_ne!(reference.adapter.get_info().device_type, wgpu::DeviceType::Cpu);
    let mut r = staged_renderer(&reference, doc.composition().color);
    assert!(r.startup.is_some());
    r.ensure_document_metadata(EXTENT, doc.scene()).unwrap();
    let mut scene = Scene::new(&r).unwrap();
    let frame = packet(doc.scene(), EXTENT);
    let chains = effect_chains(doc.scene(), EffectPreparation::Capture);
    assert!(chains.iter().any(|(chain, execution)| chain == broad && *execution == effects::Execution::Fused));
    assert!(!chains.iter().any(|(chain, _)| chain == missing));
    for (chain, execution) in chains {
        scene.effects.prepare(&r, doc.scene(), &chain, execution, 0., 0, frame.blend_space).unwrap();
    }
    scene.effects.compile();
    assert!(scene.effects.chain_ready(doc.scene(), broad, effects::Execution::Fused, frame.blend_space));
    assert!(!scene.effects.chain_ready(doc.scene(), missing, effects::Execution::Fused, frame.blend_space));
    (r, scene)
}

fn assert_pixels(r: &WgpuRasterizer, destination: &wgpu::Texture, expected: [f32; 4]) {
    let pixels = float_pixels(r, destination);
    assert_eq!(pixels.len(), (EXTENT[0] * EXTENT[1]) as usize);
    for (index, pixel) in pixels.iter().enumerate() {
        assert!(pixel.iter().zip(expected).all(|(actual, expected)| (actual - expected).abs() < 3e-5),
            "pixel {index}: {pixel:?}, expected {expected:?}");
    }
}

fn capture_after_cold_deferral(hidden_separator: bool) {
    let (doc, broad, missing) = fixture(hidden_separator);
    let (mut r, mut scene) = prepared_scene(&doc, &broad, &missing);
    let output = if hidden_separator {Output::Artwork(None)} else {Output::EffectChannels(*broad.last().unwrap())};
    let (destination, _) = create_color_target(&r.device, EXTENT, "cold fused capture");
    let mut encoder = submission::CommandEncoder::new(&r.device, &Default::default());
    let result = scene.capture_region(&mut r, packet(doc.scene(), EXTENT), &destination,
        PixelRect::full(EXTENT), output, &mut encoder);
    assert!(matches!(result, Err(GpuRasterError::DeferredObjectWork)), "{result:?}");
    assert_eq!(encoder.pass_count(), 0);
    assert_eq!(scene.record_count, 0);
    assert!(scene.jobs.is_empty());
    assert!(scene.source_jobs.is_empty());
    pollster::block_on(scene.prepare_capture_work_async(&mut r)).unwrap();
    assert!(scene.effects.chain_ready(doc.scene(), &missing, effects::Execution::Fused, Default::default()));
    scene.capture_region(&mut r, packet(doc.scene(), EXTENT), &destination,
        PixelRect::full(EXTENT), output, &mut encoder).unwrap();
    assert!(encoder.pass_count() > 0);
    r.uploads.finish(&encoder);
    encoder.submit(&r.queue);
    assert_pixels(&r, &destination, if hidden_separator {[0., 0., 0., 1.]} else {[1.; 4]});
}

#[test]
fn cold_hidden_separator_fusion_defers_before_encoding_then_capture_preparation_retries() {
    capture_after_cold_deferral(true);
}

#[test]
fn cold_effect_channels_prefix_defers_before_encoding_then_capture_preparation_retries() {
    capture_after_cold_deferral(false);
}

#[test]
fn ordered_cold_fusion_retains_all_jobs_and_source_ranges_until_prepared() {
    let (doc, broad, missing) = fixture(true);
    let (mut r, mut scene) = prepared_scene(&doc, &broad, &missing);
    let frame = packet(doc.scene(), EXTENT);
    let region = PixelRect::full(EXTENT);
    let (texture, view) = create_color_target(&r.device, EXTENT, "ordered cold fused capture");
    let destination = Image {texture: texture.clone(), view,
        plan: display_mips::Plan::window(EXTENT, 0, region)};
    let mut encoder = submission::CommandEncoder::new(&r.device, &Default::default());
    scene.prepare_region(&mut r, frame, (region, region.into()), region, false, &mut encoder).unwrap();
    let image = scene.group(&r, frame, None, [0, 0]).unwrap();
    let image = scene.converted(&r, image, Convert::linear(frame));
    scene.copy_window_tile(image, &destination, [0, 0]);
    assert!(scene.jobs.iter().any(|job| matches!(job, Job::Effect {prepared, ..} if !prepared.pipeline.ready())));
    let jobs: Vec<_> = scene.jobs.iter().map(std::mem::discriminant).collect();
    scene.source_jobs = vec![0..1, 1..scene.jobs.len()];
    let ranges = scene.source_jobs.clone();
    assert!(!scene.encode_ordered_jobs(&mut r, &mut encoder).unwrap());
    assert_eq!(scene.jobs.iter().map(std::mem::discriminant).collect::<Vec<_>>(), jobs);
    assert_eq!(scene.source_jobs, ranges);
    assert_eq!(scene.record_count, 0);
    assert_eq!(encoder.pass_count(), 0);
    pollster::block_on(scene.prepare_capture_work_async(&mut r)).unwrap();
    assert!(scene.encode_ordered_jobs(&mut r, &mut encoder).unwrap());
    assert!(scene.jobs.is_empty());
    assert!(scene.source_jobs.is_empty());
    assert!(encoder.pass_count() > 0);
    r.uploads.finish(&encoder);
    encoder.submit(&r.queue);
    assert_pixels(&r, &texture, [0., 0., 0., 1.]);
}
