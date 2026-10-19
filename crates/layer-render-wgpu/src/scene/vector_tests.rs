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
    let mut scene = r.scene.take().unwrap_or_else(|| Scene::new(r).unwrap());
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
fn raw_object_captures_of_vector_layers_match_their_render() {
    let (editor, layer) = document([300, 200], records());
    let doc = editor.document();
    let mut r = WgpuRasterizer::new_native_headless(doc.composition().color).unwrap();
    let exact = native(&mut r, doc);
    let (texture, _) = create_color_target(&r.device, [300, 200], "vector raw capture");
    let mut scene = r.scene.take().unwrap_or_else(|| Scene::new(&r).unwrap());
    let scope = layer_core::SceneScope::RawObjects(layer);
    let mut encoder = crate::submission::CommandEncoder::new(&r.device, &Default::default());
    scene.capture_region_prepared(&mut r, FramePacket { scene: doc.scene().with_scope(&scope), ..packet(doc.scene(), [300, 200]) }, &texture, PixelRect::full([300, 200]), Output::Objects(layer), &mut encoder).unwrap();
    r.uploads.finish(&encoder); encoder.submit(&r.queue);
    r.scene = Some(scene);
    let raw = float_pixels(&r, &texture);
    assert!(raw.iter().filter(|p| p[3] > 0.5).count() > 5000);
    assert!(max_error(&raw, &exact) < 1e-5, "Rasterize Layer bakes what the layer shows: {}", max_error(&raw, &exact));
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
        let mut opening = vec![timed(&mut r, FramePacket { view, ..packet(editor.document().scene(), extent) }).1];
        while r.has_pending_work() { opening.push(timed(&mut r, FramePacket { view, composite_all: false, ..packet(editor.document().scene(), extent) }).1); }
        let open = start.elapsed().as_secs_f64() * 1e3;
        eprintln!("open over {} frames, longest wall {:.1} ms", opening.len(), opening.iter().copied().fold(0., f64::max));
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
        if count == 50_000 { unsafe { std::env::set_var("VECTOR_TRACE", "1") }; }
        let undo = timed(&mut r, FramePacket { view, composite_all: true, ..packet(editor.document().scene(), extent) });
        unsafe { std::env::remove_var("VECTOR_TRACE") };
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
fn crowded_layers_derive_across_frames_and_settle_to_the_exact_render() {
    let extent = [2000u32, 1500u32];
    let records: Vec<Record> = crate::vector::bench::scene_b([2000., 1500.], 20_000).into_iter().map(Record::Path).collect();
    let (editor, _) = document(extent, records);
    let doc = editor.document();
    let mut r = WgpuRasterizer::new_native_headless(doc.composition().color).unwrap();
    r.submit(packet(doc.scene(), extent)).unwrap();
    r.wait_idle().unwrap();
    let derived = |r: &WgpuRasterizer| r.scene.as_ref().unwrap().vectors.layers.values().flat_map(|layer| layer.items.iter().flatten()).filter(|item| !item.derived.is_empty()).count();
    let first = derived(&r);
    let opened = r.metrics.composited_pixels;
    assert!(r.has_pending_work() && first < 20_000, "one frame derived {first} of 20000 paths");
    submit_ready(&mut r, FramePacket { composite_all: false, ..packet(doc.scene(), extent) });
    let settled = r.metrics.composited_pixels - opened;
    assert!(settled <= opened + opened / 2, "frames that only derive composite nothing until the window is complete: {settled} after {opened}");
    let shown = display(&r);
    let exact = native(&mut r, doc);
    assert!(max_error(&shown, &exact) < 1e-5, "the settled display must match the exact render: {}", max_error(&shown, &exact));
}

#[test]
fn moved_and_removed_items_repaint_only_damaged_tiles_and_match_a_fresh_render() {
    use std::collections::BTreeSet;
    let extent = [1200u32, 900u32];
    for zoom in [1f32, 0.5] {
        let view = layer_render::ViewState { width_px: 600, height_px: 450, document_to_surface: [zoom, 0., 0., zoom, 0., 0.] };
        let records: Vec<Record> = crate::vector::bench::scene_b([1200., 900.], 2000).into_iter().map(Record::Path).collect();
        let (mut editor, layer) = document(extent, records);
        let mut r = WgpuRasterizer::new_native_headless(editor.document().composition().color).unwrap();
        submit_ready(&mut r, FramePacket { view, ..packet(editor.document().scene(), extent) });
        let matches = |r: &WgpuRasterizer, editor: &layer_core::Editor, step: &str| {
            let mut fresh = WgpuRasterizer::new_native_headless(editor.document().composition().color).unwrap();
            submit_ready(&mut fresh, FramePacket { view, ..packet(editor.document().scene(), extent) });
            let error = max_error(&display(r), &display(&fresh));
            assert!(error < 1e-5, "{step} at zoom {zoom} must match a fresh render: {error}");
        };
        let object = editor.document().scene().vector_layer(layer).unwrap().clone();
        let moving: BTreeSet<ItemId> = object.paths().step_by(97).map(|(_, p)| p.id).collect();
        let moved = object.transform_items(&moving, layer_core::authored::Affine64([1., 0., 0., 1., 37., -21.]), true).unwrap();
        editor.perform(editor.document().vector_object_edit(layer, moved).unwrap()).unwrap();
        submit_ready(&mut r, FramePacket { view, composite_all: true, ..packet(editor.document().scene(), extent) });
        matches(&r, &editor, "moving items");
        let object = editor.document().scene().vector_layer(layer).unwrap().clone();
        let removed = object.replace(object.paths().step_by(131).map(|(position, _)| (position, None)).collect::<Vec<_>>());
        editor.perform(editor.document().vector_object_edit(layer, removed).unwrap()).unwrap();
        submit_ready(&mut r, FramePacket { view, composite_all: true, ..packet(editor.document().scene(), extent) });
        matches(&r, &editor, "removing items");
        let object = editor.document().scene().vector_layer(layer).unwrap().insert_front(vec![stroke(9100, &[[100., 300.], [1100., 500.]], 9., [0.9, 0.2, 0.1, 0.5])]);
        editor.perform(editor.document().vector_object_edit(layer, object).unwrap()).unwrap();
        submit_ready(&mut r, FramePacket { view, composite_all: true, ..packet(editor.document().scene(), extent) });
        submit_ready(&mut r, FramePacket { view, composite_all: true, ..packet(editor.document().scene(), extent) });
        matches(&r, &editor, "an append followed by an unchanged frame");
    }
}

fn derivation_work(r: &WgpuRasterizer, since: u64, known: &mut HashSet<usize>) -> (usize, usize) {
    let (mut derived, mut batched) = (0, 0);
    for layer in r.scene.as_ref().unwrap().vectors.layers.values() {
        for item in layer.items.iter().flatten() {
            batched += usize::from(item.derived.iter().any(|d| d.used > since));
            derived += item.derived.iter().filter(|d| known.insert(Arc::as_ptr(&d.geometry) as usize)).count();
        }
    }
    (derived, batched)
}

#[test]
fn frame_work_after_small_edits_and_drags_stays_flat_from_ten_to_fifty_thousand_paths() {
    use std::collections::BTreeSet;
    let view = layer_render::ViewState { width_px: 800, height_px: 600, document_to_surface: [1., 0., 0., 1., 0., 0.] };
    let rows: Vec<Vec<(&str, usize, usize)>> = [10_000u64, 50_000].into_iter().map(|count| {
        let columns = (count as f64).sqrt().ceil() as u64;
        let at = |i: u64| [(i % columns) as f64 * 12. + 4., (i / columns) as f64 * 12. + 4.];
        let records = (0..count).map(|i| { let [x, y] = at(i); stroke(i + 1, &[[x, y], [x + 7., y + 9.]], 2., [0.1, 0.1, 0.1, 1.]) }).collect();
        let side = (columns * 12 + 16) as u32;
        let extent = [side, side];
        let (mut editor, layer) = document(extent, records);
        let mut r = WgpuRasterizer::new_native_headless(editor.document().composition().color).unwrap();
        submit_ready(&mut r, FramePacket { view, ..packet(editor.document().scene(), extent) });
        let mut known = HashSet::new();
        derivation_work(&r, 0, &mut known);
        let mut rows = Vec::new();
        let mut step = |r: &mut WgpuRasterizer, editor: &layer_core::Editor, name: &'static str| {
            let since = r.scene.as_ref().unwrap().vectors.frame;
            submit_ready(r, FramePacket { view, composite_all: false, ..packet(editor.document().scene(), extent) });
            let (derived, batched) = derivation_work(r, since, &mut known);
            rows.push((name, derived, batched));
        };
        step(&mut r, &editor, "idle frame");
        let object = editor.document().scene().vector_layer(layer).unwrap().clone();
        let target = ItemId(5 * columns + 6);
        let position = object.find(target).unwrap();
        let Some(Record::Path(path)) = object.record(position).cloned() else { unreachable!() };
        let mut ink = Stroke::new(paint([0.9, 0.1, 0.1, 1.]), 2.);
        (ink.cap, ink.join) = (Cap::Round, Join::Round);
        let recolored = path.with_style(Arc::new(Style::new(vec![PaintEntry::Stroke(ink)])));
        editor.perform(editor.document().vector_object_edit(layer, object.replace([(position, Some(Record::Path(recolored)))])).unwrap()).unwrap();
        step(&mut r, &editor, "recolor one path");
        editor.undo().unwrap();
        step(&mut r, &editor, "undo");
        editor.redo().unwrap();
        step(&mut r, &editor, "redo");
        let near: Vec<ItemId> = (0..count).filter(|i| { let [x, y] = at(*i); x < 400. && y < 300. }).map(|i| ItemId(i + 1)).collect();
        for (name, moving) in [("drag one path", vec![target]), ("drag 500 paths", near[..500].to_vec())] {
            let items: Arc<BTreeSet<ItemId>> = Arc::new(moving.into_iter().collect());
            for offset in [[3., 4.], [9., 7.]] {
                r.set_vector_motion(Some(layer_render::VectorMotion { owner: layer, items: items.clone(), offset }));
                step(&mut r, &editor, name);
            }
            r.set_vector_motion(None);
            step(&mut r, &editor, "drag cancelled");
        }
        eprintln!("{count} paths: {rows:?}");
        rows
    }).collect();
    assert_eq!(rows[0], rows[1], "frame work must not grow with paths outside the view");
}

