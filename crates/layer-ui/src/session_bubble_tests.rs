use super::{test_support::*, *};
use layer_core::DefaultBrushPreset;

const VIEW: [f32; 2] = [1200., 900.];
const FAR: [f32; 2] = [640., 520.];

fn desk() -> UiSession<Recorder> {
    let mut s = session(Platform::Gtk);
    s.set_viewport(VIEW, [1200, 900]).unwrap();
    s
}

fn act(s: &mut UiSession<Recorder>, action: BubbleAction) -> Result<UiChange, String> {
    s.dispatch(UiAction::Bubble { action })
}

fn contact(s: &mut UiSession<Recorder>, grab: BubbleGrab, phase: ContactPhase, position: [f32; 2]) -> Result<UiChange, String> {
    act(s, BubbleAction::Drag { grab, phase, position, viewport: VIEW })
}

fn drag(s: &mut UiSession<Recorder>, grab: BubbleGrab, from: [f32; 2], to: [f32; 2]) {
    contact(s, grab, ContactPhase::Down, from).unwrap();
    contact(s, grab, ContactPhase::Move, to).unwrap();
    contact(s, grab, ContactPhase::Up, to).unwrap();
}

fn take(s: &mut UiSession<Recorder>, grab: BubbleGrab, at: [f32; 2]) -> u32 {
    drag(s, grab, at, at);
    s.state.workspace.layout.bubbles.bubbles.last().unwrap().id
}

fn brush(id: DefaultBrushPreset) -> BubbleGrab {
    BubbleGrab::Variant { variant: ToolVariant::BrushPreset { id: id as u32 } }
}

fn toolbar_tile(s: &mut UiSession<Recorder>, control: ToolbarControl) -> (Panel, u32) {
    let panel = s.layout(VIEW).groups.iter().flat_map(|g| g.panels.clone()).find(|p| p.kind() == PanelKind::Tiles).unwrap();
    let layout = &mut s.state.workspace.layout;
    layout.insert_tools(panel, None, &[control]).unwrap();
    (panel, *tile_ids(layout, panel).last().unwrap())
}

fn view(s: &UiSession<Recorder>, id: u32) -> BubbleView {
    s.bubbles().into_iter().find(|b| b.id == id).unwrap()
}

fn tap(s: &mut UiSession<Recorder>, id: u32) {
    act(s, BubbleAction::Tap { id }).unwrap();
}

fn drawer_anchor(s: &UiSession<Recorder>) -> Option<DrawerAnchor> {
    s.state.customization.drawer.as_ref().map(|d| d.anchor)
}

#[test]
fn dragging_a_tool_past_the_line_takes_one_bubble_in_one_undo_step() {
    let mut s = desk();
    let control = ToolbarControl::ToolSlot { slot: ToolSlotId::Drawing };
    let (panel, tile) = toolbar_tile(&mut s, control);
    let grab = BubbleGrab::Tile { panel, tile };
    let group = find_group(&s, VIEW, |g| g.panels.contains(&panel)).bounds;
    let close = [(group.x + group.width + 40.).min(VIEW[0] - 10.), group.y + 40.];
    let close = if group.contains(close[0], close[1]) { [group.x - 40., group.y + 40.] } else { close };
    drag(&mut s, grab, close, close);
    assert!(s.state.workspace.layout.bubbles.bubbles.is_empty(), "within the line the tool stays in its toolbar");
    let id = take(&mut s, grab, FAR);
    let bubble = s.state.workspace.layout.bubbles.get(id).unwrap().clone();
    assert_eq!(bubble.source, BubbleSource::Control { control });
    assert_eq!(bubble.center, FAR);
    assert!(s.state.workspace.layout.panel(panel).unwrap().tiles().iter().any(|t| t.id == tile), "the tile stays");
    assert!(s.workspace_history.can_undo());
    drag(&mut s, grab, [300., 300.], [300., 300.]);
    assert_eq!(s.state.workspace.layout.bubbles.bubbles.len(), 1, "one bubble per source");
    assert_eq!(s.state.workspace.layout.bubbles.get(id).unwrap().center, [300., 300.]);
    invoke(&mut s, CommandId::UndoWorkspace);
    assert_eq!(s.state.workspace.layout.bubbles.get(id).unwrap().center, FAR);
    invoke(&mut s, CommandId::UndoWorkspace);
    assert!(s.state.workspace.layout.bubbles.bubbles.is_empty());
    invoke(&mut s, CommandId::RedoWorkspace);
    assert_eq!(s.state.workspace.layout.bubbles.get(id), Some(&bubble));
    s.state.workspace.layout.validate().unwrap();
}

