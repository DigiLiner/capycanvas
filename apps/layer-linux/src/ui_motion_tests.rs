// Compositor recordings of workspace motion, analysed by tools/visual/ui_motion.py.
use super::*;

#[test]
#[ignore = "isolated Mutter recording; LAYER_NATIVE_CAPTURE_DIR=<dir> workspace-motion.sh gtk --native-test=native_ui_motion_recording --native-storage"]
fn native_ui_motion_recording() {
    let mut input = RemoteInput::new().settle_ms(1200).timeout_secs(30);
    let app = native_test_app("art.capycanvas.UiMotion");
    gtk::Settings::default().unwrap().set_gtk_enable_animations(true);
    let w = Workspace::new(&app);
    apply_fixture_theme(&w);
    w.window.maximize();
    w.window.present();
    wait_workspaces(&w);
    pump(1500);
    stripes(&w);
    input.ready();
    pump(500);
    let mut record = |name: &str, widget: &gtk::Widget| {
        let point = screen_point(widget, &w.window, [0.5, 0.5]);
        input.perform(serde_json::json!([
            {"point": point},
            {"wait_ms": 300},
            {"record_start": name},
            {"down": true},
            {"down": false},
            {"wait_ms": 1200},
            {"record_stop": true},
        ]));
        until(|| !w.workspaces.busy(), "workspace settled");
    };
    let switcher = |name: &str| find_named(w.header.root.upcast_ref(), &format!("workspace-switch-{name}")).unwrap();
    let column_icon = |panel: Panel| {
        let resolved = w.resolved();
        let column = resolved.collapsed.iter().find(|c| c.groups.iter().any(|g| g.icons.iter().any(|i| i.panel == panel))).unwrap();
        w.columns.button(column.id, panel).unwrap().upcast::<gtk::Widget>()
    };
    record("switch-to-paint", &switcher("illustrator"));
    record("close-column", &column_icon(Panel::Layers));
    record("open-column", &column_icon(Panel::Layers));
    let proof = w.groups.borrow().iter().flat_map(|g| g.tabs.iter()).find(|(p, _)| *p == Panel::Proof).unwrap().1.clone();
    record("select-tab", proof.upcast_ref());
    record("switch-to-photo", &switcher("photographer"));
    record("switch-to-sketch", &switcher("painter"));
    record("switch-back-to-paint", &switcher("illustrator"));
    input.finish();
}

fn stripes(w: &Rc<Workspace>) {
    w.dispatch(UiAction::Invoke { command: CommandId::FitCanvas });
    for _ in 0..3 {
        w.dispatch(UiAction::Invoke { command: CommandId::ZoomIn });
    }
    pump(300);
    let [a, b, c, e, x0, y0] = state(w).camera.document_to_surface();
    let det = a * e - b * c;
    let document = |x: f32, y: f32| {
        let (x, y) = (x - x0, y - y0);
        [(e * x - c * y) / det, (a * y - b * x) / det]
    };
    let [width, height] = [w.surface.width() as f32, w.surface.height() as f32];
    w.dispatch(UiAction::SelectBrush { id: layer_core::DefaultBrushPreset::GPen as u32 });
    w.dispatch(UiAction::SetBrushSize { value: 6. / det.abs().sqrt() });
    w.dispatch(UiAction::SetColor { rgba: [0., 0., 0., 1.] });
    let mut x = 4.;
    while x < width {
        native_pen_path(w, &[document(x, 0.), document(x, height * 0.5), document(x, height)]);
        x += 13.;
    }
    pump(500);
}