#[test]
fn frames_over_the_derived_budget_rescan_only_after_growth_or_a_stale_level() {
    let (mut editor, layer) = document([300, 200], records());
    let mut r = WgpuRasterizer::new_native_headless(editor.document().composition().color).unwrap();
    let at = |zoom: f32, editor: &layer_core::Editor, r: &mut WgpuRasterizer| submit_ready(r, FramePacket {
        view: layer_render::ViewState { width_px: 300, height_px: 200, document_to_surface: [zoom, 0., 0., zoom, 0., 0.] }, composite_all: false, ..packet(editor.document().scene(), [300, 200]) });
    let entries = |r: &WgpuRasterizer, level: u32| -> usize { r.scene.as_ref().unwrap().vectors.layers.values().flat_map(|layer| layer.items.iter().flatten()).map(|item| item.derived.iter().filter(|d| d.level == level).count()).sum() };
    at(1., &editor, &mut r);
    let drawn = entries(&r, 0);
    for layer in r.scene.as_mut().unwrap().vectors.layers.values_mut() { layer.derived_bytes += DERIVED_BYTES; }
    let draw = |i: u64, zoom: f32, editor: &mut layer_core::Editor, r: &mut WgpuRasterizer| {
        let object = editor.document().scene().vector_layer(layer).unwrap().insert_front(vec![stroke(1000 + i, &[[10. + i as f64 * 9., 5.], [14. + i as f64 * 9., 9.]], 2., [0.9, 0.1, 0.1, 1.])]);
        editor.perform(editor.document().vector_object_edit(layer, object).unwrap()).unwrap();
        at(zoom, editor, r);
    };
    for i in 0..4 { draw(i, 1., &mut editor, &mut r); }
    assert_eq!(entries(&r, 0), drawn + 4, "frames that derive little keep the unused entries of the level in use");
    for i in 4..8 { draw(i, 0.5, &mut editor, &mut r); }
    assert!(entries(&r, 1) > 0);
    assert_eq!(entries(&r, 0), 0, "a level no longer drawn is evicted");
}

