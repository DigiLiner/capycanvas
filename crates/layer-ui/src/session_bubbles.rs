use super::*;
use crate::bubbles::{Desk, bead_scroll, bead_scroll_limit, halo_reach};
use layer_core::BrushSnapshot;

#[derive(Clone, Copy, Debug)]
struct BubbleDrag {
    id: u32,
    offset: [f32; 2],
    source: Option<Bounds>,
}

#[derive(Debug, Default)]
pub(crate) struct Bubbles {
    desk: Desk,
    drag: Option<BubbleDrag>,
    halo: Option<u32>,
    ring: Option<(u32, &'static str)>,
    scroll: Vec<(u32, [f32; 2])>,
    pill: Option<u32>,
}

fn control_face(state: &UiState, control: ToolbarControl) -> (String, &'static str, bool, bool) {
    if let Some((choice, enabled, _, _)) = state.resolve_group(control) {
        return (choice.label, choice.icon, choice.selected, enabled);
    }
    let choice = customization::tool_choice_localized(control, &state.localization);
    let (enabled, selected) = tool_state(state, control);
    let enabled = enabled && control.slider().is_none_or(|binding| binding.field(state).is_some());
    (choice.label, tool_icon(state, control), selected, enabled)
}

fn near(bounds: Bounds, point: [f32; 2], distance: f32) -> bool {
    let dx = (bounds.x - point[0]).max(point[0] - bounds.x - bounds.width).max(0.);
    let dy = (bounds.y - point[1]).max(point[1] - bounds.y - bounds.height).max(0.);
    dx.hypot(dy) <= distance
}

impl<R: CanvasRenderer> UiSession<R> {
    pub fn bubbles(&self) -> Vec<BubbleView> {
        self.state.workspace.layout.bubbles.bubbles.iter().map(|bubble| self.bubble_view(bubble)).collect()
    }

    pub fn bubble_placements(&mut self) -> Vec<BubblePlacement> {
        self.sync_bubbles();
        self.bubble_state.desk.placements()
    }

    pub fn step_bubbles(&mut self, now_ns: u64) -> bool {
        self.sync_bubbles();
        self.bubble_state.desk.advance(now_ns)
    }

    fn sync_bubbles(&mut self) {
        let Some(viewport) = self.logical_viewport else { return };
        let bounds = self.state.workspace.layout.free_area(viewport);
        let layout = &self.state.workspace.layout.bubbles;
        let state = &mut self.bubble_state;
        for open in [&mut state.halo, &mut state.pill] {
            if open.is_some_and(|id| layout.get(id).is_none()) {
                *open = None;
            }
        }
        state.scroll.retain(|(id, _)| layout.get(*id).is_some());
        if !self.bubble_state.desk.matches(layout, bounds) {
            self.bubble_state.desk.sync(layout, bounds);
        }
        let grown = self.bubble_state.halo.map(|id| (id, halo_reach(self.halo_settings().len(), TOOL_BUBBLE)));
        self.bubble_state.desk.grow(grown);
    }

