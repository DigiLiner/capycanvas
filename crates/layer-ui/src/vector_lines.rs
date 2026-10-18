use super::*;
use layer_core::authored::vector::{Cap, Eraser, ItemId, Join, Paint, PaintEntry, PathItem, Record, Stroke, Style, VectorObject};
use layer_engine::{SampleFlags, VectorStroke};
use std::sync::Arc;

const LINE_TOLERANCE: f64 = 0.5;

struct Line { layer: OccurrenceHandle, stroke: VectorStroke, paint: Paint, tolerance: f64, stale: bool }
struct Drawn { line: Line, id: ItemId, checkpoint: u64 }
struct Erasing { layer: OccurrenceHandle, original: VectorObject, current: VectorObject, last: [f64; 2], radius: f64, whole: bool }

#[derive(Default)]
pub(super) struct VectorLines { line: Option<Line>, drawn: Option<Drawn>, erasing: Option<Erasing>, revision: u64 }
impl VectorLines {
    pub fn active(&self) -> bool { self.line.is_some() }
    pub fn erasing(&self) -> bool { self.erasing.is_some() }
    pub fn corrects(&self, event: &PenEvent) -> bool { self.drawn.as_ref().is_some_and(|drawn| drawn.line.stroke.corrects(event)) }
}

fn path(id: ItemId, paint: &Paint, line: (layer_core::authored::vector::PathGeometry, Option<layer_core::authored::vector::WidthProfile>, f64)) -> PathItem {
    let (geometry, profile, width) = line;
    let mut stroke = Stroke::new(paint.clone(), width);
    stroke.cap = Cap::Round;
    stroke.join = Join::Round;
    stroke.profile = profile.map(Arc::new);
    PathItem::new(id, Arc::new(geometry), Arc::new(Style::new(vec![PaintEntry::Stroke(stroke)])))
}

