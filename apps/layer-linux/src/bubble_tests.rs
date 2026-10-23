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
    layout
        .panels
        .iter()
        .flat_map(|p| p.tiles().to_vec())
        .filter(|t| t.control == control)
        .map(|t| format!("tile-{}", t.id))
        .find(|name| find_named(d.w.window.upcast_ref(), name).is_some_and(|w| w.is_mapped()))
        .unwrap_or_else(|| panic!("no shown tile for {control:?}"))
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

fn rim(d: &Driver, view: &BubbleView, degrees: f32) -> [f32; 2] {
    let c = center(d, view.id);
    let (radius, angle) = (view.diameter * 0.5 - RING_RIM * 0.5, degrees.to_radians());
    [c[0] + radius * angle.cos(), c[1] + radius * angle.sin()]
}

fn put_back_all(d: &mut Driver, device: &str, name: &str) {
    for view in views(d) {
        let from = if view.kind == BubbleKind::Color { rim(d, &view, 200.) } else { center(d, view.id) };
        stroke(d, device, &[from, [from[0], from[1] - 20.], [from[0], 10.]]);
    }
    if !views(d).is_empty() {
        capture(d, &format!("{name}-left-out"));
    }
    let left: Vec<_> = views(d).iter().map(|v| {
        let widget = d.named(&format!("bubble-{}", v.id));
        (v.kind, center(d, v.id), widget.opacity(), widget.can_target(), v.open)
    }).collect();
    let popovers = d.w.popovers.borrow().iter().filter_map(|p| p.upgrade()).filter(|p| p.is_visible()).count();
    assert!(left.is_empty(), "{name}: dropping on the title bar puts bubbles back: {left:?}, {popovers} popovers open");
}

fn item_point(d: &Driver, id: u32, item: &ChooserItem) -> [f32; 2] {
    let c = center(d, id);
    [c[0] + item.center[0], c[1] + item.center[1]]
}

fn take(d: &mut Driver, grab: BubbleGrab, at: [f32; 2]) -> u32 {
    let viewport = [d.w.surface.width() as f32, d.w.surface.height() as f32];
    for phase in [ContactPhase::Down, ContactPhase::Up] {
        d.w.dispatch(UiAction::Bubble { action: BubbleAction::Drag { grab, phase, position: at, viewport } });
    }
    pump(400);
    views(d).last().unwrap().id
}

fn chooser(d: &mut Driver, device: &str, name: &str, id: u32) {
    let open = find_id(d, id).unwrap();
    let media: Vec<_> = open.chooser.iter().filter(|i| i.kind == ChooserKind::Medium).cloned().collect();
    assert!(media.len() > 2 && open.rings.is_empty(), "{name}: the third tap rings the media around the dial");
    assert_eq!(media.iter().filter(|m| m.shown).count(), 1, "{name}: the current medium shows its brushes");
    assert!(open.chooser.iter().any(|i| i.kind == ChooserKind::Brush), "{name}: brushes curve beside the medium");
    capture(d, &format!("{name}-chooser"));
    let other = media.iter().find(|m| !m.shown && m.enabled).unwrap().clone();
    tap(d, device, item_point(d, id, &other));
    let switched = find_id(d, id).unwrap();
    assert!(switched.chooser.iter().any(|m| m.shown && m.label == other.label), "{name}: tapping a medium fans its brushes");
    capture(d, &format!("{name}-medium"));
    if switched.chooser.iter().all(|i| i.kind != ChooserKind::Brush) {
        assert!(find_id(d, id).unwrap().chooser.is_empty(), "{name}: a medium with one brush picks it");
        return;
    }
    let media: Vec<_> = switched.chooser.iter().filter(|i| i.kind == ChooserKind::Medium).cloned().collect();
    let through = media.iter().find(|m| !m.shown && m.enabled && m.label != other.label).unwrap().clone();
    let from = item_point(d, id, &other);
    let via = item_point(d, id, &through);
    let mut events = vec![contact(device, "down", from)];
    for step in 1..=6 {
        let f = step as f32 / 6.;
        events.push(contact(device, "move", [from[0] + (via[0] - from[0]) * f, from[1] + (via[1] - from[1]) * f]));
    }
    d.input.perform(serde_json::Value::Array(events));
    pump(300);
    let fanned = find_id(d, id).unwrap();
    let target = fanned.chooser.iter().filter(|i| i.kind == ChooserKind::Brush).last().cloned();
    let finish = target.as_ref().map_or(via, |t| item_point(d, id, t));
    let mut events = Vec::new();
    for step in 1..=6 {
        let f = step as f32 / 6.;
        events.push(contact(device, "move", [via[0] + (finish[0] - via[0]) * f, via[1] + (finish[1] - via[1]) * f]));
    }
    events.push(contact(device, "up", finish));
    d.input.perform(serde_json::Value::Array(events));
    pump(350);
    if let Some(target) = target {
        assert_eq!(Some(state(&d.w).brush.preset), target.preview, "{name}: sliding through a medium to a brush picks it");
    }
    assert!(find_id(d, id).unwrap().chooser.is_empty(), "{name}: picking closes the ring");
}