#[test]
fn removing_or_dragging_a_lone_path_clears_tiles_no_other_path_covers() {
    use std::collections::BTreeSet;
    let records = vec![stroke(1, &[[40., 40.], [120., 60.]], 6., [0.9, 0.2, 0.1, 1.]), stroke(2, &[[200., 150.], [280., 170.]], 6., [0.1, 0.2, 0.9, 1.])];
    let (mut editor, layer) = document([300, 200], records);
    let mut r = WgpuRasterizer::new_native_headless(editor.document().composition().color).unwrap();
    submit_ready(&mut r, packet(editor.document().scene(), [300, 200]));
    let fresh = |editor: &layer_core::Editor| {
        let mut fresh = WgpuRasterizer::new_native_headless(editor.document().composition().color).unwrap();
        submit_ready(&mut fresh, packet(editor.document().scene(), [300, 200]));
        display(&fresh)
    };
    let items: Arc<BTreeSet<ItemId>> = Arc::new([ItemId(1)].into());
    r.set_vector_motion(Some(layer_render::VectorMotion { owner: layer, items: items.clone(), offset: [30., 20.] }));
    submit_ready(&mut r, FramePacket { composite_all: false, ..packet(editor.document().scene(), [300, 200]) });
    r.set_vector_motion(None);
    submit_ready(&mut r, FramePacket { composite_all: false, ..packet(editor.document().scene(), [300, 200]) });
    assert!(max_error(&display(&r), &fresh(&editor)) < 1e-5, "a cancelled drag restores the lone path");
    let object = editor.document().scene().vector_layer(layer).unwrap().clone();
    editor.perform(editor.document().vector_object_edit(layer, object.replace([(object.find(ItemId(1)).unwrap(), None)])).unwrap()).unwrap();
    submit_ready(&mut r, FramePacket { composite_all: true, ..packet(editor.document().scene(), [300, 200]) });
    assert!(max_error(&display(&r), &fresh(&editor)) < 1e-5, "removing the lone path clears its tiles");
}

