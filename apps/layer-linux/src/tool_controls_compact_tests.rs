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

fn float_panel(w: &Rc<Workspace>, panel: Panel, width: i32) {
    let mut workspace = state(w).workspace;
    for candidate in [Panel::Properties,Panel::Histogram,Panel::Waveform,Panel::Color,Panel::Sizes,Panel::Adjustments,Panel::Proof,Panel::FilterTypes] {
        workspace.layout.set_panel_visible(candidate, candidate == panel).unwrap();
    }
    workspace.layout.set_panel_visible(panel, true).unwrap();
    workspace.layout.move_panel([w.surface.width() as f32, w.surface.height() as f32], panel, DockTarget::Float { position: [470., 100.] }).unwrap();
    let group = workspace.layout.panel_group(panel).unwrap();
    workspace.layout.fit_tab_groups.retain(|id| *id != group);
    let floating = workspace.layout.floating.iter_mut().find(|floating| floating.root.id() == group).unwrap();
    floating.width = width as f32;
    floating.height = Some(760.);
    w.dispatch(UiAction::RestoreWorkspace { workspace: Box::new(workspace) });
    pump(180);
}

fn rounded_corners(node: &gtk::gsk::RenderNode) -> Vec<[f32; 4]> {
    if let Some(clip) = node.downcast_ref::<gtk::gsk::RoundedClipNode>() {
        let rect = clip.clip();
        let mut corners = vec![std::array::from_fn(|index| rect.corner()[index].width())];
        corners.extend(rounded_corners(&clip.child()));
        corners
    } else if let Some(container) = node.downcast_ref::<gtk::gsk::ContainerNode>() {
        (0..container.n_children()).flat_map(|index| rounded_corners(&container.child(index))).collect()
    } else if let Some(border) = node.downcast_ref::<gtk::gsk::BorderNode>() {
        let outline = border.outline();
        vec![std::array::from_fn(|index| outline.corner()[index].width())]
    } else if let Some(shadow) = node.downcast_ref::<gtk::gsk::OutsetShadowNode>() {
        let outline = shadow.outline();
        vec![std::array::from_fn(|index| outline.corner()[index].width())]
    } else if let Some(transform) = node.downcast_ref::<gtk::gsk::TransformNode>() {
        rounded_corners(&transform.child())
    } else if let Some(shadow) = node.downcast_ref::<gtk::gsk::ShadowNode>() {
        rounded_corners(&shadow.child())
    } else if let Some(clip) = node.downcast_ref::<gtk::gsk::ClipNode>() {
        rounded_corners(&clip.child())
    } else if let Some(opacity) = node.downcast_ref::<gtk::gsk::OpacityNode>() {
        rounded_corners(&opacity.child())
    } else { Vec::new() }
}

fn contains_mask(node: &gtk::gsk::RenderNode) -> bool {
    node.is::<gtk::gsk::MaskNode>()
        || node.downcast_ref::<gtk::gsk::ContainerNode>().is_some_and(|container| (0..container.n_children()).any(|index| contains_mask(&container.child(index))))
        || node.downcast_ref::<gtk::gsk::OpacityNode>().is_some_and(|opacity| contains_mask(&opacity.child()))
        || node.downcast_ref::<gtk::gsk::TransformNode>().is_some_and(|transform| contains_mask(&transform.child()))
        || node.downcast_ref::<gtk::gsk::BlurNode>().is_some_and(|blur| contains_mask(&blur.child()))
}