fn layers_card(d: &mut Driver, device: &str, name: &str) {
    let (item, spot) = (header_of(d, ToolbarControl::Panel { panel: Panel::Layers }), canvas_spot(d, 0.3, 0.6));
    tear(d, device, &item, spot);
    let layers = find(d, BubbleSource::Control { control: ToolbarControl::Panel { panel: Panel::Layers } })
        .unwrap_or_else(|| panic!("{name}: Layers comes out of the title bar"));
    assert!(layers.layer.is_some() && layers.layers > 0, "{name}: the folded card shows the current layer and the count");
    capture(d, &format!("{name}-layers-folded"));
    let at = center(d, layers.id);
    tap(d, device, at);
    pump(400);
    let card = find_id(d, layers.id).unwrap().card.unwrap_or_else(|| panic!("{name}: tapping unfolds the card"));
    assert!(!card.full && !card.properties);
    assert!(d.named("bubble-nub").is_mapped(), "{name}: the card has a grab bar");
    capture(d, &format!("{name}-layers-card"));
    let properties = d.named("bubble-layer-properties");
    d.click(&properties);
    pump(500);
    assert!(find_id(d, layers.id).unwrap().card.unwrap().properties, "{name}: Layer properties slide up");
    capture(d, &format!("{name}-layers-properties"));
    d.click(&properties);
    pump(500);
    assert!(!find_id(d, layers.id).unwrap().card.unwrap().properties, "{name}: All layers goes back");
    let count = state(&d.w).layers.len();
    let add = d.named("bubble-new-layer");
    tap(d, device, d.point(&add));
    assert_eq!(state(&d.w).layers.len(), count + 1, "{name}: the footer's icon adds a layer");
    tap(d, device, canvas_spot(d, 0.85, 0.15));
    assert!(find_id(d, layers.id).unwrap().card.is_none(), "{name}: a contact elsewhere folds the card");
    let card = d.named("bubble-card");
    pump(500);
    assert!(!card.is_child_visible(), "{name}: the card finishes folding away");
    d.w.dispatch(UiAction::Invoke { command: CommandId::Undo });
    pump(200);
}

fn menu_open(d: &Driver) -> bool {
    d.w.popovers.borrow().iter().filter_map(|p| p.upgrade()).any(|p| p.is_visible() && p.has_css_class("panel-context-menu"))
}

fn touches(d: &mut Driver, a: [f32; 2], b: [f32; 2], step: [f32; 2], spread: f32) {
    let touch = |phase: &str, slot: u32, point: [f32; 2]| serde_json::json!({ "touch": phase, "slot": slot, "point": point });
    let mut events = vec![touch("down", 0, a), touch("down", 1, b)];
    for i in 1..=8 {
        let f = i as f32 / 8.;
        events.push(touch("move", 0, [a[0] + step[0] * f - spread * f, a[1] + step[1] * f]));
        events.push(touch("move", 1, [b[0] + step[0] * f + spread * f, b[1] + step[1] * f]));
    }
    events.push(serde_json::json!({ "touch": "up", "slot": 0 }));
    events.push(serde_json::json!({ "touch": "up", "slot": 1 }));
    d.input.perform(serde_json::Value::Array(events));
    pump(600);
}

