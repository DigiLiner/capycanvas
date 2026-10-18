use super::*;
use crate::test_support::{packet, float_pixels, max_error};
use layer_core::authored::vector::*;
use layer_core::color::RgbColor;
use crate::scene::Output;

fn paint(rgba: [f32; 4]) -> Paint { Paint::color(RgbColor::new(Default::default(), rgba).unwrap()) }
fn stroke(id: u64, points: &[[f64; 2]], width: f64, rgba: [f32; 4]) -> Record {
    let anchors: Vec<_> = points.iter().map(|p| Anchor::corner(*p)).collect();
    let mut stroke = Stroke::new(paint(rgba), width);
    stroke.cap = Cap::Round; stroke.join = Join::Round;
    Record::Path(PathItem::new(ItemId(id), Arc::new(PathGeometry::new(vec![Subpath::new(&anchors, false)]).unwrap()), Arc::new(Style::new(vec![PaintEntry::Stroke(stroke)]))))
}
fn shape(id: u64, rect: [f64; 4], rgba: [f32; 4]) -> Record {
    let [x0, y0, x1, y1] = rect;
    let anchors = [[x0, y0], [x1, y0], [x1, y1], [x0, y1]].map(Anchor::corner);
    Record::Path(PathItem::new(ItemId(id), Arc::new(PathGeometry::new(vec![Subpath::new(&anchors, true)]).unwrap()),
        Arc::new(Style::new(vec![PaintEntry::Fill(Fill { paint: paint(rgba), unknown: None })]))))
}
fn document(extent: [u32; 2], records: Vec<Record>) -> (layer_core::Editor, OccurrenceHandle) {
    let artwork = layer_core::authored::Artwork::new(extent).unwrap();
    let mut editor = layer_core::Editor::new(layer_core::Document::from_artwork(artwork).unwrap());
    let (layer, edit) = editor.document().create_vector_layer_edit("Ink", VectorObject::from_records(records), None, 0).unwrap();
    editor.perform(edit).unwrap();
    (editor, layer)
}
fn records() -> Vec<Record> {
    let mut list: Vec<Record> = (0..40).map(|i| stroke(i + 1, &[[10. + i as f64 * 7., 20.], [60. + i as f64 * 5., 150.], [20. + i as f64 * 6., 190.]], 2. + (i % 4) as f64, [0.05, 0.1, 0.3, 1.])).collect();
    list.push(shape(100, [40., 60., 260., 120.], [0.9, 0.5, 0.2, 0.8]));
    list
}
fn submit_ready(r: &mut WgpuRasterizer, frame: FramePacket<'_>) {
    r.submit(frame).unwrap();
    for _ in 0..200 {
        r.wait_idle().unwrap();
        if !r.has_pending_work() { return; }
        r.submit(FramePacket { composite_all: false, reset_layers: false, ..frame }).unwrap();
    }
    panic!("vector display must settle");
}
fn native(r: &mut WgpuRasterizer, doc: &layer_core::Document) -> Vec<[f32; 4]> {
    let extent = doc.composition().size;
    let (texture, _) = create_color_target(&r.device, extent, "vector native reference");
    let mut scene = r.scene.take().unwrap_or_else(|| Scene::new(r));
    let mut encoder = crate::submission::CommandEncoder::new(&r.device, &Default::default());
    scene.capture_region_prepared(r, packet(doc.scene(), extent), &texture, PixelRect::full(extent), Output::Artwork(None), &mut encoder).unwrap();
    r.uploads.finish(&encoder); encoder.submit(&r.queue);
    r.scene = Some(scene);
    float_pixels(r, &texture)
}
fn display(r: &WgpuRasterizer) -> Vec<[f32; 4]> { float_pixels(r, r.scale_display.as_ref().unwrap().texture()) }

#[test]
fn vector_layers_render_the_same_through_display_and_native_paths() {
    let (editor, _) = document([300, 200], records());
    let doc = editor.document();
    let mut r = WgpuRasterizer::new_native_headless(doc.composition().color).unwrap();
    submit_ready(&mut r, packet(doc.scene(), doc.composition().size));
    let shown = display(&r);
    let exact = native(&mut r, doc);
    let covered = exact.iter().filter(|p| p[3] > 0.5).count();
    eprintln!("covered {covered}, display vs native {}", max_error(&shown, &exact));
    assert!(covered > 5000, "vector content must be visible");
    assert!(max_error(&shown, &exact) < 1e-5);
}

