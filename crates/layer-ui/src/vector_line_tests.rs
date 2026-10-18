fn vector_session() -> (UiSession<Recorder>, OccurrenceHandle) {
    let mut s = session(Platform::Gtk);
    s.select_brush(DefaultBrushPreset::GPen as u32).unwrap();
    layer(&mut s, LayerAction::NewVector);
    let layer = s.engine.document().working.occurrence.unwrap();
    assert!(s.engine.document().scene().vector_layer(layer).is_some());
    (s, layer)
}
fn draw_line(s: &mut UiSession<Recorder>, first: u64, samples: u64) {
    for i in 0..samples {
        let phase = if i == 0 { PenPhase::Down } else if i + 1 == samples { PenPhase::Up } else { PenPhase::Move };
        let mut e = event(s, first + i, phase, 0.3 + 0.6 * (i as f32 / samples as f32 * 3.1).sin());
        e.surface_position.y += (i as f32 * 0.15).sin() * 60.;
        e.surface_position.x = 150. + i as f32 * 6.;
        s.pen(e).unwrap();
        s.frame((first + i) * 10_000_000, (first + i) * 10_000_000).unwrap();
    }
}

#[test]
fn vector_layers_draw_fitted_lines_with_one_undo_step() {
    let (mut s, layer) = vector_session();
    let revision = s.engine.document().revision;
    draw_line(&mut s, 1, 80);
    let object = s.engine.document().scene().vector_layer(layer).unwrap().clone();
    assert_eq!(object.path_count(), 1, "a pen stroke becomes one path");
    let (_, path) = object.paths().next().unwrap();
    let anchors = path.geometry.anchor_count();
    let stroke = path.style.strokes().next().unwrap();
    eprintln!("anchors {anchors}, width {}, knots {:?}", stroke.width, stroke.profile.as_ref().map(|p| p.knots().len()));
    assert!((2..30).contains(&anchors), "fitting keeps few anchors: {anchors}");
    assert!(stroke.profile.is_some(), "pressure becomes a width profile");
    assert_ne!(s.engine.document().revision, revision);
    assert_eq!(s.state.notice, None);
    draw_line(&mut s, 200, 40);
    assert_eq!(s.engine.document().scene().vector_layer(layer).unwrap().path_count(), 2);
    s.dispatch(UiAction::Invoke { command: CommandId::Undo }).unwrap();
    s.frame(5_000_000_000, 5_000_000_000).unwrap();
    assert_eq!(s.engine.document().scene().vector_layer(layer).unwrap().path_count(), 1, "undo removes the last line only");
    s.dispatch(UiAction::Invoke { command: CommandId::Redo }).unwrap();
    s.frame(5_100_000_000, 5_100_000_000).unwrap();
    assert_eq!(s.engine.document().scene().vector_layer(layer).unwrap().path_count(), 2);
}

fn surface_at(s: &UiSession<Recorder>, doc: [f64; 2]) -> Point {
    let [a, b, c, d, x, y] = s.state.camera.view().document_to_surface;
    let [px, py] = doc.map(|v| v as f32);
    Point { x: a * px + c * py + x, y: b * px + d * py + y }
}
fn press(s: &mut UiSession<Recorder>, first: u64, points: &[Point]) {
    for (i, point) in points.iter().enumerate() {
        let phase = if i == 0 { PenPhase::Down } else if i + 1 == points.len() { PenPhase::Up } else { PenPhase::Move };
        let mut e = event(s, first + i as u64, phase, 0.5);
        e.surface_position = *point;
        s.pen(e).unwrap();
        s.frame((first + i as u64) * 10_000_000, (first + i as u64) * 10_000_000).unwrap();
    }
}