fn color_ring(d: &mut Driver, device: &str, name: &str) {
    let (item, spot) = (header_of(d, ToolbarControl::Color), canvas_spot(d, 0.68, 0.45));
    tear(d, device, &item, spot);
    let color = find(d, BubbleSource::Control { control: ToolbarControl::Color }).unwrap_or_else(|| panic!("{name}: Color comes out of the title bar"));
    assert!(d.named(&format!("bubble-{}", color.id)).is_mapped());
    if color.beads.is_empty() {
        assert_eq!(color.diameter, COLOR_RING_SIZES[1], "{name}: a new ring is the medium circle");
    }
    let c = center(d, color.id);
    let before = state(&d.w).colors.foreground;
    let hold = [c[0] + 8., c[1] - 6.];
    d.input.perform(serde_json::json!([contact(device, "down", hold), { "wait_ms": 1300 }, contact(device, "up", hold)]));
    pump(300);
    assert!(menu_open(d), "{name}: holding the wheel opens the ring's menu");
    assert_eq!(state(&d.w).colors.foreground, before, "{name}: a hold keeps the color");
    capture(d, &format!("{name}-color-menu"));
    d.input.key(0xff1b);
    pump(200);
    let then = [hold[0] - 30., hold[1] + 24.];
    d.input.perform(serde_json::json!([contact(device, "down", hold), { "wait_ms": 1300 }, contact(device, "move", [hold[0] - 12., hold[1] + 10.]), contact(device, "move", then), { "wait_ms": 100 }]));
    pump(200);
    assert!(!menu_open(d), "{name}: dragging after a hold closes the menu");
    assert_ne!(state(&d.w).colors.foreground, before, "{name}: and goes on picking");
    d.input.perform(serde_json::json!([contact(device, "up", then)]));
    pump(200);
    assert!(!menu_open(d), "{name}: letting go after that keeps the menu closed");
    let options = BubbleOptions { recent: true, palette: true, pair: true, ..color.options };
    d.w.dispatch(UiAction::Bubble { action: BubbleAction::SetOptions { id: color.id, options } });
    pump(500);
    let transparent = bead_point(d, color.id, Bead::Transparent).unwrap();
    tap(d, device, transparent);
    assert_eq!(state(&d.w).colors.slot, ColorSlot::Transparent, "{name}: the transparent bead");
    let foreground = bead_point(d, color.id, Bead::Foreground).unwrap();
    tap(d, device, foreground);
    assert_eq!(state(&d.w).colors.slot, ColorSlot::Foreground, "{name}: the foreground bead");
    let colors = state(&d.w).colors;
    let background = bead_point(d, color.id, Bead::Background).unwrap();
    let away = [background[0] - foreground[0], background[1] - foreground[1]];
    let length = away[0].hypot(away[1]);
    tap(d, device, [background[0] + away[0] / length * 8., background[1] + away[1] / length * 8.]);
    let chosen = state(&d.w).colors;
    assert_eq!(chosen.slot, ColorSlot::Background, "{name}: tapping the background chooses it");
    assert_eq!((chosen.foreground, chosen.background), (colors.foreground, colors.background), "{name}: nothing swaps");
    tap(d, device, foreground);
    let c = center(d, color.id);
    if device == "mouse" && std::env::var("WAYLAND_DISPLAY").is_ok_and(|display| display.ends_with("-tablet")) {
        let widget = d.named(&format!("bubble-{}", color.id));
        d.input.perform(serde_json::json!([contact("pen", "down", [c[0] + 30., c[1]]), contact("pen", "up", [c[0] + 30., c[1]])]));
        pump(200);
        assert_eq!(widget.cursor().and_then(|c| c.name()).as_deref(), Some("none"), "{name}: a pen on the ring shows no pointer");
        d.input.perform(serde_json::json!([{ "pen": "leave" }, contact("mouse", "move", [c[0] + 30., c[1] + 4.])]));
        pump(200);
        assert!(widget.cursor().is_none(), "{name}: the mouse gets its pointer back");
    }
    d.input.perform(serde_json::json!([contact(device, "down", [c[0] - 12., c[1]])]));
    pump(150);
    let first = state(&d.w).colors.foreground;
    d.input.perform(serde_json::json!([contact(device, "move", [c[0] - 4., c[1] + 6.]), contact(device, "move", [c[0] + 16., c[1] + 14.]), contact(device, "up", [c[0] + 16., c[1] + 14.])]));
    pump(350);
    assert_ne!(state(&d.w).colors.foreground, first, "{name}: dragging on the wheel picks colors");
    let picked = state(&d.w).colors.foreground;
    let size = find_id(d, color.id).unwrap().diameter;
    let out: Vec<_> = (0..=12).map(|i| [c[0] - 4. - size * 1.3 * i as f32 / 12., c[1] + 4.]).collect();
    stroke(d, device, &out);
    pump(400);
    let dragged = center(d, color.id);
    assert!(c[0] - dragged[0] > size * 0.25, "{name}: dragging out past two radii moves the ring ({c:?} -> {dragged:?})");
    assert_eq!(state(&d.w).colors.foreground, picked, "{name}: and keeps the color it had");
    let c = dragged;
    let radius = size * 0.5 + 10.;
    let angle = 205f32.to_radians();
    let gap = [c[0] + radius * angle.cos(), c[1] + radius * angle.sin()];
    stroke(d, device, &[gap, [gap[0] + 10., gap[1] - 4.], [gap[0] + 60., gap[1] - 20.]]);
    pump(400);
    let moved = center(d, color.id);
    assert!((moved[0] - c[0]).hypot(moved[1] - c[1]) > 30., "{name}: the space around the wheel moves the ring");
    if device == "touch" {
        let c = moved;
        let held = state(&d.w).colors.foreground;
        touches(d, c, [c[0] + 70., c[1]], [40., 20.], 30.);
        let after = find_id(d, color.id).unwrap();
        assert!(after.diameter > size + 10., "{name}: two fingers starting on the wheel pinch the ring ({size} -> {})", after.diameter);
        let shifted = center(d, color.id);
        assert!((shifted[0] - c[0]).hypot(shifted[1] - c[1]) > 20., "{name}: and move it");
        assert_eq!(state(&d.w).colors.foreground, held, "{name}: without picking a color");
        d.w.dispatch(UiAction::Bubble { action: BubbleAction::SetOptions { id: color.id, options: BubbleOptions { size, ..after.options } } });
        pump(500);
    }
    let first = bead_point(d, color.id, Bead::Swatch { index: 0 }).unwrap();
    let further = bead_point(d, color.id, Bead::Swatch { index: 4 }).unwrap();
    let picked = state(&d.w).colors.foreground;
    stroke(d, device, &[further, [further[0] - 6., further[1] + 1.], first]);
    assert_eq!(state(&d.w).colors.foreground, picked, "{name}: sliding the palette picks nothing");
    let palettes = bead_point(d, color.id, Bead::Palettes).unwrap();
    tap(d, device, palettes);
    assert!(find_id(d, color.id).unwrap().pill.is_some(), "{name}: the palette bead opens its name");
    capture(d, &format!("{name}-color"));
    tap(d, device, canvas_spot(d, 0.15, 0.2));
    assert!(find_id(d, color.id).unwrap().pill.is_none(), "{name}: a contact elsewhere closes the name");
}