impl<R: CanvasRenderer> UiSession<R> {
    pub(super) fn vector_drawing_layer(&self) -> Option<OccurrenceHandle> {
        let document = self.engine.document();
        let layer = document.working.occurrence?;
        document.scene().vector_layer(layer)?;
        let erasing = self.state.brush.tool == Tool::Eraser || self.state.colors.transparent();
        (!erasing && self.engine.configured_brush().draws_vector_lines()).then_some(layer)
    }
    pub(super) fn erasing_event(&self, event: &PenEvent) -> bool {
        event.tool == layer_engine::ToolKind::Eraser || event.flags.contains(SampleFlags::INVERTED)
    }
    pub(super) fn vector_erasing_layer(&self, eraser_end: bool) -> Option<OccurrenceHandle> {
        let document = self.engine.document();
        let layer = document.working.occurrence?;
        document.scene().vector_layer(layer)?;
        let erasing = eraser_end || self.state.brush.tool == Tool::Eraser || self.state.colors.transparent();
        (erasing && self.layer_interaction.tool == LayerCanvasTool::Paint).then_some(layer)
    }
    pub(super) fn vector_erase_pen(&mut self, event: PenEvent) -> Result<(), String> {
        if event.flags.contains(SampleFlags::PREDICTED) || event.flags.contains(SampleFlags::CORRECTION) { return Ok(()); }
        let p = self.state.camera.input_transform().map(event.surface_position);
        let point = [f64::from(p.x), f64::from(p.y)];
        match event.phase {
            PenPhase::Down => {
                self.cancel_vector_erase();
                let Some(layer) = self.vector_erasing_layer(self.erasing_event(&event)) else { return Ok(()) };
                if self.engine.document().is_locked(layer) { return Err(self.localization().text(MessageId::COMMANDS_THE_ACTIVE_LAYER_IS_LOCKED).to_string()); }
                let document = self.engine.document();
                let object = document.scene().vector_layer(layer).ok_or("The vector layer was removed")?.clone();
                let offset = document.scene().occurrence_offset64(layer);
                let local = [point[0] - offset[0], point[1] - offset[1]];
                let radius = f64::from(self.engine.configured_brush().diameter).max(1.) * 0.5;
                self.vector_lines.erasing = Some(Erasing { layer, original: object.clone(), current: object, last: local, radius, whole: self.state.settings.erase_whole_paths });
                self.erase_vector_sample(point)
            }
            PenPhase::Move => self.erase_vector_sample(point),
            PenPhase::Up => {
                self.erase_vector_sample(point)?;
                self.finish_vector_erase()
            }
            PenPhase::Cancel => { self.cancel_vector_erase(); Ok(()) }
            PenPhase::Hover => Ok(()),
        }
    }
    fn erase_vector_sample(&mut self, point: [f64; 2]) -> Result<(), String> {
        let Some(erasing) = &mut self.vector_lines.erasing else { return Ok(()) };
        let offset = self.engine.document().scene().occurrence_offset64(erasing.layer);
        let local = [point[0] - offset[0], point[1] - offset[1]];
        let eraser = Eraser { from: erasing.last, to: local, radius: erasing.radius };
        erasing.last = local;
        let candidates = self.vector_moves.index.near(&erasing.current, eraser.bounds());
        let Some(erased) = erasing.current.erase(&candidates, &eraser, erasing.whole) else { return Ok(()) };
        erasing.current = erased.clone();
        let layer = erasing.layer;
        let edit = self.engine.document().vector_object_edit(layer, erased).map_err(|e| e.to_string())?;
        self.engine.preview_edit(edit).map_err(|e| e.to_string())?;
        self.layer_interaction.changed = true;
        Ok(())
    }
    fn finish_vector_erase(&mut self) -> Result<(), String> {
        let Some(erasing) = self.vector_lines.erasing.take() else { return Ok(()) };
        if erasing.current.same_root(&erasing.original) { return Ok(()); }
        let document = self.engine.document();
        self.engine.preview_edit(document.vector_object_edit(erasing.layer, erasing.original).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
        let edit = self.engine.document().vector_object_edit(erasing.layer, erasing.current).map_err(|e| e.to_string())?;
        self.layer_edit(edit)
    }
    pub(super) fn cancel_vector_erase(&mut self) {
        let Some(erasing) = self.vector_lines.erasing.take() else { return };
        if erasing.current.same_root(&erasing.original) { return; }
        if let Ok(edit) = self.engine.document().vector_object_edit(erasing.layer, erasing.original) { let _ = self.engine.preview_edit(edit); }
        self.layer_interaction.changed = true;
    }
    pub(super) fn vector_line_pen(&mut self, event: PenEvent) -> Result<(), String> {
        if event.flags.contains(SampleFlags::CORRECTION) { return self.correct_vector_line(event); }
        if event.flags.contains(SampleFlags::PREDICTED) { return Ok(()); }
        match event.phase {
            PenPhase::Down => {
                self.cancel_vector_line();
                self.vector_lines.drawn = None;
                let Some(layer) = self.vector_drawing_layer() else { return Ok(()) };
                if self.engine.document().is_locked(layer) { return Err(self.localization().text(MessageId::COMMANDS_THE_ACTIVE_LAYER_IS_LOCKED).to_string()); }
                let brush = self.engine.configured_brush();
                let space = self.engine.document().composition().color.space;
                let [r, g, b, a] = brush.color_rgba_linear;
                let color = layer_core::color::RgbColor::from_linear(space, [r, g, b, (a * brush.opacity).clamp(0., 1.)]).map_err(|e| e.to_string())?;
                let offset = self.engine.document().scene().occurrence_offset64(layer);
                let stroke = self.engine.begin_vector_stroke(self.vector_lines.revision + 1, event, offset);
                self.vector_lines.line = Some(Line { layer, stroke, paint: Paint::color(color), tolerance: LINE_TOLERANCE * self.document_per_surface_pixel(), stale: true });
            }
            PenPhase::Move => self.push_vector_sample(event),
            PenPhase::Up => {
                self.push_vector_sample(event);
                self.finish_vector_line()?;
            }
            PenPhase::Cancel => self.cancel_vector_line(),
            PenPhase::Hover => {}
        }
        Ok(())
    }
    fn push_vector_sample(&mut self, event: PenEvent) {
        let Some(line) = &mut self.vector_lines.line else { return };
        line.stale |= self.engine.push_vector_stroke(&mut line.stroke, event);
    }
    pub(super) fn sync_vector_line_preview(&mut self) {
        let Some(line) = &mut self.vector_lines.line else { return };
        if !std::mem::take(&mut line.stale) { return; }
        let Some(preview) = layer_engine::polyline(&line.stroke.preview()) else { return };
        self.vector_lines.revision += 1;
        let preview = layer_render::VectorPreview { owner: line.layer, path: Arc::new(path(ItemId(1), &line.paint, preview)), revision: self.vector_lines.revision };
        self.engine.backend_mut().set_vector_preview(Some(preview));
    }
    pub(super) fn cancel_vector_line(&mut self) {
        if self.vector_lines.line.take().is_some() { self.engine.backend_mut().set_vector_preview(None); }
    }
    fn finish_vector_line(&mut self) -> Result<(), String> {
        let Some(line) = self.vector_lines.line.take() else { return Ok(()) };
        let result = (|| {
            let Some(fitted) = layer_engine::fit_line(&line.stroke.samples(), line.tolerance) else { return Ok(()) };
            let document = self.engine.document();
            let object = document.scene().vector_layer(line.layer).ok_or("The vector layer was removed")?;
            let id = object.allocate_id();
            let drawn = object.insert_front(vec![Record::Path(path(id, &line.paint, fitted))]);
            let edit = document.vector_object_edit(line.layer, drawn).map_err(|e| e.to_string())?;
            self.layer_edit(edit)?;
            self.layer_interaction.changed = true;
            if line.stroke.awaits_corrections() { self.vector_lines.drawn = Some(Drawn { line, id, checkpoint: self.engine.checkpoint() }); }
            Ok(())
        })();
        self.engine.backend_mut().set_vector_preview(None);
        result
    }
    fn correct_vector_line(&mut self, event: PenEvent) -> Result<(), String> {
        if let Some(line) = &mut self.vector_lines.line {
            line.stale |= self.engine.push_vector_stroke(&mut line.stroke, event);
            return Ok(());
        }
        let Some(mut drawn) = self.vector_lines.drawn.take() else { return Ok(()) };
        if drawn.checkpoint != self.engine.checkpoint() || !self.engine.push_vector_stroke(&mut drawn.line.stroke, event) {
            if drawn.checkpoint == self.engine.checkpoint() && drawn.line.stroke.awaits_corrections() { self.vector_lines.drawn = Some(drawn); }
            return Ok(());
        }
        let Some(fitted) = layer_engine::fit_line(&drawn.line.stroke.samples(), drawn.line.tolerance) else { return Ok(()) };
        let document = self.engine.document();
        let object = document.scene().vector_layer(drawn.line.layer).ok_or("The vector layer was removed")?;
        let Some(position) = object.find(drawn.id) else { return Ok(()) };
        let corrected = object.replace([(position, Some(Record::Path(path(drawn.id, &drawn.line.paint, fitted))))]);
        let edit = document.vector_object_edit(drawn.line.layer, corrected).map_err(|e| e.to_string())?;
        self.engine.amend_edit(edit).map_err(|e| e.to_string())?;
        self.layer_interaction.changed = true;
        drawn.checkpoint = self.engine.checkpoint();
        if drawn.line.stroke.awaits_corrections() { self.vector_lines.drawn = Some(drawn); }
        Ok(())
    }
}
