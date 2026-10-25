fn open_column_drawer(s: &mut UiSession<Recorder>) {
    customize(s, CustomizationAction::SetPanelVisible { panel: Panel::Stats, visible: true });
    let group = s.state.workspace.layout.panel_group(Panel::Stats).unwrap();
    customize(s, CustomizationAction::SetColumnCollapsed { group, collapsed: true });
    let column = s.state.workspace.layout.collapsed_column_for_group(group).unwrap();
    customize(s, CustomizationAction::SetColumnDrawers { column, drawers: true });
    customize(s, CustomizationAction::ToggleColumnDrawer { group, panel: Panel::Stats });
    assert!(!s.state.customization.column_drawers.is_empty());
}
fn hold(s: &mut UiSession<Recorder>, first: u64, points: &[[f64; 2]]) {
    for (i, point) in points.iter().enumerate() {
        let mut e = event(s, first + i as u64, if i == 0 { PenPhase::Down } else { PenPhase::Move }, 0.5);
        e.surface_position = surface_at(s, *point);
        s.pen(e);
        s.frame(e.timestamp_ns, e.timestamp_ns).unwrap();
    }
}
fn release(s: &mut UiSession<Recorder>, sequence: u64, point: [f64; 2]) {
    let mut e = event(s, sequence, PenPhase::Up, 0.5);
    e.surface_position = surface_at(s, point);
    s.pen(e);
    s.frame(e.timestamp_ns, e.timestamp_ns).unwrap();
}

#[test]
fn escape_during_a_drag_or_line_only_cancels_it() {
    let (mut s, handle) = vector_session();
    put_paths(&mut s, handle, vec![straight(1, &[[100., 300.], [700., 300.]], false)]);
    open_column_drawer(&mut s);
    let drawn = s.engine.document().scene().vector_layer(handle).unwrap().clone();
    invoke(&mut s, CommandId::Move);
    hold(&mut s, 10, &[[400., 300.], [420., 340.], [440., 380.]]);
    assert!(key(&mut s, "escape", true, false, false).handled);
    key(&mut s, "escape", false, false, false);
    assert!(!s.state.customization.column_drawers.is_empty(), "Escape during a drag keeps the column open");
    release(&mut s, 13, [460., 400.]);
    assert_eq!(s.engine.document().scene().vector_layer(handle).unwrap(), &drawn, "the cancelled drag leaves the path");

    invoke(&mut s, CommandId::Pen);
    s.select_brush(DefaultBrushPreset::GPen as u32).unwrap();
    hold(&mut s, 20, &[[100., 600.], [300., 620.], [500., 640.]]);
    assert!(key(&mut s, "escape", true, false, false).handled);
    key(&mut s, "escape", false, false, false);
    assert!(!s.state.customization.column_drawers.is_empty(), "Escape during a line keeps the column open");
    release(&mut s, 23, [600., 650.]);
    assert_eq!(path_ids(&s, handle).len(), 1, "the cancelled line adds no path");

    key(&mut s, "escape", true, false, false);
    key(&mut s, "escape", false, false, false);
    assert!(s.state.customization.column_drawers.is_empty(), "Escape with nothing in hand closes the column");
}

fn one_finger(s: &mut UiSession<Recorder>, phase: ContactPhase, at: [f64; 2], sequence: u64) -> bool {
    let p = surface_at(s, at);
    let reply = s.input(pointer_input(7, phase, PointerKind::Touch, PointerButton::Primary, [p.x, p.y], sequence * 10_000_000)).unwrap();
    if reply.paint {
        let mut e = event(s, sequence, pen_phase(phase), 1.);
        e.surface_position = p;
        e.tool = ToolKind::Finger;
        e.device_id = 1 << 40 | 7;
        s.pen(e);
    }
    s.frame(sequence * 10_000_000, sequence * 10_000_000).unwrap();
    reply.paint
}

#[test]
fn one_finger_moves_paths_and_edits_points_but_never_draws_or_erases() {
    let (mut s, handle) = vector_session();
    put_paths(&mut s, handle, vec![straight(1, &[[100., 300.], [700., 300.]], false)]);
    let path = |s: &UiSession<Recorder>| s.engine.document().scene().vector_layer(handle).unwrap().paths().next().unwrap().1.clone();
    let anchor = |s: &UiSession<Recorder>, i: usize| path(s).geometry.subpaths()[0].anchor(i).point();
    let near = |a: [f64; 2], b: [f64; 2]| (a[0] - b[0]).hypot(a[1] - b[1]) < 1.;
    let drawn = path(&s);
    assert!(!one_finger(&mut s, ContactPhase::Down, [400., 300.], 10), "one finger never draws a line");
    one_finger(&mut s, ContactPhase::Up, [400., 300.], 11);
    invoke(&mut s, CommandId::Eraser);
    assert!(!one_finger(&mut s, ContactPhase::Down, [400., 300.], 12), "one finger never erases");
    one_finger(&mut s, ContactPhase::Up, [400., 300.], 13);
    assert_eq!(path(&s), drawn);

    invoke(&mut s, CommandId::Move);
    assert!(!one_finger(&mut s, ContactPhase::Down, [400., 600.], 20), "a finger off the paths navigates");
    one_finger(&mut s, ContactPhase::Up, [400., 600.], 21);
    assert!(one_finger(&mut s, ContactPhase::Down, [400., 300.], 30), "a finger on a path takes it");
    one_finger(&mut s, ContactPhase::Move, [450., 350.], 31);
    one_finger(&mut s, ContactPhase::Up, [500., 400.], 32);
    assert!(near(anchor(&s, 0), [200., 400.]) && near(anchor(&s, 1), [800., 400.]), "{:?}", path(&s).geometry);

    s.dispatch(UiAction::Layer { action: LayerAction::Tool { tool: LayerCanvasTool::EditPoints } }).unwrap();
    assert!(one_finger(&mut s, ContactPhase::Down, [200., 400.], 40), "a finger on an anchor takes it");
    one_finger(&mut s, ContactPhase::Move, [200., 450.], 41);
    one_finger(&mut s, ContactPhase::Up, [200., 500.], 42);
    assert!(near(anchor(&s, 0), [200., 500.]) && near(anchor(&s, 1), [800., 400.]), "{:?}", path(&s).geometry);
    assert!(one_finger(&mut s, ContactPhase::Down, [500., 450.], 50), "a finger on a segment takes it");
    one_finger(&mut s, ContactPhase::Up, [500., 450.], 51);
    assert_eq!(path(&s).geometry.anchor_count(), 3, "a tap on a segment adds an anchor");
}