#[test]
fn dropping_a_bubble_on_a_panel_or_the_title_bar_puts_it_back() {
    let mut s = desk();
    let id = take(&mut s, brush(DefaultBrushPreset::PointyPencil), FAR);
    let group = s.layout(VIEW).groups[0].bounds;
    let grab = BubbleGrab::Bubble { id };
    contact(&mut s, grab, ContactPhase::Down, FAR).unwrap();
    contact(&mut s, grab, ContactPhase::Move, [group.x + 4., group.y + 4.]).unwrap();
    assert!(s.bubble_placements()[0].homing, "the lifted bubble shows it will go back");
    contact(&mut s, grab, ContactPhase::Up, [group.x + 4., group.y + 4.]).unwrap();
    assert!(s.state.workspace.layout.bubbles.bubbles.is_empty());
    invoke(&mut s, CommandId::UndoWorkspace);
    assert_eq!(s.state.workspace.layout.bubbles.get(id).unwrap().center, FAR);
    drag(&mut s, grab, FAR, [600., 8.]);
    assert!(s.state.workspace.layout.bubbles.bubbles.is_empty());
    let id = take(&mut s, brush(DefaultBrushPreset::PointyPencil), FAR);
    act(&mut s, BubbleAction::PutBack { id }).unwrap();
    assert!(s.state.workspace.layout.bubbles.bubbles.is_empty());
}

#[test]
fn cancelled_and_competing_drags_leave_the_layout_unchanged() {
    let mut s = desk();
    let before = s.state.workspace.layout.clone();
    let grab = brush(DefaultBrushPreset::PointyPencil);
    contact(&mut s, grab, ContactPhase::Down, FAR).unwrap();
    assert!(contact(&mut s, brush(DefaultBrushPreset::Charcoal), ContactPhase::Down, [200., 300.]).is_err());
    contact(&mut s, grab, ContactPhase::Cancel, FAR).unwrap();
    assert_eq!(s.state.workspace.layout, before);
    assert!(s.bubble_placements().is_empty());
    assert!(contact(&mut s, grab, ContactPhase::Up, FAR).unwrap().regions == 0, "a late release is ignored");
}

#[test]
fn brush_bubble_taps_select_then_open_and_close_its_settings() {
    let mut s = desk();
    let pencil = DefaultBrushPreset::PointyPencil as u32;
    let id = take(&mut s, brush(DefaultBrushPreset::PointyPencil), FAR);
    assert!(!view(&s, id).selected);
    tap(&mut s, id);
    assert_eq!(s.state.brush.preset, pencil);
    assert!(!view(&s, id).open && view(&s, id).rings.is_empty(), "the first tap only selects");
    tap(&mut s, id);
    let open = view(&s, id);
    let defaults = tool_settings::ring_defaults(s.engine.configured_brush(), &s.state.tool_settings);
    assert!(open.open && !defaults.is_empty());
    assert_eq!(open.rings.iter().map(|r| r.setting).collect::<Vec<_>>(), defaults);
    tap(&mut s, id);
    assert!(!view(&s, id).open);
    s.dispatch(UiAction::SelectBrush { id: DefaultBrushPreset::GPen as u32 }).unwrap();
    tap(&mut s, id);
    assert!(!view(&s, id).open);
    s.dispatch(UiAction::SelectBrush { id: DefaultBrushPreset::GPen as u32 }).unwrap();
    s.dispatch(UiAction::SelectBrush { id: pencil }).unwrap();
    tap(&mut s, id);
    assert!(view(&s, id).open, "a selected brush shows its settings on the first tap");
}