#[allow(deprecated)]
fn shape(widget: &gtk::Widget, radius: f32) -> serde_json::Value {
    let flags = widget.state_flags();
    widget.set_state_flags(gtk::StateFlags::PRELIGHT, false);
    let snapshot = gtk::Snapshot::new();
    snapshot.render_background(&widget.style_context(), 0., 0., widget.width() as f64, widget.height() as f64);
    snapshot.render_frame(&widget.style_context(), 0., 0., widget.width() as f64, widget.height() as f64);
    let node = snapshot.to_node().unwrap_or_else(|| panic!("{} has no inspectable background", widget.widget_name()));
    widget.set_state_flags(flags, true);
    let corners = rounded_corners(&node);
    assert!(corners.iter().flatten().any(|corner| *corner > 0.), "{} background has rounded corners: {node:?}", widget.widget_name());
    let outer = corners.iter().flatten().copied().fold(0., f32::max);
    assert!((outer-radius*crate::squircle::CORNER_FIT).abs()<0.05, "{} fitted outer CSS corner {outer}, design radius {radius}", widget.widget_name());
    let converted = crate::squircle::converted(&node, 1.);
    if node.is::<gtk::gsk::OutsetShadowNode>() {
        assert!(!converted.is::<gtk::gsk::OutsetShadowNode>(), "native surface shadow uses the squircle contour");
    } else {
        assert!(contains_mask(&converted), "native workspace conversion preserves the squircle contour");
    }
    json!({"name": widget.widget_name().to_string(), "size": [widget.width(), widget.height()], "css_corners": corners, "design_radius": radius})
}