#[test]
fn the_layers_footer_lists_its_lowest_buttons_in_the_actions_menu_when_narrow() {
    let s = session(Platform::Gtk);
    let footer = s.state.layer_tools.footer.clone();
    assert_eq!(footer.iter().map(|slot| slot.button).collect::<Vec<_>>(), LayerFooterButton::ORDER);
    let m = crate::PANEL_CONTROL_METRICS;
    let need = |count: usize| 2. * crate::LAYER_EDGE_INSET + (count + 1) as f32 * m.control_height + count as f32 * m.compound_spacing;
    for viewport in [1600., 1440., 1280., 640.] {
        let width = Panel::Layers.default_width(viewport);
        let shown: Vec<_> = footer.iter().filter(|slot| slot.shown(width)).map(|slot| slot.button).collect();
        assert!(need(shown.len()) <= width, "{viewport}: {shown:?} need {} of {width}", need(shown.len()));
        assert!(shown.len() == footer.len() || need(shown.len() + 1) > width, "{viewport}: a hidden button would fit");
        assert!(shown.contains(&LayerFooterButton::NewLayer) && shown.contains(&LayerFooterButton::NewVectorLayer));
        let overflow = layer_footer_overflow(&footer, width);
        assert_eq!(overflow.len() + shown.len(), footer.len());
        assert!(overflow.iter().all(|item| item.action.is_some() || !item.sections.is_empty() || !item.enabled));
    }
    assert!(footer.iter().all(|slot| slot.shown(Panel::Layers.default_width(1600.))), "a wide Layers panel shows every button");
    let narrow = layer_footer_overflow(&footer, crate::PANEL_MIN_WIDTH);
    assert_eq!(narrow.iter().map(|item| item.label.clone()).collect::<Vec<_>>(), [s.localization().text(MessageId::COMMAND_NEW_SELECTION_LAYER).to_string()]);
    let tight = footer.iter().filter(|slot| slot.shown(need(2))).map(|slot| slot.button).collect::<Vec<_>>();
    assert_eq!(tight, [LayerFooterButton::NewLayer, LayerFooterButton::NewVectorLayer], "New layer and New vector layer stay longest");
}

#[test]
fn notices_carry_a_localized_accessible_name() {
    let (mut s, _) = vector_session();
    s.select_brush(DefaultBrushPreset::Pencil as u32).unwrap();
    draw_line(&mut s, 1, 10);
    assert_eq!(s.state.notice.as_ref().expect("a vector refusal").name, "Notice");
    assert!(s.set_localization(Localizer::shared(UiLanguage::German)));
    assert_eq!(s.state.notice.as_ref().unwrap().name, "Hinweis");
}

#[test]
fn undo_pressed_during_a_held_path_drag_waits_for_the_drag_and_stays_refused() {
    let (mut s, handle) = vector_session();
    put_paths(&mut s, handle, vec![straight(1, &[[100., 300.], [700., 300.]], false), straight(2, &[[100., 500.], [700., 500.]], false)]);
    invoke(&mut s, CommandId::Move);
    let at = surface_at(&s, [400., 500.]);
    press(&mut s, 5, &[at, at]);
    s.renderer_mut().painting_unready = true;
    hold(&mut s, 100, &[[400., 500.], [400., 540.], [400., 580.]]);
    key(&mut s, "z", true, true, false);
    key(&mut s, "z", false, true, false);
    release(&mut s, 103, [400., 600.]);
    s.renderer_mut().painting_unready = false;
    for frame in 20..40 { s.frame(frame * 10_000_000, frame * 10_000_000).unwrap(); }
    let object = s.engine.document().scene().vector_layer(handle).unwrap().clone();
    let first = |id: u64| object.paths().find(|(_, path)| path.id.0 == id).map(|(_, path)| path.geometry.subpaths()[0].anchor(0).point());
    assert_eq!(first(1), Some([100., 300.]), "Ctrl+Z during the drag undoes nothing else");
    assert_eq!(first(2), Some([100., 600.]), "the drag lands");
}
