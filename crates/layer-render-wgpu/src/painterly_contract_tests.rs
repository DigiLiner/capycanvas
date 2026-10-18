use super::*;
use layer_render::CanvasRenderer;

fn read(gpu: &SnapshotGpu, document: &Document) -> Vec<[f32; 4]> {
    let [w, h] = document.composition().size;
    gpu.capture_scene(document.snapshot(), SceneScope::All, Default::default()).unwrap()
        .read_region([0, 0, w, h]).unwrap()
}
fn painterly(radius: f32, strength: f32, previous: Option<&str>) -> EffectInstance {
    let mut effect = layer_core::bundled_effect_catalog().get("painterly").unwrap().preview().unwrap();
    effect.set("radius", layer_core::EffectValue::Number(radius)).unwrap();
    effect.set("strength", layer_core::EffectValue::Number(strength)).unwrap();
    painterly_variant(effect, previous)
}
fn painterly_variant(mut effect: EffectInstance, previous: Option<&str>) -> EffectInstance {
    if let Some(previous) = previous {
        let start = previous.find("fn capy_painterly(").unwrap();
        let end = previous[start..].find("\nfn capy_solarize").unwrap() + start;
        let mut program = (*effect.program).clone();
        let sources = program.wgsl.sources().unwrap().iter().map(|source| {
            let Some(a) = source.find("fn capy_painterly(") else { return source.clone(); };
            let b = source[a..].find("\nfn capy_solarize").unwrap() + a;
            Arc::<str>::from(format!("{}{}{}", &source[..a], &previous[start..end], &source[b..])).into()
        }).collect::<Vec<_>>();
        program.wgsl = layer_core::EffectShader::Linked { sources: sources.into() };
        effect.program = Arc::new(program);
    }
    effect
}
fn mosaic(extent: [u32; 2], depth: SampleDepth) -> Document {
    let mut doc = empty_document(extent, DocumentColor { space: RgbSpace::Srgb, depth });
    let source = crate::test_support::depth_source(extent, depth, RgbSpace::Srgb, 32 << 20, |x, y| {
        let [u, v] = [x as f32 / extent[0] as f32, y as f32 / extent[1] as f32];
        let alpha = if x < 2 || y < 2 { 0. } else if (x + y) % 7 == 0 { 0.25 } else { 0.85 };
        let rgb = if u < 0.25 { [0.2, 0.4, 0.7] }
            else if u < 0.5 { if y < extent[1] / 2 { [0.8, 0.1, 0.2] } else { [0.1, 0.6, 0.7] } }
            else if u < 0.75 { [u, v, 0.3 + 0.1 * ((x + y) % 2) as f32] }
            else if depth.is_float() { [-2. + u, 2. + v, 8. * v] } else { [u, v, 0.4] };
        [rgb[0], rgb[1], rgb[2], alpha]
    });
    insert_source(&mut doc, "Painterly contract", source);
    doc
}
fn sample(input: &[[f32; 4]], extent: [u32; 2], p: [f64; 2]) -> [f64; 4] {
    let p = [p[0].clamp(0., f64::from(extent[0] - 1)), p[1].clamp(0., f64::from(extent[1] - 1))];
    let low = p.map(f64::floor);
    let f = [p[0] - low[0], p[1] - low[1]];
    let mut result = [0.; 4];
    for y in 0..2 { for x in 0..2 {
        let at = [low[0] as i64 + x, low[1] as i64 + y];
        if at[0] < 0 || at[1] < 0 || at[0] >= i64::from(extent[0]) || at[1] >= i64::from(extent[1]) { continue; }
        let weight = if x == 0 { 1. - f[0] } else { f[0] } * if y == 0 { 1. - f[1] } else { f[1] };
        for (c, out) in result.iter_mut().enumerate() { *out += f64::from(input[(at[1] * i64::from(extent[0]) + at[0]) as usize][c]) * weight; }
    }}
    result
}
fn reference(input: &[[f32; 4]], extent: [u32; 2], p: [u32; 2], radius: f64, strength: f64) -> [f64; 4] {
    let center = input[(p[1] * extent[0] + p[0]) as usize].map(f64::from);
    let mut quadrants = Vec::new();
    for q in 0..4 {
        let sign = [if q & 1 == 0 { -1. } else { 1. }, if q & 2 == 0 { -1. } else { 1. }];
        let samples = (0..3).flat_map(|y| (0..3).map(move |x| sample(input, extent,
            [f64::from(p[0]) + sign[0] * f64::from(x) * radius * 0.5,
             f64::from(p[1]) + sign[1] * f64::from(y) * radius * 0.5]))).collect::<Vec<_>>();
        let total = samples.iter().map(|v| v[3]).sum::<f64>();
        if total <= 0.0001 { continue; }
        let mean: [f64; 3] = std::array::from_fn(|c| samples.iter().map(|v| v[c]).sum::<f64>() / total);
        let mut variance = 0.;
        for a in &samples { for b in &samples {
            if a[3] > 0. && b[3] > 0. {
                variance += a[3] * b[3] * (0..3).map(|c| (a[c] / a[3] - b[c] / b[3]).powi(2)).sum::<f64>();
            }
        }}
        quadrants.push((mean, variance / (2. * total * total)));
    }
    let least = quadrants.iter().map(|(_, v)| *v).fold(f64::INFINITY, f64::min);
    let weights = quadrants.iter().map(|(_, v)| ((least + 1e-6) / (v + 1e-6)).powi(4)).collect::<Vec<_>>();
    let total = weights.iter().sum::<f64>();
    let mut result = center;
    for c in 0..3 {
        let mean = if total > 0. { quadrants.iter().zip(&weights).map(|((m, _), w)| m[c] * w).sum::<f64>() / total }
            else if center[3] > 0. { center[c] / center[3] } else { 0. };
        result[c] += (mean * center[3] - center[c]) * strength;
    }
    result
}
#[test]
fn painterly_matches_pairwise_variance_with_alpha_edges_and_signed_hdr() {
    let r = WgpuRasterizer::new_native_headless(Default::default()).unwrap();
    let gpu = r.snapshot_gpu();
    let extent = [48, 40];
    for depth in [SampleDepth::U8, SampleDepth::F32] {
        let doc = mosaic(extent, depth); let input = read(&gpu, &doc);
        for radius in [1., 3.] { for strength in [0., 100.] {
            let actual = read(&gpu, &filtered(doc.clone(), painterly(radius, strength, None), None));
            for y in 0..extent[1] { for x in 0..extent[0] {
                let expected = reference(&input, extent, [x, y], f64::from(radius), f64::from(strength) / 100.);
                let pixel = actual[(y * extent[0] + x) as usize];
                assert!((f64::from(pixel[3]) - expected[3]).abs() <= 1e-6, "alpha {depth:?}/{x}/{y}");
                for c in 0..3 { assert!((f64::from(pixel[c]) - expected[c]).abs() <= 0.0002,
                    "{depth:?}/{radius}/{strength}/{x}/{y}/{c}: {} != {}", pixel[c], expected[c]); }
            }}
        }}
    }
}
#[test]
fn painterly_near_tied_quadrants_change_continuously() {
    let r = WgpuRasterizer::new_native_headless(Default::default()).unwrap();
    let gpu = r.snapshot_gpu();
    let mut outputs = Vec::new();
    for perturbation in [-0.0001, 0.0001] {
        let mut doc = empty_document([9; 2], DocumentColor { space: RgbSpace::Srgb, depth: SampleDepth::F32 });
        let source = crate::test_support::depth_source([9; 2], SampleDepth::F32, RgbSpace::Srgb, 1 << 20, |x, y| {
            let mut rgb = if x == 4 || y == 4 { [0.3; 3] }
                else { match (x < 4, y < 4) {
                    (true, true) => [0.6, 0., 0.3], (false, false) => [0., 0.6, 0.3],
                    (true, false) => [0.3, 0.6, 0.], (false, true) => [0.6, 0.3, 0.],
                }};
            if x == 3 && y == 3 { rgb[0] += perturbation; }
            [rgb[0], rgb[1], rgb[2], 0.65]
        });
        insert_source(&mut doc, "Near-tied quadrants", source);
        let input = read(&gpu, &doc);
        let expected = reference(&input, [9; 2], [4; 2], 2., 1.);
        let actual = read(&gpu, &filtered(doc, painterly(2., 100., None), None))[40];
        for c in 0..4 {
            assert!((f64::from(actual[c]) - expected[c]).abs() <= 0.0002,
                "perturbation={perturbation}, channel={c}: {} != {}", actual[c], expected[c]);
        }
        outputs.push(actual);
    }
    for c in 0..4 {
        assert!((outputs[0][c] - outputs[1][c]).abs() <= 0.0002,
            "near-tie discontinuity in channel {c}: {:?}", outputs);
    }
}
#[test]
#[ignore = "Writes old/new Painterly review images to an explicitly owned artifact directory"]
fn capture_painterly_before_and_after_review_images() {
    let out = std::path::PathBuf::from(std::env::var_os("CAPY_PAINTERLY_CAPTURE_DIR").unwrap());
    assert!(out.is_dir());
    let previous = std::fs::read_to_string(out.join("filter_library.before.wgsl")).unwrap();
    let mut r = WgpuRasterizer::new_native_headless(Default::default()).unwrap(); let gpu = r.snapshot_gpu();
    let save = |name: &str, size: [u32; 2], pixels: &[[f32; 4]]| {
        std::fs::write(out.join(format!("{name}.rgba32f")), pixels.iter().flatten().flat_map(|v| v.to_le_bytes()).collect::<Vec<_>>()).unwrap();
        let bytes = pixels.iter().flat_map(|p| {
            let a = p[3].clamp(0., 1.); let checker = 0.35;
            let rgb = std::array::from_fn::<_, 3, _>(|c| (RgbSpace::Srgb.encode(f64::from(p[c] + checker * (1. - a))).clamp(0., 1.) * 255.).round() as u8);
            [rgb[0], rgb[1], rgb[2], 255]
        }).collect::<Vec<_>>();
        let mut encoder = png::Encoder::new(std::fs::File::create(out.join(format!("{name}.png"))).unwrap(), size[0], size[1]);
        encoder.set_color(png::ColorType::Rgba); encoder.set_depth(png::BitDepth::Eight);
        encoder.write_header().unwrap().write_image_data(&bytes).unwrap();
    };
    let backing = layer_core::package::ImmutableBacking::new(Arc::new(Arc::<[u8]>::from(FIXTURE))).unwrap();
    let layer_core::package::codec::OpenOutcome::Candidate { artwork, .. } = layer_core::package::codec::open(backing,
        Default::default(), &std::sync::atomic::AtomicBool::new(false)).unwrap() else { panic!("saved fixture must reopen"); };
    let saved = Document::from_artwork(artwork).unwrap();
    let authored = cases(&saved);
    let mut timing = String::from("arm,frame,case,cpu_ms,complete_ms,gpu_ms,command_passes,cpu_phases\n");
    r.set_telemetry_enabled(true);
    for depth in [SampleDepth::U8, SampleDepth::F32] {
        let doc = mosaic([256; 2], depth); save(&format!("mosaic-{depth:?}-input"), [256; 2], &read(&gpu, &doc));
        for (label, old) in [("before", Some(previous.as_str())), ("after", None)] {
            let rendered = filtered(doc.clone(), painterly(5., 100., old), None);
            save(&format!("mosaic-{depth:?}-{label}"), [256; 2], &read(&gpu, &rendered));
            let (mut fixed, sdr) = authored[&format!("filter/painterly/{depth:?}")].clone();
            assert!(!sdr);
            let targets = fixed.artwork.effects.iter().filter_map(|(h, _, a)| (a.program.id.as_ref() == "painterly").then_some(h)).collect::<Vec<_>>();
            for target in targets {
                let application = fixed.artwork.effects.get_mut(target).unwrap();
                {
                    application.program = painterly_variant(EffectInstance {
                        program: application.program.clone(), values: application.values.clone(),
                    }, old).program;
                }
            }
            refresh(&mut fixed);
            save(&format!("fixed-{depth:?}-{label}"), EXTENT, &pixels(&gpu, &fixed, false));
            if depth == SampleDepth::F32 {
                let mut moving = rendered.clone();
                let target = moving.artwork.effects.iter().find_map(|(h, _, a)| (a.program.id.as_ref() == "painterly").then_some(h)).unwrap();
                for case in ["cold", "parameter", "cached"] {
                    for frame in 0..if case == "cold" { 1 } else { 8 } {
                        if case == "parameter" {
                            let application = moving.artwork.effects.get_mut(target).unwrap();
                            let mut effect = EffectInstance { program: application.program.clone(), values: application.values.clone() };
                            effect.set("strength", layer_core::EffectValue::Number(if frame % 2 == 0 { 80. } else { 100. })).unwrap();
                            application.values = effect.values;
                        }
                        refresh(&mut moving);
                        let before = r.metrics(); let start = std::time::Instant::now();
                        r.submit(layer_render::FramePacket { reset_layers: case == "cold", composite_all: case != "cached",
                            ..crate::test_support::packet(moving.scene(), [256; 2]) }).unwrap();
                        let cpu = start.elapsed().as_secs_f64() * 1000.;
                        r.wait_idle().unwrap(); let complete = start.elapsed().as_secs_f64() * 1000.;
                        let telemetry = r.telemetry.completed_snapshot(&r.device, &r.queue);
                        let gpu = telemetry.gpu.ordered().last().copied().unwrap_or(f32::NAN); let metrics = r.metrics();
                        timing.push_str(&format!("{label},{frame},{case},{cpu},{complete},{gpu},{},{:?}\n",
                            metrics.command_passes - before.command_passes, metrics.frame_cpu_ms));
                    }
                }
            }
        }
    }
    std::fs::write(out.join("timings.csv"), timing).unwrap();
}
