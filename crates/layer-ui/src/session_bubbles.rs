use super::*;
use crate::bubbles::{Desk, Shape, bead_scroll_limit, bead_travel};
use layer_core::BrushSnapshot;

const TAP_SLOP: f32 = 8.;

#[derive(Clone, Copy, Debug)]
struct BubbleDrag {
    id: u32,
    offset: [f32; 2],
    source: Option<Bounds>,
}

#[derive(Clone, Copy, Debug)]
struct BeadContact {
    id: u32,
    strip: Option<BeadStrip>,
    start: [f32; 2],
    last: [f32; 2],
    moved: bool,
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct Chooser {
    id: u32,
    medium: Option<ToolGroup>,
}

#[derive(Debug, Default)]
pub(crate) struct Bubbles {
    desk: Desk,
    bounds: Option<((u64, [f32; 2], bool), Bounds)>,
    drag: Option<BubbleDrag>,
    halo: Option<(u32, u32)>,
    ring: Option<(u32, &'static str)>,
    beads: Option<BeadContact>,
    scroll: Vec<(u32, [f32; 2])>,
    pill: Option<u32>,
    chooser: Option<Chooser>,
    card: Option<u32>,
    properties: bool,
    stroke: bool,
    dirty: bool,
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

fn inset(bounds: Bounds, by: f32) -> Bounds {
    Bounds { x: bounds.x + by, y: bounds.y + by, width: (bounds.width - by * 2.).max(1.), height: (bounds.height - by * 2.).max(1.) }
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

    pub fn reduce_bubble_motion(&mut self, reduce: bool) {
        self.bubble_state.desk.still(reduce);
    }

    pub fn bubble_fade(&self) -> (bool, Vec<u32>) {
        (self.bubble_state.stroke, self.bubble_state.desk.faded().to_vec())
    }

    pub fn bubble_out(&self, control: ToolbarControl) -> bool {
        let own = BubbleSource::for_control(control);
        let group = control.tool_group();
        self.state.workspace.layout.bubbles.bubbles.iter().any(|bubble| {
            Some(bubble.source) == own
                || group.is_some_and(|group| match bubble.source {
                    BubbleSource::Medium { group: medium } => group.contains(ToolVariant::BrushGroup { group: medium }),
                    BubbleSource::Control { control: ToolbarControl::Brush { id } } => {
                        group.contains(ToolVariant::BrushPreset { id }) || group.contains(ToolVariant::BrushGroup { group: tools::group(id) })
                    }
                    BubbleSource::Control { .. } => false,
                })
        })
    }

    fn bubble_viewport(&self) -> [f32; 2] {
        self.logical_viewport.unwrap_or([1200., 900.])
    }

    fn narrow(&self) -> bool {
        self.bubble_viewport()[0] < 600.
    }

    fn bubble_bounds(&mut self) -> Bounds {
        let viewport = self.bubble_viewport();
        let key = (self.workspace_model_revision, viewport, self.interaction.hidden);
        if let Some((cached, bounds)) = self.bubble_state.bounds
            && cached == key
        {
            return bounds;
        }
        let area = if self.interaction.hidden { self.state.workspace.layout.free_area(viewport) } else { self.layout(viewport).work_area };
        let bounds = inset(area, 8.);
        self.bubble_state.bounds = Some((key, bounds));
        bounds
    }

    fn bubble_center(&self, bubble: &ToolBubble) -> [f32; 2] {
        self.bubble_state.desk.center(bubble.id).unwrap_or_else(|| bubble.center(self.bubble_viewport()))
    }

    fn sync_bubbles(&mut self) {
        let bounds = self.bubble_bounds();
        let viewport = self.bubble_viewport();
        let narrow = self.narrow();
        let layout = &self.state.workspace.layout.bubbles;
        let state = &mut self.bubble_state;
        for open in [&mut state.pill, &mut state.card] {
            if open.is_some_and(|id| layout.get(id).is_none()) {
                *open = None;
            }
        }
        if state.halo.is_some_and(|(id, _)| layout.get(id).is_none()) {
            state.halo = None;
        }
        if state.chooser.is_some_and(|c| layout.get(c.id).is_none()) {
            state.chooser = None;
        }
        state.scroll.retain(|(id, _)| layout.get(*id).is_some());
        let items: Vec<_> = layout.bubbles.iter().map(|b| (b.id, b.center(viewport), b.shape(narrow))).collect();
        if !state.desk.matches(&items, bounds) {
            state.desk.sync(&items, bounds);
        }
        let grown = self.bubble_growth();
        self.bubble_state.desk.grow(grown);
    }

    fn bubble_growth(&self) -> Vec<(u32, Shape)> {
        let mut grown = Vec::new();
        if let Some(id) = self.halo() {
            grown.push((id, Shape::circle(halo_disc(self.halo_settings().len()) * 0.5)));
        }
        if let Some(id) = self.bubble_state.card
            && let Some(bubble) = self.state.workspace.layout.bubbles.get(id)
        {
            grown.push((id, Shape::rect(layers_card(bubble.options.full, self.state.layers.len()))));
        }
        grown
    }

    fn bubble_view(&self, bubble: &ToolBubble) -> BubbleView {
        let state = &self.state;
        let kind = bubble.source.kind();
        let narrow = self.narrow();
        let (label, icon, selected, enabled) = match bubble.source {
            BubbleSource::Medium { group } => (
                group.localized_label(self.localization()).to_string(),
                group.icon(),
                state.layer_tools.tool == LayerCanvasTool::Paint && tools::group(state.brush.preset) == group,
                true,
            ),
            BubbleSource::Control { control } => control_face(state, control),
        };
        let halo = self.halo() == Some(bubble.id);
        let rings: Vec<_> = match bubble.source {
            BubbleSource::Control { control } if kind == BubbleKind::Slider => {
                let binding = control.slider().unwrap();
                binding.field(state).map(|field| vec![self.ring_view(slider_ring(), &field, binding.numeric())]).unwrap_or_default()
            }
            _ if halo => {
                let settings = self.halo_settings();
                halo_arcs(settings.len())
                    .into_iter()
                    .zip(settings)
                    .map(|(arc, setting)| self.ring_view(arc, setting, setting.numeric.clone()))
                    .collect()
            }
            _ => Vec::new(),
        };
        let readout = rings.iter().find(|r| r.active).map(|r| format!("{}  {}", r.label, r.text));
        let chooser = self.bubble_state.chooser.filter(|c| c.id == bubble.id).map(|c| self.chooser_items(bubble, c).0).unwrap_or_default();
        let card = (self.bubble_state.card == Some(bubble.id)).then(|| BubbleCard {
            size: layers_card(bubble.options.full, state.layers.len()),
            full: bubble.options.full,
            properties: self.bubble_state.properties,
        });
        BubbleView {
            id: bubble.id,
            source: bubble.source,
            kind,
            diameter: bubble.diameter(narrow),
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
            open: halo || !chooser.is_empty() || card.is_some(),
            halo: halo.then(|| halo_disc(rings.len())),
            rings,
            readout,
            beads: if kind == BubbleKind::Color { self.bubble_beads(bubble, narrow) } else { Vec::new() },
            pill: (kind == BubbleKind::Color && self.bubble_state.pill == Some(bubble.id) && bubble.options.palette)
                .then(|| state.color_library.active_palette().name.clone()),
            chooser,
            card,
            layer: state.layers.iter().find(|l| l.selected).map(|l| l.id),
            layers: state.layers.len(),
            options: bubble.options,
        }
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

    fn bead_scroll(&self, id: u32) -> [f32; 2] {
        self.bubble_state.scroll.iter().find(|(bubble, _)| *bubble == id).map_or([0.; 2], |(_, scroll)| *scroll)
    }

    fn bubble_beads(&self, bubble: &ToolBubble, narrow: bool) -> Vec<BubbleBead> {
        let state = &self.state;
        let library = &state.color_library;
        let swatches = &library.active_palette().swatches;
        let text = |id: MessageId| self.localization().text(id).to_string();
        color_beads(bubble.diameter(narrow), bubble.options, library.history.len(), swatches.len(), self.bead_scroll(bubble.id))
            .into_iter()
            .map(|placement| {
                let (color, label, selected) = match placement.bead {
                    Bead::Recent { index } => (library.history.get(index).copied(), text(MessageId::WORKSPACE_BUBBLE_RECENT_COLORS), false),
                    Bead::Swatch { index } => {
                        let swatch = swatches.get(index);
                        (swatch.map(|s| s.color), swatch.map(|s| s.name.clone()).unwrap_or_default(), false)
                    }
                    Bead::Foreground => (Some(state.colors.foreground), text(MessageId::COMMANDS_FOREGROUND_COLOR), state.colors.slot == ColorSlot::Foreground),
                    Bead::Background => (Some(state.colors.background), text(MessageId::COMMANDS_BACKGROUND_COLOR), false),
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

    fn chooser_items(&self, bubble: &ToolBubble, chooser: Chooser) -> (Vec<ChooserItem>, Vec<Option<ToolVariant>>) {
        let state = &self.state;
        let center = self.bubble_center(bubble);
        let bounds = self.bubble_state.bounds.map_or_else(|| self.state.workspace.layout.free_area(self.bubble_viewport()), |(_, b)| b);
        let brush_item = |id: u32, at: [f32; 2]| ChooserItem {
            kind: ChooserKind::Brush,
            center: at,
            label: tools::brush_label_localized(id, self.localization()).map(|l| l.to_string()).unwrap_or_default(),
            icon: tools::icon(id),
            preview: Some(id),
            selected: state.layer_tools.tool == LayerCanvasTool::Paint && state.brush.preset == id,
            shown: false,
            enabled: true,
        };
        let column = |group: ToolGroup, medium: Option<([f32; 2], f32)>| {
            let ids: Vec<_> = tools::group_presets(group).collect();
            chooser_column(ids.len(), center, medium, bounds)
                .into_iter()
                .zip(ids)
                .map(|(at, id)| (brush_item(id, at), Some(ToolVariant::BrushPreset { id })))
                .collect::<Vec<_>>()
        };
        let pairs: Vec<(ChooserItem, Option<ToolVariant>)> = match bubble.source {
            BubbleSource::Medium { group } => column(group, None),
            BubbleSource::Control { control } => {
                let Some(group) = control.tool_group() else { return Default::default() };
                let choices = self.group_choices(group, None);
                let shown = chooser.medium.or_else(|| {
                    choices.iter().find_map(|(variant, item)| match variant {
                        ToolVariant::BrushGroup { group } if item.selected => Some(*group),
                        _ => None,
                    })
                });
                let ring = chooser_ring(choices.len(), center, bounds);
                let mut pairs: Vec<_> = choices
                    .into_iter()
                    .zip(ring)
                    .map(|((variant, item), at)| {
                        let medium = match variant {
                            ToolVariant::BrushGroup { group } => Some(group),
                            _ => None,
                        };
                        (
                            ChooserItem {
                                kind: if medium.is_some() { ChooserKind::Medium } else { ChooserKind::Tool },
                                center: at,
                                label: item.label.to_string(),
                                icon: item.icon,
                                preview: None,
                                selected: item.selected,
                                shown: medium.is_some() && medium == shown,
                                enabled: item.enabled,
                            },
                            Some(variant),
                        )
                    })
                    .collect();
                let anchor = pairs.iter().find(|(_, variant)| shown.is_some_and(|group| *variant == Some(ToolVariant::BrushGroup { group })));
                if let (Some(group), Some((item, _))) = (shown, anchor)
                    && tools::group_presets(group).nth(1).is_some()
                {
                    let reach = pairs.iter().map(|(item, _)| item.center[0].hypot(item.center[1])).fold(0., f32::max);
                    let at = item.center;
                    pairs.extend(column(group, Some((at, reach))));
                }
                pairs
            }
        };
        pairs.into_iter().unzip()
    }

    fn halo(&self) -> Option<u32> {
        let tools = &self.state;
        self.bubble_state
            .halo
            .filter(|(_, brush)| tools.layer_tools.tool == LayerCanvasTool::Paint && tools.brush.preset == *brush)
            .map(|(id, _)| id)
    }

    pub(crate) fn close_halo(&mut self) -> bool {
        self.bubble_state.ring = None;
        self.bubble_state.halo.take().is_some()
    }

    fn set_halo(&mut self, id: Option<u32>) {
        self.bubble_state.ring = None;
        self.bubble_state.halo = id.map(|id| (id, self.state.brush.preset));
    }

    fn close_bubble_parts(&mut self, keep: Option<u32>) {
        let state = &mut self.bubble_state;
        if state.chooser.is_some_and(|c| Some(c.id) != keep) {
            state.chooser = None;
        }
        if state.pill != keep {
            state.pill = None;
        }
    }

    fn fold_cards(&mut self) {
        let folds = self.bubble_state.card.and_then(|id| self.state.workspace.layout.bubbles.get(id)).is_some_and(|b| b.options.fold);
        if folds {
            self.bubble_state.card = None;
            self.bubble_state.properties = false;
        }
    }

    fn contains_open_part(&self, id: u32, point: [f32; 2]) -> bool {
        let Some(bubble) = self.state.workspace.layout.bubbles.get(id) else { return false };
        let view = self.bubble_view(bubble);
        let center = self.bubble_center(bubble);
        let local = [point[0] - center[0], point[1] - center[1]];
        let radius = view.halo.unwrap_or(view.diameter).max(view.diameter) * 0.5;
        local[0].hypot(local[1]) <= radius
            || view.chooser.iter().any(|item| item.contains(local))
            || view.card.is_some_and(|card| local[0].abs() <= card.size[0] * 0.5 && local[1].abs() <= card.size[1] * 0.5)
            || view.beads.iter().any(|b| bead_at(&[b.placement], local).is_some())
            || (view.pill.is_some() && palette_pill(view.diameter).contains(local[0], local[1]))
    }

    pub(crate) fn dismiss_bubble_parts(&mut self, position: [f32; 2]) -> bool {
        let state = &self.bubble_state;
        let open: Vec<_> = [state.chooser.map(|c| c.id), state.pill, state.card].into_iter().flatten().collect();
        let mut changed = false;
        for id in open {
            if self.contains_open_part(id, position) {
                continue;
            }
            let before = (self.bubble_state.chooser, self.bubble_state.pill, self.bubble_state.card);
            if self.bubble_state.chooser.is_some_and(|c| c.id == id) {
                self.bubble_state.chooser = None;
            }
            if self.bubble_state.pill == Some(id) {
                self.bubble_state.pill = None;
            }
            if self.bubble_state.card == Some(id) {
                self.fold_cards();
            }
            changed |= before != (self.bubble_state.chooser, self.bubble_state.pill, self.bubble_state.card);
        }
        changed
    }

    pub(crate) fn bubble_stroke(&mut self, phase: PenPhase, surface: [f32; 2]) {
        if self.state.workspace.layout.bubbles.bubbles.is_empty() {
            return;
        }
        let scale = self.logical_viewport.map_or(1., |v| (self.state.camera.viewport[0] as f32 / v[0]).max(f32::EPSILON));
        let point = surface.map(|v| v / scale);
        match phase {
            PenPhase::Down => {
                let before = (self.bubble_state.chooser, self.bubble_state.pill, self.bubble_state.card);
                self.bubble_state.chooser = None;
                self.bubble_state.pill = None;
                self.fold_cards();
                self.bubble_state.stroke = true;
                let cleared = self.bubble_state.desk.fade(None);
                let faded = self.bubble_state.desk.fade(Some(point));
                self.bubble_state.dirty |= cleared || faded || before != (self.bubble_state.chooser, self.bubble_state.pill, self.bubble_state.card);
            }
            PenPhase::Move => {
                self.bubble_state.dirty |= self.bubble_state.desk.fade(Some(point));
            }
            PenPhase::Up | PenPhase::Cancel => {
                self.bubble_state.dirty |= std::mem::replace(&mut self.bubble_state.stroke, false) && !self.bubble_state.desk.faded().is_empty();
            }
            PenPhase::Hover => {}
        }
    }

    pub(crate) fn take_bubble_changes(&mut self) -> u32 {
        if std::mem::take(&mut self.bubble_state.dirty) { regions::BUBBLES } else { 0 }
    }

    pub(crate) fn bubble_action(&mut self, action: BubbleAction) -> Result<UiChange, String> {
        match action {
            BubbleAction::Drag { grab, phase, position, viewport } => self.drag_bubble(grab, phase, position, viewport),
            BubbleAction::Tap { id } => self.tap_bubble(id),
            BubbleAction::Ring { id, phase, position } => self.bubble_ring(id, phase, position),
            BubbleAction::Beads { id, phase, position } => self.bead_contact(id, phase, position),
            BubbleAction::Choose { id, phase, position } => self.choose(id, phase, position),
            BubbleAction::Palette { id, step } => self.cycle_palette(id, step),
            BubbleAction::Properties { id } => {
                if self.bubble_state.card != Some(id) {
                    return Err("Open the Layers card first".into());
                }
                self.bubble_state.properties = !self.bubble_state.properties;
                Ok(self.changed(regions::BUBBLES, false))
            }
            BubbleAction::PutBack { id } => {
                let source = self.bubble(id)?.source;
                self.edit_bubbles(LayoutChange::Bubble { action: LayoutPanelAction::Deleted, source }, |layout| {
                    layout.bubbles.bubbles.retain(|b| b.id != id);
                    Ok(())
                })
            }
            BubbleAction::PutBackAll => {
                let source = self.state.workspace.layout.bubbles.bubbles.first().ok_or("No bubbles are out")?.source;
                self.edit_bubbles(LayoutChange::Bubble { action: LayoutPanelAction::Deleted, source }, |layout| {
                    layout.bubbles.bubbles.clear();
                    Ok(())
                })
            }
            BubbleAction::SetOptions { id, options } => {
                let source = self.bubble(id)?.source;
                self.edit_bubbles(LayoutChange::Bubble { action: LayoutPanelAction::Customized, source }, |layout| {
                    layout.bubbles.get_mut(id)?.options = options;
                    Ok(())
                })
            }
            BubbleAction::Resize { id, scale } => {
                let bubble = self.bubble(id)?;
                if bubble.source.kind() != BubbleKind::Color || !scale.is_finite() || scale <= 0. {
                    return Err("Only the color ring changes size".into());
                }
                let (source, options) = (bubble.source, BubbleOptions { size: pinched_size(bubble.options.size, scale), ..bubble.options });
                self.edit_bubbles(LayoutChange::Bubble { action: LayoutPanelAction::Resized, source }, |layout| {
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

    fn edit_bubbles(&mut self, description: LayoutChange, edit: impl FnOnce(&mut DockLayout) -> Result<(), String>) -> Result<UiChange, String> {
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
        Ok(self.changed(regions::BUBBLES | regions::COMMANDS | regions::LAYOUT, false))
    }

    fn select_bubble(&mut self, bubble: &ToolBubble) -> Result<UiChange, String> {
        let action = match bubble.source {
            BubbleSource::Medium { group } => UiAction::SelectBrushSet { group },
            BubbleSource::Control { control: ToolbarControl::Brush { id } } => UiAction::SelectBrush { id },
            BubbleSource::Control { control } => {
                let group = control.tool_group().ok_or("This tool is unavailable")?;
                self.variant_action(self.group_variant(group))
            }
        };
        let change = self.dispatch(action)?;
        self.remember_tool_slots();
        Ok(change)
    }

    fn tap_bubble(&mut self, id: u32) -> Result<UiChange, String> {
        let bubble = self.bubble(id)?.clone();
        let view = self.bubble_view(&bubble);
        let opened = |s: &mut Self| {
            s.sync_bubbles();
            Ok(s.changed(regions::BUBBLES, false))
        };
        match view.kind {
            BubbleKind::Slider | BubbleKind::Color => return Ok(UiChange::default()),
            BubbleKind::Layers => {
                self.close_bubble_parts(None);
                if self.bubble_state.card == Some(id) {
                    self.bubble_state.card = None;
                } else {
                    self.bubble_state.card = Some(id);
                }
                self.bubble_state.properties = false;
                return opened(self);
            }
            BubbleKind::Dial | BubbleKind::Brush | BubbleKind::Medium => {}
        }
        if !view.enabled {
            return Err(self.localization().text(MessageId::COMMANDS_UNAVAILABLE_IN_THE_CURRENT_TOOL_OR_EDIT_TARGET).to_string());
        }
        let chooser = self.bubble_state.chooser.is_some_and(|c| c.id == id);
        let halo = self.halo() == Some(id);
        self.close_bubble_parts(Some(id));
        if !view.selected {
            self.close_bubble_parts(None);
            let mut change = self.select_bubble(&bubble)?;
            change.regions |= regions::BUBBLES;
            return Ok(change);
        }
        if chooser {
            self.bubble_state.chooser = None;
        } else if halo {
            self.set_halo(None);
            if view.kind != BubbleKind::Brush {
                self.bubble_state.chooser = Some(Chooser { id, medium: None });
            }
        } else if !self.halo_settings().is_empty() {
            self.set_halo(Some(id));
        } else if view.kind != BubbleKind::Brush {
            self.bubble_state.chooser = Some(Chooser { id, medium: None });
        }
        opened(self)
    }

    fn choose(&mut self, id: u32, phase: ContactPhase, position: [f32; 2]) -> Result<UiChange, String> {
        let Some(chooser) = self.bubble_state.chooser.filter(|c| c.id == id) else { return Ok(UiChange::default()) };
        let bubble = self.bubble(id)?.clone();
        let (items, variants) = self.chooser_items(&bubble, chooser);
        let hit = items.iter().position(|item| item.contains(position) && item.enabled);
        let medium = hit.and_then(|i| match variants[i] {
            Some(ToolVariant::BrushGroup { group }) => Some(group),
            _ => None,
        });
        match phase {
            ContactPhase::Down | ContactPhase::Move => {
                if let Some(group) = medium
                    && chooser.medium != Some(group)
                {
                    self.bubble_state.chooser = Some(Chooser { id, medium: Some(group) });
                    return Ok(self.changed(regions::BUBBLES, false));
                }
                Ok(UiChange::default())
            }
            ContactPhase::Up => match hit.and_then(|i| variants[i]) {
                Some(ToolVariant::BrushGroup { group }) if tools::group_presets(group).nth(1).is_some() => {
                    self.bubble_state.chooser = Some(Chooser { id, medium: Some(group) });
                    Ok(self.changed(regions::BUBBLES, false))
                }
                Some(variant) => {
                    self.bubble_state.chooser = None;
                    let mut change = self.dispatch(self.variant_action(variant))?;
                    self.remember_tool_slots();
                    change.regions |= regions::BUBBLES;
                    Ok(change)
                }
                None => Ok(UiChange::default()),
            },
            ContactPhase::Cancel => Ok(UiChange::default()),
        }
    }

    fn bubble_ring(&mut self, id: u32, phase: ContactPhase, position: [f32; 2]) -> Result<UiChange, String> {
        if !position.into_iter().all(f32::is_finite) {
            return Err("Invalid ring position".into());
        }
        let bubble = self.bubble(id)?.clone();
        let view = self.bubble_view(&bubble);
        let setting = match phase {
            ContactPhase::Down => {
                let arcs: Vec<_> = view.rings.iter().map(|r| r.arc).collect();
                let ring = &view.rings[ring_at(&arcs, position).ok_or("No setting under the contact")?];
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

    fn bead_contact(&mut self, id: u32, phase: ContactPhase, position: [f32; 2]) -> Result<UiChange, String> {
        let bubble = self.bubble(id)?.clone();
        let size = bubble.diameter(self.narrow());
        let library = &self.state.color_library;
        let (recent, swatches) = (library.history.len(), library.active_palette().swatches.len());
        match phase {
            ContactPhase::Down => {
                let beads: Vec<_> = self.bubble_beads(&bubble, self.narrow()).into_iter().map(|b| b.placement).collect();
                let strip = bead_at(&beads, position).and_then(Bead::strip);
                self.bubble_state.beads = Some(BeadContact { id, strip, start: position, last: position, moved: false });
                Ok(UiChange::default())
            }
            ContactPhase::Move => {
                let Some(mut contact) = self.bubble_state.beads.filter(|c| c.id == id) else { return Ok(UiChange::default()) };
                contact.moved |= (position[0] - contact.start[0]).hypot(position[1] - contact.start[1]) > TAP_SLOP;
                let mut regions = 0;
                if let (true, Some(strip)) = (contact.moved, contact.strip) {
                    let limit = bead_scroll_limit(strip, size, if strip == BeadStrip::Recent { recent } else { swatches });
                    let mut scroll = self.bead_scroll(id);
                    let axis = usize::from(strip == BeadStrip::Palette);
                    scroll[axis] = (scroll[axis] + bead_travel(strip, size, contact.last, position)).clamp(0., limit);
                    self.bubble_state.scroll.retain(|(bubble, _)| *bubble != id);
                    self.bubble_state.scroll.push((id, scroll));
                    regions = regions::BUBBLES;
                }
                contact.last = position;
                self.bubble_state.beads = Some(contact);
                Ok(self.changed(regions, false))
            }
            ContactPhase::Up | ContactPhase::Cancel => {
                let Some(contact) = self.bubble_state.beads.take().filter(|c| c.id == id) else { return Ok(UiChange::default()) };
                if contact.moved || phase == ContactPhase::Cancel {
                    if let Some((_, scroll)) = self.bubble_state.scroll.iter_mut().find(|(bubble, _)| *bubble == id) {
                        *scroll = scroll.map(f32::round);
                    }
                    return Ok(self.changed(regions::BUBBLES, false));
                }
                let beads: Vec<_> = self.bubble_beads(&bubble, self.narrow()).into_iter().map(|b| b.placement).collect();
                match bead_at(&beads, position) {
                    Some(bead) => self.bead_action(id, bead),
                    None => Ok(UiChange::default()),
                }
            }
        }
    }

    fn bead_action(&mut self, id: u32, bead: Bead) -> Result<UiChange, String> {
        let library = &self.state.color_library;
        let action = match bead {
            Bead::Recent { index } => ColorAction::Definition { color: *library.history.get(index).ok_or("The color is no longer in the history")? },
            Bead::Swatch { index } => ColorAction::Library {
                action: ColorLibraryAction::Use { id: library.active_palette().swatches.get(index).ok_or("The color is no longer in the palette")?.id },
            },
            Bead::Foreground => ColorAction::Select { slot: ColorSlot::Foreground },
            Bead::Background => ColorAction::Swap,
            Bead::Transparent => ColorAction::Select { slot: ColorSlot::Transparent },
            Bead::Palettes => {
                self.bubble_state.pill = (self.bubble_state.pill != Some(id)).then_some(id);
                return Ok(self.changed(regions::BUBBLES, false));
            }
        };
        self.bubble_state.pill = None;
        let mut change = self.dispatch(UiAction::Color { action })?;
        change.regions |= regions::BUBBLES;
        Ok(change)
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
        let mut rows: Vec<_> = library
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
        rows.push(ContextMenuItem::edit(
            self.localization().text(MessageId::WORKSPACE_BUBBLE_EDIT_PALETTES).to_string(),
            CustomizationAction::SetPanelVisible { panel: Panel::Palettes, visible: true },
        ));
        Ok(ContextMenu { title: self.localization().text(MessageId::WORKSPACE_BUBBLE_PALETTE).to_string(), sections: vec![rows] })
    }

    fn bubble_home(&mut self, viewport: [f32; 2], point: [f32; 2]) -> bool {
        let hidden = self.interaction.hidden;
        let header = self.state.workspace.layout.header_presentation.height.max(HEADER_HEIGHT);
        (!hidden && point[1] < header)
            || self.layout(viewport).groups.iter().any(|g| {
                (!hidden || g.floating || self.state.workspace.layout.shows_in_zen(g.id)) && g.bounds.contains(point[0], point[1])
            })
    }

    fn bubble_homing(&mut self, viewport: [f32; 2], point: [f32; 2], source: Option<Bounds>) -> bool {
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
                    let viewport = self.bubble_viewport();
                    for (id, rest) in self.bubble_state.desk.settled() {
                        if let Ok(bubble) = self.state.workspace.layout.bubbles.get_mut(id) {
                            bubble.place(rest, viewport);
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
        Ok(self.changed(regions::BUBBLES | regions::COMMANDS, false))
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
                let bubble = ToolBubble::new(id, source, position, self.bubble_viewport());
                self.state.workspace.layout.bubbles.bubbles.push(bubble);
                id
            }
        };
        let center = match grab {
            BubbleGrab::Bubble { .. } => self.bubble_state.desk.center(id).unwrap_or(position),
            _ => position,
        };
        let closes = matches!(grab, BubbleGrab::Variant { .. }) && self.state.customization.drawer.is_some();
        if closes {
            self.state.customization.drawer = None;
        }
        self.close_bubble_parts(None);
        if self.bubble_state.card == Some(id) {
            self.bubble_state.card = None;
        }
        self.bubble_state.drag = Some(BubbleDrag { id, offset: [center[0] - position[0], center[1] - position[1]], source: bounds });
        self.sync_bubbles();
        self.bubble_state.desk.grab(id);
        self.bubble_state.desk.drag(center);
        let homing = self.bubble_homing(viewport, position, bounds);
        self.bubble_state.desk.home(homing);
        Ok(self.changed(regions::BUBBLES | if closes { regions::CUSTOMIZATION } else { 0 }, false))
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
            BubbleSource::Control { .. } | BubbleSource::Medium { .. } if view.kind != BubbleKind::Slider && view.selected => {
                sections.extend(current.and_then(|brush| self.ring_menu(brush)));
            }
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
