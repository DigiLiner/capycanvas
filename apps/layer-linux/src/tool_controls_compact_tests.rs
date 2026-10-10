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

fn assert_gap(panel: &gtk::Widget, before: &str, after: &str, gap: f32, horizontal: bool) {
    let a = named::<gtk::Widget>(panel, before).compute_bounds(panel).unwrap();
    let b = named::<gtk::Widget>(panel, after).compute_bounds(panel).unwrap();
    let actual = if horizontal { b.x() - a.x() - a.width() } else { b.y() - a.y() - a.height() };
    assert_eq!(actual, gap, "{before} → {after}: allocated gap");
}

fn assert_spacing(panel: &gtk::Widget, context: &str) {
    let first = match context {
        "painting" | "wet-mixing" => named::<gtk::Widget>(panel, "tool-setting-size"),
        "fill" | "enclose-fill" => named::<gtk::Widget>(panel, "tool-setting-tolerance"),
        "zoom" => named::<gtk::Widget>(panel, "tool-choice-bar-zoom-click").parent().unwrap(),
        "eyedropper" => named::<gtk::Widget>(panel, "color-picker-source").parent().unwrap(),
        "gradient" => named::<gtk::Widget>(panel, "tool-choice-bar-gradient-shape"),
        "crop" => widgets(panel).find(|widget| widget.is_mapped() && widget.has_css_class("heading") && widget.has_css_class("dim-label")).unwrap(),
        _ => named::<gtk::Widget>(panel, "selection-mode-row"),
    };
    let inset = first.compute_bounds(panel).unwrap();
    assert_eq!(inset.y(), 8., "{context}: first row respects top inset");
    assert_eq!(inset.x(), 8., "{context}: main row respects left inset");
    assert_eq!(panel.width() as f32 - inset.x() - inset.width(), 8., "{context}: main row respects right inset");
    if matches!(context, "auto-select" | "rectangle-select" | "polygon-select" | "selection-brush") {
        let mode = if context == "selection-brush" { "tool-action-SelectionAdd" } else { "tool-action-SelectionNew" };
        assert_gap(panel, mode, "selection-actions", 4., false);
        let first_setting = match context {
            "auto-select" => "tool-setting-tolerance",
            "selection-brush" => "tool-setting-selection_brush_size",
            _ => "tool-setting-selection_feather",
        };
        assert_gap(panel, "selection-actions", first_setting, 6., false);
    }
    match context {
        "painting" | "wet-mixing" => {
            assert_gap(panel, "tool-setting-size", "tool-setting-opacity", 4., false);
            assert_gap(panel, "tool-setting-opacity", "tool-setting-flow", 4., false);
        }
        "fill" | "enclose-fill" | "auto-select" => {
            assert_gap(panel, "tool-setting-gap_closing", "tool-setting-expansion", 4., false);
            assert_gap(panel, "tool-setting-expansion", "tool-setting-smoothing", 4., false);
        }
        "tonal-presets" | "tonal-custom" => {
            assert_gap(panel, "selection-mode-row", "tool-choice-bar-tonal-tones", 6., false);
            let first = if context == "tonal-custom" { "tool-range-tonal" } else { "tool-setting-tonal_softness" };
            assert_gap(panel, "tool-choice-bar-tonal-tones", first, 4., false);
            if context == "tonal-custom" { assert_gap(panel, first, "tool-setting-tonal_softness", 4., false); }
            assert_gap(panel, "tool-setting-tonal_softness", "tool-setting-selection_feather", 4., false);
        }
        "crop" => {
            assert_gap(panel, "tool-setting-crop_width", "tool-setting-crop_height", 4., false);
            assert_gap(panel, "tool-setting-crop_height", "tool-setting-crop_angle", 4., false);
            assert_gap(panel, "tool-action-CropFitContent", "tool-action-group-crop-overlay", 4., false);
            assert_gap(panel, "tool-action-group-crop-overlay", "tool-action-CropStraighten", 4., false);
            assert_gap(panel, "tool-action-ApplyTransform", "tool-action-CancelTransform", 4., true);
        }
        "zoom" => {
            assert_gap(panel, "tool-choice-bar-zoom-click", "tool-choice-bar-zoom-drag", 4., false);
            assert_gap(panel, "tool-choice-bar-zoom-drag", "tool-action-CenterZoomClicks", 4., false);
        }
        "eyedropper" => assert_gap(panel, "color-picker-source", "color-picker-size", 4., false),
        "gradient" => {
            assert_gap(panel, "tool-choice-bar-gradient-shape", "gradient-interpolation", 4., false);
            assert_gap(panel, "gradient-interpolation", "effect-gradient", 4., false);
            assert_gap(panel, "effect-gradient", "effect-gradient-remove", 4., false);
            assert_gap(panel, "effect-gradient-remove", "tool-setting-opacity", 4., false);
            assert_gap(panel, "gradient-reverse", "effect-gradient-reset", 4., true);
            assert_gap(panel, "effect-gradient-remove", "effect-gradient-color", 4., true);
        }
        "transform-free" | "transform-warp" => {
            if context == "transform-warp" { assert_gap(panel, "selection-mode-row", "tool-action-TransformSnapping", 6., false); }
            else {
                let title = widgets(panel).find(|widget| widget.is_mapped() && widget.has_css_class("heading") && widget.has_css_class("dim-label")).unwrap();
                let modes = named::<gtk::Widget>(panel, "selection-mode-row").compute_bounds(panel).unwrap();
                assert_eq!(title.compute_bounds(panel).unwrap().y() - modes.y() - modes.height(), 6., "mode section ends six pixels before heading");
            }
            assert_gap(panel, "tool-action-TransformFlipHorizontal", "tool-action-TransformFlipVertical", 4., true);
            assert_gap(panel, "tool-action-TransformFlipHorizontal", "tool-action-TransformRotateLeft", 4., false);
            assert_gap(panel, "tool-action-TransformRotateLeft", "tool-action-TransformRotateRight", 4., true);
            assert_gap(panel, "tool-action-TransformRotateLeft", "tool-action-ResetTransform", 4., false);
            assert_gap(panel, "tool-action-ApplyTransform", "tool-action-CancelTransform", 4., true);
            if context == "transform-free" {
                assert_gap(panel, "tool-choice-bar-transform-reference", "tool-setting-transform_x", 2., true);
                assert_gap(panel, "tool-setting-transform_x", "tool-setting-transform_y", 2., false);
            }
        }
        _ => {}
    }
    for title in widgets(panel).filter(|widget| widget.is_mapped() && widget.has_css_class("heading") && widget.has_css_class("dim-label")) {
        let bounds = title.compute_bounds(panel).unwrap();
        if let Some(before) = title.prev_sibling().filter(|widget| widget.is_mapped()) {
            let previous = before.compute_bounds(panel).unwrap();
            assert_eq!(bounds.y() - previous.y() - previous.height(), 6., "{context}: section space before heading");
        } else {
            assert_eq!(title.compute_bounds(&title.parent().unwrap()).unwrap().y(), 0., "{context}: first heading has no extra top margin");
            if context == "crop" { assert_eq!(bounds.y(), 8., "first section respects panel inset"); }
        }
        if let Some(after) = title.next_sibling().filter(|widget| widget.is_mapped()) {
            assert_eq!(after.compute_bounds(panel).unwrap().y() - bounds.y() - bounds.height(), 4., "{context}: section space after heading");
        }
    }
    for group in widgets(panel).filter(|widget| widget.is_mapped() && widget.has_css_class("selection-modes")) {
        let mut buttons: Vec<_> = widgets(&group).filter(|widget| widget.is_mapped() && widget.is::<gtk::ToggleButton>()).collect();
        buttons.sort_by(|a, b| {
            let a = a.compute_bounds(&group).unwrap();
            let b = b.compute_bounds(&group).unwrap();
            a.y().total_cmp(&b.y()).then_with(|| a.x().total_cmp(&b.x()))
        });
        for pair in buttons.windows(2) {
            let a = pair[0].compute_bounds(&group).unwrap();
            let b = pair[1].compute_bounds(&group).unwrap();
            if a.y() == b.y() { assert_eq!(b.x() - a.x() - a.width(), 0., "{context}: joined choices have no horizontal gap"); }
            else if group.has_css_class("choice-grid") { assert_eq!(b.y() - a.y() - a.height(), 0., "reference grid has no vertical gap"); }
        }
    }
}