#[test]
fn vector_edits_damage_only_changed_items_and_match_a_fresh_render() {
    let (mut editor, layer) = document([300, 200], records());
    let mut r = WgpuRasterizer::new_native_headless(editor.document().composition().color).unwrap();
    submit_ready(&mut r, packet(editor.document().scene(), editor.document().composition().size));
    let before = editor.document().clone();
    let object = editor.document().scene().vector_layer(layer).unwrap().clone();
    let drawn = object.insert_front(vec![stroke(500, &[[250., 10.], [290., 40.]], 4., [1., 0., 0., 1.])]);
    editor.perform(editor.document().vector_object_edit(layer, drawn).unwrap()).unwrap();
    let damage = objects::edited_damage(before.scene(), editor.document().scene(), [300, 200]).expect("vector edits must not composite everything");
    let area: u64 = damage.regions.iter().map(|r| u64::from(r.width()) * u64::from(r.height())).sum();
    eprintln!("drawing damage {area} px over {:?}", damage.regions);
    assert!(area < 300 * 200 / 4, "a new stroke damages only its own bounds");
    submit_ready(&mut r, FramePacket { composite_all: true, ..packet(editor.document().scene(), [300, 200]) });
    let incremental = display(&r);
    let mut fresh = WgpuRasterizer::new_native_headless(editor.document().composition().color).unwrap();
    submit_ready(&mut fresh, packet(editor.document().scene(), [300, 200]));
    assert!(max_error(&incremental, &display(&fresh)) < 1e-5);
    let target = editor.document().scene().vector_layer(layer).unwrap().find(ItemId(7)).unwrap();
    let recolored = editor.document().scene().vector_layer(layer).unwrap().replace([(target, Some(stroke(7, &[[59., 20.], [95., 150.], [62., 190.]], 5., [0., 0.8, 0., 1.])))]);
    let before = editor.document().clone();
    editor.perform(editor.document().vector_object_edit(layer, recolored).unwrap()).unwrap();
    assert!(objects::edited_damage(before.scene(), editor.document().scene(), [300, 200]).is_some());
    submit_ready(&mut r, FramePacket { composite_all: true, ..packet(editor.document().scene(), [300, 200]) });
    let mut fresh = WgpuRasterizer::new_native_headless(editor.document().composition().color).unwrap();
    submit_ready(&mut fresh, packet(editor.document().scene(), [300, 200]));
    assert!(max_error(&display(&r), &display(&fresh)) < 1e-5);
}

#[test]
fn provisional_strokes_draw_over_the_cached_layer_and_hand_off_without_change() {
    use layer_render::CanvasRenderer;
    let (mut editor, layer) = document([300, 200], records());
    let mut r = WgpuRasterizer::new_native_headless(editor.document().composition().color).unwrap();
    submit_ready(&mut r, packet(editor.document().scene(), [300, 200]));
    let before = display(&r);
    let Record::Path(path) = stroke(900, &[[30., 30.], [150., 90.], [280., 40.]], 6., [0.9, 0.1, 0.1, 1.]) else { unreachable!() };
    let path = Arc::new(path);
    r.set_vector_preview(Some(layer_render::VectorPreview { owner: layer, path: path.clone(), revision: 1 }));
    submit_ready(&mut r, FramePacket { composite_all: false, ..packet(editor.document().scene(), [300, 200]) });
    let drawn = display(&r);
    let changed = drawn.iter().zip(&before).filter(|(a, b)| (a[0] - b[0]).abs() > 0.01).count();
    eprintln!("preview changed {changed} pixels");
    assert!(changed > 500, "the provisional stroke must show");
    let object = editor.document().scene().vector_layer(layer).unwrap().insert_front(vec![Record::Path((*path).clone())]);
    editor.perform(editor.document().vector_object_edit(layer, object).unwrap()).unwrap();
    r.set_vector_preview(None);
    submit_ready(&mut r, FramePacket { composite_all: true, ..packet(editor.document().scene(), [300, 200]) });
    assert!(max_error(&display(&r), &drawn) < 1e-5, "committing the stroke must look the same as its preview");
}