#[test]
fn the_eraser_cuts_or_removes_vector_paths_in_one_undo_step() {
    let (mut s, layer) = vector_session();
    let prohibited = |s: &mut UiSession<Recorder>| { s.cursor_input(Some(event(s, 1, PenPhase::Hover, 0.))); s.canvas_cursor().is_some_and(|c| c.segments.iter().any(|segment| segment.marker == 6.)) };
    assert!(!prohibited(&mut s), "G-Pen draws on a vector layer");
    let revision = |s: &UiSession<Recorder>| s.state.layers.iter().find(|row| row.id == occurrence_token(layer)).unwrap().paint_revision;
    let before = revision(&s);
    draw_line(&mut s, 1, 30);
    assert_ne!(revision(&s), before, "a new path refreshes the layer thumbnail");
    let drawn = s.engine.document().scene().vector_layer(layer).unwrap().clone();
    let subpath = &drawn.paths().next().unwrap().1.geometry.subpaths()[0];
    let middle = subpath.segment(subpath.segment_count() / 2).point(0.5);
    invoke(&mut s, CommandId::Eraser);
    s.dispatch(UiAction::SetBrushSize { value: 8. }).unwrap();
    assert!(!prohibited(&mut s), "the eraser works on vector layers");
    assert!(s.state.tool_actions.iter().any(|action| action.command == CommandId::EraseWholePaths));
    let across = |s: &UiSession<Recorder>| [-40., 0., 40.].map(|dy| surface_at(s, [middle[0], middle[1] + dy]));
    let points = across(&s);
    press(&mut s, 100, &points);
    let cut = s.engine.document().scene().vector_layer(layer).unwrap().clone();
    assert_eq!(cut.path_count(), 2, "Touched Part splits the line where the eraser crosses it");
    assert_eq!(s.state.notice, None);
    s.dispatch(UiAction::Invoke { command: CommandId::Undo }).unwrap();
    s.frame(5_000_000_000, 5_000_000_000).unwrap();
    assert_eq!(s.engine.document().scene().vector_layer(layer).unwrap(), &drawn, "one undo step restores the line");
    invoke(&mut s, CommandId::EraseWholePaths);
    assert!(s.state.settings.erase_whole_paths);
    let points = across(&s);
    press(&mut s, 200, &points);
    assert_eq!(s.engine.document().scene().vector_layer(layer).unwrap().path_count(), 0, "Whole Path removes the touched line");
}

#[test]
fn move_tool_selects_drags_and_deletes_vector_paths() {
    let (mut s, layer) = vector_session();
    draw_line(&mut s, 1, 40);
    draw_line(&mut s, 200, 40);
    let object = s.engine.document().scene().vector_layer(layer).unwrap().clone();
    let (front, back) = { let mut paths = object.paths().map(|(_, p)| p.clone()); (paths.next().unwrap(), paths.next().unwrap()) };
    invoke(&mut s, CommandId::Move);
    let on_front = front.geometry.subpaths()[0].anchor(0).point();
    let start = surface_at(&s, on_front);
    let revision = s.engine.document().revision;
    press(&mut s, 400, &[start, Point { x: start.x + 20., y: start.y }, Point { x: start.x + 40., y: start.y + 10. }]);
    let selected = s.engine.document().working.vector_items.clone().expect("pressing a path selects it");
    assert_eq!(*selected.items, [front.id].into());
    let moved = s.engine.document().scene().vector_layer(layer).unwrap().clone();
    let delta = |id: layer_core::authored::vector::ItemId| { let p = moved.record(moved.find(id).unwrap()).unwrap().path().unwrap().geometry.subpaths()[0].anchor(0).point(); let q = if id == front.id { on_front } else { back.geometry.subpaths()[0].anchor(0).point() }; [p[0] - q[0], p[1] - q[1]] };
    let scale = f64::from(s.state.camera.input_transform().surface_to_document[0]);
    assert!((delta(front.id)[0] - 40. * scale).abs() < 1e-3 && (delta(front.id)[1] - 10. * scale).abs() < 1e-3, "{:?}", delta(front.id));
    assert_eq!(delta(back.id), [0., 0.], "other paths stay");
    assert_eq!(moved.path_count(), 2);
    s.dispatch(UiAction::Invoke { command: CommandId::Undo }).unwrap();
    s.frame(6_000_000_000, 6_000_000_000).unwrap();
    assert_eq!(s.engine.document().scene().vector_layer(layer).unwrap(), &object, "one undo step restores the move");
    assert!(s.engine.document().revision > revision);
    let empty = surface_at(&s, [3000., 2000.]);
    press(&mut s, 500, &[empty, empty]);
    assert!(s.engine.document().working.vector_items.is_none(), "a click on empty canvas clears the selection");
    let [x0, y0, x1, y1] = back.bounds();
    let corners = [surface_at(&s, [x0 - 5., y0 - 5.]), surface_at(&s, [x1 + 5., y1 + 5.])];
    press(&mut s, 600, &corners);
    let marquee = s.engine.document().working.vector_items.clone().expect("a marquee selects enclosed paths");
    assert!(marquee.items.contains(&back.id));
    invoke(&mut s, CommandId::ClearSelected);
    let left = s.engine.document().scene().vector_layer(layer).unwrap();
    assert!(left.find(back.id).is_none() && left.path_count() == 2 - marquee.items.len());
    assert!(s.engine.document().working.vector_items.is_none());
}

