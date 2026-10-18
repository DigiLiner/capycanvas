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