fn take_tile(d: &mut Driver, device: &str, control: ToolbarControl, to: [f32; 2]) {
    let from = d.point(&d.named(&tile_of(d, control)));
    stroke(d, device, &[from, [from[0] + 8., from[1]], [from[0] + 90., from[1]], [to[0] - 30., to[1]], to]);
}

fn brush_controls(d: &mut Driver, device: &str, name: &str) {
    take_tile(d, device, ToolbarControl::ColorPicker, canvas_spot(d, 0.3, 0.3));
    let controls = find(d, BubbleSource::Control { control: ToolbarControl::ColorPicker })
        .unwrap_or_else(|| panic!("{name}: dragging the color picker out of its bar takes a bubble"));
    assert_eq!(controls.rings.len(), 2, "{name}: size and strength are open at once");
    let at = center(d, controls.id);
    let ring = controls.rings[0].arc;
    let arc = |f: f32| [at[0] + ring.point(f)[0], at[1] + ring.point(f)[1]];
    d.w.dispatch(ToolbarNumericBinding::BrushSize.action(3.));
    let before = state(&d.w).brush.diameter;
    d.input.perform(serde_json::json!([contact(device, "down", arc(0.3)), contact(device, "move", arc(0.5)), contact(device, "move", arc(0.75))]));
    pump(300);
    assert_ne!(state(&d.w).brush.diameter, before, "{name}: the left arc sets the brush size");
    let stamp = find_id(d, controls.id).unwrap().stamp.unwrap_or_else(|| panic!("{name}: a dab follows the contact"));
    let under = arc(0.75);
    assert!((at[0] + stamp.center[0] - under[0]).hypot(at[1] + stamp.center[1] - under[1]) < 0.5, "{name}: the dab sits under the contact");
    capture(d, &format!("{name}-brush-controls"));
    d.input.perform(serde_json::json!([contact(device, "up", arc(0.75))]));
    pump(300);
    assert!(find_id(d, controls.id).unwrap().stamp.is_none(), "{name}: the dab goes with the contact");
    let strength = controls.rings[1].arc;
    d.w.dispatch(ToolbarNumericBinding::BrushStrength.action(0.95));
    let before = state(&d.w).tool_settings.iter().find(|f| f.id == controls.rings[1].setting).unwrap().value;
    stroke(d, device, &[[at[0] + strength.point(0.2)[0], at[1] + strength.point(0.2)[1]], [at[0] + strength.point(0.6)[0], at[1] + strength.point(0.6)[1]]]);
    let after = state(&d.w).tool_settings.iter().find(|f| f.id == controls.rings[1].setting).unwrap().value;
    assert_ne!(after, before, "{name}: the right arc sets the strength");
    tap(d, device, at);
    let picking = find_id(d, controls.id).unwrap();
    assert!(picking.selected, "{name}: the center starts the color picker");
    assert!(picking.rings.len() == 2 && picking.rings.iter().all(|r| !r.enabled), "{name}: its sliders stay, unavailable while picking");
    capture(d, &format!("{name}-brush-controls-picking"));
    tap(d, device, at);
    assert!(!find_id(d, controls.id).unwrap().selected, "{name}: and stops it");
}

fn history(d: &mut Driver, device: &str, name: &str) {
    take_tile(d, device, ToolbarControl::Command { command: CommandId::Redo }, canvas_spot(d, 0.3, 0.75));
    let source = BubbleSource::Control { control: ToolbarControl::Command { command: CommandId::Undo } };
    let bubble = find(d, source).unwrap_or_else(|| panic!("{name}: dragging redo out takes the undo and redo bubble"));
    take_tile(d, device, ToolbarControl::Command { command: CommandId::Undo }, canvas_spot(d, 0.45, 0.8));
    assert_eq!(views(d).iter().filter(|v| v.source == source).count(), 1, "{name}: undo moves the same bubble");
    let from = canvas_spot(d, 0.7, 0.2);
    stroke(d, device, &[from, [from[0] + 60., from[1] + 10.], [from[0] + 120., from[1]]]);
    let enabled = |d: &Driver| find_id(d, bubble.id).unwrap().buttons.iter().map(|b| b.enabled).collect::<Vec<_>>();
    assert_eq!(enabled(d), [true, false], "{name}: a stroke can be undone");
    let c = center(d, bubble.id);
    tap(d, device, [c[0] - 22., c[1]]);
    assert!(enabled(d)[1], "{name}: the left half undoes");
    tap(d, device, [c[0] + 22., c[1]]);
    assert!(!enabled(d)[1], "{name}: the right half redoes");
    capture(d, &format!("{name}-history"));
}