fn timed(r: &mut WgpuRasterizer, frame: FramePacket<'_>) -> (f64, f64) {
    let start = std::time::Instant::now();
    r.submit(frame).unwrap();
    let cpu = start.elapsed().as_secs_f64() * 1e3;
    r.wait_idle().unwrap();
    (cpu, start.elapsed().as_secs_f64() * 1e3)
}
fn percentile(values: &mut [f64], p: f64) -> f64 { values.sort_by(f64::total_cmp); values[((values.len() - 1) as f64 * p) as usize] }

#[test]
#[ignore = "crowded vector layer drawing benchmark"]
fn crowded_vector_layer_drawing_frames() {
    use layer_render::CanvasRenderer;
    let extent = [4000u32, 3000u32];
    let zoom: f32 = std::env::var("VECTOR_ZOOM").ok().and_then(|v| v.parse().ok()).unwrap_or(0.4);
    let view = layer_render::ViewState { width_px: 1920, height_px: 1200, document_to_surface: [zoom, 0., 0., zoom, 160. - 1200. * (zoom - 0.4).max(0.), -1300. * (zoom - 0.4).max(0.)] };
    for count in [10_000u64, 50_000] {
        let records: Vec<Record> = crate::vector::bench::scene_b([4000., 3000.], count).into_iter().map(Record::Path).collect();
        let (mut editor, layer) = document(extent, records);
        let mut r = WgpuRasterizer::new_native_headless(editor.document().composition().color).unwrap();
        let start = std::time::Instant::now();
        submit_ready(&mut r, FramePacket { view, ..packet(editor.document().scene(), extent) });
        let open = start.elapsed().as_secs_f64() * 1e3;
        let initial = display(&r);
        let mut frames = Vec::new();
        let mut points = Vec::new();
        let loop_start = std::time::Instant::now();
        for i in 0..120u64 {
            points.push([1200. + i as f64 * 9., 1400. + (i as f64 * 0.1).sin() * 300.]);
            let anchors: Vec<_> = points.iter().map(|p| Anchor::corner(*p)).collect();
            let mut stroke = Stroke::new(paint([0.8, 0.1, 0.1, 1.]), 6.);
            stroke.cap = Cap::Round; stroke.join = Join::Round;
            let path = PathItem::new(ItemId(u64::MAX - 1), Arc::new(PathGeometry::new(vec![Subpath::new(&anchors, false)]).unwrap()), Arc::new(Style::new(vec![PaintEntry::Stroke(stroke)])));
            r.set_vector_preview(Some(layer_render::VectorPreview { owner: layer, path: Arc::new(path), revision: i + 1 }));
            frames.push(timed(&mut r, FramePacket { view, composite_all: false, ..packet(editor.document().scene(), extent) }));
            let mut extra = 0;
            while r.has_pending_work() && extra < 50 { r.submit(FramePacket { view, composite_all: false, ..packet(editor.document().scene(), extent) }).unwrap(); r.wait_idle().unwrap(); extra += 1; }
            assert!(extra < 50, "drawing frames settle");
        }
        eprintln!("120 drawing frames took {:.1} ms; changed {} pixels", loop_start.elapsed().as_secs_f64() * 1e3, display(&r).iter().zip(&initial).filter(|(a, b)| (a[0] - b[0]).abs() > 0.05).count());
        let Some(layer_render::VectorPreview { path, .. }) = r.vector_preview.clone() else { unreachable!() };
        let object = editor.document().scene().vector_layer(layer).unwrap().insert_front(vec![Record::Path(PathItem { id: ItemId(u64::MAX - 2), ..(*path).clone() })]);
        let start = std::time::Instant::now();
        editor.perform(editor.document().vector_object_edit(layer, object).unwrap()).unwrap();
        let perform = start.elapsed().as_secs_f64() * 1e3;
        r.set_vector_preview(None);
        let commit = timed(&mut r, FramePacket { view, composite_all: true, ..packet(editor.document().scene(), extent) });
        let start = std::time::Instant::now();
        editor.undo().unwrap();
        let undo_perform = start.elapsed().as_secs_f64() * 1e3;
        let undo = timed(&mut r, FramePacket { view, composite_all: true, ..packet(editor.document().scene(), extent) });
        let mut small = Vec::new();
        for k in 0..5u64 {
            let object = editor.document().scene().vector_layer(layer).unwrap().insert_front(vec![stroke(70_000 + k, &[[1500. + k as f64 * 40., 1500.], [1510. + k as f64 * 40., 1505.]], 4., [0.2, 0.7, 0.2, 1.])]);
            editor.perform(editor.document().vector_object_edit(layer, object).unwrap()).unwrap();
            small.push(timed(&mut r, FramePacket { view, composite_all: true, ..packet(editor.document().scene(), extent) }).1);
        }
        eprintln!("small commits wall {:?}", small.iter().map(|v| format!("{v:.2}")).collect::<Vec<_>>());
        let (mut cpu, mut wall): (Vec<f64>, Vec<f64>) = frames.into_iter().unzip();
        eprintln!("{count} strokes: open {open:.0} ms; drawing frame cpu p50 {:.2} p95 {:.2} ms, wall p50 {:.2} p95 {:.2} ms; commit perform {perform:.2} ms frame cpu {:.2} wall {:.2} ms; undo perform {undo_perform:.2} ms frame cpu {:.2} wall {:.2} ms",
            percentile(&mut cpu, 0.5), percentile(&mut cpu, 0.95), percentile(&mut wall, 0.5), percentile(&mut wall, 0.95), commit.0, commit.1, undo.0, undo.1);
    }
}

