use super::*;
use serde_json::json;

fn select_group(w: &Rc<Workspace>, id: &str, index: u32) {
    let panel = w.panel_widget(Panel::ToolSettings);
    let dropdown = named::<gtk::DropDown>(&panel, &format!("tool-action-group-{id}"));
    let groups: Vec<_> = state(w).tool_actions.into_iter().filter(|action| action.group().is_some_and(|group| group.id() == id)).collect();
    let command = groups[index as usize].command;
    dropdown.set_selected(index);
    pump(120);
    assert!(state(w).commands.iter().any(|item| item.id == command && item.selected), "{id} dispatches {command:?}");
}

fn capture(w: &Rc<Workspace>, output: &std::path::Path, context: &str, theme: Theme, width: i32, reports: &mut Vec<serde_json::Value>) {
    pump(180);
    let panel = w.panel_widget(Panel::ToolSettings);
    capture_reference(w, output.join(format!("{context}-{theme:?}-{width}.png")).to_str().unwrap(), 1.);
    assert!(panel.measure(gtk::Orientation::Horizontal, -1).0 <= width, "{context}: minimum width fits {width}");
    for widget in widgets(&panel).filter(|widget| widget.is_mapped()) {
        let bounds = widget.compute_bounds(&panel).unwrap();
        let allocated_height = bounds.height();
        assert!(bounds.x() >= -1. && bounds.x() + bounds.width() <= panel.width() as f32 + 1., "{context}: {} {} fits panel horizontally: {bounds:?}, panel {}", widget.widget_name(), widget.type_().name(), panel.width());
        if matches!(widget.widget_name().as_str(), "tool-action-ApplyTransform" | "tool-action-CancelTransform") {
            for label in widgets(&widget).filter_map(|child| child.downcast::<gtk::Label>().ok()) {
                assert!(!label.layout().is_ellipsized(), "{context}: completion label stays readable: {}", label.text());
            }
        }
        if widget.is::<gtk::SpinButton>() || widget.is::<gtk::CheckButton>() || widget.is::<gtk::DropDown>() || widget.widget_name().starts_with("tool-action-") || widget.widget_name().starts_with("tool-choice-") && !widget.widget_name().starts_with("tool-choice-transform-reference-") && widget.is::<gtk::ToggleButton>() {
            assert_eq!(allocated_height, 24., "{context}: {} {}", widget.widget_name(), widget.type_().name());
        }
        if let Ok(number) = widget.clone().downcast::<crate::number_control::NumberControl>() {
            let slider = !number.has_css_class("number-inline") && descendant::<gtk::Scale>(&number).is_some_and(|scale| scale.is_mapped());
            let interval = number.parent().is_some_and(|parent| parent.widget_name().starts_with("tool-range-"));
            let height = if slider { 36. } else if interval { 28. } else { 24. };
            assert_eq!(allocated_height, height, "{context}: {}", widget.widget_name());
        }
    }
    let controls: Vec<_> = widgets(&panel).filter(|widget| widget.is_mapped() && (widget.widget_name().starts_with("tool-") || widget.is::<gtk::SpinButton>() || widget.is::<gtk::CheckButton>() || widget.is::<gtk::DropDown>() || widget.is::<gtk::Button>() || widget.is::<gtk::MenuButton>() || widget.has_css_class("number-step"))).map(|widget| {
        let bounds = widget.compute_bounds(&panel).unwrap();
        json!({"name": widget.widget_name().as_str(), "type": widget.type_().name(), "x": bounds.x(), "y": bounds.y(), "width": bounds.width(), "height": bounds.height()})
    }).collect();
    reports.push(json!({"context": context, "theme": format!("{theme:?}"), "requested_width": width, "panel_width": panel.width(), "panel_height": panel.height(), "controls": controls}));
    std::fs::write(output.join("controls.json"), serde_json::to_vec_pretty(&reports).unwrap()).unwrap();
}

fn spin_edit(w: &Rc<Workspace>, id: &str, expression: &str, expected: f64, output: &std::path::Path, theme: Theme, width: i32) {
    let panel = w.panel_widget(Panel::ToolSettings);
    let number = named::<crate::number_control::NumberControl>(&panel, &format!("tool-setting-{id}"));
    let spin = descendant::<gtk::SpinButton>(&number).expect("small integers use a spin field");
    spin.grab_focus();
    spin.set_text(expression);
    pump(100);
    capture_reference(w, output.join(format!("spin-edit-{id}-{theme:?}-{width}.png")).to_str().unwrap(), 1.);
    spin.update();
    pump(100);
    assert_eq!(number.value(), expected);
    assert!(state(w).tool_settings.iter().any(|setting| setting.id == id && setting.value as f64 == expected));
    let focus = spin.root().unwrap().focus();
    spin.set_text("sqrt(-1)");
    spin.update();
    assert!(!number.input_valid());
    assert_eq!(number.value(), expected);
    assert_eq!(spin.root().unwrap().focus(), focus);
    capture_reference(w, output.join(format!("spin-invalid-{id}-{theme:?}-{width}.png")).to_str().unwrap(), 1.);
    spin.set_text(expression);
    spin.update();
    spin.spin(gtk::SpinType::StepForward, 1.);
    pump(100);
    assert_eq!(number.value(), expected + 1.);
    assert_eq!(spin.root().unwrap().focus(), focus);
}