fn painting_fades(d: &mut Driver, name: &str, id: u32) {
    let widget = d.named(&format!("bubble-{id}"));
    let at = center(d, id);
    let from = [at[0] - 140., at[1] + 50.];
    let mut events = vec![contact("mouse", "down", from)];
    for step in 1..=20 {
        events.push(contact("mouse", "move", [from[0] + step as f32 * 12., from[1]]));
    }
    d.input.perform(serde_json::Value::Array(events));
    pump(120);
    assert!(widget.opacity() < 0.5, "{name}: a stroke near a bubble fades it");
    assert!(find_id(d, id).unwrap().open, "{name}: an open halo stays while painting");
    d.input.perform(serde_json::json!([contact("mouse", "up", [from[0] + 240., from[1]])]));
    pump(400);
    assert_eq!(widget.opacity(), 1., "{name}: it comes back after the stroke");
    d.w.dispatch(UiAction::SetTheme { theme: Some(state(&d.w).theme) });
    pump(20);
    assert!(widget.opacity() == 1. && widget.can_target(), "{name}: later refreshes keep it back");
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
    for device in ["mouse", "touch"] {
        let ring = find(d, BubbleSource::Control { control: ToolbarControl::Color }).unwrap();
        let from = rim(d, &ring, 200.);
        let to = [from[0] + 60., from[1] + 10.];
        d.input.perform(serde_json::json!([contact(device, "down", from), contact(device, "move", [from[0] + 20., from[1]]), contact(device, "move", to)]));
        pump(250);
        assert!(hidden(d), "{name}-{device}: moving a bubble in Zen keeps the toolbars hidden");
        d.input.perform(serde_json::json!([contact(device, "up", to)]));
        pump(300);
        assert!(hidden(d), "{name}-{device}: and after it lands");
        let c = center(d, ring.id);
        let hold = [c[0] + 8., c[1] - 6.];
        d.input.perform(serde_json::json!([contact(device, "down", hold), { "wait_ms": 1300 }, contact(device, "up", hold)]));
        pump(300);
        assert!(menu_open(d), "{name}-{device}: a hold opens the bubble's menu in Zen");
        assert!(hidden(d), "{name}-{device}: its menu keeps the toolbars hidden");
        d.input.key(0xff1b);
        pump(200);
        tap(d, device, [c[0] - 10., c[1] + 4.]);
        assert!(hidden(d) && state(&d.w).workspace.zen_mode, "{name}-{device}: picking on a bubble stays in Zen");
    }
    d.w.dispatch(UiAction::Invoke { command: CommandId::ZenMode });
    pump(300);
    pinch(d, name);
    put_back_all(d, "mouse", name);
}

