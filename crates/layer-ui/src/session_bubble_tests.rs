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

fn take_control(s: &mut UiSession<Recorder>, control: ToolbarControl, at: [f32; 2]) -> u32 {
    let (panel, tile) = toolbar_tile(s, control);
    take(s, BubbleGrab::Tile { panel, tile }, at)
}

fn view(s: &UiSession<Recorder>, id: u32) -> BubbleView {
    s.bubbles().into_iter().find(|b| b.id == id).unwrap()
}

fn tap(s: &mut UiSession<Recorder>, id: u32) {
    act(s, BubbleAction::Tap { id }).unwrap();
}

fn open_ring(s: &mut UiSession<Recorder>, id: u32) {
    invoke(s, CommandId::Lasso);
    for _ in 0..3 {
        tap(s, id);
    }
    assert!(!view(s, id).chooser.is_empty());
}

fn saved(s: &UiSession<Recorder>, id: u32) -> [f32; 2] {
    s.state.workspace.layout.bubbles.get(id).unwrap().center(VIEW)
}

fn choose(s: &mut UiSession<Recorder>, id: u32, phase: ContactPhase, position: [f32; 2]) {
    act(s, BubbleAction::Choose { id, phase, position }).unwrap();
}

fn beads(s: &mut UiSession<Recorder>, id: u32, phase: ContactPhase, position: [f32; 2]) -> UiChange {
    act(s, BubbleAction::Beads { id, phase, position }).unwrap()
}

fn bead(view: &BubbleView, bead: Bead) -> [f32; 2] {
    view.beads.iter().find(|b| b.placement.bead == bead).unwrap().placement.center
}

fn tap_bead(s: &mut UiSession<Recorder>, id: u32, which: Bead) {
    let at = bead(&view(s, id), which);
    beads(s, id, ContactPhase::Down, at);
    beads(s, id, ContactPhase::Up, at);
}

fn items(view: &BubbleView, kind: ChooserKind) -> Vec<ChooserItem> {
    view.chooser.iter().filter(|i| i.kind == kind).cloned().collect()
}

fn big_palette(s: &mut UiSession<Recorder>) {
    let space = s.state.colors.rgb_space();
    let swatches = (0..40).map(|i| (format!("{i}"), layer_core::color::RgbColor { linear_rgb: None, space, rgba: [i as f32 / 40., 0.5, 0.2, 1.] })).collect();
    s.dispatch(UiAction::Color { action: ColorAction::Library { action: ColorLibraryAction::Import { name: "Forty".into(), swatches } } }).unwrap();
    let id = s.state.color_library.palettes.iter().find(|p| p.name == "Forty").unwrap().id;
    s.dispatch(UiAction::Color { action: ColorAction::Library { action: ColorLibraryAction::SelectPalette { id } } }).unwrap();
}

fn stroke(s: &mut UiSession<Recorder>, sequence: u64, phase: PenPhase, at: [f32; 2]) {
    let mut e = event(s, sequence, phase, 1.);
    e.surface_position.x = at[0];
    e.surface_position.y = at[1];
    s.pen(e);
}

fn several_brushes(s: &UiSession<Recorder>, item: &ChooserItem) -> bool {
    ToolGroup::ALL.into_iter().any(|g| *g.localized_label(s.localization()) == *item.label && tools::group_presets(g).nth(1).is_some())
}

fn record_colors(s: &mut UiSession<Recorder>, count: usize) {
    for i in 0..count {
        let color = layer_core::color::RgbColor { linear_rgb: None, space: s.state.colors.rgb_space(), rgba: [i as f32 / count as f32, 0.2, 0.4, 1.] };
        s.dispatch(UiAction::Color { action: ColorAction::Definition { color } }).unwrap();
        s.state.color_library.record_use(color);
    }
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
    assert_eq!(saved(&s, id), FAR);
    assert!(s.state.workspace.layout.panel(panel).unwrap().tiles().iter().any(|t| t.id == tile), "the tile stays");
    assert!(s.bubble_out(control), "the toolbar shows its tool is out");
    drag(&mut s, grab, [300., 300.], [300., 300.]);
    assert_eq!(s.state.workspace.layout.bubbles.bubbles.len(), 1, "one bubble per source");
    assert_eq!(saved(&s, id), [300., 300.]);
    invoke(&mut s, CommandId::UndoWorkspace);
    assert_eq!(saved(&s, id), FAR);
    invoke(&mut s, CommandId::UndoWorkspace);
    assert!(s.state.workspace.layout.bubbles.bubbles.is_empty() && !s.bubble_out(control));
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
    assert_eq!(saved(&s, id), FAR);
    drag(&mut s, grab, FAR, [600., 8.]);
    assert!(s.state.workspace.layout.bubbles.bubbles.is_empty());
    let id = take(&mut s, brush(DefaultBrushPreset::PointyPencil), FAR);
    act(&mut s, BubbleAction::PutBack { id }).unwrap();
    assert!(s.state.workspace.layout.bubbles.bubbles.is_empty());
}