fn capture_panel(w: &Rc<Workspace>, output: &std::path::Path, panel_id: Panel, context: &str, theme: Theme, width: i32, reports: &mut Vec<serde_json::Value>) {
    pump(150);
    let panel = w.panel_widget(panel_id);
    capture_reference(w, output.join(format!("panel-{context}-{theme:?}-{width}.png")).to_str().unwrap(), 1.);
    assert_eq!(panel.width()+panel.margin_start()+panel.margin_end(), width, "{context}: requested panel width is preserved");
    assert!(panel.measure(gtk::Orientation::Horizontal, -1).0 <= panel.width(), "{context}: minimum content fits narrow panel");
    if panel_id == Panel::Properties {
        let mut child = w.effects.properties.first_child();
        let mut previous: Option<gtk::graphene::Rect> = None;
        while let Some(row) = child {
            if row.is_mapped() {
                let bounds = row.compute_bounds(&panel).unwrap();
                if bounds.height() > 0. {
                    if let Some(before) = previous {
                        assert_eq!(bounds.y()-before.y()-before.height(),4.,"{context}: Properties title, metadata, toolbar and fields share the peer gap");
                    }
                    previous = Some(bounds);
                }
            }
            child = row.next_sibling();
        }
    }
    for widget in widgets(&panel).filter(|widget| widget.is_mapped()) {
        let bounds = widget.compute_bounds(&panel).unwrap();
        let mut ancestor = Some(widget.clone());
        let mut scrollbar = false;
        while let Some(parent) = ancestor {
            scrollbar |= parent.is::<gtk::Scrollbar>();
            if parent == panel { break; }
            ancestor = parent.parent();
        }
        if !scrollbar {
            assert!(bounds.x() >= -1. && bounds.x()+bounds.width() <= panel.width() as f32+1., "{context}: {} fits panel: {bounds:?}", widget.widget_name());
        }
        if widget.is::<gtk::DropDown>() || widget.is::<gtk::CheckButton>() || widget.is::<gtk::Switch>() || widget.is::<gtk::SearchEntry>() {
            assert_eq!(bounds.height(), 24., "{context}: {} is a single compact row", widget.widget_name());
        }
        if panel_id == Panel::Proof && (widget.is::<adw::ToggleGroup>() || widget.is::<gtk::ToggleButton>()) {
            assert_eq!(bounds.height(),24.,"{context}: Proof mode {} CSS node {} uses the compact row",widget.type_().name(),widget.css_name());
        }
        if let Ok(number) = widget.clone().downcast::<crate::number_control::NumberControl>() {
            if number.has_css_class("number-panel") {
                let slider = descendant::<gtk::Scale>(&number).is_some_and(|scale| scale.is_mapped());
                assert_eq!(bounds.height(), if slider {36.} else {24.}, "{context}: {} retains approved number height", number.widget_name());
            }
            if let Some(title) = find_css(number.upcast_ref(), "number-title").filter(|title| title.is_mapped()) {
                assert_eq!(title.compute_bounds(&panel).unwrap().x(), bounds.x(), "{context}: numeric caption aligns with its row");
            }
            if let Some(readout) = find_css(number.upcast_ref(), "number-value").filter(|readout| readout.is_mapped()) {
                let padding = padding(&readout);
                assert_eq!([padding[3],padding[1]], [6,6], "{context}: numeric readout shares compact padding");
            }
            if let Some(next) = number.next_sibling().filter(|next| next.is_mapped() && next.is::<crate::number_control::NumberControl>()) {
                assert_eq!(next.compute_bounds(&panel).unwrap().y()-bounds.y()-bounds.height(), 4., "{context}: numeric peers are four pixels apart");
            }
        }
    }
    if matches!(panel_id, Panel::Histogram | Panel::Waveform) {
        let prefix = if panel_id == Panel::Histogram {"histogram"} else {"waveform"};
        assert_gap(&panel, &format!("{prefix}-source"), &format!("{prefix}-channel"), 4., true);
        let source = named::<gtk::Widget>(&panel, &format!("{prefix}-source")).compute_bounds(&panel).unwrap();
        assert_eq!([source.x(),source.y()], [8.,8.], "chart controls respect the panel inset");
    }
    if context == "layer-properties" {
        let blend = named::<gtk::DropDown>(&panel, "property-blend").compute_bounds(&panel).unwrap();
        let color = named::<gtk::DropDown>(&panel, "property-color_mode").compute_bounds(&panel).unwrap();
        assert_eq!([blend.x(),blend.x()+blend.width()], [color.x(),color.x()+color.width()], "layer property choices share both field edges");
    }
    if context == "proof-print" {
        let profile = named::<gtk::Widget>(&panel,"proof-profile-choose").compute_bounds(&panel).unwrap();
        for name in ["proof-simulation","proof-intent"] {
            let field = named::<gtk::Widget>(&panel,name);
            let bounds = field.compute_bounds(&panel).unwrap();
            assert_eq!(bounds.x(),profile.x(),"print proof fields share the caption column");
            let label = field.parent().unwrap().first_child().unwrap().compute_bounds(&panel).unwrap();
            assert_eq!(bounds.x()-label.x()-label.width(),4.,"print proof caption and field are peers");
        }
        assert_gap(&panel,"proof-profile-choose","proof-simulation",4.,false);
        assert_gap(&panel,"proof-simulation","proof-intent",4.,false);
        assert_gap(&panel,"proof-intent","proof-bpc",4.,false);
        assert_gap(&panel,"proof-bpc","proof-gamut-warning",4.,false);
    }
    let mut shapes = Vec::new();
    if let Some(button) = find_named(&panel, "gradient-reverse").filter(|button| button.is_mapped()) {
        let bounds = button.compute_bounds(&panel).unwrap();
        assert_eq!([bounds.width(),bounds.height()], [24.,24.], "gradient action is a small square control");
        shapes.push(shape(&button, 12.));
    }
    let mut ancestor = panel.parent();
    while let Some(widget) = ancestor {
        if widget.has_css_class("dock-panel") { shapes.push(shape(&widget, 18.)); break; }
        ancestor = widget.parent();
    }
    let controls: Vec<_> = widgets(&panel).filter(|widget| widget.is_mapped()).filter_map(|widget| {
        let bounds = widget.compute_bounds(&panel)?;
        Some(json!({"name": widget.widget_name().to_string(), "type": widget.type_().name(), "css_node":widget.css_name().to_string(), "text": widget.downcast_ref::<gtk::Label>().map(|label|label.text().to_string()), "x":bounds.x(),"y":bounds.y(),"width":bounds.width(),"height":bounds.height(),"padding":padding(&widget)}))
    }).collect();
    reports.push(json!({"context":context,"panel":format!("{panel_id:?}"),"theme":format!("{theme:?}"),"requested_width":width,"controls":controls,"shapes":shapes}));
    std::fs::write(output.join("panels.json"),serde_json::to_vec_pretty(&reports).unwrap()).unwrap();
}