#[test]
fn vector_layer_thumbnails_show_their_paths() {
    let (editor, layer) = document([2048, 1536], vec![stroke(1, &[[200., 700.], [1800., 760.]], 24., [0.2, 0.6, 0.2, 1.])]);
    let mut r = WgpuRasterizer::new_native_headless(editor.document().composition().color).unwrap();
    submit_ready(&mut r, packet(editor.document().scene(), [2048, 1536]));
    let inked = |r: &mut WgpuRasterizer| {
        for _ in 0..50 { if r.prepare_thumbnail_batch(layer_render::ThumbnailTarget::Occurrence(layer)).unwrap() { break; } crate::test_support::complete(r); }
        r.request_thumbnail(1, layer_render::ThumbnailTarget::Occurrence(layer)).unwrap();
        crate::test_support::complete(r);
        r.take_thumbnail().unwrap().unwrap().bytes.chunks_exact(4).filter(|p| p[3] > 128).count()
    };
    let shown = inked(&mut r);
    assert!(shown > 20, "the thumbnail shows the line: {shown} pixels");
    r.set_vector_motion(Some(layer_render::VectorMotion { owner: layer, items: Arc::new([ItemId(1)].into()), offset: [40., 30.] }));
    submit_ready(&mut r, FramePacket { composite_all: false, ..packet(editor.document().scene(), [2048, 1536]) });
    assert_eq!(inked(&mut r), shown, "a drag in progress keeps its paths in the thumbnail");
    let mut editor = editor;
    let moved = editor.document().scene().vector_layer(layer).unwrap().transform_items(&[ItemId(1)].into(), layer_core::authored::Affine64([1., 0., 0., 1., 40., 30.]), true).unwrap();
    editor.perform(editor.document().vector_object_edit(layer, moved).unwrap()).unwrap();
    r.set_vector_motion(None);
    r.submit(FramePacket { composite_all: true, ..packet(editor.document().scene(), [2048, 1536]) }).unwrap();
    assert!(inked(&mut r) > 20, "the thumbnail after the move shows the line");
}

