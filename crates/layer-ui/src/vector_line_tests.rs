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