    fn bubble_view(&self, bubble: &ToolBubble) -> BubbleView {
        let state = &self.state;
        let kind = bubble.source.kind();
        let (label, icon, selected, enabled) = match bubble.source {
            BubbleSource::Medium { group } => (
                group.localized_label(self.localization()).to_string(),
                group.icon(),
                state.layer_tools.tool == LayerCanvasTool::Paint && tools::group(state.brush.preset) == group,
                true,
            ),
            BubbleSource::Control { control } => control_face(state, control),
        };
        let rings = match bubble.source {
            BubbleSource::Control { control } if kind == BubbleKind::Slider => {
                let binding = control.slider().unwrap();
                binding.field(state).map(|field| {
                    vec![self.ring_view(slider_ring(bubble.diameter()), &field, binding.numeric())]
                }).unwrap_or_default()
            }
            _ if self.bubble_state.halo == Some(bubble.id) => {
                let settings = self.halo_settings();
                halo_arcs(settings.len(), bubble.diameter())
                    .into_iter()
                    .zip(settings)
                    .map(|(arc, setting)| self.ring_view(arc, setting, setting.numeric.clone()))
                    .collect()
            }
            _ => Vec::new(),
        };
        BubbleView {
            id: bubble.id,
            source: bubble.source,
            kind,
            diameter: bubble.diameter(),
            label,
            icon,
            preview: match bubble.source {
                BubbleSource::Control { control: ToolbarControl::Brush { id } } => Some(id),
                _ => None,
            },
            variants: kind == BubbleKind::Dial,
            dots: match bubble.source {
                BubbleSource::Medium { group } => tools::group_presets(group).map(|id| id == state.brush.preset).collect(),
                _ => Vec::new(),
            },
            selected,
            enabled,
            open: self.bubble_state.halo == Some(bubble.id)
                || state.customization.drawer.as_ref().is_some_and(|d| d.anchor == DrawerAnchor::Bubble { id: bubble.id }),
            rings,
            beads: if kind == BubbleKind::Color { self.bubble_beads(bubble) } else { Vec::new() },
            pill: (kind == BubbleKind::Color && self.bubble_state.pill == Some(bubble.id) && bubble.options.palette)
                .then(|| state.color_library.active_palette().name.clone()),
            options: bubble.options,
        }
    }

    fn bead_scroll(&self, id: u32) -> [f32; 2] {
        self.bubble_state.scroll.iter().find(|(bubble, _)| *bubble == id).map_or([0.; 2], |(_, scroll)| *scroll)
    }

    fn bubble_beads(&self, bubble: &ToolBubble) -> Vec<BubbleBead> {
        let state = &self.state;
        let library = &state.color_library;
        let swatches = &library.active_palette().swatches;
        let text = |id: MessageId| self.localization().text(id).to_string();
        color_beads(bubble.options.size, bubble.options, library.history.len(), swatches.len(), self.bead_scroll(bubble.id))
            .into_iter()
            .map(|placement| {
                let (color, label, selected) = match placement.bead {
                    Bead::Recent { index } => (library.history.get(index).copied(), text(MessageId::WORKSPACE_BUBBLE_RECENT_COLORS), false),
                    Bead::Swatch { index } => {
                        let swatch = swatches.get(index);
                        (swatch.map(|s| s.color), swatch.map(|s| s.name.clone()).unwrap_or_default(), false)
                    }
                    Bead::Foreground => (Some(state.colors.foreground), text(MessageId::COMMANDS_FOREGROUND_COLOR), state.colors.slot == ColorSlot::Foreground),
                    Bead::Background => (Some(state.colors.background), text(MessageId::COMMANDS_BACKGROUND_COLOR), state.colors.slot == ColorSlot::Background),
                    Bead::Transparent => (
                        Some(layer_core::color::RgbColor { linear_rgb: None, space: state.colors.rgb_space(), rgba: [0.; 4] }),
                        text(MessageId::COMMANDS_TRANSPARENT_PAINT),
                        state.colors.slot == ColorSlot::Transparent,
                    ),
                    Bead::Palettes => (None, text(MessageId::WORKSPACE_BUBBLE_PALETTE), self.bubble_state.pill == Some(bubble.id)),
                };
                BubbleBead { placement, color, label, selected }
            })
            .collect()
    }

    fn bead_action(&mut self, id: u32, bead: Bead) -> Result<UiChange, String> {
        let library = &self.state.color_library;
        let action = match bead {
            Bead::Recent { index } => ColorAction::Definition { color: *library.history.get(index).ok_or("The color is no longer in the history")? },
            Bead::Swatch { index } => ColorAction::Library {
                action: ColorLibraryAction::Use { id: library.active_palette().swatches.get(index).ok_or("The color is no longer in the palette")?.id },
            },
            Bead::Foreground => ColorAction::Select { slot: ColorSlot::Foreground },
            Bead::Background => ColorAction::Select { slot: ColorSlot::Background },
            Bead::Transparent => ColorAction::Select { slot: ColorSlot::Transparent },
            Bead::Palettes => {
                self.bubble_state.pill = (self.bubble_state.pill != Some(id)).then_some(id);
                return Ok(self.changed(regions::BUBBLES, false));
            }
        };
        let mut change = self.dispatch(UiAction::Color { action })?;
        change.regions |= regions::BUBBLES;
        Ok(change)
    }