fn presented(r: &WgpuRasterizer, view: layer_render::ViewState) -> Vec<u8> {
    let (texture, target) = crate::create_target(r.device(), [view.width_px, view.height_px], wgpu::TextureFormat::Rgba8UnormSrgb, "vector motion presentation");
    let mut presenter = crate::present::ViewportPresenter::for_surface(r, wgpu::TextureFormat::Rgba8UnormSrgb, crate::SdrSurfaceColor::Srgb).unwrap();
    presenter.present(r, &target, view, [1.; 4]).unwrap();
    crate::layer_tests::page_bytes(r, &texture)
}
#[test]
fn moving_paths_show_at_their_offset_under_other_layers_and_hand_off_to_the_committed_move() {
    use std::collections::BTreeSet;
    let extent = [1200u32, 900u32];
    for (zoom, blend, opacity) in [(1f32, layer_core::LayerBlend::Normal, 1f32), (0.5, layer_core::LayerBlend::Normal, 1.), (1., layer_core::LayerBlend::Multiply, 1.), (0.5, layer_core::LayerBlend::Normal, 0.8)] {
        let view = layer_render::ViewState { width_px: 600, height_px: 450, document_to_surface: [zoom, 0., 0., zoom, 0., 0.] };
        let mut records: Vec<Record> = (0..6).map(|i| stroke(9000 + i, &[[200. + i as f64 * 60., 300.], [400. + i as f64 * 50., 500.]], 9., [0.9, 0.2, 0.1, 0.6])).collect();
        records.extend(crate::vector::bench::scene_b([1200., 900.], 1500).into_iter().map(Record::Path));
        let (mut editor, layer) = document(extent, records);
        let mut occurrence = editor.document().scene().occurrence(layer).unwrap().clone();
        occurrence.opacity = opacity;
        editor.perform(layer_core::Edit::Occurrence(layer_core::RecordChange::replace(&editor.document().artwork.occurrences, layer, Some(occurrence)).unwrap())).unwrap();
        let cover = VectorObject::from_records(vec![shape(1, [150., 350., 700., 520.], [0.1, 0.3, 0.9, 0.5])]);
        let (upper, edit) = editor.document().create_vector_layer_edit("Above", cover, None, 0).unwrap();
        editor.perform(edit).unwrap();
        let mut occurrence = editor.document().scene().occurrence(upper).unwrap().clone();
        occurrence.blend = blend;
        editor.perform(layer_core::Edit::Occurrence(layer_core::RecordChange::replace(&editor.document().artwork.occurrences, upper, Some(occurrence)).unwrap())).unwrap();
        let split = blend == layer_core::LayerBlend::Normal && opacity == 1.;
        let mut r = WgpuRasterizer::new_native_headless(editor.document().composition().color).unwrap();
        submit_ready(&mut r, FramePacket { view, ..packet(editor.document().scene(), extent) });
        let shown = |r: &WgpuRasterizer| if split { presented(r, view).into_iter().map(f32::from).collect::<Vec<_>>() } else { display(r).into_iter().flatten().collect() };
        let tolerance = if split { 1.5 } else { 1e-5 };
        let close = |a: &[f32], b: &[f32]| a.iter().zip(b).map(|(x, y)| (x - y).abs()).fold(0., f32::max);
        let original = shown(&r);
        let items: Arc<BTreeSet<ItemId>> = Arc::new((9000..9006).map(ItemId).collect());
        let moved_object = editor.document().scene().vector_layer(layer).unwrap().transform_items(&items, layer_core::authored::Affine64([1., 0., 0., 1., 38., -22.]), true).unwrap();
        let mut moved = layer_core::Editor::new(editor.document().clone());
        moved.perform(moved.document().vector_object_edit(layer, moved_object.clone()).unwrap()).unwrap();
        let mut fresh = WgpuRasterizer::new_native_headless(moved.document().composition().color).unwrap();
        submit_ready(&mut fresh, FramePacket { view, ..packet(moved.document().scene(), extent) });
        let expected = shown(&fresh);
        for offset in [[10., 6.], [24., -12.], [38., -22.]] {
            r.set_vector_motion(Some(layer_render::VectorMotion { owner: layer, items: items.clone(), offset }));
            submit_ready(&mut r, FramePacket { view, composite_all: false, ..packet(editor.document().scene(), extent) });
        }
        let error = close(&shown(&r), &expected);
        assert!(error < tolerance, "moving paths at zoom {zoom} under a {blend:?} layer must show where the move puts them: {error}");
        assert_eq!(max_error(&display(&r), &display(&fresh)) > 0.1, split, "drags under normal layers present moving paths without recompositing them");
        r.set_vector_motion(None);
        submit_ready(&mut r, FramePacket { view, composite_all: false, ..packet(editor.document().scene(), extent) });
        assert!(close(&shown(&r), &original) < tolerance, "cancelling a move restores the layer at zoom {zoom} under a {blend:?} layer");
        r.set_vector_motion(Some(layer_render::VectorMotion { owner: layer, items: items.clone(), offset: [38., -22.] }));
        submit_ready(&mut r, FramePacket { view, composite_all: false, ..packet(editor.document().scene(), extent) });
        editor.perform(editor.document().vector_object_edit(layer, moved_object).unwrap()).unwrap();
        r.set_vector_motion(None);
        submit_ready(&mut r, FramePacket { view, composite_all: true, ..packet(editor.document().scene(), extent) });
        let error = close(&shown(&r), &expected);
        assert!(error < tolerance, "committing the move looks like its preview at zoom {zoom} under a {blend:?} layer: {error}");
    }
}