#[allow(deprecated)]
fn padding(widget: &impl IsA<gtk::Widget>) -> [i16; 4] {
    let padding = widget.style_context().padding();
    [padding.top(), padding.right(), padding.bottom(), padding.left()]
}

fn assert_margins(panel: &gtk::Widget, context: &str) {
    let aligned = match context {
        "eyedropper" => &["color-picker-source", "color-picker-size"][..],
        "crop" => &["tool-action-group-crop-ratio", "tool-action-group-crop-overlay"][..],
        "transform-warp" => &["tool-action-group-transform-warp-split", "tool-action-group-transform-warp-grid", "tool-action-group-transform-interpolation"][..],
        _ => &[],
    };
    let mut column = None;
    for name in aligned {
        let dropdown = named::<gtk::Widget>(panel, name);
        let bounds = dropdown.compute_bounds(panel).unwrap();
        if let Some(x) = column { assert_eq!(bounds.x(), x, "{context}: adjacent dropdowns share a column"); }
        column = Some(bounds.x());
        let label = dropdown.parent().unwrap().first_child().and_downcast::<gtk::Label>().unwrap();
        assert_eq!(label.compute_bounds(panel).unwrap().x(), 8., "{context}: dropdown caption aligns to outer rail");
    }
    if context == "zoom" {
        for name in ["tool-choice-bar-zoom-click", "tool-choice-bar-zoom-drag"] {
            let label = named::<gtk::Widget>(panel, name).parent().unwrap().first_child().and_downcast::<gtk::Label>().unwrap();
            assert_eq!(label.compute_bounds(panel).unwrap().x(), 8., "Zoom caption aligns to outer rail");
        }
    }
    for number in widgets(panel).filter_map(|widget| widget.downcast::<crate::number_control::NumberControl>().ok()).filter(|number| number.is_mapped()) {
        let label = if number.has_css_class("number-inline") {
            number.first_child().and_then(|row| row.first_child()).and_downcast::<gtk::Label>()
        } else { find_css(number.upcast_ref(), "number-title").and_then(|widget| widget.downcast::<gtk::Label>().ok()) };
        if let Some(label) = label.filter(|label| label.is_mapped()) {
            assert_eq!(label.compute_bounds(panel).unwrap().x(), number.compute_bounds(panel).unwrap().x(), "{context}: {} caption has no extra indent", number.widget_name());
        }
        if let Some(readout) = find_css(number.upcast_ref(), "number-value").filter(|widget| widget.is_mapped()) {
            let padding = padding(&readout);
            assert_eq!([padding[3], padding[1]], [6, 6], "{context}: numeric readout uses six-pixel horizontal padding");
        }
    }
}