fn pinch(d: &mut Driver, name: &str) {
    let ring = find(d, BubbleSource::Control { control: ToolbarControl::Color }).unwrap();
    let c = center(d, ring.id);
    let radius = ring.diameter * 0.5 - RING_RIM * 0.5;
    let rim = |degrees: f32, spread: f32, shift: f32| {
        let a = degrees.to_radians();
        let r = radius + spread;
        [c[0] + r * a.cos() + shift, c[1] + r * a.sin()]
    };
    let touch = |phase: &str, slot: u32, point: [f32; 2]| serde_json::json!({ "touch": phase, "slot": slot, "point": point });
    let mut events = vec![touch("down", 0, rim(200., 0., 0.)), touch("down", 1, rim(340., 0., 0.))];
    for step in 1..=6 {
        let s = step as f32;
        events.push(touch("move", 0, rim(200., s * 4., s * 6.)));
        events.push(touch("move", 1, rim(340., s * 4., s * 6.)));
    }
    events.push(serde_json::json!({ "touch": "up", "slot": 0 }));
    events.push(serde_json::json!({ "touch": "up", "slot": 1 }));
    d.input.perform(serde_json::Value::Array(events));
    pump(600);
    let after = find(d, BubbleSource::Control { control: ToolbarControl::Color }).unwrap();
    assert!(after.diameter > ring.diameter + 10., "{name}: pinching the rim enlarges the ring ({} -> {})", ring.diameter, after.diameter);
    let moved = center(d, ring.id);
    assert!(moved[0] > c[0] + 15., "{name}: two fingers move the ring");
    capture(d, &format!("{name}-pinched"));
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

            d.w.dispatch(UiAction::SelectBrush { id: layer_core::DefaultBrushPreset::PointyPencil as u32 });
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

            painting_fades(&mut d, &name, taken.id);
            tap(&mut d, device, at);
            pump(300);
            chooser(&mut d, device, &name, taken.id);

            let brush = BubbleSource::Control { control: ToolbarControl::Brush { id: layer_core::DefaultBrushPreset::Charcoal as u32 } };
            let spot = canvas_spot(&d, 0.62, 0.45);
            let torn = take(&mut d, BubbleGrab::Variant { variant: ToolVariant::BrushPreset { id: layer_core::DefaultBrushPreset::Charcoal as u32 } }, spot);
            let before = center(&d, taken.id);
            let toward = center(&d, torn);
            stroke(&mut d, device, &[toward, [toward[0] - 20., toward[1]], [before[0] + 40., before[1]], [before[0] + 20., before[1]]]);
            pump(800);
            let pushed = center(&d, taken.id);
            assert!((pushed[0] - before[0]).hypot(pushed[1] - before[1]) > 20., "{name}: a dragged bubble pushes its neighbour");
            capture(&mut d, &format!("{name}-pushed"));
            let torn = find_id(&d, torn).unwrap();
            assert!(!find(&d, brush).unwrap().selected, "{name}: tearing a row does not choose it");
            if device == "touch" {
                let at = center(&d, taken.id);
                touches(&mut d, at, [at[0] + 20., at[1] + 4.], [-60., 50.], 0.);
                let shifted = center(&d, taken.id);
                assert!((shifted[0] - at[0]).hypot(shifted[1] - at[1]) > 40., "{name}: two fingers on a face move the bubble");
            }
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
                let at = center(&d, torn.id);
                let to = [at[0] + 50., at[1] + 60.];
                d.input.perform(serde_json::json!([contact(device, "down", at), { "wait_ms": 900 }, contact(device, "move", [at[0] + 20., at[1] + 20.]), contact(device, "move", to), contact(device, "up", to)]));
                pump(400);
                let moved = center(&d, torn.id);
                assert!((moved[0] - at[0]).hypot(moved[1] - at[1]) > 40., "{name}: dragging after a hold moves the bubble");
                let open: Vec<_> = d.w.popovers.borrow().iter().filter_map(|p| p.upgrade()).filter(|p| p.is_visible()).map(|p| p.css_classes()).collect();
                assert!(open.is_empty(), "{name}: and closes its menu: {open:?}");
                assert!(!find(&d, brush).unwrap().selected, "{name}: without choosing it");
                let open: Vec<_> = d.w.popovers.borrow().iter().filter_map(|p| p.upgrade()).filter(|p| p.is_visible()).map(|p| p.css_classes()).collect();
                assert!(open.is_empty(), "{name}: Escape closes the bubble menu: {open:?}");
            }

            put_back_all(&mut d, device, &name);

            brush_controls(&mut d, device, &name);
            history(&mut d, device, &name);

            layers_card(&mut d, device, &name);
            color_ring(&mut d, device, &name);
            put_back_all(&mut d, device, &name);
        }
        zen(&mut d, &format!("{theme:?}").to_lowercase());
    }
    d.finish();
}