#[test]
#[ignore = "crowded vector layer editing benchmark"]
fn crowded_vector_layer_edit_frames() {
    use std::collections::BTreeSet;
    let extent = [4000u32, 3000u32];
    let zoom: f32 = std::env::var("VECTOR_ZOOM").ok().and_then(|v| v.parse().ok()).unwrap_or(0.4);
    let view = layer_render::ViewState { width_px: 1920, height_px: 1200, document_to_surface: [zoom, 0., 0., zoom, 160. - 1200. * (zoom - 0.4).max(0.), -1300. * (zoom - 0.4).max(0.)] };
    let count = 50_000u64;
    let records: Vec<Record> = crate::vector::bench::scene_b([4000., 3000.], count).into_iter().map(Record::Path).collect();
    let (mut editor, layer) = document(extent, records);
    let tones = VectorObject::from_records((0..12).map(|i| shape(i + 1, [300. * i as f64, 200., 300. * i as f64 + 220., 2900.], [0.2, 0.4, 0.9, 0.3])).collect());
    let (_, edit) = editor.document().create_vector_layer_edit("Tones", tones, None, 0).unwrap();
    editor.perform(edit).unwrap();
    let mut r = WgpuRasterizer::new_native_headless(editor.document().composition().color).unwrap();
    submit_ready(&mut r, FramePacket { view, ..packet(editor.document().scene(), extent) });
    let object = editor.document().scene().vector_layer(layer).unwrap().clone();
    let mut index = layer_core::authored::vector::VectorIndex::default();
    let start = std::time::Instant::now();
    index.hit(&object, [0.; 2], 4.);
    let build = start.elapsed().as_secs_f64() * 1e3;
    let start = std::time::Instant::now();
    let probes: Vec<_> = (0..100).map(|i| index.hit(&object, [1200. + i as f64 * 13., 1400. + (i % 7) as f64 * 31.], 4.)).collect();
    let hit = start.elapsed().as_secs_f64() * 1e3 / 100.;
    eprintln!("hit index build {build:.2} ms, then {hit:.4} ms per probe, {} hits of 100", probes.iter().flatten().count());
    let [a, _, _, d, tx, ty] = view.document_to_surface.map(f64::from);
    let center = [(f64::from(view.width_px) * 0.5 - tx) / a, (f64::from(view.height_px) * 0.5 - ty) / d];
    let mut nearest: Vec<(f64, ItemId)> = object.paths().map(|(_, p)| { let b = p.bounds(); ((b[0] + b[2] - 2. * center[0]).hypot(b[1] + b[3] - 2. * center[1]), p.id) }).collect();
    nearest.sort_by(|x, y| x.0.total_cmp(&y.0));
    let ids: Vec<ItemId> = nearest.into_iter().map(|(_, id)| id).collect();
    let (_texture, target) = crate::create_target(r.device(), [view.width_px, view.height_px], wgpu::TextureFormat::Rgba8UnormSrgb, "vector motion presentation");
    let mut presenter = crate::present::ViewportPresenter::for_surface(&r, wgpu::TextureFormat::Rgba8UnormSrgb, crate::SdrSurfaceColor::Srgb).unwrap();
    presenter.set_target_retention(true);
    {
        let points: Vec<_> = (0..400).map(|i| Anchor::corner([1200. + (i % 40) as f64 * 70., 1300. + (i / 40) as f64 * 170. + (i % 2) as f64 * 60.])).collect();
        let mut stroke = Stroke::new(paint([0.1, 0.2, 0.9, 1.]), 3.);
        stroke.cap = Cap::Round; stroke.join = Join::Round;
        let path = Arc::new(PathItem::new(ItemId(u64::MAX - 9), Arc::new(PathGeometry::new(vec![Subpath::new(&points, false)]).unwrap()), Arc::new(Style::new(vec![PaintEntry::Stroke(stroke)]))));
        r.set_vector_preview(Some(layer_render::VectorPreview { owner: layer, path, revision: 1 }));
        timed(&mut r, FramePacket { view, composite_all: false, ..packet(editor.document().scene(), extent) });
        r.set_vector_preview(None);
        let hidden = timed(&mut r, FramePacket { view, composite_all: false, ..packet(editor.document().scene(), extent) });
        eprintln!("recompositing without vector work over {} pages: frame cpu {:.2} wall {:.2} ms", r.metrics.frame_composited_pages.len(), hidden.0, hidden.1);
        submit_ready(&mut r, FramePacket { view, composite_all: false, ..packet(editor.document().scene(), extent) });
        presenter.present(&r, &target, view, [1.; 4]).unwrap();
        r.wait_idle().unwrap();
    }
    for k in [1usize, 100, 5000] {
        let selected: BTreeSet<ItemId> = ids.iter().copied().take(k).collect();
        let items = Arc::new(selected.clone());
        let mut drags = Vec::new();
        for i in 0..60 {
            r.set_vector_motion(Some(layer_render::VectorMotion { owner: layer, items: items.clone(), offset: [i as f64 * 0.7, i as f64 * -0.4] }));
            let start = std::time::Instant::now();
            r.submit(FramePacket { view, composite_all: false, ..packet(editor.document().scene(), extent) }).unwrap();
            presenter.present(&r, &target, view, [1.; 4]).unwrap();
            let cpu = start.elapsed().as_secs_f64() * 1e3;
            r.wait_idle().unwrap();
            drags.push((cpu, start.elapsed().as_secs_f64() * 1e3));
        }
        let first = drags[0];
        let (mut cpu, mut wall): (Vec<f64>, Vec<f64>) = drags[1..].iter().copied().unzip();
        eprintln!("drag {k} of {count}: first frame cpu {:.2} wall {:.2} ms; frame cpu p50 {:.2} p95 {:.2} ms, wall p50 {:.2} p95 {:.2} ms", first.0, first.1,
            percentile(&mut cpu, 0.5), percentile(&mut cpu, 0.95), percentile(&mut wall, 0.5), percentile(&mut wall, 0.95));
        r.set_vector_motion(None);
        submit_ready(&mut r, FramePacket { view, composite_all: false, ..packet(editor.document().scene(), extent) });
        let object = editor.document().scene().vector_layer(layer).unwrap().clone();
        let start = std::time::Instant::now();
        let moved = object.transform_items(&selected, layer_core::authored::Affine64([1., 0., 0., 1., 37., -21.]), true).unwrap();
        let transform = start.elapsed().as_secs_f64() * 1e3;
        let start = std::time::Instant::now();
        editor.perform(editor.document().vector_object_edit(layer, moved).unwrap()).unwrap();
        let perform = start.elapsed().as_secs_f64() * 1e3;
        let commit = timed(&mut r, FramePacket { view, composite_all: true, ..packet(editor.document().scene(), extent) });
        let pages = r.metrics.frame_composited_pages.len();
        let start = std::time::Instant::now();
        editor.undo().unwrap();
        let undo_perform = start.elapsed().as_secs_f64() * 1e3;
        let undo = timed(&mut r, FramePacket { view, composite_all: true, ..packet(editor.document().scene(), extent) });
        submit_ready(&mut r, FramePacket { view, composite_all: false, ..packet(editor.document().scene(), extent) });
        eprintln!("move {k} of {count}: transform {transform:.2} ms, perform {perform:.2} ms, frame over {pages} pages cpu {:.2} wall {:.2} ms; undo perform {undo_perform:.2} ms, frame cpu {:.2} wall {:.2} ms",
            commit.0, commit.1, undo.0, undo.1);
    }
}