fn capture(w: &Rc<Workspace>, output: &std::path::Path, context: &str, theme: Theme, width: i32, reports: &mut Vec<serde_json::Value>) {
    pump(180);
    let panel = w.panel_widget(Panel::ToolSettings);
    capture_reference(w, output.join(format!("{context}-{theme:?}-{width}.png")).to_str().unwrap(), 1.);
    assert_spacing(&panel, context);
    assert_margins(&panel, context);
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
    let controls: Vec<_> = widgets(&panel).filter(|widget| widget.is_mapped() && (widget.widget_name().starts_with("tool-") || widget.widget_name().starts_with("effect-gradient") || widget.has_css_class("heading") || widget.is::<gtk::Label>() || widget.is::<gtk::Entry>() || widget.is::<gtk::SpinButton>() || widget.is::<gtk::CheckButton>() || widget.is::<gtk::DropDown>() || widget.is::<gtk::Button>() || widget.is::<gtk::MenuButton>() || widget.has_css_class("number-step"))).map(|widget| {
        let bounds = widget.compute_bounds(&panel).unwrap();
        json!({"name": widget.widget_name().as_str(), "type": widget.type_().name(), "text": widget.downcast_ref::<gtk::Label>().map(|label| label.text().to_string()), "x": bounds.x(), "y": bounds.y(), "width": bounds.width(), "height": bounds.height(), "padding": padding(&widget)})
    }).collect();
    reports.push(json!({"context": context, "theme": format!("{theme:?}"), "requested_width": width, "panel_width": panel.width(), "panel_height": panel.height(), "controls": controls}));
    std::fs::write(output.join("controls.json"), serde_json::to_vec_pretty(&reports).unwrap()).unwrap();
}

fn inline_edit(w: &Rc<Workspace>, output: &std::path::Path, context: &str, id: &str, theme: Theme, width: i32, reports: &mut Vec<serde_json::Value>) {
    let panel = w.panel_widget(Panel::ToolSettings);
    let number = named::<crate::number_control::NumberControl>(&panel, &format!("tool-setting-{id}"));
    let readout = find_css(number.upcast_ref(), "number-value").unwrap().downcast::<gtk::Button>().unwrap();
    let before = readout.compute_bounds(&panel).unwrap();
    click(&readout);
    pump(100);
    let editor = find_css(number.upcast_ref(), "number-entry").unwrap().downcast::<gtk::Entry>().unwrap();
    let editing = editor.compute_bounds(&panel).unwrap();
    assert_eq!(editing.height(), 24., "{context}: inline editor preserves approved height");
    assert_eq!(editing.x() + editing.width(), before.x() + before.width(), "{context}: editor keeps outer right edge");
    let padding = padding(&editor);
    assert_eq!([padding[3],padding[1]], [6,6], "{context}: editor and readout horizontal padding match");
    assert!(editor.has_focus() || editor.root().unwrap().focus().is_some_and(|focus| focus.is_ancestor(&editor)), "inline editor retains focus");
    let original = number.value();
    editor.set_text(&format!("{}+1", editor.text()));
    capture_reference(w, output.join(format!("inline-edit-{id}-{theme:?}-{width}.png")).to_str().unwrap(), 1.);
    reports.push(json!({"context": context, "theme": format!("{theme:?}"), "requested_width": width, "inline_editor": id, "readout": [before.x(),before.y(),before.width(),before.height()], "editor": [editing.x(),editing.y(),editing.width(),editing.height()], "padding": padding}));
    editor.emit_activate();
    pump(100);
    assert!(number.input_valid());
    assert!(number.value() > original, "inline expression commits to shared state");
    assert!(state(w).tool_settings.iter().any(|setting| setting.id == id && (setting.value as f64-number.value()).abs()<0.0001));
    w.dispatch(UiAction::SetToolSetting { id: id.into(), value: original as f32 });
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
                    inline_edit(&w, &output, "tonal-custom", "tonal_softness", theme, width, &mut reports);
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
            inline_edit(&w, &output, "transform-free", "transform_x", theme, width, &mut reports);
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
