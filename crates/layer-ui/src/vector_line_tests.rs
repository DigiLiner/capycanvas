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
    for (shape, paint, first) in [(FigureShape::Rectangle, FigurePaint::Both, 100), (FigureShape::Ellipse, FigurePaint::Fill, 200), (FigureShape::Line, FigurePaint::Outline, 300)] {
        s.dispatch(UiAction::Layer { action: LayerAction::Tool { tool: LayerCanvasTool::Figure { shape, paint } } }).unwrap();
        assert!(!prohibited(&mut s), "{shape:?} works on a vector layer");
        drag(&mut s, first, [100., 120.], [260., 200.]);
        let path = front(&s).expect("the figure becomes a path");
        assert_eq!(s.engine.document().working.vector_items.as_ref().map(|selection| (*selection.items).clone()), Some([path.id].into()), "the new path is selected");
        let [x0, y0, x1, y1] = path.geometry.hull();
        assert!((x0 - 100.).abs() < 1e-3 && (y0 - 120.).abs() < 1e-3 && (x1 - 260.).abs() < 1e-3 && (y1 - 200.).abs() < 1e-3, "{shape:?} spans the drag: {:?}", path.geometry.hull());
        match shape {
            FigureShape::Rectangle => {
                assert!(matches!(path.shape.as_deref(), Some(layer_core::authored::vector::LiveShape { kind: ShapeKind::Rectangle { .. }, .. })));
                assert!(matches!(path.style.entries[..], [PaintEntry::Fill(_), PaintEntry::Stroke(ref s)] if s.join == Join::Miter), "Both fills below the outline");
            }
            FigureShape::Ellipse => {
                assert!(matches!(path.shape.as_deref(), Some(layer_core::authored::vector::LiveShape { kind: ShapeKind::Ellipse, .. })));
                assert!(matches!(path.style.entries[..], [PaintEntry::Fill(_)]));
            }
            FigureShape::Line | FigureShape::Path => assert!(path.shape.is_none() && matches!(path.style.entries[..], [PaintEntry::Stroke(ref s)] if s.cap == Cap::Round)),
        }
    }
    assert_eq!(s.engine.document().scene().vector_layer(layer).unwrap().path_count(), 3);
    s.dispatch(UiAction::Invoke { command: CommandId::Undo }).unwrap();
    s.frame(9_000_000_000, 9_000_000_000).unwrap();
    assert_eq!(s.engine.document().scene().vector_layer(layer).unwrap().path_count(), 2, "each figure is one undo step");
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
    click(&mut s, 300, [100., 100.]);
    assert!(s.state.notice.is_some(), "paint layers refuse the Path tool");
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
