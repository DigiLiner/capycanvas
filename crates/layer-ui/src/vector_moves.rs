use super::*;
use layer_core::{Edit, Point, authored::{Affine64, VectorItemSelection, vector::{ItemId, VectorIndex, VectorObject}}};
use layer_render::CursorSegment;
use std::{cell::RefCell, collections::BTreeSet, sync::Arc};

const HIT_SLOP: f64 = 6.;
const CLICK: f64 = 3.;

enum Gesture {
    Drag { layer: OccurrenceHandle, items: Arc<BTreeSet<ItemId>>, start: [f64; 2], offset: [f64; 2] },
    Marquee { layer: OccurrenceHandle, start: [f64; 2], end: [f64; 2], adding: bool },
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
                    self.engine.backend_mut().set_vector_motion(Some(layer_render::VectorMotion { owner: layer, items: items.clone(), offset: [0.; 2] }));
                    self.vector_moves.gesture = Some(Gesture::Drag { layer, items, start: point, offset: [0.; 2] });
                }
                Ok(true)
            }
            PenPhase::Move | PenPhase::Up => {
                let Some(gesture) = &mut self.vector_moves.gesture else { return Ok(false) };
                let mut motion = None;
                match gesture {
                    Gesture::Drag { layer, items, start, offset } => {
                        *offset = [point[0] - start[0], point[1] - start[1]];
                        motion = Some(layer_render::VectorMotion { owner: *layer, items: items.clone(), offset: *offset });
                    }
                    Gesture::Marquee { end, .. } => *end = point,
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
    pub(super) fn cancel_vector_move(&mut self) -> bool {
        let Some(gesture) = self.vector_moves.gesture.take() else { return false };
        if matches!(gesture, Gesture::Drag { .. }) { self.engine.backend_mut().set_vector_motion(None); }
        self.layer_interaction.changed = true;
        true
    }
    fn finish_vector_move(&mut self) -> Result<(), String> {
        let Some(gesture) = self.vector_moves.gesture.take() else { return Ok(()) };
        let result = match gesture {
            Gesture::Drag { layer, items, offset, .. } if offset != [0.; 2] => (|| {
                let doc = self.engine.document();
                let object = doc.scene().vector_layer(layer).ok_or("The vector layer was removed")?;
                let moved = object.transform_items(&items, Affine64([1., 0., 0., 1., offset[0], offset[1]])).map_err(str::to_string)?;
                let edit = doc.vector_object_edit(layer, moved).map_err(|e| e.to_string())?;
                self.layer_edit(edit)
            })(),
            Gesture::Drag { .. } => Ok(()),
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
        let Some(selection) = self.selected_vector_items() else { return };
        let doc = self.engine.document();
        let Some(object) = doc.scene().vector_layer(selection.layer) else { return };
        let mut cached = self.vector_moves.bounds.borrow_mut();
        if !cached.as_ref().is_some_and(|(root, items, _)| root.same_root(object) && Arc::ptr_eq(items, &selection.items)) {
            *cached = Some((object.clone(), selection.items.clone(), object.items_bounds(&selection.items)));
        }
        let Some(bounds) = cached.as_ref().and_then(|(_, _, bounds)| *bounds) else { return };
        let layer = doc.scene().occurrence_offset64(selection.layer);
        let drag = match &self.vector_moves.gesture { Some(Gesture::Drag { offset, .. }) => *offset, _ => [0.; 2] };
        let shift = [layer[0] + drag[0], layer[1] + drag[1]];
        rect([bounds[0] + shift[0], bounds[1] + shift[1], bounds[2] + shift[0], bounds[3] + shift[1]]);
    }
}