#[test]
fn dial_taps_select_then_show_settings_then_the_chooser_and_a_brush_choice_closes_it() {
    let mut s = desk();
    let control = ToolbarControl::ToolSlot { slot: ToolSlotId::Drawing };
    let (panel, tile) = toolbar_tile(&mut s, control);
    let id = take(&mut s, BubbleGrab::Tile { panel, tile }, FAR);
    invoke(&mut s, CommandId::Lasso);
    assert!(!view(&s, id).selected);
    tap(&mut s, id);
    assert!(view(&s, id).selected);
    assert_eq!(drawer_anchor(&s), None);
    tap(&mut s, id);
    assert!(!view(&s, id).rings.is_empty());
    tap(&mut s, id);
    assert!(view(&s, id).rings.is_empty());
    let drawer = s.state.customization.drawer.clone().unwrap();
    assert_eq!(drawer.anchor, DrawerAnchor::Bubble { id });
    assert_eq!(drawer.columns, vec![vec![Panel::Brushes]]);
    let placement = s.state.customization.drawer_placement(&drawer, &s.state.workspace.layout, VIEW, &[300.]).unwrap();
    assert!(placement.bounds.x >= FAR[0] + TOOL_BUBBLE * 0.5 || placement.bounds.x + placement.bounds.width <= FAR[0] - TOOL_BUBBLE * 0.5);
    let choice = drawer.tool_set.as_ref().unwrap().subtools.iter().find(|t| !t.selected && t.preview.is_some()).unwrap().action.clone();
    s.dispatch(choice).unwrap();
    assert_eq!(drawer_anchor(&s), None, "picking a brush closes the chooser");
    assert!(!view(&s, id).open);
    tap(&mut s, id);
    tap(&mut s, id);
    tap(&mut s, id);
    assert_eq!(drawer_anchor(&s), None, "the third tap closes the chooser");
}

#[test]
fn medium_bubbles_select_their_brushes_and_list_only_them() {
    let mut s = desk();
    let id = take(&mut s, BubbleGrab::Variant { variant: ToolVariant::BrushGroup { group: ToolGroup::Pastel } }, FAR);
    let brushes = tools::group_presets(ToolGroup::Pastel).count();
    assert_eq!(view(&s, id).dots.len(), brushes);
    tap(&mut s, id);
    assert_eq!(tools::group(s.state.brush.preset), ToolGroup::Pastel);
    assert_eq!(view(&s, id).dots.iter().filter(|d| **d).count(), 1);
    tap(&mut s, id);
    tap(&mut s, id);
    let drawer = s.state.customization.drawer.clone().unwrap();
    assert_eq!(drawer.anchor, DrawerAnchor::Bubble { id });
    let set = drawer.tool_set.unwrap();
    assert!(set.groups.is_empty());
    assert_eq!(set.subtools.len(), brushes);
}

#[test]
fn ring_contacts_set_values_along_their_arc() {
    let mut s = desk();
    let id = take(&mut s, brush(DefaultBrushPreset::PointyPencil), FAR);
    tap(&mut s, id);
    tap(&mut s, id);
    let ring = view(&s, id).rings[0].clone();
    let value = |s: &UiSession<Recorder>| s.state.tool_settings.iter().find(|c| c.id == ring.setting).unwrap().value;
    let numeric = s.state.tool_settings.iter().find(|c| c.id == ring.setting).unwrap().numeric.clone();
    let at = |f: f64| numeric.resolve(0., NumericOperation::Position { position: f }).unwrap().value as f32;
    act(&mut s, BubbleAction::Ring { id, phase: ContactPhase::Down, position: ring.arc.point(0.2) }).unwrap();
    assert!(view(&s, id).rings[0].active);
    assert!((value(&s) - at(0.2)).abs() < 1e-3);
    act(&mut s, BubbleAction::Ring { id, phase: ContactPhase::Move, position: [ring.arc.point(0.9)[0] * 3., ring.arc.point(0.9)[1] * 3.] }).unwrap();
    assert!((value(&s) - at(0.9)).abs() < 1e-3, "the contact stays with its ring");
    act(&mut s, BubbleAction::Ring { id, phase: ContactPhase::Up, position: ring.arc.point(0.9) }).unwrap();
    assert!(!view(&s, id).rings[0].active);
    assert!(act(&mut s, BubbleAction::Ring { id, phase: ContactPhase::Down, position: [0., 0.] }).is_err());
}

