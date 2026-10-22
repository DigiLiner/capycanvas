//! Tool bubbles through compositor-delivered mouse and touch input, in both themes.
use super::workspace_header::Driver;
use super::*;

fn views(d: &Driver) -> Vec<BubbleView> {
    ui_session(&d.w).bubbles()
}

fn find(d: &Driver, source: BubbleSource) -> Option<BubbleView> {
    views(d).into_iter().find(|v| v.source == source)
}

fn center(d: &Driver, id: u32) -> [f32; 2] {
    d.point(&d.named(&format!("bubble-{id}")))
}

fn stroke(d: &mut Driver, device: &str, path: &[[f32; 2]]) {
    let mut events = vec![contact(device, "down", path[0])];
    events.extend(path[1..].iter().map(|&p| contact(device, "move", p)));
    events.push(contact(device, "up", *path.last().unwrap()));
    d.input.perform(serde_json::Value::Array(events));
    pump(350);
}

fn tap(d: &mut Driver, device: &str, at: [f32; 2]) {
    stroke(d, device, &[at]);
}

fn capture(d: &mut Driver, name: &str) {
    if std::env::var_os("LAYER_NATIVE_CAPTURE_DIR").is_some() {
        d.input.perform(serde_json::json!([{ "wait_ms": 300 }, { "capture": name }]));
    }
}

fn tile_of(d: &Driver, control: ToolbarControl) -> String {
    let layout = state(&d.w).workspace.layout;
    let id = layout.panels.iter().flat_map(|p| p.tiles().to_vec()).find(|t| t.control == control).unwrap().id;
    format!("tile-{id}")
}

fn header_of(d: &Driver, control: ToolbarControl) -> String {
    let id = state(&d.w).workspace.layout.header.entries().find(|e| e.item == HeaderItem::Tool { control }).unwrap().id;
    format!("header-item-{id}")
}

fn tear(d: &mut Driver, device: &str, widget: &str, to: [f32; 2]) {
    let from = d.point(&d.named(widget));
    stroke(d, device, &[from, [from[0], from[1] + 8.], [from[0], from[1] + 70.], [to[0], to[1] - 30.], to]);
}

fn bead_point(d: &Driver, id: u32, bead: Bead) -> Option<[f32; 2]> {
    let view = find_id(d, id)?;
    let b = view.beads.iter().find(|b| b.placement.bead == bead && b.placement.alpha == 1.)?;
    let c = center(d, id);
    Some([c[0] + b.placement.center[0], c[1] + b.placement.center[1]])
}

fn find_id(d: &Driver, id: u32) -> Option<BubbleView> {
    views(d).into_iter().find(|v| v.id == id)
}

fn put_back_all(d: &mut Driver, device: &str, name: &str) {
    for view in views(d) {
        let mut from = center(d, view.id);
        if view.kind == BubbleKind::Color {
            let [inner, outer] = color_rim(view.diameter);
            let (radius, angle) = ((inner + outer) * 0.5, 200f32.to_radians());
            from = [from[0] + radius * angle.cos(), from[1] + radius * angle.sin()];
        }
        stroke(d, device, &[from, [from[0], from[1] - 20.], [from[0], 10.]]);
    }
    assert!(views(d).is_empty(), "{name}: dropping on the title bar puts bubbles back");
}

fn canvas_spot(d: &Driver, x: f32, y: f32) -> [f32; 2] {
    let area = d.w.area.compute_bounds(&d.w.window).unwrap();
    [area.x() + area.width() * x, area.y() + area.height() * y]
}

fn zen(d: &mut Driver, name: &str) {
    let panel = state(&d.w).workspace.layout.panels.iter().find(|p| p.tiles().iter().any(|t| t.control == ToolbarControl::BrushSizeSlider)).unwrap().id;
    let group = state(&d.w).workspace.layout.panel_group(panel).unwrap();
    let hidden = |d: &Driver| d.w.groups.borrow().iter().find(|g| g.id == group).unwrap().root.has_css_class("zen-hidden");
    let option = |d: &Driver| {
        ui_session(&d.w).context_menu(ContextTarget::Ribbon { panel }).unwrap().sections.into_iter().flatten().find(|i| i.label == "Show in Zen mode").unwrap()
    };
    let (item, spot) = (header_of(d, ToolbarControl::Color), canvas_spot(d, 0.6, 0.5));
    tear(d, "mouse", &item, spot);
    assert_eq!(option(d).selected, Some(false));
    d.w.dispatch(option(d).action.unwrap());
    d.w.dispatch(UiAction::Invoke { command: CommandId::ZenMode });
    let away = canvas_spot(d, 0.5, 0.5);
    tap(d, "touch", away);
    pump(400);
    assert!(state(&d.w).workspace.zen_mode);
    assert!(!hidden(d), "{name}: a toolbar shown in Zen stays");
    assert!(!d.w.bubbles.layer.has_css_class("zen-hidden"), "{name}: bubbles stay in Zen");
    capture(d, &format!("{name}-zen-kept"));
    d.w.dispatch(option(d).action.unwrap());
    tap(d, "touch", away);
    pump(400);
    assert!(hidden(d), "{name}: other toolbars hide in Zen");
    let height = d.w.window.height() as f32;
    stroke(d, "touch", &[[3., height * 0.5], [20., height * 0.5], [80., height * 0.5]]);
    pump(300);
    assert!(!hidden(d), "{name}: swiping in from the left edge peeks its toolbar");
    capture(d, &format!("{name}-zen-peek"));
    tap(d, "touch", away);
    pump(400);
    assert!(hidden(d), "{name}: a contact elsewhere hides it again");
    d.w.dispatch(UiAction::Invoke { command: CommandId::ZenMode });
    pump(300);
    put_back_all(d, "mouse", name);
}

