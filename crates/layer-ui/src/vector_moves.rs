use super::*;
use layer_core::{Edit, Point, authored::{Affine64, VectorItemSelection, vector::{ItemId, PathItem, Position, VectorIndex, VectorObject}}};
use layer_render::CursorSegment;
use std::{cell::RefCell, collections::BTreeSet, sync::Arc};
use super::{transform_frame::{Frame, FrameHandle, about, translation}, vector_properties::PathStyles};

const HIT_SLOP: f64 = 6.;
const CLICK: f64 = 3.;
const DOUBLE_CLICK_NS: u64 = 400_000_000;

enum Gesture {
    Drag { layer: OccurrenceHandle, items: Arc<BTreeSet<ItemId>>, start: [f64; 2], offset: [f64; 2], nudge: Option<String>, original: Option<VectorObject> },
    Marquee { layer: OccurrenceHandle, start: [f64; 2], end: [f64; 2], adding: bool },
    Transform { layer: OccurrenceHandle, items: Arc<BTreeSet<ItemId>>, paths: Vec<Position>, frame: Frame, handle: FrameHandle, press: [f64; 2], original: VectorObject, latest: Option<([f64; 2], Modifiers)>, delta: Option<Affine64> },
}

#[derive(Clone, Default)]
pub(super) struct SelectedPaths { bounds: Option<[f64; 4]>, count: usize, closable: bool, joinable: usize, compound: bool, styles: Option<PathStyles> }
impl SelectedPaths {
    fn of(paths: &[(Position, &PathItem)]) -> Self {
        let facts = paths.iter().fold(Self::default(), |facts, (_, path)| Self {
            bounds: Some(layer_core::authored::vector::union(facts.bounds, path.bounds())),
            count: facts.count + 1,
            closable: facts.closable || path.can_close(),
            joinable: facts.joinable + usize::from(path.joinable()),
            compound: facts.compound || path.geometry.subpaths().len() > 1,
            styles: None,
        });
        Self { styles: PathStyles::of(paths.iter().map(|(_, path)| *path)), ..facts }
    }
}

type CachedSelection = (VectorObject, Arc<BTreeSet<ItemId>>, SelectedPaths);

#[derive(Default)]
pub(super) struct VectorMoves {
    gesture: Option<Gesture>,
    selected: RefCell<Option<CachedSelection>>,
    pub(super) shown_styles: Option<PathStyles>,
    pub(super) index: VectorIndex,
    context: (bool, bool),
    click: Option<(ItemId, u64)>,
}

impl VectorMoves {
    pub(super) fn gesturing(&self) -> bool { self.gesture.is_some() }
    pub(super) fn transform_waiting(&self) -> bool { matches!(self.gesture, Some(Gesture::Transform { latest: Some(_), .. })) }
}