#[test]
fn slider_bubbles_set_their_value_from_the_ring() {
    let mut s = desk();
    let (panel, tile) = toolbar_tile(&mut s, ToolbarControl::Opacity);
    let id = take(&mut s, BubbleGrab::Tile { panel, tile }, FAR);
    let bubble = view(&s, id);
    assert_eq!(bubble.source, BubbleSource::Control { control: ToolbarControl::BrushOpacitySlider });
    assert_eq!(bubble.diameter, SLIDER_BUBBLE);
    let ring = bubble.rings[0].clone();
    act(&mut s, BubbleAction::Ring { id, phase: ContactPhase::Down, position: ring.arc.point(0.5) }).unwrap();
    act(&mut s, BubbleAction::Ring { id, phase: ContactPhase::Up, position: ring.arc.point(0.5) }).unwrap();
    let opacity = s.state.tool_settings.iter().find(|c| c.id == "opacity").unwrap().value;
    assert!((opacity - 0.5).abs() < 0.011, "{opacity}");
    assert!((view(&s, id).rings[0].fill - 0.5).abs() < 0.011);
    tap(&mut s, id);
    assert!(!view(&s, id).open, "taps on sliders change nothing");
}

#[test]
fn the_hold_menu_chooses_ring_settings_up_to_six_with_undo() {
    let mut s = desk();
    let pencil = DefaultBrushPreset::PointyPencil as u32;
    let id = take(&mut s, brush(DefaultBrushPreset::PointyPencil), FAR);
    let menu = s.context_menu(ContextTarget::Bubble { id }).unwrap();
    let rows = |menu: &ContextMenu| menu.sections[0][0].sections[0].clone();
    assert_eq!(menu.sections[0][0].label, "Settings in the ring");
    assert_eq!(menu.sections.last().unwrap()[0].label, "Put back");
    let all = rows(&menu);
    assert!(all.len() > MAX_RING_SETTINGS);
    let shown = all.iter().filter(|r| r.selected == Some(true)).count();
    for row in all.iter().filter(|r| r.selected == Some(false)) {
        let menu = s.context_menu(ContextTarget::Bubble { id }).unwrap();
        let current = rows(&menu).into_iter().find(|r| r.action == row.action).unwrap();
        let count = rows(&menu).iter().filter(|r| r.selected == Some(true)).count();
        assert_eq!(current.enabled, count < MAX_RING_SETTINGS);
        if current.enabled {
            s.dispatch(current.action.unwrap()).unwrap();
        } else {
            assert!(s.dispatch(current.action.unwrap()).is_err());
        }
    }
    assert_eq!(s.state.workspace.layout.bubbles.ring(pencil).unwrap().len(), MAX_RING_SETTINGS);
    s.dispatch(UiAction::SelectBrush { id: pencil }).unwrap();
    tap(&mut s, id);
    assert_eq!(view(&s, id).rings.len(), MAX_RING_SETTINGS);
    for _ in shown..MAX_RING_SETTINGS {
        invoke(&mut s, CommandId::UndoWorkspace);
    }
    assert_eq!(s.state.workspace.layout.bubbles.ring(pencil), None);
}

#[test]
fn outside_contacts_close_the_halo() {
    let mut s = desk();
    let id = take(&mut s, brush(DefaultBrushPreset::PointyPencil), FAR);
    tap(&mut s, id);
    tap(&mut s, id);
    chrome(&mut s, ChromeEvent::Contact { position: [FAR[0] + 30., FAR[1]], canvas: false }, ChromeFacts::default());
    assert!(view(&s, id).open, "contacts on the halo keep it");
    let reply = chrome(&mut s, ChromeEvent::Contact { position: [100., 700.], canvas: true }, ChromeFacts::default());
    assert!(!view(&s, id).open);
    assert_ne!(reply.change.regions & regions::BUBBLES, 0);
}