#[test]
#[ignore = "isolated Mutter compositor captures; LAYER_NATIVE_CAPTURE_DIR=<dir> workspace-motion.sh gtk --native-test=native_active_tab_matches_panel"]
fn native_active_tab_matches_panel() {
    let captures = std::path::PathBuf::from(std::env::var("LAYER_NATIVE_CAPTURE_DIR").expect("compositor captures"));
    let mut input = RemoteInput::new().settle_ms(300).timeout_secs(30);
    let app = native_test_app("art.capycanvas.ActiveTabShade");
    let w = fixture_workspace(&app);
    w.window.maximize();
    w.window.present();
    pump(1600);
    input.ready();
    pump(300);
    let origin = w.surface.compute_point(&w.window, &gtk::graphene::Point::zero()).unwrap();
    let (window_x, window_y) = w.window.surface_transform();
    let offset = [origin.x() + window_x as f32, origin.y() + window_y as f32];
    let mut mismatches = Vec::new();
    for theme in [Theme::Dark, Theme::Light] {
        w.dispatch(UiAction::SetTheme { theme: Some(theme) });
        for level in [layer_ui::Transparency::Off, layer_ui::Transparency::Low, layer_ui::Transparency::Medium, layer_ui::Transparency::High] {
            use_transparency(&w, level);
            pump(400);
            let name = format!("tab-{theme:?}-{level:?}").to_lowercase();
            input.perform(serde_json::json!([{ "wait_ms": 300 }, { "capture": name }]));
            let texture = gdk::Texture::from_filename(captures.join(format!("{name}.png"))).unwrap();
            let stride = texture.width() as usize * 4;
            let mut pixels = vec![0; stride * texture.height() as usize];
            texture.download(&mut pixels, stride);
            let sample = |x: f32, y: f32| {
                let i = (y + offset[1]).round() as usize * stride + (x + offset[0]).round() as usize * 4;
                [pixels[i + 2], pixels[i + 1], pixels[i]]
            };
            for group in w.groups.borrow().iter().filter(|g| g.tabs_visible) {
                let Some((_, tab)) = group.tabs.iter().find(|(_, tab)| tab.has_css_class("selected-tool")) else { continue };
                let tab = tab.compute_bounds(&w.surface).unwrap();
                let body = group.stack.compute_bounds(&w.surface).unwrap();
                let tab_pixel = sample(tab.x() + 4., tab.y() + tab.height() - 3.);
                let body_pixel = sample(body.x() + 3., body.y() + 3.);
                if tab_pixel != body_pixel {
                    mismatches.push(format!("{name} group {}: tab {tab_pixel:?} panel {body_pixel:?}", group.id));
                }
            }
        }
    }
    input.finish();
    assert!(mismatches.is_empty(), "active tabs differ from their panels:\n{}", mismatches.join("\n"));
}

#[test]
#[ignore = "isolated Mutter captures; LAYER_NATIVE_CAPTURE_DIR=<dir> workspace-motion.sh gtk --native-test=native_workspace_switch_repaints_vacated_regions --native-storage"]
fn native_workspace_switch_repaints_vacated_regions() {
    let captures = std::path::PathBuf::from(std::env::var("LAYER_NATIVE_CAPTURE_DIR").expect("compositor captures"));
    let mut input = RemoteInput::new().settle_ms(1200).timeout_secs(30);
    let app = native_test_app("art.capycanvas.VacatedRegions");
    let w = Workspace::new(&app);
    apply_fixture_theme(&w);
    w.window.maximize();
    w.window.present();
    wait_workspaces(&w);
    pump(1500);
    stripes(&w);
    input.ready();
    pump(500);
    let mut switch = |name: &str| {
        let button = find_named(w.header.root.upcast_ref(), &format!("workspace-switch-{name}")).unwrap();
        let point = screen_point(&button, &w.window, [0.5, 0.5]);
        input.perform(serde_json::json!([{"point": point}, {"wait_ms": 300}, {"down": true}, {"down": false}, {"wait_ms": 1200}]));
        until(|| !w.workspaces.busy(), "workspace settled");
    };
    switch("photographer");
    switch("painter");
    input.perform(serde_json::json!([{"wait_ms": 300}, {"capture": "switched"}]));
    w.window.unmaximize();
    pump(800);
    w.window.maximize();
    pump(1500);
    input.perform(serde_json::json!([{"wait_ms": 300}, {"capture": "repainted"}]));
    input.finish();
    let pixels = |name: &str| {
        let texture = gdk::Texture::from_filename(captures.join(format!("{name}.png"))).unwrap();
        let stride = texture.width() as usize * 4;
        let mut bytes = vec![0; stride * texture.height() as usize];
        texture.download(&mut bytes, stride);
        (bytes, stride)
    };
    let ((switched, stride), (repainted, _)) = (pixels("switched"), pixels("repainted"));
    let below_header = (w.header.height() as usize + 8) * stride;
    let stale: Vec<_> = (below_header..switched.len())
        .step_by(4)
        .filter(|&i| (0..3).any(|c| switched[i + c].abs_diff(repainted[i + c]) > 2))
        .map(|i| [(i % stride) / 4, i / stride])
        .collect();
    let extent = stale.iter().fold(None, |e: Option<[usize; 4]>, &[x, y]| {
        Some(e.map_or([x, y, x, y], |[x0, y0, x1, y1]| [x0.min(x), y0.min(y), x1.max(x), y1.max(y)]))
    });
    assert!(stale.is_empty(), "{} pixels kept the previous workspace after the switch, within {extent:?}", stale.len());
}
