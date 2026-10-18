use super::new_photo::{chooser, ready, response};
use super::*;
use layer_core::{DefaultBrushPreset, EffectValue};

fn shade(w: &Rc<Workspace>, origin: [f32; 2], pressure: f32) {
    let deadline = Instant::now() + Duration::from_secs(30);
    until(|| Instant::now() > deadline || w.gpu.borrow().as_ref().is_some_and(|g| {
        let engine = g.session.engine();
        engine.backend().paint_ready(engine.document(), engine.brush(), engine.transform_preview().is_some())
    }), "brush and paper are ready");
    let camera = state(w).camera;
    let points: Vec<[f32; 2]> = (0..=180).map(|i| {
        let t = i as f32 / 180.;
        [origin[0] + 220. * ((t * 18.).fract() - 0.5).abs() * 2., origin[1] + 140. * t]
    }).collect();
    for (i, point) in points.iter().enumerate() {
        let phase = match i { 0 => PenPhase::Down, _ if i + 1 == points.len() => PenPhase::Up, _ => PenPhase::Move };
        let now = glib::monotonic_time() as u64 * 1000;
        let event = pen_event(&camera, *point, phase, now);
        w.input.send(w, PenEvent { device_id: 93, timestamp_ns: now, pressure, ..event });
        pump(8);
    }
    pump(200);
}

fn texture(w: &Rc<Workspace>) -> layer_core::TextureSource {
    ui_session(w).engine().document().paper(None).texture
}