#[test]
#[ignore = "private Wayland display, hardware GPU and production Tool Settings controls"]
fn native_tool_controls_compact() {
    let app = native_test_app("art.capycanvas.ToolControlsCompact");
    let w = fixture_workspace(&app);
    w.window.maximize();
    w.window.present();
    pump(1200);
    super::new_photo::ready(&w);
    let mut input = RemoteInput::new();
    input.ready();
    let output = std::env::var_os("LAYER_TEST_ARTIFACTS").map(std::path::PathBuf::from).unwrap_or_else(|| std::path::PathBuf::from("../../artifacts/ui/tool-controls-gtk"));
    std::fs::create_dir_all(&output).unwrap();
    let mut reports = Vec::new();
    for theme in [Theme::Light, Theme::Dark] {
        w.dispatch(UiAction::SetTheme { theme: Some(theme) });
        for width in [242, 320] {
            let mut workspace = tool_settings_workspace(&w, &[], true, false);
            workspace.layout.move_panel([w.surface.width() as f32, w.surface.height() as f32], Panel::ToolSettings, DockTarget::Float { position: [470., 100.] }).unwrap();
            let group = workspace.layout.panel_group(Panel::ToolSettings).unwrap();
            workspace.layout.fit_tab_groups.retain(|id| *id != group);
            let floating = workspace.layout.floating.last_mut().unwrap();
            floating.width = width as f32;
            floating.height = Some(760.);
            w.dispatch(UiAction::RestoreWorkspace { workspace: Box::new(workspace) });
            for (context, preset) in [("painting", layer_core::DefaultBrushPreset::GPen), ("wet-mixing", layer_core::DefaultBrushPreset::WetRound)] {
                w.dispatch(UiAction::SelectBrush { id: preset as u32 });
                capture(&w, &output, context, theme, width, &mut reports);
                if context == "wet-mixing" { select_group(&w, "color-mixing", 1); select_group(&w, "color-mixing", 0); }
            }
            for (context, command) in [("fill", CommandId::Fill), ("enclose-fill", CommandId::EncloseFill), ("auto-select", CommandId::AutoSelect), ("rectangle-select", CommandId::RectangleSelect), ("polygon-select", CommandId::PolygonSelect), ("selection-brush", CommandId::SelectionBrush), ("tonal-presets", CommandId::TonalSelect), ("gradient", CommandId::Gradient), ("eyedropper", CommandId::Eyedropper), ("zoom", CommandId::Zoom), ("crop", CommandId::Crop)] {
                w.dispatch(UiAction::Invoke { command });
                if context == "tonal-presets" { w.dispatch(UiAction::Tonal { action: layer_ui::TonalAction::Preset { index: 4 } }); }
                capture(&w, &output, context, theme, width, &mut reports);
                if context == "fill" || context == "auto-select" {
                    spin_edit(&w, "gap_closing", "3*2", 6., &output, theme, width);
                    spin_edit(&w, "expansion", "-3*2", -6., &output, theme, width);
                }
                if context == "enclose-fill" || context == "auto-select" { select_group(&w, "selection-source", 1); select_group(&w, "selection-source", 0); }
                if context == "tonal-presets" {
                    w.dispatch(UiAction::Tonal { action: layer_ui::TonalAction::Preset { index: 7 } });
                    capture(&w, &output, "tonal-custom", theme, width, &mut reports);
                }
                if context == "crop" {
                    let dropdown = named::<gtk::DropDown>(&w.panel_widget(Panel::ToolSettings), "tool-action-group-crop-ratio");
                    input.click(screen_point(dropdown.upcast_ref(), &w.window, [0.5, 0.5]));
                    let popup = descendant::<gtk::Popover>(&dropdown).unwrap();
                    until(|| popup.is_mapped(), "crop ratio popup opens");
                    if std::env::var_os("LAYER_NATIVE_CAPTURE_DIR").is_some() {
                        input.perform(json!([{"wait_ms": 150}, {"capture": format!("dropdown-open-{}-{width}", format!("{theme:?}").to_lowercase())}]));
                    } else {
                        capture_popover(&popup, output.join(format!("dropdown-popup-{theme:?}-{width}.png")).to_str().unwrap());
                    }
                    input.key(0xff1b);
                    select_group(&w, "crop-ratio", 2);
                    select_group(&w, "crop-overlay", 1);
                    assert!(state(&w).commands.iter().any(|item| item.id == CommandId::CropRatioSquare && item.selected));
                    w.dispatch(UiAction::Invoke { command: CommandId::CancelTransform });
                }
            }
            w.dispatch(UiAction::Invoke { command: CommandId::RectangleSelect });
            w.dispatch(UiAction::Invoke { command: CommandId::SelectAll });
            w.dispatch(UiAction::Layer { action: LayerAction::FillSelection });
            pump(350);
            w.dispatch(UiAction::Invoke { command: CommandId::ScaleRotate });
            pump(350);
            w.dispatch(UiAction::Invoke { command: CommandId::TransformFree });
            capture(&w, &output, "transform-free", theme, width, &mut reports);
            select_group(&w, "transform-interpolation", 0);
            w.dispatch(UiAction::Invoke { command: CommandId::TransformWarp });
            pump(350);
            capture(&w, &output, "transform-warp", theme, width, &mut reports);
            select_group(&w, "transform-warp-grid", 1);
            select_group(&w, "transform-warp-split", 1);
            let panel = w.panel_widget(Panel::ToolSettings);
            let split = named::<gtk::DropDown>(&panel, "tool-action-group-transform-warp-split");
            split.grab_focus();
            let focus = split.root().unwrap().focus();
            w.dispatch(UiAction::Invoke { command: CommandId::WarpSelectPoints });
            pump(100);
            assert_eq!(named::<gtk::DropDown>(&panel, "tool-action-group-transform-warp-split"), split);
            assert_eq!(split.root().unwrap().focus(), focus);
            w.dispatch(UiAction::Invoke { command: CommandId::CancelTransform });
        }
    }
    std::fs::write(output.join("controls.json"), serde_json::to_vec_pretty(&reports).unwrap()).unwrap();
    input.finish();
    w.window.destroy();
}