    fn scroll_beads(&mut self, id: u32, strip: BeadStrip, pixels: f32) -> Result<UiChange, String> {
        let bubble = self.bubble(id)?;
        let library = &self.state.color_library;
        let limit = bead_scroll_limit(strip, bubble.options.size, library.history.len(), library.active_palette().swatches.len());
        let mut scroll = self.bead_scroll(id);
        let axis = usize::from(strip == BeadStrip::Palette);
        scroll[axis] = bead_scroll(scroll[axis], pixels, limit);
        self.bubble_state.scroll.retain(|(bubble, _)| *bubble != id);
        self.bubble_state.scroll.push((id, scroll));
        Ok(self.changed(regions::BUBBLES, false))
    }

    fn cycle_palette(&mut self, id: u32, step: i32) -> Result<UiChange, String> {
        self.bubble(id)?;
        let library = &self.state.color_library;
        let count = library.palettes.len() as i32;
        let current = library.palettes.iter().position(|p| p.id == library.active).unwrap_or(0) as i32;
        let next = library.palettes[(current + step).rem_euclid(count.max(1)) as usize].id;
        if let Some((_, scroll)) = self.bubble_state.scroll.iter_mut().find(|(bubble, _)| *bubble == id) {
            scroll[1] = 0.;
        }
        let mut change = self.dispatch(UiAction::Color { action: ColorAction::Library { action: ColorLibraryAction::SelectPalette { id: next } } })?;
        change.regions |= regions::BUBBLES;
        Ok(change)
    }

    pub(crate) fn bubble_palette_menu(&self, id: u32) -> Result<ContextMenu, String> {
        self.bubble(id)?;
        let library = &self.state.color_library;
        let rows = library
            .palettes
            .iter()
            .map(|palette| {
                let mut item = ContextMenuItem::command(
                    palette.name.clone(),
                    UiAction::Color { action: ColorAction::Library { action: ColorLibraryAction::SelectPalette { id: palette.id } } },
                );
                item.selected = Some(palette.id == library.active);
                item
            })
            .collect();
        Ok(ContextMenu { title: self.localization().text(MessageId::WORKSPACE_BUBBLE_PALETTE).to_string(), sections: vec![rows] })
    }

    fn ring_view(&self, arc: RingArc, setting: &ToolSetting, numeric: NumericControl) -> BubbleRing {
        let resolved = numeric.resolve(setting.value as f64, NumericOperation::Format).ok();
        BubbleRing {
            arc,
            setting: setting.id,
            label: setting.label.clone(),
            icon: tool_setting_icon(setting.id),
            fill: resolved.as_ref().map_or(0., |r| r.fill as f32),
            text: resolved.map(|r| r.text).unwrap_or_default(),
            active: self.bubble_state.ring.is_some_and(|(_, id)| id == setting.id),
        }
    }

    fn ring_ids(&self, brush: u32, snapshot: &BrushSnapshot, controls: &[ToolSetting]) -> Vec<&'static str> {
        match self.state.workspace.layout.bubbles.ring(brush) {
            Some(saved) => controls.iter().filter(|c| saved.iter().any(|s| s == c.id)).map(|c| c.id).collect(),
            None => tool_settings::ring_defaults(snapshot, controls),
        }
    }

    fn halo_settings(&self) -> Vec<&ToolSetting> {
        if self.state.layer_tools.tool != LayerCanvasTool::Paint {
            return Vec::new();
        }
        let controls = &self.state.tool_settings;
        let ids = self.ring_ids(self.state.brush.preset, self.engine.configured_brush(), controls);
        ids.into_iter().filter_map(|id| controls.iter().find(|c| c.id == id)).collect()
    }

    fn set_halo(&mut self, id: Option<u32>) -> bool {
        self.bubble_state.ring = None;
        std::mem::replace(&mut self.bubble_state.halo, id) != id
    }