#[test]
#[ignore = "private Wayland display and hardware GPU"]
#[allow(deprecated)]
fn native_paper_picker_relief_and_scale() {
    glib::set_prgname(Some("capy-canvas-test"));
    let app = native_test_app("art.capycanvas.PaperPicker");
    let localization = layer_ui::Localizer::shared(layer_ui::UiLanguage::English);
    let project = layer_ui::NewDocumentOptions { extent: [1600, 1000], ..Default::default() }.project(&localization).unwrap();
    let w = Workspace::with_project(&app, Some((project, None)));
    apply_fixture_theme(&w);
    w.window.maximize();
    w.window.present();
    ready(&w);
    let theme = std::env::var("CAPY_NATIVE_TEST_THEME").unwrap_or_else(|_| "default".into());
    let directory = std::path::PathBuf::from(std::env::var("LAYER_TEST_ARTIFACTS").unwrap_or_else(|_| "../../artifacts/paper".into()));
    std::fs::create_dir_all(&directory).unwrap();
    let path = |name: &str| directory.join(format!("{name}-{theme}.png"));

    w.dispatch(UiAction::SelectBrush { id: DefaultBrushPreset::Pencil as u32 });
    shade(&w, [180., 200.], 0.45);
    w.dispatch(UiAction::SelectBrush { id: DefaultBrushPreset::Charcoal as u32 });
    shade(&w, [480., 200.], 0.45);

    let paper = layer_ui::occurrence_token(layer_core::paper::paper_occurrence(ui_session(&w).engine().document().scene(), None).unwrap());
    w.dispatch(UiAction::SelectLayer { id: paper });
    ready(&w);
    let properties = state(&w).layer_properties;
    assert_eq!(properties.name, "Paper");
    assert_eq!(properties.controls.iter().map(|c| c.key.as_str()).collect::<Vec<_>>(), ["color", "texture", "scale"]);
    save_snapshot(&w, 200, || path("paper-properties"));

    let button: gtk::MenuButton = named(w.window.upcast_ref(), "property-texture");
    assert!(button.is_mapped(), "the Texture tile is visible in Properties");
    button.popup();
    pump(300);
    let popover = button.popover().unwrap();
    super::zoom_readout::save_widget(&popover, &path("paper-picker"));
    let grid: gtk::FlowBox = descendant(&popover).unwrap();
    assert_eq!(grid.selected_children().first().map(|c| c.index()), Some(3), "Drawing paper is selected");
    let before = ui_session(&w).engine().checkpoint();
    grid.child_at_index(2).unwrap().activate();
    pump(100);
    assert!(!popover.is_visible(), "choosing a paper closes the picker");
    assert_eq!(texture(&w), layer_core::paper::built_in_paper("rough").unwrap().texture);
    pump(250);
    save_snapshot(&w, 50, || path("paper-relief-rough"));
    assert!(ui_session(&w).engine().backend().has_paper_overlay(), "the relief shows after a choice");
    pump(1800);
    assert!(!ui_session(&w).engine().backend().has_paper_overlay(), "and then hides");

    let scale: crate::number_control::NumberControl = named(w.window.upcast_ref(), "property-scale");
    edit_number(&scale, "200");
    pump(200);
    assert_eq!(state(&w).layer_properties.controls[2].value, EffectValue::Number(2.));
    save_snapshot(&w, 50, || path("paper-relief-scaled"));
    pump(1800);

    let scan = directory.join("cotton-rag.png");
    let pixels: Vec<u8> = (0..400u32 * 300).flat_map(|i| {
        let (x, y) = (i % 400, i / 400);
        let v = (128. + 60. * ((x as f32 * 0.37).sin() * (y as f32 * 0.29).cos()) + ((x * 7919 + y * 104_729) % 61) as f32 - 30.) as u8;
        [v, v, v, 255]
    }).collect();
    gtk::gdk::MemoryTexture::new(400, 300, gtk::gdk::MemoryFormat::R8g8b8a8, &glib::Bytes::from_owned(pixels), 1600).save_to_png(&scan).unwrap();
    button.popup();
    pump(300);
    let import: gtk::FlowBoxChild = named(popover.upcast_ref(), "paper-import");
    import.activate();
    let file = chooser();
    file.set_file(&gtk::gio::File::for_path(&scan)).unwrap();
    pump(150);
    file.response(gtk::ResponseType::Accept);
    until(|| matches!(texture(&w), layer_core::TextureSource::Image(_)), "the imported paper is applied");
    let tiles = || state(&w).layer_properties.controls[1].paper.clone().unwrap().tiles;
    assert_eq!(tiles().len(), 14);
    assert_eq!(tiles()[13].label.as_ref(), "cotton-rag");
    assert_eq!(state(&w).layer_properties.controls[1].value, EffectValue::Choice(13));
    let imported = ui_session(&w).engine().checkpoint();
    let remembered = ui_session(&w).state().settings.new_document.paper.clone().unwrap();
    assert_eq!(remembered.len(), 64, "new drawings start on the imported paper");
    pump(1800);
    let picker = || {
        let button: gtk::MenuButton = named(w.window.upcast_ref(), "property-texture");
        button.popup();
        pump(300);
        let popover = button.popover().unwrap();
        let grid: gtk::FlowBox = descendant(&popover).unwrap();
        (popover, grid)
    };
    let (popover, _) = picker();
    super::zoom_readout::save_widget(&popover, &path("paper-picker-imported"));
    let menu = |index: i32, label: &str| {
        let (_, grid) = picker();
        let child = grid.child_at_index(index).unwrap();
        let press = child.observe_controllers().iter::<glib::Object>().filter_map(Result::ok)
            .find_map(|controller| controller.downcast::<gtk::GestureClick>().ok()).expect("the tile has a menu");
        press.emit_by_name::<()>("pressed", &[&1i32, &0f64, &0f64]);
        pump(200);
        let menu: gtk::Popover = named(child.upcast_ref(), "paper-tile-menu");
        assert!(menu.is_visible());
        click(&find_button(menu.upcast_ref(), label).unwrap());
        pump(200);
    };
    menu(13, "Rename…");
    let dialog = w.window.visible_dialog().unwrap();
    named::<adw::EntryRow>(dialog.upcast_ref(), "paper-rename-name").set_text("Cotton rag");
    pump(100);
    response(&w, "rename");
    until(|| tiles()[13].label.as_ref() == "Cotton rag", "the paper is renamed");
    menu(13, "Delete");
    until(|| tiles().len() == 14 && tiles()[0].label.as_ref() == "This drawing’s paper", "the drawing keeps its deleted paper");
    assert!(matches!(texture(&w), layer_core::TextureSource::Image(_)));
    menu(0, "Add to Library");
    until(|| tiles().len() == 14 && tiles()[13].label.as_ref() == state(&w).document_file.title(), "the drawing's paper returns to the library");
    assert_eq!(state(&w).layer_properties.controls[1].value, EffectValue::Choice(13));

    let ink = layer_ui::occurrence_token(ui_session(&w).engine().document().scene().order()[0]);
    w.dispatch(UiAction::SelectLayer { id: ink });
    w.dispatch(UiAction::SelectBrush { id: DefaultBrushPreset::Pencil as u32 });
    shade(&w, [180., 520.], 0.45);
    w.dispatch(UiAction::SelectBrush { id: DefaultBrushPreset::Charcoal as u32 });
    shade(&w, [480., 520.], 0.45);
    ready(&w);
    save_snapshot(&w, 200, || path("paper-rough-strokes"));

    for _ in 0..2 { w.dispatch(UiAction::Invoke { command: CommandId::Undo }); }
    ready(&w);
    assert_eq!(ui_session(&w).engine().checkpoint(), imported, "each stroke is one step; library changes are none");
    for _ in 0..3 { w.dispatch(UiAction::Invoke { command: CommandId::Undo }); }
    ready(&w);
    assert_eq!(ui_session(&w).engine().checkpoint(), before, "the import, Scale and the choice are one step each");
    assert_eq!(texture(&w), layer_core::paper::default_texture());
}