#[test]
fn split_drags_redraw_neither_the_overview_nor_the_navigator() {
    use std::collections::BTreeSet;
    let extent = [2400u32, 1800u32];
    let view = layer_render::ViewState { width_px: 600, height_px: 450, document_to_surface: [1., 0., 0., 1., -300., -200.] };
    let mut records: Vec<Record> = (0..4).map(|i| stroke(9000 + i, &[[400. + i as f64 * 40., 300.], [600. + i as f64 * 30., 500.]], 9., [0.9, 0.2, 0.1, 0.8])).collect();
    records.extend(crate::vector::bench::scene_b([2400., 1800.], 1500).into_iter().map(Record::Path));
    let (mut editor, layer) = document(extent, records);
    let mut r = WgpuRasterizer::new_native_headless(editor.document().composition().color).unwrap();
    fn frame(editor: &layer_core::Editor, view: layer_render::ViewState, extent: [u32; 2], time_seconds: f32) -> FramePacket<'_> {
        FramePacket { view, time_seconds, composite_all: false, ..packet(editor.document().scene(), extent) }
    }
    submit_ready(&mut r, FramePacket { view, ..packet(editor.document().scene(), extent) });
    let navigator = r.navigator.revision;
    let items: Arc<BTreeSet<ItemId>> = Arc::new((9000..9004).map(ItemId).collect());
    let mut filled = BTreeSet::new();
    for i in 1..=12u32 {
        r.set_vector_motion(Some(layer_render::VectorMotion { owner: layer, items: items.clone(), offset: [f64::from(i) * 3., f64::from(i) * -2.] }));
        r.submit(frame(&editor, view, extent, i as f32 * 0.03)).unwrap();
        r.wait_idle().unwrap();
        assert!(r.display_motion().is_some(), "a top vector layer drags through the split preview");
        let region = r.vector_motion_region(extent).unwrap();
        for &(level, page) in &r.metrics.frame_composited_pages {
            assert!(level == 0 && !page_rect(page).intersect(region).is_empty() && filled.insert(page), "drag frame {i} recomposited {:?} outside {region:?} or again", (level, page));
        }
    }
    assert_eq!(r.navigator.revision, navigator, "the Navigator keeps the layer as it was until the move commits");
    let moved = editor.document().scene().vector_layer(layer).unwrap().transform_items(&items, layer_core::authored::Affine64([1., 0., 0., 1., 36., -24.]), true).unwrap();
    editor.perform(editor.document().vector_object_edit(layer, moved).unwrap()).unwrap();
    r.set_vector_motion(None);
    r.submit(FramePacket { composite_all: true, ..frame(&editor, view, extent, 1.) }).unwrap();
    r.wait_idle().unwrap();
    assert_ne!(r.navigator.revision, navigator, "the committed move refreshes the Navigator");
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