#[test]
#[ignore = "private display: workspace-motion.sh gtk --native-test=native_tool_bubbles"]
fn native_tool_bubbles() {
    let mut d = Driver::new("art.capycanvas.ToolBubbles");
    let sketch = || UiAction::RestoreWorkspace {
        workspace: Box::new(WorkspaceState { layout: WorkspacePreset::Painter.layout(Platform::Gtk), ..WorkspaceState::default() }),
    };
    d.w.dispatch(sketch());
    pump(300);
    let (item, control) = state(&d.w)
        .workspace
        .layout
        .header
        .entries()
        .find_map(|e| match e.item {
            HeaderItem::Tool { control: control @ ToolbarControl::Command { command: CommandId::DrawingBrush } } => Some((e.id, control)),
            _ => None,
        })
        .expect("Sketch keeps Drawing in the title bar");
    let dial = BubbleSource::Control { control };
    for theme in [Theme::Light, Theme::Dark] {
        d.w.dispatch(UiAction::SetTheme { theme: Some(theme) });
        d.w.dispatch(sketch());
        pump(300);
        for device in ["mouse", "touch"] {
            let name = format!("{theme:?}-{device}").to_lowercase();
            let start = d.point(&d.named(&format!("header-item-{item}")));
            let spot = canvas_spot(&d, 0.4, 0.5);
            stroke(&mut d, device, &[start, [start[0], start[1] + 8.], [start[0], start[1] + 60.], [spot[0], spot[1] - 40.], spot]);
            let taken = find(&d, dial).unwrap_or_else(|| panic!("{name}: dragging a title-bar tool out takes a bubble"));
            assert_eq!(views(&d).len(), 1, "{name}");
            capture(&mut d, &format!("{name}-taken"));

            d.w.dispatch(UiAction::Invoke { command: CommandId::Lasso });
            pump(200);
            let at = center(&d, taken.id);
            tap(&mut d, device, at);
            assert!(find(&d, dial).unwrap().selected, "{name}: the first tap selects");
            assert!(!find(&d, dial).unwrap().open, "{name}: the first tap shows nothing");
            tap(&mut d, device, at);
            let open = find(&d, dial).unwrap();
            assert!(open.open && !open.rings.is_empty(), "{name}: the second tap shows settings");
            capture(&mut d, &format!("{name}-settings"));

            let ring = open.rings[0].clone();
            let value = |d: &Driver| state(&d.w).tool_settings.iter().find(|s| s.id == ring.setting).map(|s| s.value);
            let before = value(&d);
            let arc = |f: f32| [at[0] + ring.arc.point(f)[0], at[1] + ring.arc.point(f)[1]];
            let path = if device == "mouse" { [0.1, 0.4, 0.8] } else { [0.9, 0.6, 0.3] };
            stroke(&mut d, device, &path.map(arc));
            assert_ne!(value(&d), before, "{name}: dragging along the ring changes {}", ring.setting);
            assert!(find(&d, dial).unwrap().open, "{name}: the ring stays open while adjusting");

            tap(&mut d, device, at);
            assert_eq!(
                state(&d.w).customization.drawer.as_ref().map(|d| d.anchor),
                Some(DrawerAnchor::Bubble { id: taken.id }),
                "{name}: the third tap opens the chooser"
            );
            pump(300);
            capture(&mut d, &format!("{name}-chooser"));

            let shown = state(&d.w);
            let drawer = shown.customization.drawer.clone().unwrap();
            assert!(!drawer.columns.iter().flatten().any(|p| *p == Panel::ToolSettings), "{name}: the ring holds the settings");
            let row = drawer
                .tool_set
                .unwrap_or(shown.tool_set)
                .subtools
                .into_iter()
                .find(|t| !t.selected && t.preview.is_some())
                .and_then(|t| t.preview)
                .expect("the chooser lists brushes");
            let row_point = d.point(&d.named(&format!("brush-{row}")));
            let brush_spot = canvas_spot(&d, 0.62, 0.45);
            stroke(&mut d, device, &[row_point, [row_point[0] + 8., row_point[1]], [row_point[0] + 160., row_point[1]], brush_spot]);
            let brush = BubbleSource::Control { control: ToolbarControl::Brush { id: row } };
            let torn = find(&d, brush).unwrap_or_else(|| panic!("{name}: a drawer row tears off as a brush bubble"));
            assert!(state(&d.w).customization.drawer.is_none(), "{name}: tearing closes the chooser");

            let before = center(&d, taken.id);
            let toward = center(&d, torn.id);
            stroke(&mut d, device, &[toward, [toward[0] - 20., toward[1]], [before[0] + 40., before[1]], [before[0] + 20., before[1]]]);
            pump(800);
            let pushed = center(&d, taken.id);
            assert!((pushed[0] - before[0]).hypot(pushed[1] - before[1]) > 20., "{name}: a dragged bubble pushes its neighbour");
            capture(&mut d, &format!("{name}-pushed"));

            assert!(!find(&d, brush).unwrap().selected, "{name}: tearing a row does not choose it");
            if device == "touch" {
                let at = center(&d, torn.id);
                d.input.perform(serde_json::json!([contact(device, "down", at), { "wait_ms": 900 }, contact(device, "up", at)]));
                pump(300);
                let menu = d.w.popovers.borrow().iter().filter_map(|p| p.upgrade()).any(|p| p.is_visible() && p.has_css_class("panel-context-menu"));
                assert!(menu, "{name}: holding a bubble opens its menu");
                assert!(!find(&d, brush).unwrap().selected, "{name}: a hold is not a tap");
                capture(&mut d, &format!("{name}-menu"));
                d.input.key(0xff1b);
                pump(200);
            }

            put_back_all(&mut d, device, &name);

            let size = BubbleSource::Control { control: ToolbarControl::BrushSizeSlider };
            let from = d.point(&d.named(&tile_of(&d, ToolbarControl::BrushSizeSlider)));
            let to = canvas_spot(&d, 0.3, 0.3);
            d.input.perform(serde_json::json!([
                contact(device, "down", from), { "wait_ms": 900 },
                contact(device, "move", [from[0] + 30., from[1]]), contact(device, "move", [to[0] - 40., to[1]]),
                contact(device, "move", to), contact(device, "up", to)
            ]));
            pump(400);
            let slider = find(&d, size).unwrap_or_else(|| panic!("{name}: holding then dragging a slider out of its bar takes a bubble"));
            let before = state(&d.w).brush.diameter;
            let at = center(&d, slider.id);
            let ring = slider.rings[0].arc;
            let arc = |f: f32| [at[0] + ring.point(f)[0], at[1] + ring.point(f)[1]];
            stroke(&mut d, device, &[arc(0.2), arc(0.5), arc(if device == "mouse" { 0.7 } else { 0.4 })]);
            assert_ne!(state(&d.w).brush.diameter, before, "{name}: the slider ring sets the brush size");
            capture(&mut d, &format!("{name}-slider"));

            if device == "mouse" {
                let (item, spot) = (header_of(&d, ToolbarControl::Panel { panel: Panel::Layers }), canvas_spot(&d, 0.25, 0.75));
                tear(&mut d, device, &item, spot);
                let layers = find(&d, BubbleSource::Control { control: ToolbarControl::Panel { panel: Panel::Layers } })
                    .unwrap_or_else(|| panic!("{name}: Layers comes out of the title bar"));
                let at = center(&d, layers.id);
                tap(&mut d, device, at);
                pump(400);
                assert_eq!(state(&d.w).customization.drawer.map(|d| d.anchor), Some(DrawerAnchor::Bubble { id: layers.id }));
                capture(&mut d, &format!("{name}-layers"));
                tap(&mut d, device, at);
                assert!(state(&d.w).customization.drawer.is_none(), "{name}: tapping the card again folds it");

                let (item, spot) = (header_of(&d, ToolbarControl::Color), canvas_spot(&d, 0.68, 0.45));
                tear(&mut d, device, &item, spot);
                let color = find(&d, BubbleSource::Control { control: ToolbarControl::Color })
                    .unwrap_or_else(|| panic!("{name}: Color comes out of the title bar"));
                let transparent = bead_point(&d, color.id, Bead::Transparent).unwrap();
                tap(&mut d, device, transparent);
                assert_eq!(state(&d.w).colors.slot, ColorSlot::Transparent, "{name}: the transparent bead");
                let foreground = bead_point(&d, color.id, Bead::Foreground).unwrap();
                tap(&mut d, device, foreground);
                assert_eq!(state(&d.w).colors.slot, ColorSlot::Foreground, "{name}: the foreground bead");
                let first = bead_point(&d, color.id, Bead::Swatch { index: 0 }).unwrap();
                stroke(&mut d, device, &[first, [first[0] - 10., first[1]], [first[0] - 200., first[1]]]);
                if let Some(palettes) = bead_point(&d, color.id, Bead::Palettes) {
                    tap(&mut d, device, palettes);
                    assert!(find_id(&d, color.id).unwrap().pill.is_some(), "{name}: the palette bead opens its name");
                }
                capture(&mut d, &format!("{name}-color"));
            }
            put_back_all(&mut d, device, &name);
        }
        zen(&mut d, &format!("{theme:?}").to_lowercase());
    }
    d.finish();
}