#[test]
#[ignore = "private Wayland display and hardware GPU"]
#[allow(deprecated)]
fn native_fill_drawing_targets_and_paper_order() {
    let app = native_test_app("art.capycanvas.FillTargets");
    let w = fixture_workspace(&app);
    apply_fixture_theme(&w);
    w.window.maximize();
    w.window.present();
    ready(&w);
    let mut input = RemoteInput::new().settle_ms(120);
    input.ready();
    let directory = std::path::PathBuf::from(std::env::var("LAYER_TEST_ARTIFACTS").unwrap_or_else(|_| "../../artifacts/paper-bottom".into()));
    std::fs::create_dir_all(&directory).unwrap();
    for program in ["paper", "solid_color", "gradient_fill"] {
        w.dispatch(UiAction::Effect { action: layer_ui::EffectAction::Insert { effect: program.into() } });
        ready(&w);
        let fill = layer_ui::occurrence_token(ui_session(&w).engine().document().working.occurrence.unwrap());
        w.dispatch(UiAction::Layer { action: layer_ui::LayerAction::New { group: false, clipped: false } });
        ready(&w);
        let ink = layer_ui::occurrence_token(ui_session(&w).engine().document().working.occurrence.unwrap());
        let change = ui_session_mut(&w).reveal_panel(Panel::Layers);
        w.changed(change);
        ready(&w);
        let row: gtk::Widget = named(w.window.upcast_ref(), &format!("art-layer-{fill}"));
        assert!(row.is_mapped());
        let bounds = find_css(&row, "layer-name").unwrap().compute_bounds(&w.surface).unwrap();
        let point = [bounds.x() + bounds.width() * 0.5, bounds.y() + bounds.height() * 0.5];
        input.perform(serde_json::json!([{"point":point}, {"down":true}, {"down":false}]));
        ready(&w);
        let fill_row = || state(&w).layers.into_iter().find(|row| row.id == fill).unwrap();
        let ink_row = || state(&w).layers.into_iter().find(|row| row.id == ink).unwrap();
        save_snapshot(&w, 100, || directory.join(format!("{program}-selected.png")));
        assert!(fill_row().editing && ink_row().drawing, "{program}: selecting a fill paints above it: fill={:?}, ink={:?}", fill_row(), ink_row());
        if program == "paper" {
            assert_eq!(state(&w).layers.last().unwrap().id, fill);
            assert!(!row.last_child().unwrap().is_visible());
        }
        let before = ink_row().paint_revision;
        shade(&w, [180., 200.], 0.45);
        ready(&w);
        assert_ne!(ink_row().paint_revision, before, "{program}: the layer above receives the stroke");
        w.dispatch(UiAction::Invoke { command: CommandId::Undo });
        ready(&w);
        w.dispatch(UiAction::SelectLayer { id: ink });
        w.dispatch(UiAction::Layer { action: layer_ui::LayerAction::GroupSelected });
        w.dispatch(UiAction::SelectLayer { id: fill });
        ready(&w);
        assert!(state(&w).layers.iter().all(|row| !row.drawing), "{program}: a grouped layer is not a fallback");
        w.dispatch(UiAction::Invoke { command: CommandId::Undo });
        w.dispatch(UiAction::Layer { action: layer_ui::LayerAction::AddMask { id: fill, replace: false } });
        w.dispatch(UiAction::SelectLayer { id: fill });
        ready(&w);
        assert!(fill_row().mask_selected && !ink_row().drawing, "{program}: selecting a masked fill targets its mask");
        let before = fill_row().mask_revision;
        shade(&w, [180., 200.], 0.45);
        ready(&w);
        assert_ne!(fill_row().mask_revision, before, "{program}: the mask receives the stroke");
        w.dispatch(UiAction::Invoke { command: CommandId::Undo });
        w.dispatch(UiAction::Layer { action: layer_ui::LayerAction::DeleteMask { id: fill } });
        w.dispatch(UiAction::Layer { action: layer_ui::LayerAction::Delete { id: ink } });
        if program != "paper" { w.dispatch(UiAction::Layer { action: layer_ui::LayerAction::Delete { id: fill } }); }
        ready(&w);
    }
    let paper = layer_ui::occurrence_token(ui_session(&w).engine().document().bottom_paper().unwrap());
    w.dispatch(UiAction::Layer { action: layer_ui::LayerAction::Delete { id: paper } });
    ready(&w);
    let add = || widgets(w.window.upcast_ref()).filter_map(|node| node.downcast::<gtk::Box>().ok())
        .find(|row| row.widget_name() == "layer-add-paper" && row.is_mapped());
    let row = add().unwrap();
    let surface = row.parent().unwrap();
    assert!(descendant::<gtk::Button>(&row).is_none(), "the list row owns all action feedback");
    let label: gtk::Label = descendant(&row).unwrap();
    assert_eq!(label.layout().iter().run_readonly().unwrap().item().analysis().font().describe().weight(), gtk::pango::Weight::Normal, "Add paper uses regular text like the layer names");
    let bounds = row.compute_bounds(&w.surface).unwrap();
    let label_bounds = label.compute_bounds(&w.surface).unwrap();
    let (_, text_y) = label.layout_offsets();
    let (_, text_bounds) = label.layout().pixel_extents();
    let text_center = label_bounds.y() + text_y as f32 + text_bounds.y() as f32 + text_bounds.height() as f32 * 0.5;
    assert!((text_center - bounds.y() - bounds.height() * 0.5).abs() <= 1., "the text is vertically centered");
    assert!(bounds.height() - text_bounds.height() as f32 >= (crate::panel_controls::CONTROL_SPACING * 2) as f32, "the text has room above and below");
    let check_position = || {
        let row = add().unwrap();
        let last = state(&w).layers.last().unwrap().id;
        let last: gtk::Widget = named(w.window.upcast_ref(), &format!("art-layer-{last}"));
        let layer_bounds = last.compute_bounds(&w.surface).unwrap();
        let bounds = row.compute_bounds(&w.surface).unwrap();
        assert_eq!(bounds.height(), layer_bounds.height(), "Add paper has the standard layer row height");
        assert!((bounds.y() - layer_bounds.y() - layer_bounds.height()).abs() <= 2., "Add paper follows the last layer: {layer_bounds:?} {bounds:?}");
        let content = row.first_child().unwrap().compute_bounds(&w.surface).unwrap();
        assert!((content.x() + content.width() * 0.5 - bounds.x() - bounds.width() * 0.5).abs() <= 1., "the icon and label are centered together");
    };
    check_position();
    input.perform(serde_json::json!([{"point":[800., 500.]}]));
    save_snapshot(&w, 100, || directory.join("add-paper.png"));
    let bounds = row.compute_bounds(&w.surface).unwrap();
    let point = [bounds.x() + bounds.width() * 0.5, bounds.y() + bounds.height() * 0.5];
    input.perform(serde_json::json!([{"point":point}]));
    assert!(surface.state_flags().contains(gtk::StateFlags::PRELIGHT));
    let check_feedback = || {
        let snapshot = gtk::Snapshot::new();
        snapshot.render_background(&surface.style_context(), 0., 0., surface.width() as f64, surface.height() as f64);
        assert!(snapshot.to_node().unwrap().is::<gtk::gsk::ColorNode>(), "the feedback is a single square surface");
    };
    check_feedback();
    save_snapshot(&w, 100, || directory.join("add-paper-hover.png"));
    input.perform(serde_json::json!([{"down":true}]));
    assert!(surface.state_flags().contains(gtk::StateFlags::ACTIVE));
    check_feedback();
    save_snapshot(&w, 100, || directory.join("add-paper-active.png"));
    input.perform(serde_json::json!([{"point":[800., 500.]}, {"down":false}]));
    ready(&w);
    assert!(ui_session(&w).engine().document().bottom_paper().is_none(), "releasing outside cancels the action");
    assert!(!surface.state_flags().intersects(gtk::StateFlags::PRELIGHT | gtk::StateFlags::ACTIVE), "cancel clears hover and pressed feedback");
    input.perform(serde_json::json!([{"point":point}, {"down":true}, {"down":false}]));
    ready(&w);
    assert!(ui_session(&w).engine().document().bottom_paper().is_some());
    assert!(add().is_none());
    save_snapshot(&w, 100, || directory.join("paper-properties.png"));
    w.dispatch(UiAction::Invoke { command: CommandId::Undo });
    let change = ui_session_mut(&w).reveal_panel(Panel::Layers);
    w.changed(change);
    ready(&w);
    assert!(add().is_some());
    check_position();
    let before = ui_session(&w).engine().checkpoint();
    let surface = add().unwrap().parent().unwrap();
    assert!(surface.grab_focus());
    input.key(0xff52);
    input.key(0xff54);
    assert!(surface.has_focus() && surface.state_flags().contains(gtk::StateFlags::FOCUS_VISIBLE));
    save_snapshot(&w, 100, || directory.join("add-paper-focus.png"));
    input.key(0xff0d);
    ready(&w);
    assert!(ui_session(&w).engine().document().bottom_paper().is_some(), "Enter activates the focused row");
    w.dispatch(UiAction::Invoke { command: CommandId::Undo });
    let change = ui_session_mut(&w).reveal_panel(Panel::Layers);
    w.changed(change);
    ready(&w);
    assert_eq!(ui_session(&w).engine().checkpoint(), before, "keyboard activation is one undo step");
    w.dispatch(UiAction::Invoke { command: CommandId::Redo });
    ready(&w);
    assert!(add().is_none());
    w.dispatch(UiAction::Invoke { command: CommandId::Undo });
    for _ in 0..30 { w.dispatch(UiAction::Layer { action: layer_ui::LayerAction::New { group: false, clipped: false } }); }
    ready(&w);
    let scroll = w.layer_panel.list.vadjustment();
    assert!(scroll.upper() > scroll.page_size());
    scroll.set_value(0.);
    pump(120);
    assert!(add().is_none_or(|row| row.compute_bounds(&w.layer_panel.list).is_none_or(|bounds| bounds.y() >= w.layer_panel.list.height() as f32)), "Add paper scrolls out of the viewport with the layers");
    scroll.set_value(scroll.upper() - scroll.page_size());
    until(|| add().is_some(), "the final list item is visible");
    pump(120);
    check_position();
    w.dispatch(UiAction::Layer { action: layer_ui::LayerAction::SelectAllLayers { selected: true } });
    w.dispatch(UiAction::Layer { action: layer_ui::LayerAction::DeleteSelected });
    ready(&w);
    assert!(state(&w).layers.is_empty());
    let bounds = add().unwrap().compute_bounds(&w.layer_panel.list).unwrap();
    assert!(bounds.y().abs() <= 2., "Add paper starts the empty layer list");
}