#[test]
#[ignore = "crowded vector layer session costs"]
fn crowded_vector_layer_session_costs() {
    use layer_core::authored::vector::*;
    for count in [10_000u64, 50_000] {
        let (mut s, layer) = vector_session();
        let records: Vec<Record> = (0..count).map(|i| {
            let x = (i % 400) as f64 * 10.;
            let y = (i / 400) as f64 * 6.;
            let anchors: Vec<_> = (0..4).map(|k| Anchor::new([x + k as f64 * 12., y + (k % 2) as f64 * 5.], [-4., 0.], [4., 0.], AnchorKind::SMOOTH)).collect();
            let mut stroke = Stroke::new(Paint::color(layer_core::color::RgbColor::BLACK), 3.);
            stroke.cap = Cap::Round; stroke.join = Join::Round;
            Record::Path(PathItem::new(ItemId(i + 1), Arc::new(PathGeometry::new(vec![Subpath::new(&anchors, false)]).unwrap()), Arc::new(Style::new(vec![PaintEntry::Stroke(stroke)]))))
        }).collect();
        let edit = s.engine.document().vector_object_edit(layer, VectorObject::from_records(records)).unwrap();
        let start = std::time::Instant::now();
        s.engine.apply_edit(edit).unwrap();
        let populate = start.elapsed().as_secs_f64() * 1e3;
        let mut commits = Vec::new();
        for line in 0..20u64 {
            let first = 1000 + line * 100;
            for i in 0..30u64 {
                let phase = if i == 0 { PenPhase::Down } else if i + 1 == 30 { PenPhase::Up } else { PenPhase::Move };
                let mut e = event(&s, first + i, phase, 0.7);
                e.surface_position.x = 150. + i as f32 * 6.;
                e.surface_position.y = 100. + line as f32 * 20.;
                let start = std::time::Instant::now();
                s.pen(e).unwrap();
                s.frame((first + i) * 10_000_000, (first + i) * 10_000_000).unwrap();
                if phase == PenPhase::Up { commits.push(start.elapsed().as_secs_f64() * 1e3); }
            }
        }
        commits.sort_by(f64::total_cmp);
        let object = s.engine.document().scene().vector_layer(layer).unwrap().clone();
        let mut path = object.paths().next().unwrap().1.clone();
        path.id = object.allocate_id();
        let edit = s.engine.document().vector_object_edit(layer, object.insert_front(vec![Record::Path(path)])).unwrap();
        let mut document = s.engine.document().clone();
        let start = std::time::Instant::now();
        document.apply(edit.clone()).unwrap();
        let apply = start.elapsed().as_secs_f64() * 1e3;
        let start = std::time::Instant::now();
        s.engine.apply_edit(edit).unwrap();
        let engine_apply = start.elapsed().as_secs_f64() * 1e3;
        let start = std::time::Instant::now();
        s.frame(9_000_000_000, 9_000_000_000).unwrap();
        let frame = start.elapsed().as_secs_f64() * 1e3;
        let start = std::time::Instant::now();
        s.frame(9_010_000_000, 9_010_000_000).unwrap();
        let idle = start.elapsed().as_secs_f64() * 1e3;
        eprintln!("document apply {apply:.2} ms, engine apply {engine_apply:.2} ms, next frame {frame:.2} ms, idle frame {idle:.2} ms");
        let cancel = std::sync::atomic::AtomicBool::new(false);
        let start = std::time::Instant::now();
        let capture = s.engine.capture_session(0).unwrap();
        let prepared = layer_core::package::session::PreparedSession::prepare(&capture, serde_json::json!({}), &cancel).unwrap();
        let autosave = start.elapsed().as_secs_f64() * 1e3;
        let start = std::time::Instant::now();
        let capture = s.engine.capture_session(0).unwrap();
        layer_core::package::session::PreparedSession::prepare(&capture, serde_json::json!({}), &cancel).unwrap();
        let again = start.elapsed().as_secs_f64() * 1e3;
        let start = std::time::Instant::now();
        s.dispatch(UiAction::Invoke { command: CommandId::Undo }).unwrap();
        let undo = start.elapsed().as_secs_f64() * 1e3;
        drop(prepared);
        eprintln!("{count} paths: populate {populate:.1} ms; pen-up commit p50 {:.2} max {:.2} ms; autosave {autosave:.1} ms, repeated {again:.1} ms; undo {undo:.2} ms",
            commits[commits.len() / 2], commits[commits.len() - 1]);
    }
}
