use super::*;
use layer_core::{Edit, Point, authored::{Affine64, VectorItemSelection, vector::{ItemId, VectorIndex, VectorObject}}};
use layer_render::CursorSegment;
use std::{cell::RefCell, collections::BTreeSet, sync::Arc};
use super::transform_frame::{Frame, FrameHandle, translation};

const HIT_SLOP: f64 = 6.;
const CLICK: f64 = 3.;

enum Gesture {
    Drag { layer: OccurrenceHandle, items: Arc<BTreeSet<ItemId>>, start: [f64; 2], offset: [f64; 2], nudge: Option<String>, original: Option<VectorObject> },
    Marquee { layer: OccurrenceHandle, start: [f64; 2], end: [f64; 2], adding: bool },
    Transform { layer: OccurrenceHandle, items: Arc<BTreeSet<ItemId>>, frame: Frame, handle: FrameHandle, press: [f64; 2], original: VectorObject, transformed: Option<VectorObject> },
}

type SelectionBounds = (VectorObject, Arc<BTreeSet<ItemId>>, Option<[f64; 4]>);

#[derive(Default)]
pub(super) struct VectorMoves {
    gesture: Option<Gesture>,
    bounds: RefCell<Option<SelectionBounds>>,
    pub(super) index: VectorIndex,
    context: (bool, bool),
}