#[test]
fn rasterizing_a_vector_layer_bakes_what_it_shows() {
    let (editor, layer) = document([300, 200], records());
    let doc = editor.document().clone();
    let mut r = WgpuRasterizer::new_native_headless(doc.composition().color).unwrap();
    let shown = native(&mut r, &doc);
    let (_input, consumer) = layer_engine::input_queue(16);
    let mut engine = layer_engine::CanvasEngine::new(r, doc, consumer, crate::test_support::view([300, 200]), layer_engine::ViewTransform::IDENTITY).unwrap();
    let plan = engine.document().rasterize_plan(layer, false).unwrap();
    engine.insert_with_operations(plan.edits, vec![(plan.target, plan.operation)], None).unwrap();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
    loop {
        engine.render_frame().unwrap();
        if !engine.has_pending_document_edits() && !engine.backend().has_pending_work() { break; }
        assert!(std::time::Instant::now() < deadline, "the bake must finish");
        std::thread::yield_now();
    }
    assert!(engine.document().scene().vector_layer(layer).is_none());
    let document = engine.document().clone();
    let rasterized = native(engine.backend_mut(), &document);
    assert!(rasterized.iter().filter(|p| p[3] > 0.5).count() > 5000, "the paint layer holds the paths' pixels");
    assert!(max_error(&rasterized, &shown) < 2. / 255., "Rasterize Layer keeps the picture: {}", max_error(&rasterized, &shown));
}

#[test]
fn paths_painted_onto_paint_layers_match_the_path_drawn_above_them() {
    let settle = |engine: &mut layer_engine::CanvasEngine<WgpuRasterizer>| {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
        loop {
            engine.render_frame().unwrap();
            if !engine.has_pending_document_edits() && !engine.backend().has_pending_work() { break; }
            assert!(std::time::Instant::now() < deadline, "the bake must finish");
            std::thread::yield_now();
        }
    };
    let (editor, base) = document([300, 200], vec![shape(1, [30., 40., 200., 150.], [0.2, 0.6, 0.3, 0.7])]);
    let r = WgpuRasterizer::new_native_headless(editor.document().composition().color).unwrap();
    let (_input, consumer) = layer_engine::input_queue(16);
    let mut engine = layer_engine::CanvasEngine::new(r, editor.document().clone(), consumer, crate::test_support::view([300, 200]), layer_engine::ViewTransform::IDENTITY).unwrap();
    let plan = engine.document().rasterize_plan(base, false).unwrap();
    engine.insert_with_operations(plan.edits, vec![(plan.target, plan.operation)], None).unwrap();
    settle(&mut engine);
    let target = plan.target;
    assert_eq!(engine.document().target_offset(target), [0, 0]);
    let Record::Path(ink) = stroke(9, &[[20., 30.], [150., 180.], [280., 20.]], 12., [0.8, 0.1, 0.1, 0.9]) else { unreachable!() };
    for alpha_locked in [false, true] {
        let mut doc = engine.document().clone();
        if alpha_locked {
            let mut occurrence = doc.scene().occurrence(base).unwrap().clone();
            occurrence.alpha_locked = true;
            engine.apply_edit(layer_core::Edit::Occurrence(layer_core::RecordChange::replace(&doc.artwork.occurrences, base, Some(occurrence)).unwrap())).unwrap();
            settle(&mut engine);
            doc = engine.document().clone();
        }
        let (above, edit) = doc.create_vector_layer_edit("Ink", VectorObject::from_records(vec![Record::Path(ink.clone())]), None, 0).unwrap();
        let mut expected = doc.clone();
        expected.apply(edit).unwrap();
        if alpha_locked {
            let mut occurrence = expected.scene().occurrence(above).unwrap().clone();
            occurrence.attachment = layer_core::Attachment::Clip;
            expected.apply(layer_core::Edit::Occurrence(layer_core::RecordChange::replace(&expected.artwork.occurrences, above, Some(occurrence)).unwrap())).unwrap();
        }
        let shown = native(engine.backend_mut(), &expected);
        let (edits, operation) = engine.document().paint_path_plan(target, ink.clone()).unwrap();
        engine.insert_with_operations(edits, vec![(target, operation)], None).unwrap();
        settle(&mut engine);
        let document = engine.document().clone();
        let painted = native(engine.backend_mut(), &document);
        assert!(max_error(&painted, &shown) < 2. / 255., "alpha lock {alpha_locked}: {}", max_error(&painted, &shown));
        assert_eq!(document.scene().order().len(), doc.scene().order().len(), "no layer is added");
    }
}