#[test]
#[ignore = "private display: LAYER_NATIVE_EVENT_MS=4 workspace-motion.sh gtk --native-test=native_tool_bubble_motion"]
fn native_tool_bubble_motion() {
    let mut d = Driver::new("art.capycanvas.ToolBubbleMotion");
    d.w.dispatch(UiAction::RestoreWorkspace {
        workspace: Box::new(WorkspaceState { layout: WorkspacePreset::Painter.layout(Platform::Gtk), ..WorkspaceState::default() }),
    });
    pump(400);
    let viewport = [d.w.surface.width() as f32, d.w.surface.height() as f32];
    let grabs = [
        BubbleGrab::Variant { variant: ToolVariant::BrushPreset { id: layer_core::DefaultBrushPreset::PointyPencil as u32 } },
        BubbleGrab::Variant { variant: ToolVariant::BrushGroup { group: ToolGroup::Oil } },
        BubbleGrab::Variant { variant: ToolVariant::BrushPreset { id: layer_core::DefaultBrushPreset::Charcoal as u32 } },
        BubbleGrab::Variant { variant: ToolVariant::BrushGroup { group: ToolGroup::Watercolor } },
        BubbleGrab::Variant { variant: ToolVariant::BrushPreset { id: layer_core::DefaultBrushPreset::Marker as u32 } },
    ];
    for (i, grab) in grabs.into_iter().enumerate() {
        let at = canvas_spot(&d, 0.3 + i as f32 * 0.1, 0.5);
        for phase in [ContactPhase::Down, ContactPhase::Up] {
            d.w.dispatch(UiAction::Bubble { action: BubbleAction::Drag { grab, phase, position: at, viewport } });
        }
    }
    pump(600);
    let mover = views(&d)[0].id;
    let mut reports = Vec::new();
    for device in ["mouse", "touch"] {
        let origin = center(&d, mover);
        d.input.perform(serde_json::json!([contact(device, "down", origin), contact(device, "move", [origin[0] + 6., origin[1]]), contact(device, "move", [origin[0] + 12., origin[1]])]));
        pump(100);
        let refreshes = d.w.publication.refreshes.get();
        let clock = d.w.surface.frame_clock().unwrap();
        let timings = Rc::new(RefCell::new(Vec::new()));
        let after = clock.connect_after_paint(glib::clone!(#[strong] timings, move |clock| {
            if let Some(t) = clock.current_timings() {
                timings.borrow_mut().push(t);
            }
        }));
        let stats = ui_session(&d.w).engine().backend().stats.clone();
        *stats.lock().unwrap() = Default::default();
        let events: Vec<_> = (0..550)
            .map(|i| {
                let t = i as f32 * 0.004;
                let swing = |p: f32| (p * std::f32::consts::TAU).sin();
                contact(device, "move", [origin[0] + 12. + swing(t * 0.5) * 420., origin[1] + swing(t * 1.5) * 60.])
            })
            .collect();
        d.input.perform(serde_json::Value::Array(events));
        let steady = d.w.publication.refreshes.get();
        let steady_end = clock.frame_time();
        d.input.perform(serde_json::json!([contact(device, "up", [origin[0] + 12., origin[1]])]));
        let released = std::time::Instant::now();
        while ui_session_mut(&d.w).step_bubbles(clock.frame_time().max(0) as u64 * 1000) && released.elapsed().as_secs() < 3 {
            pump(16);
        }
        let settle_ms = released.elapsed().as_secs_f64() * 1000.;
        clock.disconnect(after);
        assert_eq!(steady, refreshes, "{device}: steady bubble motion retains every UI model");
        let mut presented: Vec<_> = timings
            .borrow()
            .iter()
            .filter(|t| t.is_complete() && t.presentation_time() > 0 && t.frame_time() <= steady_end)
            .map(|t| t.presentation_time())
            .collect();
        presented.sort_unstable();
        presented.dedup();
        let mut gaps: Vec<_> = presented.windows(2).map(|p| (p[1] - p[0]) as f64 / 1000.).collect();
        gaps.sort_by(f64::total_cmp);
        let hz = (presented.len().max(2) - 1) as f64 * 1_000_000. / (presented.last().unwrap() - presented[0]).max(1) as f64;
        let canvas = {
            let stats = stats.lock().unwrap();
            let mut gpu: Vec<f64> = stats.gpu.iter().map(|v| v[1]).collect();
            gpu.sort_by(f64::total_cmp);
            serde_json::json!({ "backdrop_frames": stats.backdrop_frames, "frames": stats.cpu.len(), "gpu_ms": gpu.get(gpu.len() / 2), "gpu_p95_ms": gpu.get(gpu.len() * 95 / 100) })
        };
        let report = serde_json::json!({
            "device": device, "presented_frames": presented.len(), "presentation_hz": hz,
            "presentation_gap_p50_ms": gaps.get(gaps.len() / 2), "presentation_gap_p99_ms": gaps.get(gaps.len() * 99 / 100),
            "gaps_over_12_ms": gaps.iter().filter(|g| **g > 12.).count(), "longest_gap_ms": gaps.last(),
            "refresh_intervals_us": timings.borrow().iter().map(|t| t.refresh_interval()).collect::<std::collections::BTreeSet<_>>(),
            "settle_ms": settle_ms, "canvas": canvas,
        });
        eprintln!("{report}");
        reports.push(report);
        let moved = views(&d).iter().filter(|v| v.id != mover).count();
        assert_eq!(moved, 4);
    }
    if let Some(dir) = std::env::var_os("LAYER_TEST_ARTIFACTS") {
        std::fs::write(std::path::Path::new(&dir).join("tool-bubble-motion.json"), serde_json::to_vec_pretty(&reports).unwrap()).unwrap();
    }
    d.finish();
}