    pub(crate) fn dismiss_bubble_halo(&mut self, position: [f32; 2]) -> bool {
        let Some(id) = self.bubble_state.halo else { return false };
        let inside = self.bubble_state.desk.center(id).is_some_and(|center| {
            (position[0] - center[0]).hypot(position[1] - center[1]) <= halo_reach(self.halo_settings().len(), TOOL_BUBBLE)
        });
        !inside && self.set_halo(None)
    }

    pub(crate) fn close_bubble_chooser(&mut self) -> bool {
        let anchored = self.state.customization.drawer.as_ref().is_some_and(|d| matches!(d.anchor, DrawerAnchor::Bubble { .. }));
        if anchored {
            self.state.customization.drawer = None;
        }
        anchored
    }

    pub(crate) fn bubble_action(&mut self, action: BubbleAction) -> Result<UiChange, String> {
        match action {
            BubbleAction::Drag { grab, phase, position, viewport } => self.drag_bubble(grab, phase, position, viewport),
            BubbleAction::Tap { id } => self.tap_bubble(id),
            BubbleAction::Ring { id, phase, position } => self.bubble_ring(id, phase, position),
            BubbleAction::Bead { id, bead } => self.bead_action(id, bead),
            BubbleAction::Scroll { id, strip, pixels } => self.scroll_beads(id, strip, pixels),
            BubbleAction::Palette { id, step } => self.cycle_palette(id, step),
            BubbleAction::PutBack { id } => {
                let source = self.bubble(id)?.source;
                self.set_halo(None);
                self.close_bubble_chooser();
                self.edit_bubbles(LayoutChange::Bubble { action: LayoutPanelAction::Deleted, source }, |layout| {
                    layout.bubbles.bubbles.retain(|b| b.id != id);
                    Ok(())
                })
            }
            BubbleAction::SetOptions { id, options } => {
                let source = self.bubble(id)?.source;
                if self.state.customization.drawer.as_ref().is_some_and(|d| d.anchor == DrawerAnchor::Bubble { id }) {
                    self.state.customization.drawer = None;
                }
                self.edit_bubbles(LayoutChange::Bubble { action: LayoutPanelAction::Customized, source }, |layout| {
                    layout.bubbles.get_mut(id)?.options = options;
                    Ok(())
                })
            }
            BubbleAction::ToggleRingSetting { brush, setting } => {
                let preset = tools::preset(brush)?;
                let snapshot = self.tools.brush(preset);
                let controls = tool_settings::controls(&snapshot, self.localization());
                let mut shown = self.ring_ids(brush, &snapshot, &controls);
                let setting = controls.iter().find(|c| c.id == setting).ok_or("This setting is not used by the brush")?.id;
                if let Some(index) = shown.iter().position(|id| *id == setting) {
                    shown.remove(index);
                } else if shown.len() < MAX_RING_SETTINGS {
                    shown.push(setting);
                } else {
                    return Err(self.localization().text(MessageId::WORKSPACE_BUBBLE_RING_FULL).to_string());
                }
                let ordered = controls.iter().filter(|c| shown.contains(&c.id)).map(|c| c.id.to_string()).collect();
                let source = BubbleSource::Control { control: ToolbarControl::Brush { id: brush } };
                self.edit_bubbles(LayoutChange::Bubble { action: LayoutPanelAction::Customized, source }, |layout| {
                    layout.bubbles.set_ring(brush, ordered);
                    Ok(())
                })
            }
        }
    }

    fn bubble(&self, id: u32) -> Result<&ToolBubble, String> {
        self.state.workspace.layout.bubbles.get(id).ok_or_else(|| "The bubble no longer exists".into())
    }