#[test]
fn color_and_layers_options_change_from_their_menus() {
    let mut s = desk();
    let (panel, tile) = toolbar_tile(&mut s, ToolbarControl::Color);
    let color = take(&mut s, BubbleGrab::Tile { panel, tile }, [300., 500.]);
    let (panel, tile) = toolbar_tile(&mut s, ToolbarControl::Panel { panel: Panel::Layers });
    let layers = take(&mut s, BubbleGrab::Tile { panel, tile }, [800., 500.]);
    let menu = s.context_menu(ContextTarget::Bubble { id: color }).unwrap();
    assert_eq!(menu.sections[0].iter().map(|i| i.selected).collect::<Vec<_>>(), [Some(true); 3]);
    let large = menu.sections[1][2].action.clone().unwrap();
    s.dispatch(large).unwrap();
    assert_eq!(view(&s, color).diameter, COLOR_RING_SIZES[2]);
    invoke(&mut s, CommandId::UndoWorkspace);
    assert_eq!(view(&s, color).diameter, COLOR_RING_SIZES[1]);
    tap(&mut s, layers);
    let drawer = s.state.customization.drawer.clone().unwrap();
    assert_eq!(drawer.anchor, DrawerAnchor::Bubble { id: layers });
    assert_eq!(drawer.layer_properties, Some(false));
    assert_eq!(drawer.dismissal, DrawerDismissal::OutsideContact);
    assert!(drawer.simple, "the card starts simple");
    tap(&mut s, layers);
    assert_eq!(drawer_anchor(&s), None);
    let menu = |s: &UiSession<Recorder>| s.context_menu(ContextTarget::Bubble { id: layers }).unwrap();
    s.dispatch(menu(&s).sections[0][1].action.clone().unwrap()).unwrap();
    s.dispatch(menu(&s).sections[1][0].action.clone().unwrap()).unwrap();
    assert!(view(&s, layers).options.full);
    tap(&mut s, layers);
    let drawer = s.state.customization.drawer.as_ref().unwrap();
    assert_eq!(drawer.dismissal, DrawerDismissal::Explicit);
    assert!(!drawer.simple);
}

#[test]
fn putting_back_an_open_bubble_closes_its_halo_and_chooser() {
    let mut s = desk();
    let id = take(&mut s, BubbleGrab::Variant { variant: ToolVariant::BrushGroup { group: ToolGroup::Pastel } }, FAR);
    tap(&mut s, id);
    tap(&mut s, id);
    tap(&mut s, id);
    assert_eq!(drawer_anchor(&s), Some(DrawerAnchor::Bubble { id }));
    act(&mut s, BubbleAction::PutBack { id }).unwrap();
    assert_eq!(drawer_anchor(&s), None);
    assert!(s.bubbles().is_empty() && s.bubble_placements().is_empty());
    assert!(act(&mut s, BubbleAction::Tap { id }).is_err());
}

#[test]
fn a_smaller_window_keeps_bubbles_inside_without_changing_their_saved_place() {
    let mut s = desk();
    let work = s.layout(VIEW).work_area;
    let corner = [work.x + work.width - 40., work.y + work.height - 40.];
    let id = take(&mut s, brush(DefaultBrushPreset::PointyPencil), corner);
    s.set_viewport([600., 500.], [600, 500]).unwrap();
    let mut now = 0;
    while s.step_bubbles(now) {
        now += 16_666_667;
    }
    let area = s.state.workspace.layout.free_area([600., 500.]);
    let center = s.bubble_placements()[0].center;
    assert!(center[0] + TOOL_BUBBLE * 0.5 <= area.x + area.width + 1e-3 && center[1] + TOOL_BUBBLE * 0.5 <= area.y + area.height + 1e-3);
    assert_eq!(s.state.workspace.layout.bubbles.get(id).unwrap().center, corner);
    s.set_viewport(VIEW, [1200, 900]).unwrap();
    while s.step_bubbles(now) {
        now += 16_666_667;
    }
    assert_eq!(s.bubble_placements()[0].center, corner);
}

#[test]
fn bubbles_round_trip_with_the_workspace() {
    let mut s = desk();
    let id = take(&mut s, brush(DefaultBrushPreset::PointyPencil), FAR);
    let json = serde_json::to_string(&s.state.workspace).unwrap();
    let restored: WorkspaceState = serde_json::from_str(&json).unwrap();
    restored.layout.validate().unwrap();
    assert_eq!(restored.layout.bubbles.get(id), s.state.workspace.layout.bubbles.get(id));
    let mut legacy: serde_json::Value = serde_json::to_value(&s.state.workspace).unwrap();
    legacy["layout"].as_object_mut().unwrap().remove("bubbles");
    assert!(serde_json::from_value::<WorkspaceState>(legacy).unwrap().layout.bubbles.is_empty());
}