impl<R: CanvasRenderer> UiSession<R> {
    fn vector_move_layer(&self) -> Option<OccurrenceHandle> {
        let doc = self.engine.document();
        let layer = doc.working.occurrence?;
        (self.layer_interaction.tool == LayerCanvasTool::Move && doc.working.selection.is_none() && self.selection_masks.target().is_none()
            && doc.scene().vector_layer(layer).is_some()).then_some(layer)
    }
    pub(super) fn selected_vector_items(&self) -> Option<&VectorItemSelection> {
        let doc = self.engine.document();
        doc.working.vector_items.as_ref().filter(|selection| Some(selection.layer) == doc.working.occurrence)
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
    fn vector_frame(&self) -> Option<Frame> {
        let selection = self.selected_vector_items()?;
        let doc = self.engine.document();
        let object = doc.scene().vector_layer(selection.layer)?;
        let mut cached = self.vector_moves.bounds.borrow_mut();
        if !cached.as_ref().is_some_and(|(root, items, _)| root.same_root(object) && Arc::ptr_eq(items, &selection.items)) {
            *cached = Some((object.clone(), selection.items.clone(), object.items_bounds(&selection.items)));
        }
        let [x0, y0, x1, y1] = cached.as_ref().and_then(|(_, _, bounds)| *bounds)?;
        let [ox, oy] = doc.scene().occurrence_offset64(selection.layer);
        let (min, size) = ([x0 + ox, y0 + oy], [(x1 - x0).max(1e-3), (y1 - y0).max(1e-3)]);
        Some(Frame { unit: Affine64([size[0], 0., 0., size[1], min[0], min[1]]), pivot: [min[0] + size[0] * 0.5, min[1] + size[1] * 0.5] })
    }
    fn begin_vector_transform(&mut self, layer: OccurrenceHandle, point: [f64; 2]) -> Option<Gesture> {
        let frame = self.vector_frame()?;
        let handle = frame.hit(point, f64::from(self.ruler_reach())).filter(|handle| matches!(handle, FrameHandle::Scale(_) | FrameHandle::Rotate))?;
        let items = self.selected_vector_items()?.items.clone();
        let original = self.engine.document().scene().vector_layer(layer)?.clone();
        Some(Gesture::Transform { layer, items, frame, handle, press: point, original, transformed: None })
    }
    fn transform_vector_items(&mut self, point: [f64; 2]) -> Result<(), String> {
        let modifiers = self.interaction.modifiers;
        let Some(Gesture::Transform { layer, items, frame, handle, press, original, transformed }) = &mut self.vector_moves.gesture else { return Ok(()) };
        let Some(delta) = frame.delta(*handle, *press, point, modifiers.shift, false, modifiers.alt) else { return Ok(()) };
        let [ox, oy] = self.engine.document().scene().occurrence_offset64(*layer);
        let local = translation([-ox, -oy]).compose(delta).compose(translation([ox, oy]));
        let result = original.transform_items(items, local).map_err(str::to_string)?;
        *transformed = Some(result.clone());
        let edit = self.engine.document().vector_object_edit(*layer, result).map_err(|e| e.to_string())?;
        self.engine.preview_edit(edit).map_err(|e| e.to_string())
    }
    fn document_per_surface_pixel(&self) -> f64 {
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
                let items = match (selected.contains(&id), adding) {
                    (true, false) => selected,
                    (true, true) => selected.into_iter().filter(|item| *item != id).collect(),
                    (false, true) => selected.into_iter().chain([id]).collect(),
                    (false, false) => [id].into(),
                };
                let items = self.select_vector_items(layer, items)?;
                if items.contains(&id) {
                    let (items, original) = if self.interaction.modifiers.alt { self.preview_vector_copies(layer, &items)? } else { (items, None) };
                    self.engine.backend_mut().set_vector_motion(Some(layer_render::VectorMotion { owner: layer, items: items.clone(), offset: [0.; 2] }));
                    self.vector_moves.gesture = Some(Gesture::Drag { layer, items, start: point, offset: [0.; 2], nudge: None, original });
                }
                Ok(true)
            }
            PenPhase::Move | PenPhase::Up => {
                let Some(gesture) = &mut self.vector_moves.gesture else { return Ok(false) };
                let mut motion = None;
                match gesture {
                    Gesture::Drag { layer, items, start, offset, .. } => {
                        *offset = [point[0] - start[0], point[1] - start[1]];
                        motion = Some(layer_render::VectorMotion { owner: *layer, items: items.clone(), offset: *offset });
                    }
                    Gesture::Marquee { end, .. } => *end = point,
                    Gesture::Transform { .. } => self.transform_vector_items(point)?,
                }
                self.layer_interaction.changed = true;
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
        let step = if modifiers.shift { 10. } else { 1. };
        let delta = match key { "arrowleft" => [-step, 0.], "arrowright" => [step, 0.], "arrowup" => [0., -step], "arrowdown" => [0., step], _ => return Ok(false) };
        if !allowed || modifiers.command || modifiers.alt || (self.vector_moves.gesture.is_some() && held.is_none()) || self.vector_move_layer().is_none() || !self.vector_paths_editable() { return Ok(false); }
        if held.as_deref().is_some_and(|held| held != key) { self.finish_vector_move()?; }
        let selection = self.selected_vector_items().cloned().ok_or("Select paths first")?;
        let gesture = self.vector_moves.gesture.get_or_insert_with(|| Gesture::Drag { layer: selection.layer, items: selection.items.clone(), start: [0.; 2], offset: [0.; 2], nudge: None, original: None });
        let Gesture::Drag { layer, items, offset, nudge, .. } = gesture else { return Ok(false) };
        *nudge = Some(key.into());
        *offset = [offset[0] + delta[0], offset[1] + delta[1]];
        let motion = layer_render::VectorMotion { owner: *layer, items: items.clone(), offset: *offset };
        self.engine.backend_mut().set_vector_motion(Some(motion));
        self.layer_interaction.changed = true;
        Ok(true)
    }
    pub(super) fn cancel_vector_move(&mut self) -> bool {
        let Some(gesture) = self.vector_moves.gesture.take() else { return false };
        match gesture {
            Gesture::Drag { layer, original, .. } => {
                self.engine.backend_mut().set_vector_motion(None);
                let _ = self.restore_vector_original(layer, original);
            }
            Gesture::Transform { layer, original, transformed: Some(_), .. } => { let _ = self.restore_vector_original(layer, Some(original)); }
            _ => {}
        }
        self.layer_interaction.changed = true;
        true
    }
    fn finish_vector_move(&mut self) -> Result<(), String> {
        let Some(gesture) = self.vector_moves.gesture.take() else { return Ok(()) };
        let result = match gesture {
            Gesture::Drag { layer, items, offset, original, .. } if offset != [0.; 2] => (|| {
                let object = self.engine.document().scene().vector_layer(layer).ok_or("The vector layer was removed")?.clone();
                let moved = object.transform_items(&items, Affine64([1., 0., 0., 1., offset[0], offset[1]])).map_err(str::to_string)?;
                let copied = original.is_some();
                self.restore_vector_original(layer, original)?;
                let edit = self.engine.document().vector_object_edit(layer, moved).map_err(|e| e.to_string())?;
                self.layer_edit(edit)?;
                if copied { self.select_vector_items(layer, (*items).clone())?; }
                Ok(())
            })(),
            Gesture::Drag { layer, original, .. } => self.restore_vector_original(layer, original),
            Gesture::Transform { layer, original, transformed: Some(transformed), .. } => (|| {
                self.restore_vector_original(layer, Some(original))?;
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
            Some(Gesture::Transform { frame, .. }) => (self.vector_frame().unwrap_or(*frame), false),
            Some(Gesture::Drag { offset, .. }) => match self.vector_frame() { Some(frame) => (frame.moved(translation(*offset)), false), None => return },
            _ => match self.vector_frame() { Some(frame) => (frame, true), None => return },
        };
        let corners = [[0., 0.], [1., 0.], [1., 1.], [0., 1.]].map(|p| { let [x, y] = frame.unit.map(p); point(x, y) });
        for i in 0..4 { segments.push(CursorSegment { from: corners[i], to: corners[(i + 1) % 4], distance: 0., marker: 0., scale: 1. }); }
        if idle && self.vector_paths_editable() { frame.append_handles(f64::from(self.ruler_reach()), |[x, y]| point(x, y), false, segments); }
    }
}