#[test]
fn appended_strokes_at_zoomed_out_levels_match_a_fresh_render() {
    let extent = [1200u32, 900u32];
    let view = layer_render::ViewState { width_px: 600, height_px: 450, document_to_surface: [0.5, 0., 0., 0.5, 0., 0.] };
    let records: Vec<Record> = crate::vector::bench::scene_b([1200., 900.], 2000).into_iter().map(Record::Path).collect();
    let (mut editor, layer) = document(extent, records);
    let mut r = WgpuRasterizer::new_native_headless(editor.document().composition().color).unwrap();
    submit_ready(&mut r, FramePacket { view, ..packet(editor.document().scene(), extent) });
    for (i, id) in [9001u64, 9002, 9003].into_iter().enumerate() {
        let y = 200. + i as f64 * 180.;
        let object = editor.document().scene().vector_layer(layer).unwrap().insert_front(vec![stroke(id, &[[100., y], [600., y + 90.], [1100., y]], 7., [0.9, 0.2, 0.1, 0.7])]);
        editor.perform(editor.document().vector_object_edit(layer, object).unwrap()).unwrap();
        submit_ready(&mut r, FramePacket { view, composite_all: true, ..packet(editor.document().scene(), extent) });
    }
    let mut fresh = WgpuRasterizer::new_native_headless(editor.document().composition().color).unwrap();
    submit_ready(&mut fresh, FramePacket { view, ..packet(editor.document().scene(), extent) });
    let (a, b) = (display(&r), display(&fresh));
    let error = max_error(&a, &b);
    let width = r.scale_display.as_ref().unwrap().texture().width() as usize;
    let bad: Vec<usize> = a.iter().zip(&b).enumerate().filter(|(_, (x, y))| (0..4).any(|k| (x[k] - y[k]).abs() > 1e-3)).map(|(i, _)| i).collect();
    if !bad.is_empty() {
        let (xs, ys): (Vec<usize>, Vec<usize>) = bad.iter().map(|i| (i % width, i / width)).unzip();
        eprintln!("differing {} texels in x {}..{} y {}..{} (texture width {width})", bad.len(), xs.iter().min().unwrap(), xs.iter().max().unwrap(), ys.iter().min().unwrap(), ys.iter().max().unwrap());
    }
    assert!(error < 1e-5, "appended strokes must match a full render: {error}");
}