#[test]
fn color_ring_beads_pick_scroll_and_switch_palettes() {
    let mut s = desk();
    let (panel, tile) = toolbar_tile(&mut s, ToolbarControl::Color);
    let id = take(&mut s, BubbleGrab::Tile { panel, tile }, [500., 380.]);
    for i in 0..20 {
        let color = layer_core::color::RgbColor { linear_rgb: None, space: s.state.colors.rgb_space(), rgba: [i as f32 / 20., 0.2, 0.4, 1.] };
        s.dispatch(UiAction::Color { action: ColorAction::Definition { color } }).unwrap();
        s.state.color_library.record_use(color);
    }
    let beads = view(&s, id).beads;
    let recent = |beads: &[BubbleBead]| beads.iter().filter(|b| matches!(b.placement.bead, Bead::Recent { .. }) && b.placement.alpha == 1.).count();
    assert_eq!(recent(&beads), RECENT_BEADS);
    for kind in [Bead::Foreground, Bead::Background, Bead::Transparent, Bead::Swatch { index: 0 }] {
        assert!(beads.iter().any(|b| b.placement.bead == kind), "{kind:?}");
    }
    act(&mut s, BubbleAction::Scroll { id, strip: BeadStrip::Palette, pixels: -10_000. }).unwrap();
    assert!(view(&s, id).beads.iter().any(|b| b.placement.bead == Bead::Palettes && b.placement.alpha == 1.), "the palette icon ends the strip");
    let first = beads.iter().find(|b| b.placement.bead == Bead::Recent { index: 0 }).unwrap().placement.center;
    act(&mut s, BubbleAction::Scroll { id, strip: BeadStrip::Recent, pixels: -26. * 3. }).unwrap();
    let moved = view(&s, id).beads;
    assert!(moved.iter().find(|b| b.placement.bead == Bead::Recent { index: 3 }).unwrap().placement.center == first, "beads travel with the finger");
    act(&mut s, BubbleAction::Scroll { id, strip: BeadStrip::Recent, pixels: -10_000. }).unwrap();
    let last = s.state.color_library.history.len() - 1;
    assert!(view(&s, id).beads.iter().any(|b| b.placement.bead == Bead::Recent { index: last } && b.placement.alpha == 1.));
    let target = s.state.color_library.history[2];
    act(&mut s, BubbleAction::Bead { id, bead: Bead::Recent { index: 2 } }).unwrap();
    assert_eq!(s.state.colors.definition(), target);
    act(&mut s, BubbleAction::Bead { id, bead: Bead::Transparent }).unwrap();
    assert_eq!(s.state.colors.slot, ColorSlot::Transparent);
    assert!(view(&s, id).pill.is_none());
    act(&mut s, BubbleAction::Bead { id, bead: Bead::Palettes }).unwrap();
    let name = view(&s, id).pill.unwrap();
    assert_eq!(name, s.state.color_library.active_palette().name);
    let palettes = s.state.color_library.palettes.len();
    act(&mut s, BubbleAction::Palette { id, step: 1 }).unwrap();
    assert_eq!(view(&s, id).pill.is_some(), true);
    if palettes > 1 {
        assert_ne!(view(&s, id).pill.unwrap(), name);
    }
    act(&mut s, BubbleAction::Palette { id, step: -1 }).unwrap();
    assert_eq!(view(&s, id).pill.unwrap(), name);
    let menu = s.context_menu(ContextTarget::BubblePalettes { id }).unwrap();
    assert_eq!(menu.sections[0].len(), palettes);
    assert_eq!(menu.sections[0].iter().filter(|i| i.selected == Some(true)).count(), 1);
    let options = BubbleOptions { palette: false, ..view(&s, id).options };
    act(&mut s, BubbleAction::SetOptions { id, options }).unwrap();
    let beads = view(&s, id).beads;
    assert!(!beads.iter().any(|b| matches!(b.placement.bead, Bead::Swatch { .. } | Bead::Palettes)));
    assert!(view(&s, id).pill.is_none(), "the pill belongs to the palette");
}