fn dark_outside_rings(directory: &std::path::Path) -> Vec<(usize, usize)> {
    let frames: Vec<serde_json::Value> = serde_json::from_slice(&std::fs::read(directory.join("frames.json")).unwrap()).unwrap();
    let mut found = Vec::new();
    for (index, frame) in frames.iter().enumerate() {
        let [width, height, stride] = ["width", "height", "stride"].map(|k| frame[k].as_u64().unwrap() as usize);
        let bytes = std::fs::read(directory.join(format!("{index:04}.rgb"))).unwrap();
        let px = |x: usize, y: usize| &bytes[y * stride + x * 3..][..3];
        let [x0, y0, x1, y1] = [width / 5, height / 6, width * 4 / 5, height * 9 / 10];
        let mut ring = Vec::new();
        for y in (y0..y1).step_by(2) {
            for x in (x0..x1).step_by(2) {
                let p = px(x, y);
                if p.iter().max().unwrap() - p.iter().min().unwrap() > 120 {
                    ring.push([x as f32, y as f32]);
                }
            }
        }
        if ring.len() < 20 {
            continue;
        }
        let center = ring.iter().fold([0.; 2], |a, p| [a[0] + p[0], a[1] + p[1]]).map(|v| v / ring.len() as f32);
        let reach = ring.iter().map(|p| (p[0] - center[0]).hypot(p[1] - center[1])).fold(0., f32::max) + 12.;
        let dark = (y0..y1)
            .flat_map(|y| (x0..x1).map(move |x| (x, y)))
            .filter(|&(x, y)| *px(x, y).iter().max().unwrap() < 24 && (x as f32 - center[0]).hypot(y as f32 - center[1]) > reach)
            .count();
        if dark > 12 {
            found.push((index, dark));
        }
    }
    found
}

#[test]
#[ignore = "private display: LAYER_NATIVE_CAPTURE_DIR=... workspace-motion.sh gtk --native-test=native_tool_bubble_frames"]
fn native_tool_bubble_frames() {
    let mut d = Driver::new("art.capycanvas.ToolBubbleFrames");
    let captures = std::env::var_os("LAYER_NATIVE_CAPTURE_DIR").map(std::path::PathBuf::from);
    for theme in [Theme::Light, Theme::Dark] {
        d.w.dispatch(UiAction::SetTheme { theme: Some(theme) });
        d.w.dispatch(UiAction::RestoreWorkspace {
            workspace: Box::new(WorkspaceState { layout: WorkspacePreset::Painter.layout(Platform::Gtk), ..WorkspaceState::default() }),
        });
        pump(500);
        let name = format!("{theme:?}").to_lowercase();
        let from = d.point(&d.named(&header_of(&d, ToolbarControl::Color)));
        let spot = canvas_spot(&d, 0.5, 0.55);
        let mut events = vec![serde_json::json!({ "record_start": format!("{name}-tear") })];
        events.extend([from, [from[0], from[1] + 8.], [from[0], from[1] + 70.], [spot[0], spot[1] - 30.], spot].iter().enumerate().map(|(i, p)| contact("mouse", if i == 0 { "down" } else { "move" }, *p)));
        events.extend([contact("mouse", "up", spot), serde_json::json!({ "wait_ms": 500 }), serde_json::json!({ "record_stop": true })]);
        d.input.perform(serde_json::Value::Array(events));
        pump(300);
        let ring = find(&d, BubbleSource::Control { control: ToolbarControl::Color }).unwrap();
        let c = center(&d, ring.id);
        let radius = ring.diameter * 0.5 - RING_RIM * 0.5;
        let touch = |phase: &str, slot: u32, point: [f32; 2]| serde_json::json!({ "touch": phase, "slot": slot, "point": point });
        let mut events = vec![serde_json::json!({ "record_start": format!("{name}-pinch") }), touch("down", 0, [c[0] - radius, c[1]]), touch("down", 1, [c[0] + radius, c[1]])];
        for i in 1..=10 {
            let s = i as f32 * 5.;
            events.push(touch("move", 0, [c[0] - radius - s, c[1]]));
            events.push(touch("move", 1, [c[0] + radius + s, c[1]]));
        }
        events.extend([serde_json::json!({ "touch": "up", "slot": 0 }), serde_json::json!({ "touch": "up", "slot": 1 }), serde_json::json!({ "wait_ms": 600 }), serde_json::json!({ "record_stop": true })]);
        d.input.perform(serde_json::Value::Array(events));
        pump(300);
        assert!(find_id(&d, ring.id).unwrap().diameter > ring.diameter, "{name}: the pinch resized the ring");
        let ring = find_id(&d, ring.id).unwrap();
        let from = rim(&d, &ring, 200.);
        let mut events = vec![serde_json::json!({ "record_start": format!("{name}-putback") }), contact("mouse", "down", from)];
        events.extend([[from[0], from[1] - 20.], [from[0], 10.]].map(|p| contact("mouse", "move", p)));
        events.extend([contact("mouse", "up", [from[0], 10.]), serde_json::json!({ "wait_ms": 500 }), serde_json::json!({ "record_stop": true })]);
        d.input.perform(serde_json::Value::Array(events));
        pump(300);
        assert!(views(&d).is_empty(), "{name}: the ring went back");
        if let Some(dir) = &captures {
            for part in ["tear", "pinch", "putback"] {
                let dark = dark_outside_rings(&dir.join(format!("{name}-{part}")));
                assert!(dark.is_empty(), "{name}-{part}: dark pixels outside the ring in frames {dark:?}");
            }
        }
    }
    d.finish();
}