    fn edit_bubbles(
        &mut self,
        description: LayoutChange,
        edit: impl FnOnce(&mut DockLayout) -> Result<(), String>,
    ) -> Result<UiChange, String> {
        if self.bubble_state.drag.is_some() {
            return Err("Finish moving the bubble first".into());
        }
        let mut before = self.state.workspace.clone();
        self.state.customization.committed_header(&mut before.layout);
        let mut layout = self.state.workspace.layout.clone();
        edit(&mut layout)?;
        layout.validate()?;
        self.state.workspace.layout = layout;
        self.workspace_history.record_named(before, &self.state.workspace, description);
        self.sync_bubbles();
        Ok(self.changed(regions::BUBBLES | regions::CUSTOMIZATION, false))
    }

    fn select_bubble(&mut self, bubble: &ToolBubble) -> Result<UiChange, String> {
        match bubble.source {
            BubbleSource::Medium { group } => self.dispatch(UiAction::SelectBrushSet { group }),
            BubbleSource::Control { control: ToolbarControl::Brush { id } } => self.dispatch(UiAction::SelectBrush { id }),
            BubbleSource::Control { control } => {
                let group = control.tool_group().ok_or("This tool is unavailable")?;
                let variant = self.group_variant(group);
                self.choose_tool_variant(DrawerAnchor::Bubble { id: bubble.id }, variant)
            }
        }
    }

    fn toggle_bubble_drawer(&mut self, id: u32) -> Result<UiChange, String> {
        let mut change = self.dispatch(UiAction::Customize { action: CustomizationAction::ToggleBubbleDrawer { id } })?;
        let anchor = DrawerAnchor::Bubble { id };
        if self.state.customization.drawer.as_ref().is_some_and(|d| d.anchor == anchor) {
            match self.bubble(id)?.source {
                BubbleSource::Medium { .. } => {
                    let mut view = self.mark_path_brushes(tools::view(&self.state.brush, self.layer_interaction.tool, self.localization()));
                    view.groups.clear();
                    self.state.customization.drawer.as_mut().unwrap().tool_set = Some(view);
                }
                BubbleSource::Control { control } => {
                    if let Some(group) = control.tool_group() {
                        self.tool_origin = Some((anchor, group));
                        self.refresh_tools();
                        self.update_slot_drawer();
                    }
                }
            }
        }
        change.regions |= regions::BUBBLES | regions::CUSTOMIZATION | regions::BRUSH;
        Ok(change)
    }

    fn tap_bubble(&mut self, id: u32) -> Result<UiChange, String> {
        let bubble = self.bubble(id)?.clone();
        let view = self.bubble_view(&bubble);
        match view.kind {
            BubbleKind::Slider | BubbleKind::Color => return Ok(UiChange::default()),
            BubbleKind::Layers => {
                self.set_halo(None);
                return self.toggle_bubble_drawer(id);
            }
            BubbleKind::Dial | BubbleKind::Brush | BubbleKind::Medium => {}
        }
        if !view.enabled {
            return Err(self.localization().text(MessageId::COMMANDS_UNAVAILABLE_IN_THE_CURRENT_TOOL_OR_EDIT_TARGET).to_string());
        }
        let chooser = self.state.customization.drawer.as_ref().is_some_and(|d| d.anchor == DrawerAnchor::Bubble { id });
        if !view.selected {
            self.set_halo(None);
            self.close_bubble_chooser();
            let mut change = self.select_bubble(&bubble)?;
            change.regions |= regions::BUBBLES | regions::CUSTOMIZATION;
            return Ok(change);
        }
        if chooser {
            return self.toggle_bubble_drawer(id);
        }
        if self.bubble_state.halo != Some(id) && !self.halo_settings().is_empty() {
            let closed = self.close_bubble_chooser();
            self.set_halo(Some(id));
            self.sync_bubbles();
            return Ok(self.changed(regions::BUBBLES | if closed { regions::CUSTOMIZATION } else { 0 }, false));
        }
        self.set_halo(None);
        self.sync_bubbles();
        if view.kind == BubbleKind::Brush {
            return Ok(self.changed(regions::BUBBLES, false));
        }
        self.toggle_bubble_drawer(id)
    }

