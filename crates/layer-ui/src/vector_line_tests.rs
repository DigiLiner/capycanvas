fn vector_session() -> (UiSession<Recorder>, OccurrenceHandle) { vector_session_on(Platform::Gtk) }
fn vector_session_on(platform: Platform) -> (UiSession<Recorder>, OccurrenceHandle) {
    let mut s = session(platform);
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
        s.pen(e);
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
    assert!((2..=8).contains(&anchors), "fitting keeps few anchors: {anchors}");
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
fn vector_lines_land_under_the_pen_on_moved_layers() {
    use layer_core::{Edit, authored::RecordChange};
    let (mut s, layer) = vector_session();
    let document = s.engine.document();
    let mut occurrence = document.scene().occurrence(layer).unwrap().clone();
    occurrence.offset = [40, -25];
    let edit = Edit::Occurrence(RecordChange::replace(&document.artwork.occurrences, layer, Some(occurrence)).unwrap());
    s.engine.apply_edit(edit).unwrap();
    let points: Vec<Point> = (0..=12).map(|i| surface_at(&s, [300. + f64::from(i) * 10., 200. + f64::from(i) * 5.])).collect();
    press(&mut s, 1, &points);
    let object = s.engine.document().scene().vector_layer(layer).unwrap().clone();
    let (_, path) = object.paths().next().unwrap();
    let [ox, oy] = s.engine.document().scene().occurrence_offset64(layer);
    let anchors = path.geometry.subpaths()[0].to_vec();
    let [start, end] = [anchors[0].point(), anchors[anchors.len() - 1].point()].map(|[x, y]| [x + ox, y + oy]);
    assert!((start[0] - 300.).hypot(start[1] - 200.) < 1.5, "{start:?}");
    assert!((end[0] - 420.).hypot(end[1] - 260.) < 1.5, "{end:?}");
}

#[test]
fn late_pressure_corrections_reshape_the_drawn_line_in_its_undo_step() {
    let (mut s, layer) = vector_session();
    let estimated = SampleFlags(SampleFlags::PRIMARY.0 | SampleFlags::ESTIMATED.0);
    let events: Vec<PenEvent> = (0..40u64).map(|i| {
        let phase = if i == 0 { PenPhase::Down } else if i == 39 { PenPhase::Up } else { PenPhase::Move };
        let mut e = event(&s, i + 1, phase, 0.15);
        e.surface_position.x = 150. + i as f32 * 6.;
        e.flags = estimated;
        e
    }).collect();
    for (i, e) in events.iter().enumerate() {
        s.pen(*e);
        s.frame((i as u64 + 1) * 10_000_000, (i as u64 + 1) * 10_000_000).unwrap();
    }
    let widest = |s: &UiSession<Recorder>| {
        let object = s.engine.document().scene().vector_layer(layer).unwrap().clone();
        assert_eq!(object.path_count(), 1);
        let (_, path) = object.paths().next().unwrap();
        path.style.strokes().next().unwrap().width
    };
    let before = widest(&s);
    for (i, e) in events.iter().enumerate() {
        s.pen(PenEvent { pressure: 0.9, flags: SampleFlags(SampleFlags::PRIMARY.0 | SampleFlags::CORRECTION.0), ..*e });
        s.frame((50 + i as u64) * 10_000_000, (50 + i as u64) * 10_000_000).unwrap();
    }
    assert!(widest(&s) > before * 2., "{} vs {before}", widest(&s));
    s.dispatch(UiAction::Invoke { command: CommandId::Undo }).unwrap();
    s.frame(2_000_000_000, 2_000_000_000).unwrap();
    assert_eq!(s.engine.document().scene().vector_layer(layer).unwrap().path_count(), 0, "the line and its corrections are one step");
}

#[test]
fn vector_line_previews_reach_predicted_samples_without_drawing_them() {
    let (mut s, layer) = vector_session_on(Platform::Android);
    s.set_platform_prediction_available(true);
    let farthest = |path: &layer_core::authored::vector::PathItem, axis: usize, sign: f64| path.geometry.subpaths()[0].to_vec().iter().map(|anchor| anchor.point()[axis] * sign).fold(f64::MIN, f64::max) * sign;
    let document = |s: &UiSession<Recorder>, point: Point| { let p = s.state.camera.input_transform().map(point); [f64::from(p.x), f64::from(p.y)] };
    let (mut real, mut predicted_reach) = ([0.; 2], [0.; 2]);
    for i in 0..30u64 {
        let mut e = event(&s, i + 1, if i == 0 { PenPhase::Down } else { PenPhase::Move }, 0.6);
        e.surface_position.x = 150. + i as f32 * 6.;
        s.pen(e);
        let mut predicted = PenEvent { sequence: e.sequence + 1_000, timestamp_ns: e.timestamp_ns + 10_000_000, flags: SampleFlags(SampleFlags::PRIMARY.0 | SampleFlags::PREDICTED.0), ..e };
        predicted.surface_position.x += 6.;
        predicted.surface_position.y -= if i >= 20 { 6. } else { 0. };
        if i > 0 { s.pen(predicted); }
        s.frame(e.timestamp_ns, predicted.timestamp_ns).unwrap();
        real = document(&s, e.surface_position);
        predicted_reach = document(&s, predicted.surface_position);
    }
    let preview = &s.engine.backend().vector_preview.as_ref().expect("the line is previewed").path;
    let shown = [farthest(preview, 0, 1.), farthest(preview, 1, -1.)];
    assert!(shown[0] > real[0] + (predicted_reach[0] - real[0]) * 0.5, "the preview reaches ahead: {shown:?} vs {real:?}..{predicted_reach:?}");
    assert!(shown[1] < real[1] + (predicted_reach[1] - real[1]) * 0.5, "the preview turns with the platform prediction: {shown:?} vs {real:?}..{predicted_reach:?}");
    let up = PenEvent { phase: PenPhase::Up, ..event(&s, 31, PenPhase::Up, 0.6) };
    s.pen(PenEvent { surface_position: Point { x: 150. + 29. * 6., y: 300. }, ..up });
    s.frame(up.timestamp_ns, up.timestamp_ns).unwrap();
    let object = s.engine.document().scene().vector_layer(layer).unwrap().clone();
    let (_, path) = object.paths().next().unwrap();
    let drawn = [farthest(path, 0, 1.), farthest(path, 1, -1.)];
    assert!((drawn[0] - real[0]).abs() < 1. && (drawn[1] - real[1]).abs() < 0.5, "the drawn line ends at the pen: {drawn:?} vs {real:?}");
    assert!(s.engine.backend().vector_preview.is_none());
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
        s.pen(e);
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
fn arrange_commands_restack_selected_paths_one_step_at_a_time() {
    let (mut s, layer) = vector_session();
    for first in [1, 100, 200] { draw_line(&mut s, first, 30); }
    let order = |s: &UiSession<Recorder>| s.engine.document().scene().vector_layer(layer).unwrap().paths().map(|(_, p)| p.id).collect::<Vec<_>>();
    let drawn = order(&s);
    invoke(&mut s, CommandId::Move);
    assert!(!s.command(CommandId::BringToFront).enabled, "arranging needs selected paths");
    let object = s.engine.document().scene().vector_layer(layer).unwrap().clone();
    let at = surface_at(&s, object.paths().next().unwrap().1.geometry.subpaths()[0].anchor(0).point());
    press(&mut s, 300, &[at, at]);
    assert_eq!(s.engine.document().working.vector_items.as_ref().map(|selection| (*selection.items).clone()), Some([drawn[0]].into()), "a click picks the front path");
    assert!(s.command(CommandId::BringToFront).enabled);
    assert!(s.state.tool_actions.iter().any(|action| action.command == CommandId::SendToBack), "the Move tool offers arranging");
    invoke(&mut s, CommandId::SendBackward);
    assert_eq!(order(&s), [drawn[1], drawn[0], drawn[2]]);
    invoke(&mut s, CommandId::SendToBack);
    assert_eq!(order(&s), [drawn[1], drawn[2], drawn[0]]);
    invoke(&mut s, CommandId::BringToFront);
    assert_eq!(order(&s), drawn);
    invoke(&mut s, CommandId::BringForward);
    assert_eq!(order(&s), drawn, "the front path stays in front");
    s.dispatch(UiAction::Invoke { command: CommandId::Undo }).unwrap();
    s.frame(2_000_000_000, 2_000_000_000).unwrap();
    assert_eq!(order(&s), [drawn[1], drawn[2], drawn[0]], "each arrange is one undo step");
}

#[test]
fn figure_shapes_on_vector_layers_become_selected_editable_paths() {
    use layer_core::authored::vector::{Cap, Join, PaintEntry, ShapeKind};
    let (mut s, layer) = vector_session();
    let prohibited = |s: &mut UiSession<Recorder>| { s.cursor_input(Some(event(s, 1, PenPhase::Hover, 0.))); s.canvas_cursor().is_some_and(|c| c.segments.iter().any(|segment| segment.marker == 6.)) };
    let drag = |s: &mut UiSession<Recorder>, first: u64, from: [f64; 2], to: [f64; 2]| { let points = [surface_at(s, from), surface_at(s, [(from[0] + to[0]) * 0.5, (from[1] + to[1]) * 0.5]), surface_at(s, to)]; press(s, first, &points); };
    let front = |s: &UiSession<Recorder>| s.engine.document().scene().vector_layer(layer).unwrap().paths().next().map(|(_, p)| p.clone());
    for (shape, paint, first) in [(FigureShape::Rectangle, FigurePaint::Both, 100), (FigureShape::Ellipse, FigurePaint::Fill, 200), (FigureShape::Line, FigurePaint::Outline, 300), (FigureShape::Polygon, FigurePaint::Outline, 400), (FigureShape::Star, FigurePaint::Fill, 500)] {
        s.dispatch(UiAction::Layer { action: LayerAction::Tool { tool: LayerCanvasTool::Figure { shape, paint } } }).unwrap();
        assert!(!prohibited(&mut s), "{shape:?} works on a vector layer");
        drag(&mut s, first, [100., 120.], [260., 200.]);
        let path = front(&s).expect("the figure becomes a path");
        assert_eq!(s.engine.document().working.vector_items.as_ref().map(|selection| (*selection.items).clone()), Some([path.id].into()), "the new path is selected");
        let [x0, y0, x1, y1] = path.geometry.hull();
        let spans = (x0 - 100.).abs() < 1e-3 && (x1 - 260.).abs() < 1e-3 && (y1 - 200.).abs() < 1e-3 || matches!(shape, FigureShape::Polygon | FigureShape::Star) && x0 >= 100. && x1 <= 260. && y1 <= 200.;
        assert!((y0 - 120.).abs() < 1e-3 && spans, "{shape:?} spans the drag: {:?}", path.geometry.hull());
        match shape {
            FigureShape::Rectangle => {
                assert!(matches!(path.shape.as_deref(), Some(layer_core::authored::vector::LiveShape { kind: ShapeKind::Rectangle { .. }, .. })));
                assert!(matches!(path.style.entries[..], [PaintEntry::Fill(_), PaintEntry::Stroke(ref s)] if s.join == Join::Miter), "Both fills below the outline");
            }
            FigureShape::Ellipse => {
                assert!(matches!(path.shape.as_deref(), Some(layer_core::authored::vector::LiveShape { kind: ShapeKind::Ellipse, .. })));
                assert!(matches!(path.style.entries[..], [PaintEntry::Fill(_)]));
            }
            FigureShape::Polygon => assert!(matches!(path.shape.as_deref(), Some(layer_core::authored::vector::LiveShape { kind: ShapeKind::Polygon { corners: 5 }, .. })) && path.geometry.anchor_count() == 5),
            FigureShape::Star => assert!(matches!(path.shape.as_deref(), Some(layer_core::authored::vector::LiveShape { kind: ShapeKind::Star { points: 5, .. }, .. })) && path.geometry.anchor_count() == 10),
            FigureShape::Line | FigureShape::Path => assert!(path.shape.is_none() && matches!(path.style.entries[..], [PaintEntry::Stroke(ref s)] if s.cap == Cap::Round)),
        }
    }
    assert_eq!(s.engine.document().scene().vector_layer(layer).unwrap().path_count(), 5);
    s.dispatch(UiAction::Invoke { command: CommandId::Undo }).unwrap();
    s.frame(9_000_000_000, 9_000_000_000).unwrap();
    assert_eq!(s.engine.document().scene().vector_layer(layer).unwrap().path_count(), 4, "each figure is one undo step");
}

#[test]
fn move_tool_nudges_duplicates_and_copies_paths() {
    let (mut s, layer) = vector_session();
    draw_line(&mut s, 1, 30);
    let object = |s: &UiSession<Recorder>| s.engine.document().scene().vector_layer(layer).unwrap().clone();
    let start = |s: &UiSession<Recorder>, id| { let o = object(s); o.record(o.find(id).unwrap()).unwrap().path().unwrap().geometry.subpaths()[0].anchor(0).point() };
    let id = object(&s).paths().next().unwrap().1.id;
    let origin = start(&s, id);
    invoke(&mut s, CommandId::Move);
    let at = surface_at(&s, origin);
    press(&mut s, 100, &[at, at]);
    let nudge = |s: &mut UiSession<Recorder>, shift: bool| for pressed in [true, false] {
        s.input(UiInput::Key { key: "arrowright".into(), pressed, repeat: false, modifiers: Modifiers { shift, ..Modifiers::default() }, editing: false, divider: None }).unwrap();
    };
    nudge(&mut s, false);
    nudge(&mut s, true);
    assert_eq!(start(&s, id), [origin[0] + 11., origin[1]], "arrows nudge by one pixel, ten with Shift");
    s.dispatch(UiAction::Invoke { command: CommandId::Undo }).unwrap();
    s.frame(2_000_000_000, 2_000_000_000).unwrap();
    assert_eq!(start(&s, id), [origin[0] + 1., origin[1]], "each nudge is one undo step");

    let middle = |s: &UiSession<Recorder>, id| { let o = object(s); let subpath = &o.record(o.find(id).unwrap()).unwrap().path().unwrap().geometry.subpaths()[0]; subpath.segment(subpath.segment_count() / 2).point(0.5) };
    let at = surface_at(&s, middle(&s, id));
    s.interaction.modifiers = Modifiers { alt: true, ..Modifiers::default() };
    press(&mut s, 200, &[at, Point { x: at.x + 10., y: at.y }, Point { x: at.x + 30., y: at.y + 20. }]);
    s.interaction.modifiers = Modifiers::default();
    let copied = object(&s);
    assert_eq!(copied.path_count(), 2, "Alt-drag leaves the original and drags a copy");
    assert_eq!(start(&s, id), [origin[0] + 1., origin[1]], "the original stays");
    let copy = *s.engine.document().working.vector_items.as_ref().unwrap().items.iter().next().unwrap();
    assert_ne!(copy, id, "the copy is selected");
    assert_eq!(copied.paths().next().unwrap().1.id, copy, "the copy is in front");
    s.dispatch(UiAction::Invoke { command: CommandId::Undo }).unwrap();
    s.frame(3_000_000_000, 3_000_000_000).unwrap();
    assert_eq!(object(&s).path_count(), 1, "one undo removes the dragged copy");

    let at = surface_at(&s, middle(&s, id));
    press(&mut s, 300, &[at, at]);
    let request = |s: &UiSession<Recorder>| s.state.requests.iter().find_map(|r| match &r.kind { HostRequestKind::Document { request } => Some((r.id, request.clone())), _ => None }).unwrap().0;
    let copy_clip = |s: &mut UiSession<Recorder>, command| {
        invoke(s, command);
        let id = request(s);
        let capture = s.capture_clipboard(id).unwrap();
        let extent = [capture.crop[2], capture.crop[3]];
        let clip = capture.finish("paths".into(), layer_core::color::source::rgba8_source(extent, |_, _| [0; 4]), vec![]).unwrap();
        s.complete_document_request(id, Ok(true)).unwrap();
        clip
    };
    let clip = copy_clip(&mut s, CommandId::Copy);
    assert!(clip.layers.as_ref().is_some_and(|layers| layers.paths && layers.scene.view().vector_layer(layers.roots[0]).unwrap().path_count() == 1));
    s.paste_clip(&clip, PasteMode::InPlace).unwrap();
    let pasted = object(&s);
    assert_eq!(pasted.path_count(), 2, "pasting onto a vector layer adds the paths");
    let new = *s.engine.document().working.vector_items.as_ref().unwrap().items.iter().next().unwrap();
    assert_ne!(new, id);
    assert_eq!(start(&s, new), start(&s, id), "Paste in Place keeps the position");
    copy_clip(&mut s, CommandId::Cut);
    assert_eq!(object(&s).path_count(), 1, "Cut removes the selected paths");
    assert!(object(&s).find(new).is_none());
    s.dispatch(UiAction::Layer { action: LayerAction::New { group: false, clipped: false } }).unwrap();
    s.paste_clip(&clip, PasteMode::InPlace).unwrap();
    let pasted_layer = s.engine.document().working.occurrence.unwrap();
    assert_ne!(pasted_layer, layer);
    assert_eq!(s.engine.document().scene().vector_layer(pasted_layer).map(|o| o.path_count()), Some(1), "pasting elsewhere makes a vector layer");
}

#[test]
fn properties_restyle_every_selected_path_in_one_undo_step() {
    use layer_core::{EffectValue, authored::vector::{Cap, Join, PaintEntry}};
    let (mut s, layer) = vector_session();
    for (first, size) in [(1, 10.), (100, 30.)] {
        s.dispatch(UiAction::SetBrushSize { value: size }).unwrap();
        draw_line(&mut s, first, 30);
    }
    let object = |s: &UiSession<Recorder>| s.engine.document().scene().vector_layer(layer).unwrap().clone();
    assert!(s.state.layer_properties.controls.iter().all(|c| !c.key.starts_with("path_")), "path properties need selected paths");
    let label = |s: &UiSession<Recorder>, key: &str| s.state.layer_properties.controls.iter().find(|c| c.key == key).map(|c| c.label.clone()).unwrap();
    let keys_of = |s: &UiSession<Recorder>| s.state.layer_properties.controls.iter().map(|c| c.key.clone()).collect::<Vec<_>>();
    let ids: std::collections::BTreeSet<_> = object(&s).paths().map(|(_, p)| p.id).collect();
    invoke(&mut s, CommandId::Move);
    let corners = { let [x0, y0, x1, y1] = object(&s).bounds().unwrap(); [surface_at(&s, [x0 - 5., y0 - 5.]), surface_at(&s, [x1 + 5., y1 + 5.])] };
    press(&mut s, 50, &corners);
    assert_eq!(s.engine.document().working.vector_items.as_ref().map(|selection| (*selection.items).clone()), Some(ids.clone()));
    let keys: Vec<&str> = s.state.layer_properties.controls.iter().map(|c| c.key.as_str()).filter(|k| k.starts_with("path_")).collect();
    assert_eq!(keys, ["path_fill", "path_outline", "path_outline_color", "path_width", "path_ends", "path_corners"]);
    assert_eq!((label(&s, "path_width"), label(&s, "path_outline_color")), ("Outline width (mixed)".to_string(), "Outline color".to_string()), "differing values say so");
    let token = occurrence_token(layer);
    let set = |s: &mut UiSession<Recorder>, key: &str, value: EffectValue| s.dispatch(UiAction::Effect { action: EffectAction::Set { layer: token, key: key.into(), value } }).unwrap();
    let red = layer_core::color::RgbColor::new(Default::default(), [0.9, 0.1, 0.1, 1.]).unwrap();
    set(&mut s, "path_outline_color", EffectValue::Color(red));
    set(&mut s, "path_corners", EffectValue::Choice(2));
    set(&mut s, "path_fill", EffectValue::Toggle(true));
    let styled = object(&s);
    for (_, path) in styled.paths() {
        assert!(matches!(path.style.entries[..], [PaintEntry::Fill(_), PaintEntry::Stroke(ref stroke)] if stroke.paint.rgb() == Some(red) && stroke.join == Join::Bevel && stroke.cap == Cap::Round), "{:?}", path.style);
    }
    for (phase, width) in [(ContactPhase::Down, 6.), (ContactPhase::Move, 9.), (ContactPhase::Up, 12.)] {
        s.dispatch(UiAction::Effect { action: EffectAction::Gesture { phase, action: Box::new(EffectAction::Set { layer: token, key: "path_width".into(), value: EffectValue::Number(width) }) } }).unwrap();
    }
    assert!(object(&s).paths().all(|(_, p)| p.style.strokes().last().unwrap().width == 12.), "a width drag sets every selected path");
    assert_eq!(label(&s, "path_width"), "Outline width");
    s.dispatch(UiAction::Invoke { command: CommandId::Undo }).unwrap();
    s.frame(8_000_000_000, 8_000_000_000).unwrap();
    assert_eq!(object(&s), styled, "a drag is one undo step");
    set(&mut s, "path_overlaps", EffectValue::Choice(1));
    assert!(object(&s).paths().all(|(_, p)| p.fill_rule == layer_core::authored::vector::FillRule::EvenOdd), "Overlaps: Holes cuts overlaps out of every selected fill");
    assert!(!keys_of(&s).contains(&"path_miter_limit".to_string()), "round corners have no limit");
    set(&mut s, "path_corners", EffectValue::Choice(0));
    assert!(keys_of(&s).contains(&"path_miter_limit".to_string()), "sharp corners show their limit");
    set(&mut s, "path_miter_limit", EffectValue::Number(8.));
    assert!(object(&s).paths().all(|(_, p)| p.style.strokes().last().unwrap().miter_limit == 8.));
    set(&mut s, "path_outline", EffectValue::Toggle(false));
    assert!(object(&s).paths().all(|(_, p)| matches!(p.style.entries[..], [PaintEntry::Fill(_)])), "turning the outline off keeps the fill");
}

#[test]
fn rasterize_layer_turns_a_vector_layer_into_paint_in_one_undo_step() {
    let (mut s, layer) = vector_session();
    draw_line(&mut s, 1, 30);
    assert!(s.command(CommandId::RasterizeLayer).enabled);
    invoke(&mut s, CommandId::RasterizeLayer);
    for t in 0..20 { s.frame(1_000_000_000 + t * 16_000_000, 1_000_000_000 + t * 16_000_000).unwrap(); }
    let document = s.engine.document();
    assert!(document.scene().paint_source(layer).is_some() && document.scene().vector_layer(layer).is_none(), "the layer holds paint");
    assert!(document.artwork.vectors.is_empty());
    s.dispatch(UiAction::Invoke { command: CommandId::Undo }).unwrap();
    s.frame(3_000_000_000, 3_000_000_000).unwrap();
    assert_eq!(s.engine.document().scene().vector_layer(layer).map(|o| o.path_count()), Some(1), "undo restores the paths");
}

#[test]
fn monoline_draws_constant_width_vector_lines() {
    let (mut s, layer) = vector_session();
    s.select_brush(DefaultBrushPreset::Monoline as u32).unwrap();
    draw_line(&mut s, 1, 40);
    let object = s.engine.document().scene().vector_layer(layer).unwrap().clone();
    assert_eq!(object.path_count(), 1);
    let (_, path) = object.paths().next().unwrap();
    let stroke = path.style.strokes().next().unwrap();
    assert!(stroke.profile.is_none(), "pressure does not change Monoline's width");
    assert_eq!(stroke.width, f64::from(s.engine.configured_brush().diameter));
}

#[test]
fn move_frame_scales_and_rotates_selected_paths_in_one_undo_step() {
    let (mut s, layer) = vector_session();
    draw_line(&mut s, 1, 30);
    let object = |s: &UiSession<Recorder>| s.engine.document().scene().vector_layer(layer).unwrap().clone();
    let drawn = object(&s);
    let id = drawn.paths().next().unwrap().1.id;
    let bounds = |s: &UiSession<Recorder>| object(s).items_bounds(&[id].into()).unwrap();
    invoke(&mut s, CommandId::Move);
    let at = surface_at(&s, drawn.paths().next().unwrap().1.geometry.subpaths()[0].anchor(0).point());
    press(&mut s, 50, &[at, at]);
    let mut overlay = Vec::new();
    s.append_layer_overlay(&mut overlay);
    assert_eq!(overlay.iter().filter(|segment| segment.marker == 2.).count(), 9, "the selection shows eight scale handles and a rotate handle");
    let [x0, y0, x1, y1] = bounds(&s);
    let corner = surface_at(&s, [x1, y1]);
    let far = surface_at(&s, [x1 + (x1 - x0), y1 + (y1 - y0)]);
    press(&mut s, 100, &[corner, Point { x: (corner.x + far.x) * 0.5, y: (corner.y + far.y) * 0.5 }, far]);
    let [a0, b0, a1, b1] = bounds(&s);
    assert!((a0 - x0).abs() < 0.5 && (b0 - y0).abs() < 0.5, "the opposite corner stays put");
    assert!(((a1 - a0) / (x1 - x0) - 2.).abs() < 0.05 && ((b1 - b0) / (y1 - y0) - 2.).abs() < 0.05, "dragging a corner scales: {:?} -> {:?}", [x0, y0, x1, y1], [a0, b0, a1, b1]);
    let stroke = |s: &UiSession<Recorder>| object(s).paths().next().unwrap().1.style.strokes().next().unwrap().width;
    assert!(stroke(&s) > drawn.paths().next().unwrap().1.style.strokes().next().unwrap().width * 1.9, "strokes scale with the paths");
    s.dispatch(UiAction::Invoke { command: CommandId::Undo }).unwrap();
    s.frame(2_000_000_000, 2_000_000_000).unwrap();
    assert_eq!(object(&s), drawn, "a frame drag is one undo step");
    let [x0, y0, x1, _] = bounds(&s);
    let reach = f64::from(s.ruler_reach());
    let rotate = [(x0 + x1) * 0.5, y0 - reach * 2.5];
    let centre = { let [x0, y0, x1, y1] = bounds(&s); [(x0 + x1) * 0.5, (y0 + y1) * 0.5] };
    let quarter = [centre[0] + (centre[1] - rotate[1]), centre[1]];
    let turn = [surface_at(&s, rotate), surface_at(&s, [rotate[0] + 10., rotate[1]]), surface_at(&s, quarter)];
    press(&mut s, 200, &turn);
    let first = |o: &layer_core::authored::vector::VectorObject| o.paths().next().unwrap().1.geometry.subpaths()[0].anchor(0).point();
    let (before, after) = (first(&drawn), first(&object(&s)));
    let expected = [centre[0] - (before[1] - centre[1]), centre[1] + (before[0] - centre[0])];
    assert!((after[0] - expected[0]).abs() < 0.5 && (after[1] - expected[1]).abs() < 0.5, "the rotate handle turns the paths about their centre: {after:?} vs {expected:?}");
}

#[test]
fn path_tool_builds_open_and_closed_paths_anchor_by_anchor() {
    use layer_core::authored::vector::AnchorKind;
    let (mut s, layer) = vector_session();
    s.dispatch(UiAction::Layer { action: LayerAction::Tool { tool: LayerCanvasTool::Figure { shape: FigureShape::Path, paint: FigurePaint::Outline } } }).unwrap();
    let object = |s: &UiSession<Recorder>| s.engine.document().scene().vector_layer(layer).unwrap().clone();
    let click = |s: &mut UiSession<Recorder>, first: u64, at: [f64; 2]| { let p = surface_at(s, at); press(s, first, &[p, p]); };
    click(&mut s, 10, [100., 100.]);
    let (smooth, handle) = (surface_at(&s, [200., 150.]), surface_at(&s, [240., 150.]));
    press(&mut s, 20, &[smooth, handle]);
    click(&mut s, 30, [300., 100.]);
    click(&mut s, 40, [350., 300.]);
    assert_eq!(object(&s).path_count(), 0, "nothing is added while drawing");
    assert!(s.command(CommandId::FinishPath).enabled && s.command(CommandId::RemoveSelectionPoint).enabled);
    let mut overlay = Vec::new();
    s.append_layer_overlay(&mut overlay);
    assert!(overlay.iter().filter(|segment| segment.marker == 2.).count() >= 4, "anchors and handles show while drawing");
    invoke(&mut s, CommandId::RemoveSelectionPoint);
    key(&mut s, "enter", true, false, false);
    let open = object(&s);
    assert_eq!(open.path_count(), 1);
    let (_, path) = open.paths().next().unwrap();
    let subpath = &path.geometry.subpaths()[0];
    assert!(!subpath.closed && subpath.len() == 3, "Remove Point drops the last anchor and Enter finishes an open path");
    assert_eq!(subpath.anchor(1).kind, AnchorKind::SYMMETRIC, "dragging makes a smooth anchor");
    assert!((subpath.anchor(1).out_handle()[0] - 40.).abs() < 0.5, "{:?}", subpath.anchor(1).out_handle());
    assert_eq!(s.engine.document().working.vector_items.as_ref().map(|selection| (*selection.items).clone()), Some([path.id].into()));
    for (i, at) in [[400., 400.], [500., 400.], [450., 480.]].into_iter().enumerate() { click(&mut s, 100 + i as u64 * 5, at); }
    click(&mut s, 200, [401., 401.]);
    let closed = object(&s);
    assert_eq!(closed.path_count(), 2);
    assert!(closed.paths().next().unwrap().1.geometry.subpaths()[0].closed, "clicking the first anchor closes the path");
    s.dispatch(UiAction::Invoke { command: CommandId::Undo }).unwrap();
    s.frame(9_000_000_000, 9_000_000_000).unwrap();
    assert_eq!(object(&s).path_count(), 1, "each path is one undo step");
    s.dispatch(UiAction::Layer { action: LayerAction::New { group: false, clipped: false } }).unwrap();
    s.dispatch(UiAction::Layer { action: LayerAction::Tool { tool: LayerCanvasTool::Figure { shape: FigureShape::Path, paint: FigurePaint::Outline } } }).unwrap();
    let paint = s.engine.document().drawing_content().unwrap();
    let operations = |s: &UiSession<Recorder>| s.engine.document().target_operations(paint).map_or(0, <[_]>::len);
    let before = operations(&s);
    click(&mut s, 300, [100., 100.]);
    click(&mut s, 310, [300., 120.]);
    click(&mut s, 320, [200., 300.]);
    let mut overlay = Vec::new();
    s.append_layer_overlay(&mut overlay);
    assert!(overlay.iter().any(|segment| segment.marker == 0.), "the drafted outline shows on a paint layer");
    key(&mut s, "enter", true, false, false);
    assert_eq!(s.state.notice, None);
    assert_eq!(operations(&s), before + 1, "Finish paints the path into the paint layer");
    assert!(matches!(s.engine.document().target_operations(paint).unwrap().last().unwrap().kind, layer_core::RasterOperationKind::Bake { .. }));
    s.dispatch(UiAction::Invoke { command: CommandId::Undo }).unwrap();
    s.frame(12_000_000_000, 12_000_000_000).unwrap();
    assert_eq!(operations(&s), before, "one undo step");
}

#[test]
fn edit_points_moves_adds_deletes_and_switches_anchors() {
    use layer_core::authored::vector::AnchorKind;
    let (mut s, layer) = vector_session();
    draw_line(&mut s, 1, 30);
    let object = |s: &UiSession<Recorder>| s.engine.document().scene().vector_layer(layer).unwrap().clone();
    let id = object(&s).paths().next().unwrap().1.id;
    let subpath = |s: &UiSession<Recorder>| { let o = object(s); o.record(o.find(id).unwrap()).unwrap().path().unwrap().geometry.subpaths()[0].clone() };
    invoke(&mut s, CommandId::Move);
    let middle = { let sp = subpath(&s); sp.segment(sp.segment_count() / 2).point(0.5) };
    let at = surface_at(&s, middle);
    press(&mut s, 50, &[at, at]);
    invoke(&mut s, CommandId::EditPoints);
    assert_eq!(s.layer_interaction.tool, LayerCanvasTool::EditPoints);
    let count = subpath(&s).len();
    let mut overlay = Vec::new();
    s.append_layer_overlay(&mut overlay);
    assert_eq!(overlay.iter().filter(|segment| segment.marker == 1.).count(), count * 4, "every anchor of the selected path shows");
    let first = subpath(&s).anchor(0).point();
    let from = surface_at(&s, first);
    let to = surface_at(&s, [first[0] + 30., first[1] + 20.]);
    press(&mut s, 100, &[from, Point { x: (from.x + to.x) * 0.5, y: (from.y + to.y) * 0.5 }, to]);
    let moved = subpath(&s).anchor(0).point();
    assert!((moved[0] - first[0] - 30.).abs() < 0.5 && (moved[1] - first[1] - 20.).abs() < 0.5, "dragging an anchor moves it: {moved:?}");
    s.dispatch(UiAction::Invoke { command: CommandId::Undo }).unwrap();
    s.frame(2_000_000_000, 2_000_000_000).unwrap();
    assert_eq!(subpath(&s).anchor(0).point(), first, "an anchor drag is one undo step");
    let on_segment = { let sp = subpath(&s); sp.segment(0).point(0.5) };
    let click = surface_at(&s, on_segment);
    press(&mut s, 200, &[click, click]);
    assert_eq!(subpath(&s).len(), count + 1, "clicking a segment adds an anchor");
    invoke(&mut s, CommandId::ClearSelected);
    assert_eq!(subpath(&s).len(), count, "Delete removes the selected anchor");
    let second = surface_at(&s, subpath(&s).anchor(1).point());
    let kind = subpath(&s).anchor(1).kind;
    press(&mut s, 300, &[second, second]);
    press(&mut s, 302, &[second, second]);
    assert_ne!(subpath(&s).anchor(1).kind, kind, "a double click switches corner and smooth");
    assert!(matches!(subpath(&s).anchor(1).kind, AnchorKind::CORNER | AnchorKind::SMOOTH));
    let [x0, y0, x1, y1] = object(&s).items_bounds(&[id].into()).unwrap();
    let corners = [surface_at(&s, [x0 - 20., y0 - 20.]), surface_at(&s, [(x0 + x1) * 0.5, y1 + 20.])];
    press(&mut s, 400, &corners);
    let expected = subpath(&s).anchors().filter(|a| a.point()[0] <= (x0 + x1) * 0.5).count();
    invoke(&mut s, CommandId::ClearSelected);
    assert_eq!(subpath(&s).len(), count - expected, "a marquee selects the anchors it encloses");
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
                s.pen(e);
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

#[test]
fn vector_layer_masks_take_brush_strokes_while_paths_stay() {
    let (mut s, handle) = vector_session();
    draw_line(&mut s, 1, 30);
    let drawn = s.engine.document().scene().vector_layer(handle).unwrap().clone();
    let id = occurrence_token(handle);
    layer(&mut s, LayerAction::AddMask { id, replace: false });
    layer(&mut s, LayerAction::Select { id, mask: true });
    assert!(matches!(s.engine.document().working.target, Some(SourceTarget::Coverage(_))));
    let revision = s.engine.document().revision;
    draw_line(&mut s, 100, 30);
    assert_eq!(s.engine.document().scene().vector_layer(handle).unwrap(), &drawn, "mask strokes leave the paths");
    assert_ne!(s.engine.document().revision, revision, "the stroke paints the mask");
    assert_eq!(s.state.notice, None);
    invoke(&mut s, CommandId::Move);
    let start = surface_at(&s, drawn.paths().next().unwrap().1.geometry.subpaths()[0].anchor(0).point());
    press(&mut s, 200, &[start, start]);
    assert!(s.engine.document().working.vector_items.is_none(), "Move on the mask leaves the paths unselected");
}

#[test]
fn move_and_edit_points_reach_paths_while_pixels_are_selected() {
    let (mut s, handle) = vector_session();
    draw_line(&mut s, 1, 40);
    let path = s.engine.document().scene().vector_layer(handle).unwrap().paths().next().unwrap().1.clone();
    select(&mut s, rectangle([0., 0., 50., 50.]));
    invoke(&mut s, CommandId::Move);
    let start = surface_at(&s, path.geometry.subpaths()[0].anchor(0).point());
    press(&mut s, 100, &[start, start]);
    assert_eq!(s.engine.document().working.vector_items.as_ref().map(|selected| (*selected.items).clone()), Some([path.id].into()));
    assert_eq!(s.state.notice, None);
    invoke(&mut s, CommandId::EditPoints);
    let mut overlay = Vec::new();
    s.append_layer_overlay(&mut overlay);
    assert_eq!(overlay.iter().filter(|segment| segment.marker == 1.).count(), path.geometry.subpaths()[0].len() * 4, "Edit Points shows the anchors");
}

#[test]
fn clear_layer_removes_every_path_in_one_undo_step() {
    let (mut s, handle) = vector_session();
    draw_line(&mut s, 1, 30);
    draw_line(&mut s, 100, 30);
    let drawn = s.engine.document().scene().vector_layer(handle).unwrap().clone();
    assert!(s.command(CommandId::ClearLayer).enabled);
    invoke(&mut s, CommandId::ClearLayer);
    s.frame(3_000_000_000, 3_000_000_000).unwrap();
    assert_eq!(s.engine.document().scene().vector_layer(handle).map(|object| object.path_count()), Some(0), "the layer stays a vector layer");
    s.dispatch(UiAction::Invoke { command: CommandId::Undo }).unwrap();
    s.frame(4_000_000_000, 4_000_000_000).unwrap();
    assert_eq!(s.engine.document().scene().vector_layer(handle).unwrap(), &drawn, "one undo step restores the paths");
    select(&mut s, rectangle([0., 0., 500., 500.]));
    assert!(!s.command(CommandId::FillSelection).enabled, "vector layers hold no pixels to fill");
}

#[test]
fn copy_and_the_wand_read_what_a_vector_layer_shows() {
    let (mut s, handle) = vector_session();
    draw_line(&mut s, 1, 30);
    select(&mut s, rectangle([0., 0., 2000., 2000.]));
    invoke(&mut s, CommandId::Copy);
    assert_eq!(s.state.notice, None);
    let copy = s.state.requests.iter().find_map(|r| match &r.kind {
        HostRequestKind::Document { request: DocumentRequest::Copy { .. } } => Some(r.id),
        _ => None,
    }).expect("a copy request");
    assert_eq!(s.capture_clipboard(copy).unwrap().scope, SceneScope::RawObjects(handle));
    s.layer_action(LayerAction::Tool { tool: LayerCanvasTool::Region { fill: false, source: crate::RegionSource::Editing } }).unwrap();
    let point = surface_at(&s, [600., 600.]);
    press(&mut s, 100, &[point, point]);
    let request = s.renderer_mut().region_requests.last().cloned().expect("the wand reads the layer");
    assert!(matches!(request.source, layer_render::RegionSource::Scene { scope: SceneScope::RawObjects(layer), .. } if layer == handle));
}

#[test]
fn new_vector_layers_switch_to_a_pen_and_paint_layers_switch_back() {
    let mut s = session(Platform::Gtk);
    s.select_brush(DefaultBrushPreset::Pencil as u32).unwrap();
    let paint = s.engine.document().working.occurrence.unwrap();
    let command = s.command(CommandId::NewVectorLayer);
    assert!(command.enabled);
    assert_eq!((command.icon, command.shortcut.as_str()), (Some("add-vector-layer"), "Ctrl+Alt+Shift+N"));
    invoke(&mut s, CommandId::NewVectorLayer);
    let vector = s.engine.document().working.occurrence.unwrap();
    assert!(s.engine.document().scene().vector_layer(vector).is_some());
    assert_eq!(s.state.brush.preset, DefaultBrushPreset::GPen as u32, "a new vector layer picks a pen that draws paths");
    let row = s.state.layers.iter().find(|row| row.id == occurrence_token(vector)).unwrap();
    assert_eq!(row.content_icon.as_deref(), Some("layer-path-symbolic"));
    assert_eq!(s.state.layer_properties.layer_type, s.localization().text(MessageId::RESOURCES_LAYER_TYPE_VECTOR).as_ref());
    assert_eq!(s.state.layer_properties.title, row.label);
    let usable = |s: &UiSession<Recorder>, preset: DefaultBrushPreset| s.state.tool_set.subtools.iter()
        .find(|item| item.action == UiAction::SelectBrush { id: preset as u32 }).map(|item| item.enabled);
    assert_eq!((usable(&s, DefaultBrushPreset::GPen), usable(&s, DefaultBrushPreset::Monoline), usable(&s, DefaultBrushPreset::RoughGPen)), (Some(true), Some(true), Some(false)));
    layer(&mut s, LayerAction::Select { id: occurrence_token(paint), mask: false });
    assert_eq!(s.state.brush.preset, DefaultBrushPreset::Pencil as u32, "a paint layer brings the painting brush back");
    s.select_brush(DefaultBrushPreset::GPen as u32).unwrap();
    assert_eq!(usable(&s, DefaultBrushPreset::RoughGPen), Some(true), "paint layers take every brush");
    layer(&mut s, LayerAction::Select { id: occurrence_token(vector), mask: false });
    s.select_brush(DefaultBrushPreset::Monoline as u32).unwrap();
    layer(&mut s, LayerAction::Select { id: occurrence_token(paint), mask: false });
    assert_eq!(s.state.brush.preset, DefaultBrushPreset::Monoline as u32, "a pen the painter chose stays");
    s.select_brush(DefaultBrushPreset::Pencil as u32).unwrap();
    layer(&mut s, LayerAction::Select { id: occurrence_token(vector), mask: false });
    assert_eq!(s.state.brush.preset, DefaultBrushPreset::Monoline as u32, "the last pen used on paths comes back");
}

#[test]
fn the_layer_menu_offers_new_vector_layer_in_every_state() {
    let mut s = session(Platform::Gtk);
    let finds = |s: &UiSession<Recorder>| s.application_menu(ApplicationMenu::Layer).sections.iter().flatten()
        .flat_map(|item| item.sections.iter().flatten())
        .find(|item| item.action == Some(UiAction::Layer { action: LayerAction::NewVector }))
        .map(|item| (item.label.clone(), item.enabled));
    let label = CommandId::NewVectorLayer.localized_label(s.localization()).to_string();
    assert_eq!(finds(&s), Some((label.clone(), true)));
    let id = occurrence_token(s.engine.document().working.occurrence.unwrap());
    layer(&mut s, LayerAction::AddMask { id, replace: false });
    layer(&mut s, LayerAction::Select { id, mask: true });
    assert_eq!(finds(&s), Some((label.clone(), true)), "while a mask is edited");
    layer(&mut s, LayerAction::Select { id, mask: false });
    select(&mut s, rectangle([10., 10., 60., 60.]));
    invoke(&mut s, CommandId::NewSelectionLayer);
    assert!(s.engine.document().working.occurrence.and_then(|h| s.engine.document().scene().occurrence(h)).is_some_and(|l| l.kind() == LayerKind::Selection));
    assert_eq!(finds(&s), Some((label.clone(), true)), "while a selection layer is active");
    let mut working = s.engine.document().working.clone();
    working.occurrence = None;
    working.target = None;
    s.layer_edit(layer_core::Edit::Working(working)).unwrap();
    assert_eq!(finds(&s), Some((label, true)), "with no active layer");
    invoke(&mut s, CommandId::NewVectorLayer);
    assert!(s.engine.document().working.occurrence.is_some_and(|h| s.engine.document().scene().vector_layer(h).is_some()));
}

#[test]
fn painting_brushes_on_vector_layers_offer_a_pen_a_paint_layer_or_rasterizing() {
    let (mut s, handle) = vector_session();
    draw_line(&mut s, 1, 20);
    s.select_brush(DefaultBrushPreset::Pencil as u32).unwrap();
    let refused = |s: &mut UiSession<Recorder>, first: u64| {
        let revision = s.engine.document().revision;
        draw_line(s, first, 10);
        assert_eq!(s.engine.document().revision, revision, "nothing reaches the paths");
        s.state.notice.clone().expect("a vector refusal")
    };
    let notice = refused(&mut s, 100);
    let mut args = FluentArgs::new(); args.set("layer", s.engine.document().scene().occurrence(handle).unwrap().name.to_string());
    assert_eq!(notice.text, s.localization().format(MessageId::OBJECTS_VECTOR_REFUSAL_BRUSH, &args));
    assert_eq!(notice.actions.iter().map(|a| (a.id, a.label.as_str(), a.enabled)).collect::<Vec<_>>(),
        [(NoticeActionId::UseVectorPen, "Use G-Pen", true), (NoticeActionId::NewPaintLayer, "New Paint Layer", true), (NoticeActionId::RasterizeLayer, "Rasterize Layer", true)]);
    s.dispatch(UiAction::Notice { id: notice.id, accept: true, action: Some(NoticeActionId::UseVectorPen) }).unwrap();
    assert_eq!(s.state.brush.preset, DefaultBrushPreset::GPen as u32);
    draw_line(&mut s, 200, 20);
    assert_eq!(s.engine.document().scene().vector_layer(handle).unwrap().path_count(), 2, "the pen draws a path");
    s.select_brush(DefaultBrushPreset::Pencil as u32).unwrap();
    let notice = refused(&mut s, 300);
    s.dispatch(UiAction::Notice { id: notice.id, accept: true, action: Some(NoticeActionId::NewPaintLayer) }).unwrap();
    let paint = s.engine.document().working.occurrence.unwrap();
    assert!(s.engine.document().scene().paint_source(paint).is_some());
    assert_eq!(s.engine.document().scene().children(None).iter().position(|h| *h == paint).unwrap() + 1,
        s.engine.document().scene().children(None).iter().position(|h| *h == handle).unwrap(), "the paint layer sits above the lines");
    assert_eq!(s.state.brush.preset, DefaultBrushPreset::Pencil as u32, "the brush stays");
    layer(&mut s, LayerAction::Select { id: occurrence_token(handle), mask: false });
    s.select_brush(DefaultBrushPreset::Pencil as u32).unwrap();
    let notice = refused(&mut s, 400);
    s.dispatch(UiAction::Notice { id: notice.id, accept: true, action: Some(NoticeActionId::RasterizeLayer) }).unwrap();
    for t in 0..20 { s.frame(9_000_000_000 + t * 16_000_000, 9_000_000_000 + t * 16_000_000).unwrap(); }
    assert!(s.engine.document().scene().paint_source(handle).is_some(), "the lines become paint");
}

#[test]
fn a_pen_setting_that_stops_paths_is_named_and_reset_from_the_refusal() {
    let (mut s, handle) = vector_session();
    s.dispatch(UiAction::SetToolSetting { id: "size_jitter".into(), value: 0.5 }).unwrap();
    assert_eq!(s.state.tool_set.subtools.iter().find(|item| item.action == UiAction::SelectBrush { id: DefaultBrushPreset::GPen as u32 }).map(|item| item.enabled), Some(false));
    let revision = s.engine.document().revision;
    draw_line(&mut s, 1, 10);
    assert_eq!(s.engine.document().revision, revision);
    let notice = s.state.notice.clone().expect("a vector refusal");
    let label = s.state.tool_settings.iter().find(|setting| setting.id == "size_jitter").unwrap().label.clone();
    let mut args = FluentArgs::new(); args.set("layer", s.engine.document().scene().occurrence(handle).unwrap().name.to_string()); args.set("setting", label.as_ref()); args.set("brush", "G-Pen");
    assert_eq!(notice.text, s.localization().format(MessageId::OBJECTS_VECTOR_REFUSAL_SETTING, &args));
    assert_eq!(notice.actions[0].id, NoticeActionId::ResetSetting);
    s.dispatch(UiAction::Notice { id: notice.id, accept: true, action: Some(NoticeActionId::ResetSetting) }).unwrap();
    draw_line(&mut s, 100, 20);
    assert_eq!(s.engine.document().scene().vector_layer(handle).unwrap().path_count(), 1);
}

#[test]
fn fill_tools_on_vector_layers_offer_a_paint_layer_below() {
    let (mut s, handle) = vector_session();
    draw_line(&mut s, 1, 20);
    s.layer_action(LayerAction::Tool { tool: LayerCanvasTool::Gradient { shape: layer_core::GradientShape::Linear } }).unwrap();
    let start = surface_at(&s, [100., 100.]);
    press(&mut s, 100, &[start, Point { x: start.x + 80., y: start.y }]);
    let notice = s.state.notice.clone().expect("a vector refusal");
    let mut args = FluentArgs::new(); args.set("layer", s.engine.document().scene().occurrence(handle).unwrap().name.to_string());
    assert_eq!(notice.text, s.localization().format(MessageId::OBJECTS_VECTOR_REFUSAL_TOOL, &args));
    assert_eq!(notice.actions.iter().map(|a| a.id).collect::<Vec<_>>(), [NoticeActionId::NewPaintLayerBelow, NoticeActionId::RasterizeLayer]);
    s.dispatch(UiAction::Notice { id: notice.id, accept: true, action: Some(NoticeActionId::NewPaintLayerBelow) }).unwrap();
    let paint = s.engine.document().working.occurrence.unwrap();
    let order = s.engine.document().scene().children(None).to_vec();
    assert!(s.engine.document().scene().paint_source(paint).is_some());
    assert_eq!(order.iter().position(|h| *h == paint), order.iter().position(|h| *h == handle).map(|i| i + 1), "the paint layer sits under the lines");
    assert!(matches!(s.layer_interaction.tool, LayerCanvasTool::Gradient { .. }), "the tool stays");
}

#[test]
fn select_all_transform_flip_escape_and_delete_act_on_paths() {
    let (mut s, handle) = vector_session();
    draw_line(&mut s, 1, 30);
    draw_line(&mut s, 100, 30);
    let object = |s: &UiSession<Recorder>| s.engine.document().scene().vector_layer(handle).unwrap().clone();
    let drawn = object(&s);
    let all: std::collections::BTreeSet<_> = drawn.paths().map(|(_, path)| path.id).collect();
    assert!(!s.command(CommandId::Deselect).enabled);
    invoke(&mut s, CommandId::SelectAll);
    assert_eq!(s.engine.document().working.vector_items.as_ref().map(|v| (*v.items).clone()), Some(all.clone()));
    assert!(s.engine.document().working.selection.is_none(), "Select All selects paths, not pixels");
    assert_eq!(s.layer_interaction.tool, LayerCanvasTool::Move);
    assert_eq!(s.command(CommandId::ClearSelected).label.as_ref(), "Delete");
    for command in [CommandId::TransformFlipHorizontal, CommandId::TransformFlipVertical, CommandId::TransformRotateLeft, CommandId::TransformRotateRight, CommandId::ScaleOutlineWidths] {
        assert!(s.state.tool_actions.iter().any(|action| action.command == command), "{command:?} is a Move option for paths");
    }
    assert!(s.command(CommandId::ScaleOutlineWidths).selected, "outline widths scale by default");
    let [x0, _, x1, _] = drawn.items_bounds(&all).unwrap();
    invoke(&mut s, CommandId::TransformFlipHorizontal);
    let flipped = object(&s);
    for ((_, before), (_, after)) in drawn.paths().zip(flipped.paths()) {
        let [a, b] = [before.geometry.subpaths()[0].anchor(0).point(), after.geometry.subpaths()[0].anchor(0).point()];
        assert!((b[0] - (x0 + x1 - a[0])).abs() < 1e-6 && (b[1] - a[1]).abs() < 1e-6, "{a:?} mirrors to {b:?}");
    }
    s.dispatch(UiAction::Invoke { command: CommandId::Undo }).unwrap();
    s.frame(5_000_000_000, 5_000_000_000).unwrap();
    assert_eq!(object(&s), drawn, "one undo step restores the flip");
    invoke(&mut s, CommandId::SelectAll);
    key(&mut s, "escape", true, false, false);
    key(&mut s, "escape", false, false, false);
    assert!(s.engine.document().working.vector_items.is_none(), "Escape clears the path selection");
    invoke(&mut s, CommandId::SelectAll);
    assert!(s.command(CommandId::Deselect).enabled);
    invoke(&mut s, CommandId::Deselect);
    assert!(s.engine.document().working.vector_items.is_none());
    let start = surface_at(&s, drawn.paths().next().unwrap().1.geometry.subpaths()[0].anchor(0).point());
    press(&mut s, 300, &[start, start]);
    key(&mut s, "enter", true, false, false);
    key(&mut s, "enter", false, false, false);
    assert_eq!(s.layer_interaction.tool, LayerCanvasTool::EditPoints, "Enter edits the selected path's points");
    key(&mut s, "escape", true, false, false);
    key(&mut s, "escape", false, false, false);
    assert_eq!(s.layer_interaction.tool, LayerCanvasTool::Move, "Escape leaves Edit Points");
    assert!(s.engine.document().working.vector_items.is_some());
    press(&mut s, 400, &[start, start]);
    press(&mut s, 402, &[start, start]);
    assert_eq!(s.layer_interaction.tool, LayerCanvasTool::EditPoints, "a double-click edits the path's points");
    invoke(&mut s, CommandId::Move);
    invoke(&mut s, CommandId::SelectAll);
    invoke(&mut s, CommandId::ClearSelected);
    assert_eq!(object(&s).path_count(), 0, "Delete removes the selected paths");
}

#[test]
fn edit_transform_shows_the_move_frame_and_outline_widths_can_stay() {
    let (mut s, handle) = vector_session();
    draw_line(&mut s, 1, 30);
    let width = |s: &UiSession<Recorder>| s.engine.document().scene().vector_layer(handle).unwrap().paths().next().unwrap().1.style.strokes().next().unwrap().width;
    let before = width(&s);
    assert!(!s.command(CommandId::ScaleRotate).enabled);
    assert_eq!(s.command(CommandId::ScaleRotate).disabled_reason.as_deref(), Some(s.localization().text(MessageId::COMMANDS_SELECT_PATHS_ON_A_VECTOR_LAYER).as_ref()));
    invoke(&mut s, CommandId::SelectAll);
    s.layer_action(LayerAction::Tool { tool: LayerCanvasTool::Paint }).unwrap();
    assert!(s.command(CommandId::ScaleRotate).enabled);
    invoke(&mut s, CommandId::ScaleRotate);
    assert_eq!(s.layer_interaction.tool, LayerCanvasTool::Move, "Edit › Transform shows the Move frame");
    invoke(&mut s, CommandId::ScaleOutlineWidths);
    assert!(!s.command(CommandId::ScaleOutlineWidths).selected);
    let frame = s.vector_frame().unwrap();
    let corner = frame.unit.map([1., 1.]);
    let from = surface_at(&s, corner);
    let to = surface_at(&s, [corner[0] + (corner[0] - frame.unit.map([0., 0.])[0]), corner[1] + (corner[1] - frame.unit.map([0., 0.])[1])]);
    press(&mut s, 100, &[from, Point { x: (from.x + to.x) * 0.5, y: (from.y + to.y) * 0.5 }, to]);
    assert_eq!(width(&s), before, "outline widths stay while the path doubles");
    invoke(&mut s, CommandId::ScaleOutlineWidths);
    let frame = s.vector_frame().unwrap();
    let corner = frame.unit.map([1., 1.]);
    let from = surface_at(&s, corner);
    let to = surface_at(&s, [corner[0] + (corner[0] - frame.unit.map([0., 0.])[0]), corner[1] + (corner[1] - frame.unit.map([0., 0.])[1])]);
    press(&mut s, 200, &[from, Point { x: (from.x + to.x) * 0.5, y: (from.y + to.y) * 0.5 }, to]);
    assert!((width(&s) - before * 2.).abs() < before * 0.05, "{} scales with the path", width(&s));
}

#[test]
fn path_commands_join_split_close_reverse_and_compound_selected_paths() {
    let (mut s, handle) = vector_session();
    draw_line(&mut s, 1, 30);
    draw_line(&mut s, 100, 30);
    let object = |s: &UiSession<Recorder>| s.engine.document().scene().vector_layer(handle).unwrap().clone();
    let edit = s.application_menu(ApplicationMenu::Edit);
    let path_menu = edit.sections.iter().flatten().find(|item| item.label == s.localization().text(MessageId::MENU_PATH).as_ref()).expect("Edit › Path");
    let commands: Vec<_> = path_menu.sections.iter().flatten().filter_map(|item| match item.action { Some(UiAction::Invoke { command }) => Some(command), _ => None }).collect();
    assert_eq!(commands, [CommandId::JoinPaths, CommandId::ClosePath, CommandId::SplitPath, CommandId::ReversePath, CommandId::MakeCompoundPath, CommandId::ReleaseCompoundPath]);
    assert!(!s.command(CommandId::JoinPaths).enabled);
    invoke(&mut s, CommandId::SelectAll);
    for (command, enabled) in [(CommandId::JoinPaths, true), (CommandId::ClosePath, true), (CommandId::SplitPath, false), (CommandId::ReversePath, true), (CommandId::MakeCompoundPath, true), (CommandId::ReleaseCompoundPath, false)] {
        assert_eq!(s.command(command).enabled, enabled, "{command:?}");
    }
    let before = object(&s);
    invoke(&mut s, CommandId::JoinPaths);
    let joined = object(&s);
    assert_eq!(joined.path_count(), 1, "two open lines join into one");
    let path = joined.paths().next().unwrap().1.clone();
    let anchors = path.geometry.subpaths()[0].len();
    assert_eq!(s.engine.document().working.vector_items.as_ref().map(|v| (*v.items).clone()), Some([path.id].into()));
    s.dispatch(UiAction::Invoke { command: CommandId::Undo }).unwrap();
    s.frame(5_000_000_000, 5_000_000_000).unwrap();
    assert_eq!(object(&s), before, "one undo step restores both lines");
    s.dispatch(UiAction::Invoke { command: CommandId::Redo }).unwrap();
    s.frame(5_100_000_000, 5_100_000_000).unwrap();
    invoke(&mut s, CommandId::SelectAll);
    invoke(&mut s, CommandId::ReversePath);
    let reversed = object(&s).paths().next().unwrap().1.clone();
    assert_eq!(reversed.geometry.subpaths()[0].anchor(0).point(), path.geometry.subpaths()[0].anchor(anchors - 1).point());
    invoke(&mut s, CommandId::EditPoints);
    let middle = reversed.geometry.subpaths()[0].anchor(anchors / 2).point();
    let at = surface_at(&s, middle);
    press(&mut s, 300, &[at, at]);
    assert!(s.command(CommandId::SplitPath).enabled, "a selected anchor can be split");
    invoke(&mut s, CommandId::SplitPath);
    assert_eq!(object(&s).path_count(), 2);
    assert_eq!(s.engine.document().working.vector_items.as_ref().map(|v| v.items.len()), Some(2), "both pieces stay selected");
    invoke(&mut s, CommandId::Move);
    invoke(&mut s, CommandId::SelectAll);
    invoke(&mut s, CommandId::MakeCompoundPath);
    assert_eq!(object(&s).path_count(), 1);
    assert!(s.command(CommandId::ReleaseCompoundPath).enabled);
    invoke(&mut s, CommandId::ReleaseCompoundPath);
    assert_eq!(object(&s).path_count(), 2);
    invoke(&mut s, CommandId::SelectAll);
    invoke(&mut s, CommandId::ClosePath);
    assert!(object(&s).paths().all(|(_, path)| path.geometry.subpaths()[0].closed));
}

#[test]
fn pens_and_the_path_tool_continue_a_selected_paths_open_end() {
    let (mut s, handle) = vector_session();
    let object = |s: &UiSession<Recorder>| s.engine.document().scene().vector_layer(handle).unwrap().clone();
    let stroke = |s: &mut UiSession<Recorder>, first: u64, from: [f64; 2], to: [f64; 2]| {
        let points: Vec<Point> = (0..=20).map(|i| { let u = f64::from(i) / 20.; surface_at(s, [from[0] + (to[0] - from[0]) * u, from[1] + (to[1] - from[1]) * u + (u * 3.1).sin() * 20.]) }).collect();
        press(s, first, &points);
    };
    stroke(&mut s, 1, [200., 200.], [600., 200.]);
    let first = object(&s).paths().next().unwrap().1.clone();
    let [_, end] = first.open_ends().unwrap();
    stroke(&mut s, 100, end, [900., 400.]);
    assert_eq!(object(&s).path_count(), 2, "without a selection a new line starts");
    s.dispatch(UiAction::Invoke { command: CommandId::Undo }).unwrap();
    s.frame(2_000_000_000, 2_000_000_000).unwrap();
    invoke(&mut s, CommandId::Move);
    let on = surface_at(&s, first.geometry.subpaths()[0].anchor(0).point());
    press(&mut s, 200, &[on, on]);
    s.select_brush(DefaultBrushPreset::GPen as u32).unwrap();
    stroke(&mut s, 300, end, [900., 400.]);
    let continued = object(&s);
    assert_eq!(continued.path_count(), 1, "a line from the selected path's end continues it");
    let path = continued.paths().next().unwrap().1;
    assert_eq!(path.id, first.id);
    assert!(path.geometry.subpaths()[0].len() > first.geometry.subpaths()[0].len());
    let [start, far] = path.open_ends().unwrap();
    assert_eq!(start, first.open_ends().unwrap()[0]);
    assert!((far[0] - 900.).abs() < 2. && (far[1] - 400.).abs() < 2., "{far:?}");
    s.dispatch(UiAction::Invoke { command: CommandId::Undo }).unwrap();
    s.frame(3_000_000_000, 3_000_000_000).unwrap();
    assert_eq!(object(&s).paths().next().unwrap().1, &first, "one undo step restores the line");
    s.dispatch(UiAction::Layer { action: LayerAction::Tool { tool: LayerCanvasTool::Figure { shape: FigureShape::Path, paint: FigurePaint::Outline } } }).unwrap();
    let click = |s: &mut UiSession<Recorder>, first: u64, at: [f64; 2]| { let p = surface_at(s, at); press(s, first, &[p, p]); };
    click(&mut s, 400, end);
    click(&mut s, 410, [700., 500.]);
    click(&mut s, 420, first.open_ends().unwrap()[0]);
    let closed = object(&s);
    assert_eq!(closed.path_count(), 1, "the Path tool continues the selected path");
    let path = closed.paths().next().unwrap().1;
    assert!(path.geometry.subpaths()[0].closed, "clicking the path's other end closes it");
    assert_eq!(path.geometry.subpaths()[0].len(), first.geometry.subpaths()[0].len() + 1);
}

#[test]
fn figure_options_shape_rectangles_polygons_and_stars_on_both_layer_kinds() {
    use layer_core::authored::vector::{LiveShape, ShapeKind};
    let (mut s, layer) = vector_session();
    let ids = |s: &UiSession<Recorder>| s.state.tool_settings.iter().map(|c| c.id).filter(|id| id.starts_with("figure_")).collect::<Vec<_>>();
    let tool = |s: &mut UiSession<Recorder>, shape| s.dispatch(UiAction::Layer { action: LayerAction::Tool { tool: LayerCanvasTool::Figure { shape, paint: FigurePaint::Outline } } }).unwrap();
    let drag = |s: &mut UiSession<Recorder>, first: u64| { let points = [surface_at(s, [100., 100.]), surface_at(s, [200., 160.]), surface_at(s, [300., 260.])]; press(s, first, &points); };
    let front = |s: &UiSession<Recorder>| s.engine.document().scene().vector_layer(layer).unwrap().paths().next().unwrap().1.shape.as_deref().copied();
    tool(&mut s, FigureShape::Rectangle);
    assert_eq!(ids(&s), ["figure_corner_radius"]);
    s.dispatch(UiAction::SetToolSetting { id: "figure_corner_radius".into(), value: 24. }).unwrap();
    drag(&mut s, 10);
    assert!(matches!(front(&s), Some(LiveShape { kind: ShapeKind::Rectangle { radii: [24., 24., 24., 24.] }, .. })));
    tool(&mut s, FigureShape::Polygon);
    assert_eq!(ids(&s), ["figure_corners"]);
    s.dispatch(UiAction::SetToolSetting { id: "figure_corners".into(), value: 7. }).unwrap();
    assert!(s.dispatch(UiAction::SetToolSetting { id: "figure_corners".into(), value: 2. }).is_err());
    drag(&mut s, 30);
    assert!(matches!(front(&s), Some(LiveShape { kind: ShapeKind::Polygon { corners: 7 }, .. })));
    tool(&mut s, FigureShape::Star);
    assert_eq!(ids(&s), ["figure_points", "figure_inner_radius"]);
    s.dispatch(UiAction::SetToolSetting { id: "figure_inner_radius".into(), value: 0.3 }).unwrap();
    s.dispatch(UiAction::ResetToolSetting { id: "figure_points".into() }).unwrap();
    drag(&mut s, 40);
    assert!(matches!(front(&s), Some(LiveShape { kind: ShapeKind::Star { points: 5, inner }, .. }) if (inner - 0.3).abs() < 1e-6));
    s.layer_action(LayerAction::New { group: false, clipped: false }).unwrap();
    let paint = s.engine.document().drawing_content().unwrap();
    let last = |s: &UiSession<Recorder>| s.engine.backend().pending_operations.last().filter(|(target, _)| *target == paint).map(|(_, op)| op.kind.clone());
    for (shape, first) in [(FigureShape::Polygon, 50), (FigureShape::Star, 60), (FigureShape::Rectangle, 70)] {
        tool(&mut s, shape);
        drag(&mut s, first);
        assert!(matches!(last(&s), Some(layer_core::RasterOperationKind::Bake { .. })), "{shape:?} paints through a bake");
    }
    s.dispatch(UiAction::SetToolSetting { id: "figure_corner_radius".into(), value: 0. }).unwrap();
    drag(&mut s, 80);
    assert!(matches!(last(&s), Some(layer_core::RasterOperationKind::Figure(_))), "square rectangles keep the raster figure");
}