#[test]
fn put_back_all_empties_the_desk_in_one_step() {
    let mut s = desk();
    assert!(act(&mut s, BubbleAction::PutBackAll).is_err(), "nothing is out");
    assert!(!s.command(CommandId::PutBackBubbles).enabled);
    take(&mut s, brush(DefaultBrushPreset::PointyPencil), FAR);
    let color = take_control(&mut s, ToolbarControl::Color, [300., 500.]);
    tap_bead(&mut s, color, Bead::Palettes);
    assert!(s.command(CommandId::PutBackBubbles).enabled);
    invoke(&mut s, CommandId::PutBackBubbles);
    assert!(s.bubbles().is_empty() && s.bubble_placements().is_empty());
    invoke(&mut s, CommandId::UndoWorkspace);
    assert_eq!(s.state.workspace.layout.bubbles.bubbles.len(), 2);
    assert!(view(&s, color).pill.is_none(), "put back closes open parts");
    assert!(VIEW_MENU.sections.iter().any(|section| section.contains(&CommandId::PutBackBubbles)));
}

#[test]
fn title_bar_tools_show_when_their_bubbles_are_out() {
    let mut s = desk();
    let drawing = ToolbarControl::Command { command: CommandId::DrawingBrush };
    let header = |s: &UiSession<Recorder>, control: ToolbarControl| {
        let entry = s.state.workspace.layout.header.entries().find(|e| e.item == HeaderItem::Tool { control }).map(|e| e.id);
        entry.and_then(|id| s.header_view_with(false).items.into_iter().find(|i| i.id == id)).map(|i| i.bubble)
    };
    assert!(!s.bubble_out(drawing));
    let pastel = take(&mut s, BubbleGrab::Variant { variant: ToolVariant::BrushGroup { group: ToolGroup::Pastel } }, FAR);
    assert!(s.bubble_out(drawing), "a medium is out of the Brush dial");
    if let Some(shown) = header(&s, drawing) {
        assert!(shown);
    }
    act(&mut s, BubbleAction::PutBack { id: pastel }).unwrap();
    take(&mut s, brush(DefaultBrushPreset::PointyPencil), FAR);
    assert!(s.bubble_out(drawing), "so is a brush of one of its media");
    assert!(!s.bubble_out(ToolbarControl::Color));
    take_control(&mut s, ToolbarControl::Color, [300., 500.]);
    assert!(s.bubble_out(ToolbarControl::Color));
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
fn brush_bubble_taps_select_then_open_and_close_its_halo() {
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
    assert_eq!(open.halo, Some(halo_disc(defaults.len())));
    assert_eq!(open.rings.iter().map(|r| r.setting).collect::<Vec<_>>(), defaults);
    assert_eq!(open.rings.iter().map(|r| r.arc).collect::<Vec<_>>(), halo_arcs(defaults.len()));
    tap(&mut s, id);
    assert!(!view(&s, id).open, "the third tap closes it");
    s.dispatch(UiAction::SelectBrush { id: DefaultBrushPreset::GPen as u32 }).unwrap();
    tap(&mut s, id);
    assert!(!view(&s, id).open);
    s.dispatch(UiAction::SelectBrush { id: DefaultBrushPreset::GPen as u32 }).unwrap();
    s.dispatch(UiAction::SelectBrush { id: pencil }).unwrap();
    tap(&mut s, id);
    assert!(view(&s, id).open, "a selected brush shows its settings on the first tap");
}

#[test]
fn dial_taps_select_then_show_the_halo_then_the_media_ring() {
    let mut s = desk();
    let id = take_control(&mut s, ToolbarControl::Command { command: CommandId::DrawingBrush }, FAR);
    invoke(&mut s, CommandId::Lasso);
    assert!(!view(&s, id).selected);
    tap(&mut s, id);
    assert!(view(&s, id).selected && view(&s, id).chooser.is_empty());
    tap(&mut s, id);
    assert!(!view(&s, id).rings.is_empty() && view(&s, id).chooser.is_empty());
    tap(&mut s, id);
    let open = view(&s, id);
    assert!(open.rings.is_empty(), "the ring replaces the halo");
    let media = items(&open, ChooserKind::Medium);
    assert!(media.len() > 2);
    assert!(media.iter().all(|m| (m.center[0].hypot(m.center[1]) - CHOOSER_RING).abs() < 1. || m.center[0].hypot(m.center[1]) > CHOOSER_RING));
    let current = tools::group(s.state.brush.preset);
    let shown: Vec<_> = media.iter().filter(|m| m.shown).collect();
    assert_eq!(shown.len(), 1, "the current medium is shown with its label");
    assert!(shown[0].selected);
    let brushes = items(&open, ChooserKind::Brush);
    assert_eq!(brushes.len(), tools::group_presets(current).count(), "its brushes curve beside it");
    assert!(brushes.iter().all(|b| b.preview.is_some()));
    assert_eq!(brushes.iter().filter(|b| b.selected).count(), 1);
    tap(&mut s, id);
    assert!(!view(&s, id).open, "the fourth tap closes it");
}

#[test]
fn the_media_ring_shows_a_mediums_brushes_and_a_choice_closes_it() {
    let mut s = desk();
    let id = take_control(&mut s, ToolbarControl::Command { command: CommandId::DrawingBrush }, FAR);
    open_ring(&mut s, id);
    let open = view(&s, id);
    let other = items(&open, ChooserKind::Medium).into_iter().find(|m| !m.shown && m.enabled && several_brushes(&s, m)).unwrap();
    choose(&mut s, id, ContactPhase::Down, other.center);
    choose(&mut s, id, ContactPhase::Up, other.center);
    let switched = view(&s, id);
    assert!(items(&switched, ChooserKind::Medium).iter().any(|m| m.shown && m.label == other.label));
    let column = items(&switched, ChooserKind::Brush);
    let side = column[0].center[0].signum();
    assert_eq!(side, other.center[0].signum(), "brushes open on the medium's side");
    choose(&mut s, id, ContactPhase::Down, [0., 0.]);
    choose(&mut s, id, ContactPhase::Up, [0., 0.]);
    assert!(!view(&s, id).chooser.is_empty(), "letting go on nothing keeps the ring");
    let pick = column.iter().find(|b| !b.selected).unwrap().clone();
    choose(&mut s, id, ContactPhase::Down, pick.center);
    choose(&mut s, id, ContactPhase::Up, pick.center);
    assert_eq!(tools::brush_label_localized(s.state.brush.preset, s.localization()).unwrap().to_string(), pick.label);
    assert!(view(&s, id).chooser.is_empty(), "picking a brush closes the ring");
}

#[test]
fn a_press_can_slide_through_a_medium_to_its_brush() {
    let mut s = desk();
    let id = take_control(&mut s, ToolbarControl::Command { command: CommandId::DrawingBrush }, FAR);
    open_ring(&mut s, id);
    let media = items(&view(&s, id), ChooserKind::Medium);
    let [from, through] = [media.iter().find(|m| m.shown).unwrap(), media.iter().find(|m| !m.shown && m.enabled && several_brushes(&s, m)).unwrap()];
    choose(&mut s, id, ContactPhase::Down, from.center);
    choose(&mut s, id, ContactPhase::Move, through.center);
    let column = items(&view(&s, id), ChooserKind::Brush);
    assert!(items(&view(&s, id), ChooserKind::Medium).iter().any(|m| m.shown && m.label == through.label), "sliding onto a medium fans it");
    let target = column.last().unwrap();
    choose(&mut s, id, ContactPhase::Move, target.center);
    choose(&mut s, id, ContactPhase::Up, target.center);
    assert_eq!(s.state.brush.preset, target.preview.unwrap());
    assert!(view(&s, id).chooser.is_empty());
}

#[test]
fn a_selection_dial_rings_its_tools_directly() {
    let mut s = desk();
    let id = take_control(&mut s, ToolbarControl::ToolSlot { slot: ToolSlotId::Marquee }, FAR);
    tap(&mut s, id);
    assert!(view(&s, id).selected);
    tap(&mut s, id);
    let open = view(&s, id);
    let tools = items(&open, ChooserKind::Tool);
    assert!(tools.len() >= 2 && open.chooser.len() == tools.len(), "tools without settings go straight to the ring");
    let pick = tools.iter().find(|t| !t.selected).unwrap().clone();
    choose(&mut s, id, ContactPhase::Down, pick.center);
    choose(&mut s, id, ContactPhase::Up, pick.center);
    assert!(view(&s, id).chooser.is_empty());
    assert_eq!(view(&s, id).label, pick.label);
}

#[test]
fn the_ring_folds_toward_open_space_near_a_corner() {
    let mut s = desk();
    let work = s.layout(VIEW).work_area;
    let corner = [work.x + 40., work.y + work.height - 40.];
    let id = take_control(&mut s, ToolbarControl::Command { command: CommandId::DrawingBrush }, corner);
    open_ring(&mut s, id);
    let center = s.bubble_placements()[0].center;
    for item in view(&s, id).chooser {
        let [w, h] = item.size();
        let at = [center[0] + item.center[0], center[1] + item.center[1]];
        assert!(at[0] - w * 0.5 >= work.x && at[1] + h * 0.5 <= work.y + work.height, "{} at {at:?}", item.label);
    }
}

#[test]
fn medium_bubbles_select_their_brushes_and_fan_only_them() {
    let mut s = desk();
    let id = take(&mut s, BubbleGrab::Variant { variant: ToolVariant::BrushGroup { group: ToolGroup::Pastel } }, FAR);
    let brushes = tools::group_presets(ToolGroup::Pastel).count();
    assert_eq!(view(&s, id).dots.len(), brushes);
    tap(&mut s, id);
    assert_eq!(tools::group(s.state.brush.preset), ToolGroup::Pastel);
    assert_eq!(view(&s, id).dots.iter().filter(|d| **d).count(), 1);
    tap(&mut s, id);
    tap(&mut s, id);
    let fan = view(&s, id).chooser;
    assert_eq!(fan.len(), brushes);
    assert!(fan.iter().all(|i| i.kind == ChooserKind::Brush));
    let pick = fan.iter().find(|i| !i.selected).unwrap().clone();
    choose(&mut s, id, ContactPhase::Down, pick.center);
    choose(&mut s, id, ContactPhase::Up, pick.center);
    assert_eq!(Some(s.state.brush.preset), pick.preview);
    assert!(!view(&s, id).open);
}

#[test]
fn halo_contacts_set_values_along_their_arc() {
    let mut s = desk();
    let id = take(&mut s, brush(DefaultBrushPreset::PointyPencil), FAR);
    tap(&mut s, id);
    tap(&mut s, id);
    let ring = view(&s, id).rings[0].clone();
    let value = |s: &UiSession<Recorder>| s.state.tool_settings.iter().find(|c| c.id == ring.setting).unwrap().value;
    let numeric = s.state.tool_settings.iter().find(|c| c.id == ring.setting).unwrap().numeric.clone();
    let at = |f: f64| numeric.resolve(0., NumericOperation::Position { position: f }).unwrap().value as f32;
    act(&mut s, BubbleAction::Ring { id, phase: ContactPhase::Down, position: ring.arc.point(0.2) }).unwrap();
    let dragging = view(&s, id);
    assert!(dragging.rings[0].active);
    assert!(dragging.readout.as_ref().is_some_and(|r| r.contains(ring.label.as_ref())), "the name and value show while dragging");
    assert!((value(&s) - at(0.2)).abs() < 1e-3);
    act(&mut s, BubbleAction::Ring { id, phase: ContactPhase::Move, position: [ring.arc.point(0.9)[0] * 3., ring.arc.point(0.9)[1] * 3.] }).unwrap();
    assert!((value(&s) - at(0.9)).abs() < 1e-3, "the contact stays with its arc");
    act(&mut s, BubbleAction::Ring { id, phase: ContactPhase::Up, position: ring.arc.point(0.9) }).unwrap();
    assert!(!view(&s, id).rings[0].active && view(&s, id).readout.is_none());
    assert!(act(&mut s, BubbleAction::Ring { id, phase: ContactPhase::Down, position: [0., 0.] }).is_err());
}

#[test]
fn slider_bubbles_set_their_value_from_the_ring() {
    let mut s = desk();
    let id = take_control(&mut s, ToolbarControl::Opacity, FAR);
    let bubble = view(&s, id);
    assert_eq!(bubble.source, BubbleSource::Control { control: ToolbarControl::BrushOpacitySlider });
    assert_eq!(bubble.diameter, SLIDER_BUBBLE);
    let ring = bubble.rings[0].clone();
    assert_eq!(ring.arc, slider_ring());
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
    assert_eq!(view(&s, id).halo, Some(halo_disc(MAX_RING_SETTINGS)));
    for _ in shown..MAX_RING_SETTINGS {
        invoke(&mut s, CommandId::UndoWorkspace);
    }
    assert_eq!(s.state.workspace.layout.bubbles.ring(pencil), None);
}

#[test]
fn halos_stay_until_tapped_or_the_brush_changes_while_choosers_close_on_contacts_elsewhere() {
    let mut s = desk();
    let id = take(&mut s, brush(DefaultBrushPreset::PointyPencil), FAR);
    let dial = take_control(&mut s, ToolbarControl::ToolSlot { slot: ToolSlotId::Marquee }, [300., 300.]);
    tap(&mut s, id);
    tap(&mut s, id);
    chrome(&mut s, ChromeEvent::Contact { position: [100., 700.], canvas: true }, ChromeFacts::default());
    assert!(view(&s, id).open, "the halo stays");
    s.dispatch(UiAction::SelectBrush { id: DefaultBrushPreset::GPen as u32 }).unwrap();
    assert!(!view(&s, id).open, "another brush closes it");
    s.dispatch(UiAction::SelectBrush { id: DefaultBrushPreset::PointyPencil as u32 }).unwrap();
    assert!(!view(&s, id).open, "and it stays closed");
    tap(&mut s, dial);
    tap(&mut s, dial);
    let item = view(&s, dial).chooser[0].center;
    chrome(&mut s, ChromeEvent::Contact { position: [300. + item[0], 300. + item[1]], canvas: false }, ChromeFacts::default());
    assert!(!view(&s, dial).chooser.is_empty(), "contacts on the ring keep it");
    let reply = chrome(&mut s, ChromeEvent::Contact { position: [100., 700.], canvas: true }, ChromeFacts::default());
    assert!(view(&s, dial).chooser.is_empty());
    assert_ne!(reply.change.regions & regions::BUBBLES, 0);
}

#[test]
fn painting_fades_nearby_bubbles_and_closes_choosers_but_not_the_halo() {
    let mut s = desk();
    let pencil = take(&mut s, brush(DefaultBrushPreset::PointyPencil), [300., 300.]);
    let dial = take_control(&mut s, ToolbarControl::ToolSlot { slot: ToolSlotId::Marquee }, [900., 300.]);
    let layers = take_control(&mut s, ToolbarControl::Panel { panel: Panel::Layers }, [900., 700.]);
    tap(&mut s, pencil);
    tap(&mut s, pencil);
    tap(&mut s, layers);
    assert!(view(&s, layers).card.is_some() && view(&s, pencil).open, "opening a card keeps the halo");
    stroke(&mut s, 1, PenPhase::Down, [300., 380.]);
    assert!(view(&s, pencil).open, "the halo stays while painting");
    assert!(view(&s, layers).card.is_none(), "painting folds the card");
    let (down, faded) = s.bubble_fade();
    assert!(down && faded == [pencil], "only the bubble near the stroke fades: {faded:?}");
    stroke(&mut s, 1, PenPhase::Move, [900., 360.]);
    assert_eq!(s.bubble_fade().1, [pencil, dial]);
    stroke(&mut s, 1, PenPhase::Up, [900., 360.]);
    let (down, faded) = s.bubble_fade();
    assert!(!down && faded.len() == 2, "hosts restore faded bubbles after the stroke ends");
    stroke(&mut s, 2, PenPhase::Down, [600., 800.]);
    assert!(s.bubble_fade().1.is_empty(), "a new stroke starts fresh");
    stroke(&mut s, 2, PenPhase::Up, [600., 800.]);
}

#[test]
fn the_layers_card_unfolds_flips_to_properties_and_folds() {
    let mut s = desk();
    let color = take_control(&mut s, ToolbarControl::Color, [300., 500.]);
    let layers = take_control(&mut s, ToolbarControl::Panel { panel: Panel::Layers }, [800., 500.]);
    let folded = view(&s, layers);
    assert!(folded.card.is_none());
    assert_eq!(folded.layers, s.state.layers.len());
    assert_eq!(folded.layer, s.state.layers.iter().find(|l| l.selected).map(|l| l.id), "the folded card shows the current layer");
    tap(&mut s, layers);
    let card = view(&s, layers).card.unwrap();
    assert_eq!(card, BubbleCard { size: layers_card(false, s.state.layers.len()), full: false, properties: false });
    act(&mut s, BubbleAction::Properties { id: layers }).unwrap();
    assert!(view(&s, layers).card.unwrap().properties);
    act(&mut s, BubbleAction::Properties { id: layers }).unwrap();
    assert!(!view(&s, layers).card.unwrap().properties, "All layers goes back");
    chrome(&mut s, ChromeEvent::Contact { position: [800., 500. + card.size[1] * 0.5 - 10.], canvas: false }, ChromeFacts::default());
    assert!(view(&s, layers).card.is_some(), "contacts on the card keep it");
    chrome(&mut s, ChromeEvent::Contact { position: [100., 120.], canvas: true }, ChromeFacts::default());
    assert!(view(&s, layers).card.is_none(), "a contact elsewhere folds it");
    assert!(act(&mut s, BubbleAction::Properties { id: layers }).is_err());
    let menu = |s: &UiSession<Recorder>| s.context_menu(ContextTarget::Bubble { id: layers }).unwrap();
    s.dispatch(menu(&s).sections[0][1].action.clone().unwrap()).unwrap();
    s.dispatch(menu(&s).sections[1][0].action.clone().unwrap()).unwrap();
    assert!(view(&s, layers).options.full && !view(&s, layers).options.fold);
    tap(&mut s, layers);
    assert_eq!(view(&s, layers).card.unwrap().size, layers_card(true, s.state.layers.len()));
    chrome(&mut s, ChromeEvent::Contact { position: [100., 120.], canvas: true }, ChromeFacts::default());
    assert!(view(&s, layers).card.is_some(), "a card that doesn't fold stays open");
    tap(&mut s, layers);
    assert!(view(&s, layers).card.is_none());
    let menu = s.context_menu(ContextTarget::Bubble { id: color }).unwrap();
    assert_eq!(menu.sections[0].iter().map(|i| i.selected).collect::<Vec<_>>(), [Some(true); 3]);
    s.dispatch(menu.sections[1][2].action.clone().unwrap()).unwrap();
    assert_eq!(view(&s, color).diameter, COLOR_RING_SIZES[2]);
    invoke(&mut s, CommandId::UndoWorkspace);
    assert_eq!(view(&s, color).diameter, COLOR_RING_SIZES[1]);
}

#[test]
fn an_open_card_pushes_neighbours_which_return_when_it_folds() {
    let mut s = desk();
    let layers = take_control(&mut s, ToolbarControl::Panel { panel: Panel::Layers }, [600., 450.]);
    let pencil = take(&mut s, brush(DefaultBrushPreset::PointyPencil), [700., 450.]);
    let settle = |s: &mut UiSession<Recorder>, from: u64| {
        let mut now = from;
        while s.step_bubbles(now) && now < from + 10_000_000_000 {
            now += 16_666_667;
        }
        now
    };
    let now = settle(&mut s, 0);
    tap(&mut s, layers);
    let now = settle(&mut s, now);
    let card = layers_card(false, s.state.layers.len());
    let place = |s: &mut UiSession<Recorder>, id: u32| s.bubble_placements().into_iter().find(|p| p.id == id).unwrap().center;
    let pushed = place(&mut s, pencil);
    let card_center = place(&mut s, layers);
    assert!(pushed[0] - TOOL_BUBBLE * 0.5 >= card_center[0] + card[0] * 0.5 + WORKSPACE_SPACING - 0.5, "{pushed:?}");
    tap(&mut s, layers);
    settle(&mut s, now);
    assert_eq!(place(&mut s, pencil), [700., 450.]);
    assert_eq!(saved(&s, pencil), [700., 450.], "growth never moves saved places");
}

#[test]
fn bubbles_pushed_during_a_long_drag_keep_their_new_place() {
    let mut s = desk();
    let mover = take(&mut s, brush(DefaultBrushPreset::PointyPencil), [400., 450.]);
    let pushed = take(&mut s, brush(DefaultBrushPreset::Charcoal), [470., 450.]);
    let grab = BubbleGrab::Bubble { id: mover };
    contact(&mut s, grab, ContactPhase::Down, [400., 450.]).unwrap();
    let mut now = 0;
    for step in 0..=20 {
        contact(&mut s, grab, ContactPhase::Move, [400. + step as f32 * 3., 450.]).unwrap();
        now += 16_666_667;
        s.step_bubbles(now);
    }
    contact(&mut s, grab, ContactPhase::Move, [300., 450.]).unwrap();
    for _ in 0..240 {
        now += 16_666_667;
        s.step_bubbles(now);
    }
    let place = |s: &mut UiSession<Recorder>| s.bubble_placements().into_iter().find(|p| p.id == pushed).unwrap().center;
    let held = place(&mut s);
    assert!(held[0] >= 460. + TOOL_BUBBLE + WORKSPACE_SPACING - 0.5, "it stays pushed while the drag is held: {held:?}");
    contact(&mut s, grab, ContactPhase::Up, [300., 450.]).unwrap();
    let saved = saved(&s, pushed);
    assert!((saved[0] - held[0]).abs() < 0.5 && (saved[1] - held[1]).abs() < 0.5, "{saved:?} {held:?}");
    for _ in 0..240 {
        now += 16_666_667;
        s.step_bubbles(now);
    }
    assert!((place(&mut s)[0] - saved[0]).abs() < 0.5);
}

#[test]
fn putting_back_an_open_bubble_closes_its_halo_and_chooser() {
    let mut s = desk();
    let id = take(&mut s, BubbleGrab::Variant { variant: ToolVariant::BrushGroup { group: ToolGroup::Pastel } }, FAR);
    tap(&mut s, id);
    tap(&mut s, id);
    tap(&mut s, id);
    assert!(!view(&s, id).chooser.is_empty());
    act(&mut s, BubbleAction::PutBack { id }).unwrap();
    assert!(s.bubbles().is_empty() && s.bubble_placements().is_empty());
    assert!(act(&mut s, BubbleAction::Tap { id }).is_err());
    assert!(act(&mut s, BubbleAction::Choose { id, phase: ContactPhase::Up, position: [0., 0.] }).unwrap().regions == 0);
}

#[test]
fn a_smaller_window_keeps_bubbles_inside_and_corner_bubbles_follow_their_corner() {
    let mut s = desk();
    let work = s.layout(VIEW).work_area;
    let corner = [work.x + work.width - 40., work.y + work.height - 40.];
    let id = take(&mut s, brush(DefaultBrushPreset::PointyPencil), corner);
    let middle = take(&mut s, brush(DefaultBrushPreset::Charcoal), [560., 420.]);
    let saved_before = s.state.workspace.layout.bubbles.clone();
    s.set_viewport([700., 560.], [700, 560]).unwrap();
    let mut now = 0;
    while s.step_bubbles(now) {
        now += 16_666_667;
    }
    let area = s.state.workspace.layout.free_area([700., 560.]);
    let place = |s: &mut UiSession<Recorder>, id: u32| s.bubble_placements().into_iter().find(|p| p.id == id).unwrap().center;
    let at = place(&mut s, id);
    assert_eq!(at, [corner[0] - 500., corner[1] - 340.], "kept the same distance to the bottom-right corner");
    let inside = place(&mut s, middle);
    assert!(inside[0] + TOOL_BUBBLE * 0.5 <= area.x + area.width && inside[1] + TOOL_BUBBLE * 0.5 <= area.y + area.height);
    assert_eq!(s.state.workspace.layout.bubbles, saved_before, "resizing never rewrites saved places");
    s.set_viewport(VIEW, [1200, 900]).unwrap();
    while s.step_bubbles(now) {
        now += 16_666_667;
    }
    assert_eq!(place(&mut s, id), corner);
}

#[test]
fn narrow_windows_open_color_rings_small() {
    let mut s = desk();
    let color = take_control(&mut s, ToolbarControl::Color, [500., 400.]);
    assert_eq!(view(&s, color).diameter, COLOR_RING_SIZES[1]);
    s.set_viewport([560., 800.], [560, 800]).unwrap();
    assert_eq!(view(&s, color).diameter, COLOR_RING_SIZES[0]);
    assert_eq!(view(&s, color).options.size, COLOR_RING_SIZES[1], "the chosen size returns in a wider window");
}

#[test]
fn bubbles_round_trip_with_the_workspace() {
    let mut s = desk();
    let id = take(&mut s, brush(DefaultBrushPreset::PointyPencil), FAR);
    let json = serde_json::to_string(&s.state.workspace).unwrap();
    let restored: WorkspaceState = serde_json::from_str(&json).unwrap();
    restored.layout.validate().unwrap();
    assert_eq!(restored.layout.bubbles.get(id), s.state.workspace.layout.bubbles.get(id));
}

#[test]
fn color_beads_pick_swap_and_switch_palettes() {
    let mut s = desk();
    let id = take_control(&mut s, ToolbarControl::Color, [500., 380.]);
    record_colors(&mut s, 20);
    let open = view(&s, id);
    for kind in [Bead::Foreground, Bead::Background, Bead::Transparent, Bead::Swatch { index: 0 }, Bead::Recent { index: 0 }, Bead::Palettes] {
        assert!(open.beads.iter().any(|b| b.placement.bead == kind), "{kind:?}");
    }
    let target = s.state.color_library.history[2];
    tap_bead(&mut s, id, Bead::Recent { index: 2 });
    assert_eq!(s.state.colors.definition(), target);
    let (foreground, background) = (s.state.colors.foreground, s.state.colors.background);
    tap_bead(&mut s, id, Bead::Background);
    assert_eq!((s.state.colors.foreground, s.state.colors.background), (background, foreground), "the background bead swaps");
    tap_bead(&mut s, id, Bead::Transparent);
    assert_eq!(s.state.colors.slot, ColorSlot::Transparent);
    tap_bead(&mut s, id, Bead::Foreground);
    assert_eq!(s.state.colors.slot, ColorSlot::Foreground);
    let swatch = s.state.color_library.active_palette().swatches[1].color;
    tap_bead(&mut s, id, Bead::Swatch { index: 1 });
    assert_eq!(s.state.colors.definition(), swatch);
    assert!(view(&s, id).pill.is_none());
    tap_bead(&mut s, id, Bead::Palettes);
    let name = view(&s, id).pill.unwrap();
    assert_eq!(name, s.state.color_library.active_palette().name);
    assert!(view(&s, id).beads.iter().any(|b| b.placement.bead == Bead::Palettes && b.selected));
    let palettes = s.state.color_library.palettes.len();
    act(&mut s, BubbleAction::Palette { id, step: 1 }).unwrap();
    if palettes > 1 {
        assert_ne!(view(&s, id).pill.unwrap(), name);
    }
    act(&mut s, BubbleAction::Palette { id, step: -1 }).unwrap();
    assert_eq!(view(&s, id).pill.unwrap(), name);
    let menu = s.context_menu(ContextTarget::BubblePalettes { id }).unwrap();
    assert_eq!(menu.sections[0].len(), palettes + 1);
    assert_eq!(menu.sections[0].iter().filter(|i| i.selected == Some(true)).count(), 1);
    assert_eq!(menu.sections[0].last().unwrap().label, "Edit palettes…");
    tap_bead(&mut s, id, Bead::Swatch { index: 0 });
    assert!(view(&s, id).pill.is_none(), "picking a color closes the name");
    tap_bead(&mut s, id, Bead::Palettes);
    chrome(&mut s, ChromeEvent::Contact { position: [100., 120.], canvas: true }, ChromeFacts::default());
    assert!(view(&s, id).pill.is_none(), "so does a contact elsewhere");
    let options = BubbleOptions { palette: false, ..view(&s, id).options };
    act(&mut s, BubbleAction::SetOptions { id, options }).unwrap();
    assert!(!view(&s, id).beads.iter().any(|b| matches!(b.placement.bead, Bead::Swatch { .. } | Bead::Palettes)));
}

#[test]
fn sliding_along_beads_turns_them_and_settles_on_whole_columns() {
    let mut s = desk();
    let id = take_control(&mut s, ToolbarControl::Color, [500., 420.]);
    record_colors(&mut s, 20);
    big_palette(&mut s);
    let before = s.state.colors.definition();
    let start = view(&s, id);
    let second = bead(&start, Bead::Recent { index: 1 });
    let third = bead(&start, Bead::Recent { index: 2 });
    beads(&mut s, id, ContactPhase::Down, third);
    let turned = beads(&mut s, id, ContactPhase::Move, second);
    assert_ne!(turned.regions & regions::BUBBLES, 0);
    let moved = bead(&view(&s, id), Bead::Recent { index: 3 });
    assert!((moved[0] - third[0]).hypot(moved[1] - third[1]) < 1., "beads travel with the finger");
    let halfway = [(second[0] + third[0]) * 0.5, (second[1] + third[1]) * 0.5 - 2.];
    beads(&mut s, id, ContactPhase::Move, halfway);
    beads(&mut s, id, ContactPhase::Up, halfway);
    let settled = view(&s, id);
    assert!(settled.beads.iter().filter(|b| matches!(b.placement.bead, Bead::Recent { .. })).all(|b| b.placement.alpha == 1. || b.placement.alpha <= 0.3 + 1e-3));
    assert_eq!(s.state.colors.definition(), before, "a slide picks nothing");
    let first = bead(&start, Bead::Swatch { index: 0 });
    let further = bead(&start, Bead::Swatch { index: 4 });
    beads(&mut s, id, ContactPhase::Down, further);
    for step in 1..=20 {
        let f = step as f32 / 20.;
        beads(&mut s, id, ContactPhase::Move, [further[0] + (first[0] - further[0]) * f, further[1] + (first[1] - further[1]) * f]);
    }
    beads(&mut s, id, ContactPhase::Up, first);
    let moved = view(&s, id);
    assert!((bead(&moved, Bead::Swatch { index: 5 })[0] - bead(&start, Bead::Swatch { index: 1 })[0]).abs() < 1., "both rows turn together");
    assert_eq!(bead(&moved, Bead::Palettes), bead(&start, Bead::Palettes));
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
    big_palette(&mut s);
    let id = take(&mut s, BubbleGrab::Tile { panel, tile }, [500., 380.]);
    let start = view(&s, id);
    let [from, to] = [bead(&start, Bead::Swatch { index: 4 }), bead(&start, Bead::Swatch { index: 0 })];
    beads(&mut s, id, ContactPhase::Down, from);
    beads(&mut s, id, ContactPhase::Move, to);
    beads(&mut s, id, ContactPhase::Up, to);
    tap_bead(&mut s, id, Bead::Palettes);
    assert!(view(&s, id).pill.is_some());
    s.dispatch(UiAction::RestoreWorkspace { workspace: Box::new(before) }).unwrap();
    s.bubble_placements();
    let again = take(&mut s, BubbleGrab::Tile { panel, tile }, [500., 380.]);
    assert_eq!(again, id, "a restored layout reuses the identity");
    let fresh = view(&s, again);
    assert!(fresh.pill.is_none());
    assert_eq!(bead(&fresh, Bead::Swatch { index: 0 }), bead(&start, Bead::Swatch { index: 0 }));
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
fn bubbles_open_in_zen_without_bringing_the_docks_back() {
    let mut s = desk();
    let layers = take_control(&mut s, ToolbarControl::Panel { panel: Panel::Layers }, FAR);
    invoke(&mut s, CommandId::ZenMode);
    chrome(&mut s, ChromeEvent::Refresh, ChromeFacts::default());
    assert!(s.interaction.hidden);
    tap(&mut s, layers);
    let reply = chrome(&mut s, ChromeEvent::Refresh, ChromeFacts::default());
    assert!(reply.chrome_hidden && view(&s, layers).card.is_some());
}

#[test]
fn pinching_the_color_ring_resizes_it_within_its_range() {
    let mut s = desk();
    let id = take_control(&mut s, ToolbarControl::Color, [500., 380.]);
    act(&mut s, BubbleAction::Resize { id, scale: 1.25 }).unwrap();
    assert_eq!(view(&s, id).diameter, COLOR_RING_SIZES[1] * 1.25);
    act(&mut s, BubbleAction::Resize { id, scale: 10. }).unwrap();
    assert_eq!(view(&s, id).diameter, COLOR_RING_RANGE[1]);
    act(&mut s, BubbleAction::Resize { id, scale: 0.01 }).unwrap();
    assert_eq!(view(&s, id).diameter, COLOR_RING_RANGE[0]);
    invoke(&mut s, CommandId::UndoWorkspace);
    assert_eq!(view(&s, id).diameter, COLOR_RING_RANGE[1]);
    let brush = take(&mut s, brush(DefaultBrushPreset::PointyPencil), [900., 380.]);
    assert!(act(&mut s, BubbleAction::Resize { id: brush, scale: 2. }).is_err());
    assert!(act(&mut s, BubbleAction::Resize { id, scale: f32::NAN }).is_err());
}