    fn bubble_ring(&mut self, id: u32, phase: ContactPhase, position: [f32; 2]) -> Result<UiChange, String> {
        if !position.into_iter().all(f32::is_finite) {
            return Err("Invalid ring position".into());
        }
        let bubble = self.bubble(id)?.clone();
        let view = self.bubble_view(&bubble);
        let setting = match phase {
            ContactPhase::Down => {
                let ring = view.rings.iter().find(|r| r.arc.contains(position)).ok_or("No setting under the contact")?;
                self.bubble_state.ring = Some((id, ring.setting));
                ring.setting
            }
            ContactPhase::Cancel => {
                self.bubble_state.ring = None;
                return Ok(self.changed(regions::BUBBLES, false));
            }
            ContactPhase::Move | ContactPhase::Up => match self.bubble_state.ring {
                Some((ring_id, setting)) if ring_id == id => setting,
                _ => return Ok(UiChange::default()),
            },
        };
        let ring = view.rings.iter().find(|r| r.setting == setting).ok_or("This setting is not used by the selected tool")?;
        let fraction = ring.arc.fraction(position) as f64;
        let binding = bubble.source.control().and_then(ToolbarControl::slider);
        let (field, numeric) = match &binding {
            Some(binding) => (binding.field(&self.state).ok_or("This slider is unavailable")?, binding.numeric()),
            None => {
                let field = self.state.tool_settings.iter().find(|s| s.id == setting).ok_or("This setting is not used by the selected tool")?.clone();
                let numeric = field.numeric.clone();
                (field, numeric)
            }
        };
        let value = numeric
            .resolve(field.value as f64, NumericOperation::Position { position: fraction })
            .map_err(|reason| reason.message(&self.state.localization))?
            .value as f32;
        let mut change = self.dispatch(match binding {
            Some(binding) => binding.action(value),
            None => UiAction::SetToolSetting { id: setting.into(), value },
        })?;
        if phase == ContactPhase::Up {
            self.bubble_state.ring = None;
        }
        change.regions |= regions::BUBBLES;
        Ok(change)
    }

    fn bubble_home(&self, viewport: [f32; 2], point: [f32; 2]) -> bool {
        let hidden = self.interaction.hidden;
        let header = self.state.workspace.layout.header_presentation.height.max(HEADER_HEIGHT);
        (!hidden && point[1] < header)
            || self.layout(viewport).groups.iter().any(|g| {
                (!hidden || g.floating || self.state.workspace.layout.shows_in_zen(g.id)) && g.bounds.contains(point[0], point[1])
            })
    }

    fn bubble_homing(&self, viewport: [f32; 2], point: [f32; 2], source: Option<Bounds>) -> bool {
        self.bubble_home(viewport, point) || source.is_some_and(|s| near(s, point, crate::layout::WORKSPACE_PROXIMITY))
    }

    fn grab_source(&self, grab: BubbleGrab, viewport: [f32; 2]) -> Result<(BubbleSource, Option<Bounds>), String> {
        let layout = &self.state.workspace.layout;
        let unavailable = || self.localization().text(MessageId::COMMANDS_UNAVAILABLE_IN_THE_CURRENT_TOOL_OR_EDIT_TARGET).to_string();
        let control = |anchor| layout.anchor_control(anchor).and_then(BubbleSource::for_control).ok_or_else(unavailable);
        Ok(match grab {
            BubbleGrab::Bubble { id } => (self.bubble(id)?.source, None),
            BubbleGrab::Tile { panel, tile } => (
                control(DrawerAnchor::Tile { panel, tile })?,
                self.layout(viewport).groups.iter().find(|g| g.panels.contains(&panel)).map(|g| g.bounds),
            ),
            BubbleGrab::Header { id } => (
                control(DrawerAnchor::Header { id })?,
                Some(Bounds { x: 0., y: 0., width: viewport[0], height: layout.header_presentation.height.max(HEADER_HEIGHT) }),
            ),
            BubbleGrab::Variant { variant } => (BubbleSource::for_variant(variant).ok_or_else(unavailable)?, None),
        })
    }