#[test]
fn reset_layout_puts_bubbles_back_and_keeps_ring_choices() {
    let mut s = desk();
    let pencil = DefaultBrushPreset::PointyPencil as u32;
    take(&mut s, brush(DefaultBrushPreset::PointyPencil), FAR);
    act(&mut s, BubbleAction::ToggleRingSetting { brush: pencil, setting: "size".into() }).unwrap();
    s.state.workspace.layout.reset_docking(Platform::Gtk).unwrap();
    assert!(s.state.workspace.layout.bubbles.bubbles.is_empty());
    assert!(s.state.workspace.layout.bubbles.ring(pencil).is_some_and(|ring| ring.iter().any(|id| id == "size")));
    assert!(s.bubble_placements().is_empty());
}

#[test]
fn a_bubble_that_returns_starts_with_fresh_scrolling_and_no_open_parts() {
    let mut s = desk();
    let (panel, tile) = toolbar_tile(&mut s, ToolbarControl::Color);
    let before = s.state.workspace.clone();
    let id = take(&mut s, BubbleGrab::Tile { panel, tile }, [500., 380.]);
    act(&mut s, BubbleAction::Scroll { id, strip: BeadStrip::Palette, pixels: -10_000. }).unwrap();
    act(&mut s, BubbleAction::Bead { id, bead: Bead::Palettes }).unwrap();
    assert!(view(&s, id).pill.is_some());
    s.dispatch(UiAction::RestoreWorkspace { workspace: Box::new(before) }).unwrap();
    s.bubble_placements();
    let again = take(&mut s, BubbleGrab::Tile { panel, tile }, [500., 380.]);
    assert_eq!(again, id, "a restored layout reuses the identity");
    let fresh = view(&s, again);
    assert!(fresh.pill.is_none());
    assert!(fresh.beads.iter().any(|b| b.placement.bead == Bead::Swatch { index: 0 } && b.placement.alpha == 1.));
}

#[test]
fn toolbars_can_stay_in_zen_mode() {
    let mut s = desk();
    let bar = s.state.workspace.layout.panels.iter().find(|p| p.id.kind() == PanelKind::Tiles && s.state.workspace.layout.panel_group(p.id).is_some()).unwrap().id;
    let group = s.state.workspace.layout.panel_group(bar).unwrap();
    let item = |s: &UiSession<Recorder>| {
        s.context_menu(ContextTarget::Ribbon { panel: bar }).unwrap().sections.into_iter().flatten().find(|i| i.label == "Show in Zen mode").unwrap()
    };
    assert_eq!(item(&s).selected, Some(false), "toolbars hide in Zen by default");
    assert!(!s.state.workspace.layout.shows_in_zen(group));
    s.dispatch(item(&s).action.unwrap()).unwrap();
    assert!(s.state.workspace.layout.shows_in_zen(group));
    assert_eq!(item(&s).selected, Some(true));
    invoke(&mut s, CommandId::ZenMode);
    s.interaction.hidden = true;
    let bounds = find_group(&s, VIEW, |g| g.id == group).bounds;
    let inside = [bounds.x + bounds.width * 0.5, bounds.y + bounds.height * 0.5];
    let id = take(&mut s, brush(DefaultBrushPreset::PointyPencil), FAR);
    drag(&mut s, BubbleGrab::Bubble { id }, FAR, inside);
    assert!(s.state.workspace.layout.bubbles.bubbles.is_empty(), "a toolbar shown in Zen takes bubbles back");
    invoke(&mut s, CommandId::UndoWorkspace);
    assert_eq!(s.state.workspace.layout.bubbles.bubbles.len(), 1);
    invoke(&mut s, CommandId::UndoWorkspace);
    assert!(s.state.workspace.layout.shows_in_zen(group));
    invoke(&mut s, CommandId::UndoWorkspace);
    assert!(!s.state.workspace.layout.shows_in_zen(group), "the choice is one layout step");
}