#[test]
#[ignore = "private Wayland display and hardware GPU: real reusable workspace panels"]
fn native_workspace_panel_controls() {
    let app = native_test_app("art.capycanvas.WorkspacePanelControls");
    let w = fixture_workspace(&app);
    w.window.maximize();w.window.present();
    super::new_photo::ready(&w);
    let output = std::path::PathBuf::from(std::env::var_os("LAYER_TEST_ARTIFACTS").expect("panel capture directory"));
    std::fs::create_dir_all(&output).unwrap();
    let mut input = RemoteInput::new().settle_ms(60).timeout_secs(30);
    input.ready();
    let mut reports = Vec::new();
    for theme in [Theme::Light,Theme::Dark] {
        w.dispatch(UiAction::SetTheme {theme:Some(theme)});
        for width in [242,320] {
            float_panel(&w,Panel::Properties,width);
            capture_panel(&w,&output,Panel::Properties,"layer-properties",theme,width,&mut reports);
            for effect in ["kaleidoscope","levels","curves","color_lookup","gradient_map","hue_saturation"] {
                w.dispatch(UiAction::Effect {action:EffectAction::Insert {effect:effect.into()}});
                super::new_photo::ready(&w);
                float_panel(&w,Panel::Properties,width);
                if effect == "kaleidoscope" {
                    let number = named::<crate::number_control::NumberControl>(&w.panel_widget(Panel::Properties), "property-segments");
                    let spin = descendant::<gtk::SpinButton>(&number).expect("count property has a native spin field");
                    spin.set_text("2*3");spin.update();spin.spin(gtk::SpinType::StepForward, 1.);pump(100);
                    assert_eq!(number.value(), 7.);
                    assert!(state(&w).layer_properties.controls.iter().any(|control| control.key == "segments" && control.value == layer_core::EffectValue::Number(7.)));
                }
                capture_panel(&w,&output,Panel::Properties,effect,theme,width,&mut reports);
                w.dispatch(UiAction::Invoke {command:CommandId::DeleteLayer});
                super::new_photo::ready(&w);
            }
            for (panel,context) in [(Panel::Histogram,"histogram"),(Panel::Waveform,"waveform"),(Panel::Color,"color"),(Panel::Sizes,"sizes"),(Panel::Adjustments,"adjustments"),(Panel::Proof,"proof"),(Panel::FilterTypes,"filters")] {
                float_panel(&w,panel,width);
                capture_panel(&w,&output,panel,context,theme,width,&mut reports);
                if panel == Panel::Proof {
                    let mode = named::<adw::ToggleGroup>(&w.panel_widget(panel),"proof-mode");
                    mode.set_active_name(Some("print"));
                    capture_panel(&w,&output,panel,"proof-print",theme,width,&mut reports);
                    mode.set_active_name(Some("off"));
                }
                if panel == Panel::Adjustments {
                    let root = w.panel_widget(panel);
                    let header = find_css(&root,"filter-picker-header").unwrap();
                    let search = header.last_child().unwrap();
                    input.click(screen_point(&search,&w.window,[0.5,0.5]));
                    for key in "curves".chars() { input.key(key as u32); }
                    assert_eq!(state(&w).filter_picker.search.as_deref(),Some("curves"));
                    capture_panel(&w,&output,panel,"filters-search",theme,width,&mut reports);
                    input.key(0xff1b);
                    assert!(state(&w).filter_picker.search.is_none());
                }
            }
        }
    }
    input.finish();
    w.window.destroy();
}