    fn drag_bubble(&mut self, grab: BubbleGrab, phase: ContactPhase, position: [f32; 2], viewport: [f32; 2]) -> Result<UiChange, String> {
        valid_viewport(viewport)?;
        if !position.into_iter().all(f32::is_finite) {
            return Err("Invalid drag position".into());
        }
        if phase == ContactPhase::Down {
            return self.lift_bubble(grab, position, viewport);
        }
        let Some(drag) = self.bubble_state.drag else { return Ok(UiChange::default()) };
        let center = [position[0] + drag.offset[0], position[1] + drag.offset[1]];
        let present = self.state.workspace.layout.bubbles.get(drag.id).is_some();
        match phase {
            ContactPhase::Move if present => {
                self.bubble_state.desk.drag(center);
                let homing = self.bubble_homing(viewport, position, drag.source);
                self.bubble_state.desk.home(homing);
                return Ok(UiChange::default());
            }
            ContactPhase::Up if present => {
                self.bubble_state.drag = None;
                self.bubble_state.desk.home(false);
                if self.bubble_homing(viewport, position, drag.source) {
                    let source = self.bubble(drag.id)?.source;
                    let before = self.workspace_history.gesture_start().cloned();
                    self.workspace_history.cancel(&mut self.state.workspace);
                    self.bubble_state.desk.release();
                    if let Some(before) = before {
                        self.state.workspace.layout.bubbles.bubbles.retain(|b| b.id != drag.id);
                        self.workspace_history.record_named(before, &self.state.workspace, LayoutChange::Bubble { action: LayoutPanelAction::Deleted, source });
                    }
                } else {
                    self.bubble_state.desk.drag(center);
                    for (id, rest) in self.bubble_state.desk.settled() {
                        if let Ok(bubble) = self.state.workspace.layout.bubbles.get_mut(id) {
                            bubble.center = rest;
                        }
                    }
                    self.bubble_state.desk.release();
                    self.workspace_history.finish(&self.state.workspace);
                }
            }
            _ => {
                self.bubble_state.drag = None;
                self.bubble_state.desk.home(false);
                self.bubble_state.desk.release();
                self.workspace_history.cancel(&mut self.state.workspace);
            }
        }
        self.sync_bubbles();
        Ok(self.changed(regions::BUBBLES, false))
    }

    fn lift_bubble(&mut self, grab: BubbleGrab, position: [f32; 2], viewport: [f32; 2]) -> Result<UiChange, String> {
        if self.bubble_state.drag.is_some() || self.workspace_drag.is_some() {
            return Err("Another item is already moving".into());
        }
        let (source, bounds) = self.grab_source(grab, viewport)?;
        let existing = match grab {
            BubbleGrab::Bubble { id } => Some(id),
            _ => self.state.workspace.layout.bubbles.find(source).map(|b| b.id),
        };
        self.workspace_history.begin_named(
            &self.state.workspace,
            LayoutChange::Bubble { action: if existing.is_some() { LayoutPanelAction::Moved } else { LayoutPanelAction::Added }, source },
        );
        let id = match existing {
            Some(id) => id,
            None => {
                let id = self.state.workspace.layout.allocate()?;
                self.state.workspace.layout.bubbles.bubbles.push(ToolBubble { id, source, center: position, options: BubbleOptions::default() });
                id
            }
        };
        let center = match grab {
            BubbleGrab::Bubble { .. } => self.bubble_state.desk.center(id).unwrap_or(position),
            _ => position,
        };
        self.set_halo(None);
        if self.state.customization.drawer.as_ref().is_some_and(|d| {
            d.anchor == DrawerAnchor::Bubble { id } || matches!(grab, BubbleGrab::Variant { .. })
        }) {
            self.state.customization.drawer = None;
        }
        self.bubble_state.drag = Some(BubbleDrag { id, offset: [center[0] - position[0], center[1] - position[1]], source: bounds });
        self.sync_bubbles();
        self.bubble_state.desk.grab(id);
        self.bubble_state.desk.drag(center);
        self.bubble_state.desk.home(self.bubble_homing(viewport, position, bounds));
        Ok(self.changed(regions::BUBBLES | regions::CUSTOMIZATION, false))
    }

