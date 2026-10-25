use super::*;
use super::photo_edit::window_point;
use serde_json::json;

#[test]
#[ignore = "private Mutter display and tablet proxy; workspace-motion.sh gtk --native-test=native_smoothing_strokes --tablet"]
fn native_smoothing_strokes() {
    let (_app, w, mut input) = super::photo_edit::start("art.capycanvas.SmoothingStrokes");
    let feedback_enabled = match std::env::var("LAYER_SMOOTHING_FEEDBACK").as_deref() {
        Ok("0") => false,
        Ok("1") | Err(_) => true,
        Ok(value) => panic!("unknown feedback setting {value}"),
    };
    let mut settings = state(&w).settings;
    settings.feedback = feedback_enabled;
    w.dispatch(UiAction::RestoreSettings { settings: Box::new(settings) });
    assert_eq!(state(&w).settings.feedback, feedback_enabled);
    eprintln!("native_smoothing_strokes feedback_enabled={feedback_enabled}");
    let output = std::path::PathBuf::from(std::env::var("LAYER_TEST_ARTIFACTS").unwrap());
    std::fs::create_dir_all(&output).unwrap();
    let mut workspace = tool_settings_workspace(&w, &[], true, false);
    workspace.layout.move_panel([w.surface.width() as f32, w.surface.height() as f32], Panel::ToolSettings, DockTarget::Float { position: [470., 100.] }).unwrap();
    let group = workspace.layout.panel_group(Panel::ToolSettings).unwrap();
    workspace.layout.fit_tab_groups.retain(|id| *id != group);
    workspace.layout.floating.last_mut().unwrap().height = Some(760.);
    w.dispatch(UiAction::RestoreWorkspace { workspace: Box::new(workspace) });
    w.dispatch(UiAction::SelectBrush { id: layer_core::DefaultBrushPreset::GPen as u32 });
    w.dispatch(UiAction::SetBrushSize { value: 8. });
    pump(250);
    let smoothing = state(&w).tool_settings.into_iter().find(|f| f.id == "smoothing_strength").unwrap();
    assert_eq!(smoothing.label.as_ref(), "Smoothing");
    assert!(smoothing.group_id.is_empty() && smoothing.group.is_empty());
    for theme in [Theme::Light, Theme::Dark] {
        w.dispatch(UiAction::SetTheme { theme: Some(theme) });
        pump(150);
        let empty = glib::MainContext::default().block_on(read_canvas_pixels(&w, 800)).unwrap();
        for strength in [0.06, 0.5, 1.] {
            capture_reference(&w, output.join(format!("controls-{theme:?}-{strength}.png")).to_str().unwrap(), 1.);
            w.dispatch(UiAction::SetToolSetting { id: "smoothing_strength".into(), value: strength });
            assert_eq!(state(&w).settings.feedback, feedback_enabled);
            let before = ui_session(&w).engine().metrics().committed_strokes;
            let mut actions = Vec::new();
            for i in 0..=80 {
                let t = i as f32 / 80.;
                let at = window_point(&w, [800. + t * 700., 350. + (t * std::f32::consts::TAU).sin() * 80.]);
                if i == 0 { actions.push(contact("pen", "move", at)); }
                actions.push(contact("pen", if i == 0 { "down" } else { "move" }, at));
                actions.push(json!({"wait_ms": 8}));
            }
            actions.extend([json!({"pen": "up"}), json!({"pen": "leave"})]);
            input.perform(json!(actions));
            until(|| ui_session(&w).engine().metrics().committed_strokes == before + 1, "the native pen stroke commits");
            let painted = glib::MainContext::default().block_on(read_canvas_pixels(&w, 801)).unwrap();
            assert_ne!(painted.bytes, empty.bytes, "strength-{strength} paints ink");
            capture_reference(&w, output.join(format!("{theme:?}-strength-{strength}.png")).to_str().unwrap(), 1.);
            w.dispatch(UiAction::Invoke { command: CommandId::Undo });
            let undone = glib::MainContext::default().block_on(read_canvas_pixels(&w, 802)).unwrap();
            assert_eq!(undone.bytes, empty.bytes);
            w.dispatch(UiAction::Invoke { command: CommandId::Redo });
            let redone = glib::MainContext::default().block_on(read_canvas_pixels(&w, 803)).unwrap();
            assert_eq!(redone.bytes, painted.bytes, "strength-{strength} reproduces committed pixels on redo");
            w.dispatch(UiAction::Invoke { command: CommandId::Undo });
            pump(100);
        }
    }
    assert!(state(&w).host_error.is_none());
    input.finish();
    w.window.destroy();
}
