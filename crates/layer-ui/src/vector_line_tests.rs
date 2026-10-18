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

#[test]
#[ignore = "crowded vector layer session costs"]
fn crowded_vector_layer_session_costs() {
    use layer_core::authored::vector::*;
    for count in [10_000u64, 50_000] {
        let (mut s, layer) = vector_session();
        let records: Vec<Record> = (0..count).map(|i| {
            let x = (i % 400) as f64 * 10.;
            let y = (i / 400) as f64 * 6.;
            let anchors: Vec<_> = (0..4).map(|k| Anchor { point: [x + k as f64 * 12., y + (k % 2) as f64 * 5.], in_handle: [-4., 0.], out_handle: [4., 0.], kind: AnchorKind::SMOOTH }).collect();
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