#[test]
fn an_edge_swipe_in_zen_peeks_that_edges_toolbars_until_a_contact_elsewhere() {
    let mut s = desk();
    let layout = s.layout(VIEW);
    let group = layout.groups.iter().find(|g| !g.floating && s.state.workspace.layout.group_edge(g.id).is_some()).unwrap().clone();
    let edge = s.state.workspace.layout.group_edge(group.id).unwrap();
    invoke(&mut s, CommandId::ZenMode);
    chrome(&mut s, ChromeEvent::Refresh, ChromeFacts::default());
    assert!(s.interaction.hidden && !s.zen_keeps(group.id));
    let (start, inward) = match edge {
        Edge::Left => ([4., 450.], [1., 0.]),
        Edge::Right => ([VIEW[0] - 4., 450.], [-1., 0.]),
        Edge::Top => ([600., 4.], [0., 1.]),
        Edge::Bottom => ([600., VIEW[1] - 4.], [0., -1.]),
    };
    let at = |d: f32| [start[0] + inward[0] * d, start[1] + inward[1] * d];
    let touch = |s: &mut UiSession<Recorder>, phase, position| {
        s.input(UiInput::Pointer { id: 9, phase, kind: PointerKind::Touch, button: PointerButton::Primary, position, time_ns: 0 }).unwrap()
    };
    touch(&mut s, ContactPhase::Down, start);
    touch(&mut s, ContactPhase::Move, at(20.));
    assert!(!s.zen_keeps(group.id), "a short swipe keeps chrome hidden");
    touch(&mut s, ContactPhase::Move, at(60.));
    assert!(s.zen_keeps(group.id), "swiping in from the edge shows its toolbar");
    touch(&mut s, ContactPhase::Up, at(60.));
    let inside = [group.bounds.x + group.bounds.width * 0.5, group.bounds.y + group.bounds.height * 0.5];
    chrome(&mut s, ChromeEvent::Contact { position: inside, canvas: false }, ChromeFacts::default());
    assert!(s.zen_keeps(group.id), "using the peeked toolbar keeps it");
    chrome(&mut s, ChromeEvent::Contact { position: [600., 450.], canvas: true }, ChromeFacts::default());
    assert!(!s.zen_keeps(group.id), "a contact elsewhere hides it again");
    touch(&mut s, ContactPhase::Down, [600., 450.]);
    touch(&mut s, ContactPhase::Move, [700., 450.]);
    assert!(!s.zen_keeps(group.id), "swipes away from the edges do nothing");
}

#[test]
fn pinching_the_color_ring_resizes_it_within_its_range() {
    let mut s = desk();
    let (panel, tile) = toolbar_tile(&mut s, ToolbarControl::Color);
    let id = take(&mut s, BubbleGrab::Tile { panel, tile }, [500., 380.]);
    act(&mut s, BubbleAction::Resize { id, scale: 1.25 }).unwrap();
    assert_eq!(view(&s, id).diameter, COLOR_RING_SIZES[1] * 1.25);
    act(&mut s, BubbleAction::Resize { id, scale: 10. }).unwrap();
    assert_eq!(view(&s, id).diameter, COLOR_RING_SIZES[2]);
    act(&mut s, BubbleAction::Resize { id, scale: 0.01 }).unwrap();
    assert_eq!(view(&s, id).diameter, COLOR_RING_SIZES[0]);
    invoke(&mut s, CommandId::UndoWorkspace);
    assert_eq!(view(&s, id).diameter, COLOR_RING_SIZES[2]);
    let brush = take(&mut s, brush(DefaultBrushPreset::PointyPencil), [900., 380.]);
    assert!(act(&mut s, BubbleAction::Resize { id: brush, scale: 2. }).is_err());
    assert!(act(&mut s, BubbleAction::Resize { id, scale: f32::NAN }).is_err());
}

#[test]
fn a_bubble_drawer_in_zen_keeps_other_chrome_hidden() {
    let mut s = desk();
    let (panel, tile) = toolbar_tile(&mut s, ToolbarControl::Panel { panel: Panel::Layers });
    let layers = take(&mut s, BubbleGrab::Tile { panel, tile }, FAR);
    invoke(&mut s, CommandId::ZenMode);
    chrome(&mut s, ChromeEvent::Refresh, ChromeFacts::default());
    assert!(s.interaction.hidden);
    tap(&mut s, layers);
    let reply = chrome(&mut s, ChromeEvent::Refresh, ChromeFacts::default());
    assert!(reply.chrome_hidden && s.zen_keeps_drawer(), "the Layers card opens without bringing the docks back");
    tap(&mut s, layers);
    assert!(!s.zen_keeps_drawer());
    customize(&mut s, CustomizationAction::ManageToolbars);
    let reply = chrome(&mut s, ChromeEvent::Refresh, ChromeFacts::default());
    assert!(!reply.chrome_hidden, "other customization still shows chrome");
}
