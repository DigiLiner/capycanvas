use super::*;
use layer_core::authored::vector::{Cap, ItemId, Join, Paint, PaintEntry, PathItem, Record, Stroke, Style};
use layer_engine::{SampleFlags, VectorStroke};
use std::sync::Arc;

struct Line { layer: OccurrenceHandle, stroke: VectorStroke, paint: Paint, tolerance: f64 }

#[derive(Default)]
pub(super) struct VectorLines { line: Option<Line>, revision: u64 }
impl VectorLines {
    pub fn active(&self) -> bool { self.line.is_some() }
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
        self.engine.configured_brush().draws_vector_lines().then_some(layer)
    }
    pub(super) fn vector_line_pen(&mut self, event: PenEvent) -> Result<(), String> {
        if event.flags.contains(SampleFlags::PREDICTED) || event.flags.contains(SampleFlags::CORRECTION) { return Ok(()); }
        match event.phase {
            PenPhase::Down => {
                self.cancel_vector_line();
                let Some(layer) = self.vector_drawing_layer() else { return Ok(()) };
                if self.engine.document().is_locked(layer) { return Err(self.localization().text(MessageId::COMMANDS_THE_ACTIVE_LAYER_IS_LOCKED).to_string()); }
                let brush = self.engine.configured_brush().clone();
                let space = self.engine.document().composition().color.space;
                let [r, g, b, a] = brush.color_rgba_linear;
                let color = layer_core::color::RgbColor::from_linear(space, [r, g, b, (a * brush.opacity).clamp(0., 1.)]).map_err(|e| e.to_string())?;
                let transform = self.state.camera.input_transform();
                let [m0, m1, m2, m3, _, _] = transform.surface_to_document;
                let scale = f64::from((m0 * m3 - m1 * m2).abs().sqrt()).max(1e-6);
                let stroke = VectorStroke::new(self.vector_lines.revision + 1, brush, transform, self.engine.configured_pressure_curve().clone());
                self.vector_lines.line = Some(Line { layer, stroke, paint: Paint::color(color), tolerance: 0.25 * scale });
                self.push_vector_sample(event);
            }
            PenPhase::Move => self.push_vector_sample(event),
            PenPhase::Up => {
                self.push_vector_sample(event);
                self.finish_vector_line()?;
            }
            PenPhase::Cancel => self.cancel_vector_line(),
            _ => {}
        }
        Ok(())
    }
    fn push_vector_sample(&mut self, event: PenEvent) {
        let Some(line) = &mut self.vector_lines.line else { return };
        if !line.stroke.push(event) { return; }
        let samples = line.stroke.preview();
        let Some(preview) = layer_engine::polyline(&samples) else { return };
        self.vector_lines.revision += 1;
        let preview = layer_render::VectorPreview { owner: line.layer, path: Arc::new(path(ItemId(1), &line.paint, preview)), revision: self.vector_lines.revision };
        self.engine.backend_mut().set_vector_preview(Some(preview));
    }
    pub(super) fn cancel_vector_line(&mut self) {
        if self.vector_lines.line.take().is_some() { self.engine.backend_mut().set_vector_preview(None); }
    }
    fn finish_vector_line(&mut self) -> Result<(), String> {
        let Some(line) = self.vector_lines.line.take() else { return Ok(()) };
        let samples = line.stroke.finish();
        let result = (|| {
            let Some(fitted) = layer_engine::fit_line(&samples, line.tolerance) else { return Ok(()) };
            let document = self.engine.document();
            let object = document.scene().vector_layer(line.layer).ok_or("The vector layer was removed")?;
            let ids = object.ids();
            let id = object.allocate_id(|id| ids.contains(&id));
            let drawn = object.insert_front(vec![Record::Path(path(id, &line.paint, fitted))]);
            let edit = document.vector_object_edit(line.layer, drawn).map_err(|e| e.to_string())?;
            self.layer_edit(edit)
        })();
        self.engine.backend_mut().set_vector_preview(None);
        result
    }
}
