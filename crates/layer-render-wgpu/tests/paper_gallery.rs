mod support;
use layer_core::*;
use layer_engine::{InstantFeedbackConfig, ViewTransform};
use layer_render::ViewState;

const EXTENT: [u32; 2] = [1500, 1050];

struct Sample { x: f32, y: f32, pressure: f32, tilt: f32 }

fn stroke(engine: &mut support::Engine, input: &mut layer_engine::InputProducer<layer_engine::PenEvent>, time: &mut u64, samples: &[Sample]) {
    let last = samples.len() as u64 - 1;
    for (i, s) in samples.iter().enumerate() {
        *time += 4_000_000;
        let mut event = support::pen_sample(i as u64, last, *time, Point { x: s.x, y: s.y });
        event.sequence = *time;
        event.pressure = s.pressure;
        event.tilt_radians = [s.tilt, 0.];
        input.push(event).unwrap();
        if i % 4 == 0 || i as u64 == last { support::drain_input(engine, *time, true); }
    }
}

fn ramp(x0: f32, x1: f32, y: f32, p0: f32, p1: f32, tilt: f32) -> Vec<Sample> {
    let n = ((x1 - x0) / 3.) as usize;
    (0..=n).map(|i| { let t = i as f32 / n as f32; Sample { x: x0 + (x1 - x0) * t, y: y + (t * 40.).sin() * 2., pressure: p0 + (p1 - p0) * t, tilt } }).collect()
}

fn shading(x0: f32, y0: f32, w: f32, h: f32, pressure: f32, tilt: f32, pitch: f32) -> Vec<Sample> {
    let mut out = Vec::new();
    let rows = (h / pitch) as usize;
    for r in 0..=rows {
        let y = y0 + r as f32 * pitch;
        let (a, b) = if r % 2 == 0 { (x0, x0 + w) } else { (x0 + w, x0) };
        let n = (w / 4.) as usize;
        for i in 0..n { let t = i as f32 / n as f32; out.push(Sample { x: a + (b - a) * t, y: y + t * pitch, pressure, tilt }); }
    }
    out
}

fn save(path: &std::path::Path, image: &support::Image) {
    let file = std::fs::File::create(path).unwrap();
    let mut encoder = png::Encoder::new(std::io::BufWriter::new(file), image.size[0], image.size[1]);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    encoder.write_header().unwrap().write_image_data(&image.rgba).unwrap();
}

#[test]
#[ignore]
fn paper_gallery() {
    let directory = std::path::PathBuf::from(std::env::var("GALLERY_DIR").unwrap());
    let only = std::env::var("GALLERY_PAPERS").ok();
    for paper in paper::built_in_papers() {
        if only.as_deref().is_some_and(|only| !only.split(',').any(|id| id == paper.id.as_ref())) { continue; }
        let mut document = Document::new(PortableId::random(), EXTENT[0], EXTENT[1], DocumentNames { paint: "Ink".into(), paper: "Paper".into() });
        document.set_paper_texture(paper.texture.clone());
        let (mut engine, mut input) = support::engine(document);
        engine.set_view(ViewState { width_px: EXTENT[0], height_px: EXTENT[1], document_to_surface: [1., 0., 0., 1., 0., 0.] }, ViewTransform::IDENTITY);
        engine.set_instant_feedback(InstantFeedbackConfig { enabled: false, ..Default::default() }).unwrap();
        let mut time = 1_000_000_000u64;
        engine.render_frame_at(0).unwrap();
        let rows: [(DefaultBrushPreset, f32, f32); 7] = [
            (DefaultBrushPreset::Pencil, 0., 1.),
            (DefaultBrushPreset::PointyPencil, 0., 1.),
            (DefaultBrushPreset::ShadingPencil, 0.9, 1.),
            (DefaultBrushPreset::Charcoal, 0., 1.),
            (DefaultBrushPreset::Chalk, 0., 1.),
            (DefaultBrushPreset::PastelBlock, 0., 1.),
            (DefaultBrushPreset::Crayon, 0., 1.),
        ];
        for (row, (preset, tilt, size)) in rows.iter().enumerate() {
            let mut brush = default_brush(*preset);
            brush.diameter *= size;
            engine.set_brush(brush).unwrap();
            let y = 60. + row as f32 * 140.;
            stroke(&mut engine, &mut input, &mut time, &ramp(40., 700., y, 0.05, 1., *tilt));
            for (column, pressure) in [0.2f32, 0.45, 0.8].iter().enumerate() {
                let x = 760. + column as f32 * 245.;
                stroke(&mut engine, &mut input, &mut time, &shading(x, y - 40., 200., 90., *pressure, *tilt, if *preset == DefaultBrushPreset::Pencil || *preset == DefaultBrushPreset::PointyPencil { 3. } else { 10. }));
            }
        }
        let image = support::image(&mut engine, time + 100_000_000);
        save(&directory.join(format!("{}.png", paper.id)), &image);
    }
}