    pub(crate) fn bubble_menu(&self, id: u32) -> Result<ContextMenu, String> {
        let bubble = self.bubble(id)?;
        let localization = self.localization();
        let text = |id: MessageId| localization.text(id).to_string();
        let action = |action| UiAction::Bubble { action };
        let options = |label: MessageId, checked: bool, options: BubbleOptions| {
            let mut item = ContextMenuItem::command(text(label), action(BubbleAction::SetOptions { id, options }));
            item.selected = Some(checked);
            item
        };
        let o = bubble.options;
        let mut sections = Vec::new();
        let current = (self.state.layer_tools.tool == LayerCanvasTool::Paint).then_some(self.state.brush.preset);
        let view = self.bubble_view(bubble);
        match bubble.source {
            BubbleSource::Control { control: ToolbarControl::Brush { id: brush } } => sections.extend(self.ring_menu(brush)),
            BubbleSource::Control { control: ToolbarControl::Color } => {
                sections.push(vec![
                    options(MessageId::WORKSPACE_BUBBLE_RECENT_COLORS, o.recent, BubbleOptions { recent: !o.recent, ..o }),
                    options(MessageId::WORKSPACE_BUBBLE_PALETTE, o.palette, BubbleOptions { palette: !o.palette, ..o }),
                    options(MessageId::WORKSPACE_BUBBLE_COLOR_PAIR, o.pair, BubbleOptions { pair: !o.pair, ..o }),
                ]);
                sections.push(
                    [MessageId::WORKSPACE_SIZE_SMALL, MessageId::WORKSPACE_SIZE_MEDIUM, MessageId::WORKSPACE_SIZE_LARGE]
                        .into_iter()
                        .zip(COLOR_RING_SIZES)
                        .map(|(label, size)| options(label, o.size == size, BubbleOptions { size, ..o }))
                        .collect(),
                );
            }
            BubbleSource::Control { control: ToolbarControl::Panel { .. } } => {
                sections.push(vec![
                    options(MessageId::WORKSPACE_BUBBLE_SIMPLE, !o.full, BubbleOptions { full: false, ..o }),
                    options(MessageId::WORKSPACE_BUBBLE_FULL, o.full, BubbleOptions { full: true, ..o }),
                ]);
                sections.push(vec![options(MessageId::WORKSPACE_BUBBLE_FOLD, o.fold, BubbleOptions { fold: !o.fold, ..o })]);
            }
            BubbleSource::Control { .. } if view.kind == BubbleKind::Dial => {
                if let Ok(menu) = self.tool_variants_menu(DrawerAnchor::Bubble { id }) {
                    sections.extend(menu.sections);
                }
                if view.selected {
                    sections.extend(current.and_then(|brush| self.ring_menu(brush)));
                }
            }
            BubbleSource::Medium { .. } if view.selected => sections.extend(current.and_then(|brush| self.ring_menu(brush))),
            _ => {}
        }
        sections.push(vec![ContextMenuItem::command(text(MessageId::WORKSPACE_BUBBLE_PUT_BACK), action(BubbleAction::PutBack { id }))]);
        Ok(ContextMenu { title: view.label, sections })
    }

    fn ring_menu(&self, brush: u32) -> Option<Vec<ContextMenuItem>> {
        let snapshot = self.tools.brush(tools::preset(brush).ok()?);
        let controls = tool_settings::controls(&snapshot, self.localization());
        let shown = self.ring_ids(brush, &snapshot, &controls);
        let rows = controls
            .iter()
            .map(|c| {
                let on = shown.contains(&c.id);
                let mut item = ContextMenuItem::command(
                    c.label.to_string(),
                    UiAction::Bubble { action: BubbleAction::ToggleRingSetting { brush, setting: c.id.into() } },
                );
                item.icon = Some(tool_setting_icon(c.id).into());
                item.selected = Some(on);
                item.enabled = on || shown.len() < MAX_RING_SETTINGS;
                item
            })
            .collect();
        Some(vec![ContextMenuItem::submenu(&self.localization().text(MessageId::WORKSPACE_BUBBLE_RING_SETTINGS), vec![rows])])
    }
}