impl<R: CanvasRenderer> UiSession<R> {
    fn vector_move_layer(&self) -> Option<OccurrenceHandle> {
        self.editing_vector_layer().filter(|_| self.layer_interaction.tool == LayerCanvasTool::Move)
    }
    pub(super) fn vector_move_touch_target(&self, point: [f64; 2]) -> bool {
        let Some(layer) = self.vector_move_layer().filter(|layer| !self.engine.document().is_locked(*layer)) else { return false };
        let handle = self.vector_frame().and_then(|frame| frame.hit(point, f64::from(self.ruler_reach())));
        if matches!(handle, Some(FrameHandle::Scale(_) | FrameHandle::Rotate)) { return true; }
        let scene = self.engine.document().scene();
        let Some(object) = scene.vector_layer(layer) else { return false };
        let [ox, oy] = scene.occurrence_offset64(layer);
        let slop = HIT_SLOP * self.document_per_surface_pixel();
        object.paths().any(|(_, path)| path.pickable() && path.hit([point[0] - ox, point[1] - oy], slop))
    }
    pub(super) fn selected_vector_items(&self) -> Option<&VectorItemSelection> {
        let layer = self.editing_vector_layer()?;
        self.engine.document().working.vector_items.as_ref().filter(|selection| selection.layer == layer)
    }
    pub(super) fn select_vector_items(&mut self, layer: OccurrenceHandle, items: BTreeSet<ItemId>) -> Result<Arc<BTreeSet<ItemId>>, String> {
        let current = self.engine.document().working.vector_items.clone();
        if let Some(current) = current.filter(|c| c.layer == layer && *c.items == items) { return Ok(current.items); }
        let items = Arc::new(items);
        let mut working = self.engine.document().working.clone();
        working.vector_items = (!items.is_empty()).then(|| VectorItemSelection { layer, items: items.clone() });
        self.layer_edit(Edit::Working(working))?;
        self.layer_interaction.changed = true;
        Ok(items)
    }
    fn path_facts(&self, selection: &VectorItemSelection) -> Option<SelectedPaths> {
        let object = self.engine.document().scene().vector_layer(selection.layer)?;
        let mut cached = self.vector_moves.selected.borrow_mut();
        if !cached.as_ref().is_some_and(|(root, items, _)| root.same_root(object) && Arc::ptr_eq(items, &selection.items)) {
            *cached = Some((object.clone(), selection.items.clone(), SelectedPaths::of(&object.selected_paths(&selection.items))));
        }
        cached.as_ref().map(|(_, _, facts)| facts.clone())
    }
    pub(super) fn selected_path_facts(&self) -> Option<SelectedPaths> { self.path_facts(self.selected_vector_items()?) }
    pub(super) fn path_styles(&self) -> Option<PathStyles> {
        let document = self.engine.document();
        let selection = document.working.vector_items.as_ref().filter(|selection| Some(selection.layer) == document.working.occurrence)?;
        self.path_facts(selection)?.styles
    }
    pub(super) fn vector_frame(&self) -> Option<Frame> {
        let selection = self.selected_vector_items()?;
        let [x0, y0, x1, y1] = self.selected_path_facts()?.bounds?;
        let [ox, oy] = self.engine.document().scene().occurrence_offset64(selection.layer);
        let (min, size) = ([x0 + ox, y0 + oy], [(x1 - x0).max(1e-3), (y1 - y0).max(1e-3)]);
        Some(Frame { unit: Affine64([size[0], 0., 0., size[1], min[0], min[1]]), pivot: [min[0] + size[0] * 0.5, min[1] + size[1] * 0.5] })
    }
    fn begin_vector_transform(&mut self, layer: OccurrenceHandle, point: [f64; 2]) -> Option<Gesture> {
        let frame = self.vector_frame()?;
        let handle = frame.hit(point, f64::from(self.ruler_reach())).filter(|handle| matches!(handle, FrameHandle::Scale(_) | FrameHandle::Rotate))?;
        let items = self.selected_vector_items()?.items.clone();
        let original = self.engine.document().scene().vector_layer(layer)?.clone();
        let paths = original.selected_paths(&items).into_iter().map(|(position, _)| position).collect();
        self.engine.backend_mut().set_vector_motion(Some(layer_render::VectorMotion { owner: layer, items: items.clone(), transform: translation([0.; 2]) }));
        Some(Gesture::Transform { layer, items, paths, frame, handle, press: point, original, latest: None, delta: None })
    }
    pub(super) fn sync_vector_transform(&mut self) {
        let Some(Gesture::Transform { latest, .. }) = &mut self.vector_moves.gesture else { return };
        let Some((point, modifiers)) = latest.take() else { return };
        self.transform_vector_items(point, modifiers);
    }
    fn transform_vector_items(&mut self, point: [f64; 2], modifiers: Modifiers) {
        let Some(Gesture::Transform { layer, items, frame, handle, press, delta, .. }) = &mut self.vector_moves.gesture else { return };
        let Some(transform) = frame.delta(*handle, *press, point, modifiers.shift, false, modifiers.alt) else { return };
        *delta = Some(transform);
        let motion = layer_render::VectorMotion { owner: *layer, items: items.clone(), transform };
        self.engine.backend_mut().set_vector_motion(Some(motion));
    }
    pub(super) fn document_per_surface_pixel(&self) -> f64 {
        let [m0, m1, m2, m3, _, _] = self.state.camera.input_transform().surface_to_document;
        f64::from((m0 * m3 - m1 * m2).abs().sqrt()).max(1e-6)
    }
    pub(super) fn vector_move_pen(&mut self, event: PenEvent, p: Point) -> Result<bool, String> {
        let point = [f64::from(p.x), f64::from(p.y)];
        match event.phase {
            PenPhase::Down => {
                self.cancel_vector_move();
                let Some(layer) = self.vector_move_layer() else { return Ok(false) };
                if self.engine.document().is_locked(layer) { return Err(self.localization().text(MessageId::COMMANDS_THE_ACTIVE_LAYER_IS_LOCKED).to_string()); }
                if let Some(gesture) = self.begin_vector_transform(layer, point) {
                    self.vector_moves.gesture = Some(gesture);
                    return Ok(true);
                }
                let doc = self.engine.document();
                let object = doc.scene().vector_layer(layer).ok_or("The vector layer was removed")?;
                let offset = doc.scene().occurrence_offset64(layer);
                let local = [point[0] - offset[0], point[1] - offset[1]];
                let hit = self.vector_moves.index.hit(object, local, HIT_SLOP * self.document_per_surface_pixel());
                let adding = self.interaction.modifiers.shift;
                let selected: BTreeSet<ItemId> = self.selected_vector_items().map(|s| (*s.items).clone()).unwrap_or_default();
                let Some(id) = hit else {
                    self.vector_moves.gesture = Some(Gesture::Marquee { layer, start: point, end: point, adding });
                    return Ok(true);
                };
                let double = self.vector_moves.click.replace((id, event.timestamp_ns)).is_some_and(|(last, at)| last == id && event.timestamp_ns.saturating_sub(at) <= DOUBLE_CLICK_NS);
                if double && !adding {
                    self.vector_moves.click = None;
                    self.select_vector_items(layer, if selected.contains(&id) { selected } else { [id].into() })?;
                    return self.object_tool(LayerCanvasTool::EditPoints).map(|()| true);
                }
                let items = match (selected.contains(&id), adding) {
                    (true, false) => selected,
                    (true, true) => selected.into_iter().filter(|item| *item != id).collect(),
                    (false, true) => selected.into_iter().chain([id]).collect(),
                    (false, false) => [id].into(),
                };
                let items = self.select_vector_items(layer, items)?;
                if items.contains(&id) {
                    let (items, original) = if self.interaction.modifiers.alt { self.preview_vector_copies(layer, &items)? } else { (items, None) };
                    self.engine.backend_mut().set_vector_motion(Some(layer_render::VectorMotion { owner: layer, items: items.clone(), transform: translation([0.; 2]) }));
                    self.vector_moves.gesture = Some(Gesture::Drag { layer, items, start: point, offset: [0.; 2], nudge: None, original });
                }
                Ok(true)
            }
            PenPhase::Move | PenPhase::Up => {
                let modifiers = self.interaction.modifiers;
                let Some(gesture) = &mut self.vector_moves.gesture else { return Ok(false) };
                let mut motion = None;
                match gesture {
                    Gesture::Drag { layer, items, start, offset, .. } => {
                        *offset = [point[0] - start[0], point[1] - start[1]];
                        motion = Some(layer_render::VectorMotion { owner: *layer, items: items.clone(), transform: translation(*offset) });
                    }
                    Gesture::Marquee { end, .. } => *end = point,
                    Gesture::Transform { latest, .. } if event.phase == PenPhase::Move => *latest = Some((point, modifiers)),
                    Gesture::Transform { .. } => self.transform_vector_items(point, modifiers),
                }
                if event.phase == PenPhase::Up { return self.finish_vector_move().map(|()| true); }
                if let Some(motion) = motion { self.engine.backend_mut().set_vector_motion(Some(motion)); }
                Ok(true)
            }
            PenPhase::Cancel => Ok(self.cancel_vector_move()),
            _ => Ok(self.vector_moves.gesture.is_some()),
        }
    }
    fn preview_vector_copies(&mut self, layer: OccurrenceHandle, items: &BTreeSet<ItemId>) -> Result<(Arc<BTreeSet<ItemId>>, Option<VectorObject>), String> {
        let doc = self.engine.document();
        let object = doc.scene().vector_layer(layer).ok_or("The vector layer was removed")?.clone();
        let (copies, records) = object.copies(items, &object);
        let edit = doc.vector_object_edit(layer, object.insert_front(records)).map_err(|e| e.to_string())?;
        self.engine.preview_edit(edit).map_err(|e| e.to_string())?;
        Ok((Arc::new(copies), Some(object)))
    }
    fn restore_vector_original(&mut self, layer: OccurrenceHandle, original: Option<VectorObject>) -> Result<(), String> {
        let Some(original) = original else { return Ok(()) };
        let edit = self.engine.document().vector_object_edit(layer, original).map_err(|e| e.to_string())?;
        self.engine.preview_edit(edit).map_err(|e| e.to_string())
    }
    pub(super) fn vector_nudge(&mut self, key: &str, pressed: bool, allowed: bool, modifiers: Modifiers) -> Result<bool, String> {
        let held = match &self.vector_moves.gesture { Some(Gesture::Drag { nudge: Some(held), .. }) => Some(held.clone()), _ => None };
        if !pressed { return if held.as_deref() == Some(key) { self.finish_vector_move().map(|()| true) } else { Ok(false) }; }
        if allowed && key == "escape" && held.is_some() { return Ok(self.cancel_vector_move()); }
        if allowed && !modifiers.command && !modifiers.alt && !modifiers.shift && self.vector_moves.gesture.is_none() && self.selected_vector_items().is_some() && self.editing_vector_layer().is_some() {
            match (key, self.layer_interaction.tool) {
                ("escape", LayerCanvasTool::EditPoints) | ("enter", LayerCanvasTool::Move) => {
                    let tool = if key == "enter" { LayerCanvasTool::EditPoints } else { LayerCanvasTool::Move };
                    return self.object_tool(tool).map(|()| true);
                }
                ("escape", LayerCanvasTool::Move) => return self.deselect_vector_items().map(|()| true),
                _ => {}
            }
        }
        let step = if modifiers.shift { 10. } else { 1. };
        let delta = match key { "arrowleft" => [-step, 0.], "arrowright" => [step, 0.], "arrowup" => [0., -step], "arrowdown" => [0., step], _ => return Ok(false) };
        if !allowed || modifiers.command || modifiers.alt || (self.vector_moves.gesture.is_some() && held.is_none()) || self.vector_move_layer().is_none() || !self.vector_paths_editable() { return Ok(false); }
        if held.as_deref().is_some_and(|held| held != key) { self.finish_vector_move()?; }
        let selection = self.selected_vector_items().cloned().ok_or("Select paths first")?;
        let gesture = self.vector_moves.gesture.get_or_insert_with(|| Gesture::Drag { layer: selection.layer, items: selection.items.clone(), start: [0.; 2], offset: [0.; 2], nudge: None, original: None });
        let Gesture::Drag { layer, items, offset, nudge, .. } = gesture else { return Ok(false) };
        *nudge = Some(key.into());
        *offset = [offset[0] + delta[0], offset[1] + delta[1]];
        let motion = layer_render::VectorMotion { owner: *layer, items: items.clone(), transform: translation(*offset) };
        self.engine.backend_mut().set_vector_motion(Some(motion));
        Ok(true)
    }
    pub(super) fn cancel_vector_move(&mut self) -> bool {
        let Some(gesture) = self.vector_moves.gesture.take() else { return false };
        self.engine.backend_mut().set_vector_motion(None);
        if let Gesture::Drag { layer, original, .. } = gesture { let _ = self.restore_vector_original(layer, original); }
        self.layer_interaction.changed = true;
        true
    }
    fn finish_vector_move(&mut self) -> Result<(), String> {
        let Some(gesture) = self.vector_moves.gesture.take() else { return Ok(()) };
        let result = match gesture {
            Gesture::Drag { layer, items, offset, original, .. } if offset != [0.; 2] => (|| {
                let object = self.engine.document().scene().vector_layer(layer).ok_or("The vector layer was removed")?.clone();
                let moved = object.transform_items(&items, Affine64([1., 0., 0., 1., offset[0], offset[1]]), true).map_err(str::to_string)?;
                let copied = original.is_some();
                self.restore_vector_original(layer, original)?;
                let edit = self.engine.document().vector_object_edit(layer, moved).map_err(|e| e.to_string())?;
                self.layer_edit(edit)?;
                if copied { self.select_vector_items(layer, (*items).clone())?; }
                Ok(())
            })(),
            Gesture::Drag { layer, original, .. } => self.restore_vector_original(layer, original),
            Gesture::Transform { layer, paths, original, delta: Some(delta), .. } => (|| {
                let [ox, oy] = self.engine.document().scene().occurrence_offset64(layer);
                let local = translation([-ox, -oy]).compose(delta).compose(translation([ox, oy]));
                let transformed = original.transform_paths(&paths, local, !self.state.settings.keep_outline_widths).map_err(str::to_string)?;
                let edit = self.engine.document().vector_object_edit(layer, transformed).map_err(|e| e.to_string())?;
                self.layer_edit(edit)
            })(),
            Gesture::Transform { .. } => Ok(()),
            Gesture::Marquee { layer, start, end, adding } => {
                let reach = CLICK * self.document_per_surface_pixel();
                let doc = self.engine.document();
                let offset = doc.scene().occurrence_offset64(layer);
                let area = [start[0].min(end[0]) - offset[0], start[1].min(end[1]) - offset[1], start[0].max(end[0]) - offset[0], start[1].max(end[1]) - offset[1]];
                let swept = area[2] - area[0] > reach || area[3] - area[1] > reach;
                let found = match doc.scene().vector_layer(layer) { Some(object) if swept => self.vector_moves.index.within(object, area), _ => BTreeSet::new() };
                let kept = if adding { self.selected_vector_items().map(|s| (*s.items).clone()).unwrap_or_default() } else { BTreeSet::new() };
                self.select_vector_items(layer, kept.into_iter().chain(found).collect()).map(|_| ())
            }
        };
        self.engine.backend_mut().set_vector_motion(None);
        self.layer_interaction.changed = true;
        result
    }
    pub(super) fn selected_open_end(&self, layer: OccurrenceHandle, point: [f64; 2], reach: f64) -> Option<(PathItem, bool)> {
        let selection = self.selected_vector_items().filter(|selection| selection.layer == layer)?;
        let object = self.engine.document().scene().vector_layer(layer)?;
        let near = |p: [f64; 2]| (p[0] - point[0]).hypot(p[1] - point[1]);
        object.selected(&selection.items).into_iter().filter_map(|path| {
            let [start, end] = path.open_ends()?;
            let (distance, at_end) = if near(end) <= near(start) { (near(end), true) } else { (near(start), false) };
            (distance <= reach).then_some((distance, path, at_end))
        }).min_by(|a, b| a.0.total_cmp(&b.0)).map(|(_, path, at_end)| (path, at_end))
    }
    pub(super) fn continue_path(&self, layer: OccurrenceHandle, original: &PathItem, added: &PathItem, close: bool) -> Result<layer_core::Edit, String> {
        let joined = original.joined(added, HIT_SLOP * self.document_per_surface_pixel()).map_err(str::to_string)?;
        let joined = if close { joined.closed().map_err(str::to_string)? } else { joined };
        let document = self.engine.document();
        let object = document.scene().vector_layer(layer).ok_or("The vector layer was removed")?;
        let position = object.find(original.id).ok_or("The path was removed")?;
        document.vector_object_edit(layer, object.replace([(position, Some(layer_core::authored::vector::Record::Path(joined)))])).map_err(|e| e.to_string())
    }
    fn deselect_vector_items(&mut self) -> Result<(), String> {
        let mut working = self.engine.document().working.clone();
        working.vector_items = None;
        working.selection = None;
        self.layer_edit(Edit::Working(working))?;
        self.layer_interaction.changed = true;
        Ok(())
    }
    pub(super) fn vector_command_enabled(&self, id: CommandId) -> Option<bool> {
        let layer = self.editing_vector_layer()?;
        let idle = self.require_document_idle().is_ok();
        let selected = self.selected_vector_items().is_some();
        Some(match id {
            CommandId::SelectAll if self.engine.document().scene().vector_layer(layer).is_some_and(|object| object.paths().any(|(_, path)| path.pickable())) => idle,
            CommandId::Deselect if selected => idle,
            CommandId::ClearSelected if selected => idle && self.vector_paths_editable(),
            CommandId::ScaleRotate | CommandId::TransformFlipHorizontal | CommandId::TransformFlipVertical
            | CommandId::TransformRotateLeft | CommandId::TransformRotateRight | CommandId::ReversePath => idle && self.vector_paths_editable(),
            CommandId::JoinPaths | CommandId::ClosePath | CommandId::MakeCompoundPath | CommandId::ReleaseCompoundPath => idle && self.vector_paths_editable() && {
                let paths = self.selected_path_facts().unwrap_or_default();
                match id {
                    CommandId::JoinPaths => (paths.count == 1 && paths.closable) || (paths.count == 2 && paths.joinable == 2),
                    CommandId::ClosePath => paths.closable,
                    CommandId::MakeCompoundPath => paths.count > 1,
                    _ => paths.compound,
                }
            },
            CommandId::SplitPath => idle && self.vector_paths_editable() && self.vector_points_split(),
            _ => return None,
        })
    }
    pub(super) fn vector_disabled_reason(&self, id: CommandId) -> Option<Arc<str>> {
        let layer = self.editing_vector_layer()?;
        if self.vector_command_enabled(id)? || self.require_document_idle().is_err() { return None; }
        let l = self.localization();
        Some(l.text(if self.engine.document().is_locked(layer) { MessageId::COMMANDS_THE_ACTIVE_LAYER_IS_LOCKED }
            else if self.selected_vector_items().is_none() { MessageId::COMMANDS_SELECT_PATHS_ON_A_VECTOR_LAYER }
            else {
                match id {
                    CommandId::JoinPaths => MessageId::COMMANDS_SELECT_OPEN_PATHS_TO_JOIN,
                    CommandId::ClosePath => MessageId::COMMANDS_SELECT_AN_OPEN_PATH_TO_CLOSE,
                    CommandId::SplitPath => MessageId::COMMANDS_SELECT_AN_ANCHOR_INSIDE_A_PATH_TO_SPLIT,
                    CommandId::MakeCompoundPath => MessageId::COMMANDS_SELECT_PATHS_TO_MAKE_A_COMPOUND_PATH,
                    CommandId::ReleaseCompoundPath => MessageId::COMMANDS_SELECT_A_COMPOUND_PATH_TO_RELEASE,
                    _ => return None,
                }
            }))
    }
    pub(super) fn vector_command_label(&self, id: CommandId) -> Option<Arc<str>> {
        (id == CommandId::ClearSelected && self.selected_vector_items().is_some())
            .then(|| self.localization().text(MessageId::COMMANDS_DELETE_SELECTED_PATHS))
    }
    pub(super) fn vector_command(&mut self, id: CommandId) -> Option<Result<(), String>> {
        let layer = self.editing_vector_layer()?;
        if !self.vector_command_enabled(id)? {
            return Some(Err(self.vector_disabled_reason(id).map_or_else(|| self.localization().text(MessageId::COMMANDS_UNAVAILABLE_IN_THE_CURRENT_TOOL_OR_EDIT_TARGET).to_string(), |reason| reason.to_string())));
        }
        let quarter = |linear: [f64; 4]| move |pivot: [f64; 2]| about(pivot, linear);
        let turn = match id {
            CommandId::TransformFlipHorizontal => Some(quarter([-1., 0., 0., 1.])),
            CommandId::TransformFlipVertical => Some(quarter([1., 0., 0., -1.])),
            CommandId::TransformRotateLeft => Some(quarter([0., -1., 1., 0.])),
            CommandId::TransformRotateRight => Some(quarter([0., 1., -1., 0.])),
            _ => None,
        };
        Some(match (id, turn) {
            (_, Some(turn)) => (|| {
                let frame = self.vector_frame().ok_or("Select paths first")?;
                let items = self.selected_vector_items().ok_or("Select paths first")?.items.clone();
                let doc = self.engine.document();
                let [ox, oy] = doc.scene().occurrence_offset64(layer);
                let local = translation([-ox, -oy]).compose(turn(frame.pivot)).compose(translation([ox, oy]));
                let object = doc.scene().vector_layer(layer).ok_or("The vector layer was removed")?;
                let turned = object.transform_items(&items, local, true).map_err(str::to_string)?;
                if turned == *object { return Ok(()); }
                let edit = doc.vector_object_edit(layer, turned).map_err(|e| e.to_string())?;
                self.layer_edit(edit)
            })(),
            (CommandId::SelectAll, _) => {
                let all = self.engine.document().scene().vector_layer(layer).map(|object| object.paths().filter(|(_, path)| path.pickable()).map(|(_, path)| path.id).collect()).unwrap_or_default();
                self.select_vector_items(layer, all).and_then(|_| self.object_tool(LayerCanvasTool::Move))
            }
            (CommandId::Deselect, _) => self.deselect_vector_items(),
            (CommandId::ScaleRotate, _) => self.object_tool(LayerCanvasTool::Move),
            (CommandId::JoinPaths | CommandId::ClosePath | CommandId::SplitPath | CommandId::ReversePath | CommandId::MakeCompoundPath | CommandId::ReleaseCompoundPath, _) => self.vector_path_operation(layer, id),
            _ => return None,
        })
    }
    fn vector_path_operation(&mut self, layer: OccurrenceHandle, id: CommandId) -> Result<(), String> {
        let items = self.selected_vector_items().ok_or("Select paths first")?.items.clone();
        let tolerance = HIT_SLOP * self.document_per_surface_pixel();
        let cuts = if id == CommandId::SplitPath { self.take_selected_vector_points() } else { Default::default() };
        let object = self.engine.document().scene().vector_layer(layer).ok_or("The vector layer was removed")?;
        let Some((changed, selection)) = (match id {
            CommandId::JoinPaths => object.join_paths(&items, tolerance)?.map(|(o, id)| (o, BTreeSet::from([id]))),
            CommandId::MakeCompoundPath => object.compound_paths(&items)?.map(|(o, id)| (o, BTreeSet::from([id]))),
            CommandId::ReleaseCompoundPath => object.release_paths(&items)?,
            CommandId::SplitPath => object.split_paths(&cuts)?,
            CommandId::ClosePath => object.map_paths(&items, |path| path.can_close().then(|| path.closed()).transpose())?.map(|o| (o, (*items).clone())),
            _ => object.map_paths(&items, |path| path.reversed().map(|reversed| (reversed != *path).then_some(reversed)))?.map(|o| (o, (*items).clone())),
        }) else { return Ok(()) };
        let edit = self.engine.document().vector_object_edit(layer, changed).map_err(|e| e.to_string())?;
        self.layer_edit(edit)?;
        self.select_vector_items(layer, selection).map(|_| ())
    }
    pub(super) fn vector_tool_context_changed(&mut self) -> bool {
        let context = (self.layer_interaction.tool == LayerCanvasTool::Move && self.selected_vector_items().is_some(), self.vector_erasing_layer(false).is_some());
        std::mem::replace(&mut self.vector_moves.context, context) != context
    }
    pub(super) fn vector_paths_editable(&self) -> bool {
        self.selected_vector_items().is_some_and(|selection| !self.engine.document().is_locked(selection.layer))
    }
    pub(super) fn arrange_vector_items(&mut self, arrange: layer_core::authored::vector::Arrange) -> Result<(), String> {
        let Some(selection) = self.selected_vector_items().cloned() else { return Ok(()) };
        let doc = self.engine.document();
        let object = doc.scene().vector_layer(selection.layer).ok_or("The vector layer was removed")?;
        let arranged = object.arrange(&selection.items, arrange);
        if arranged.same_root(object) { return Ok(()); }
        let edit = doc.vector_object_edit(selection.layer, arranged).map_err(|e| e.to_string())?;
        self.layer_edit(edit)
    }
    pub(super) fn delete_selected_vector_items(&mut self) -> Result<bool, String> {
        let Some(selection) = self.selected_vector_items().cloned() else { return Ok(false) };
        let doc = self.engine.document();
        let object = doc.scene().vector_layer(selection.layer).ok_or("The vector layer was removed")?;
        let changes: Vec<_> = object.positions(&selection.items).into_iter()
            .flat_map(|(chunk, indices)| indices.into_iter().map(move |index| (layer_core::authored::vector::Position { chunk: chunk as u32, index: index as u32 }, None))).collect();
        let edit = doc.vector_object_edit(selection.layer, object.replace(changes)).map_err(|e| e.to_string())?;
        self.layer_edit(edit)?;
        Ok(true)
    }
    pub(super) fn append_vector_move_overlay(&self, segments: &mut Vec<CursorSegment>) {
        if self.vector_move_layer().is_none() { return; }
        let map = self.document_to_logical();
        let point = |x: f64, y: f64| map(Point { x: x as f32, y: y as f32 });
        let mut rect = |[x0, y0, x1, y1]: [f64; 4]| {
            let corners = [point(x0, y0), point(x1, y0), point(x1, y1), point(x0, y1)];
            for i in 0..4 { segments.push(CursorSegment { from: corners[i], to: corners[(i + 1) % 4], distance: 0., marker: 0., scale: 1. }); }
        };
        if let Some(Gesture::Marquee { start, end, .. }) = &self.vector_moves.gesture {
            rect([start[0].min(end[0]), start[1].min(end[1]), start[0].max(end[0]), start[1].max(end[1])]);
            return;
        }
        let (frame, idle) = match &self.vector_moves.gesture {
            Some(Gesture::Transform { frame, delta, .. }) => (delta.map_or(*frame, |delta| frame.moved(delta)), false),
            Some(Gesture::Drag { offset, .. }) => match self.vector_frame() { Some(frame) => (frame.moved(translation(*offset)), false), None => return },
            _ => match self.vector_frame() { Some(frame) => (frame, true), None => return },
        };
        let corners = [[0., 0.], [1., 0.], [1., 1.], [0., 1.]].map(|p| { let [x, y] = frame.unit.map(p); point(x, y) });
        for i in 0..4 { segments.push(CursorSegment { from: corners[i], to: corners[(i + 1) % 4], distance: 0., marker: 0., scale: 1. }); }
        if idle && self.vector_paths_editable() { frame.append_handles(f64::from(self.ruler_reach()), |[x, y]| point(x, y), false, segments); }
    }
}
