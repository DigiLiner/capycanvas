use super::*;
use layer_core::{Point, authored::vector::{Anchor, AnchorKind, Cap, Join, PathGeometry, PathItem, Record, Subpath}};
use layer_engine::SampleFlags;
use layer_render::CursorSegment;
use std::sync::Arc;

const CLOSE_REACH: f64 = 6.;

struct Draft { layer: OccurrenceHandle, anchors: Vec<Anchor>, dragging: bool }

#[derive(Default)]
pub(super) struct VectorPaths { draft: Option<Draft>, revision: u64 }
impl VectorPaths {
    pub fn drafting(&self) -> bool { self.draft.is_some() }
}

impl<R: CanvasRenderer> UiSession<R> {
    fn vector_path_paint(&self) -> Option<FigurePaint> {
        if let LayerCanvasTool::Figure { shape: FigureShape::Path, paint } = self.layer_interaction.tool { Some(paint) } else { None }
    }
    pub(super) fn vector_path_pen(&mut self, event: PenEvent, p: Point) -> Result<bool, String> {
        if self.vector_path_paint().is_none() { return Ok(false); }
        if event.flags.contains(SampleFlags::PREDICTED) || event.flags.contains(SampleFlags::CORRECTION) { return Ok(true); }
        let Some(layer) = self.vector_figure_layer() else {
            self.cancel_vector_path();
            if event.phase == PenPhase::Down {
                let reason = if self.editing_vector_layer().is_some() { MessageId::COMMANDS_PATHS_NEED_A_COLOR } else { MessageId::COMMANDS_DRAW_PATHS_ON_A_VECTOR_LAYER };
                return Err(self.localization().text(reason).to_string());
            }
            return Ok(true);
        };
        let [ox, oy] = self.engine.document().scene().occurrence_offset64(layer);
        let point = [f64::from(p.x) - ox, f64::from(p.y) - oy];
        match event.phase {
            PenPhase::Down => {
                if self.engine.document().is_locked(layer) { return Err(self.localization().text(MessageId::COMMANDS_THE_ACTIVE_LAYER_IS_LOCKED).to_string()); }
                if self.vector_paths.draft.as_ref().is_some_and(|draft| draft.layer != layer) { self.cancel_vector_path(); }
                let reach = CLOSE_REACH * self.document_per_surface_pixel();
                let first = self.vector_paths.draft.as_ref().filter(|draft| draft.anchors.len() >= 2).map(|draft| draft.anchors[0].point());
                if first.is_some_and(|first| (first[0] - point[0]).hypot(first[1] - point[1]) <= reach) { return self.finish_vector_path(true).map(|()| true); }
                let last = self.vector_paths.draft.as_ref().and_then(|draft| draft.anchors.last()).map(Anchor::point);
                let point = match last.filter(|_| self.interaction.modifiers.shift) {
                    Some(last) => {
                        let end = FigureShape::Line.constrained_end(Point { x: last[0] as f32, y: last[1] as f32 }, Point { x: point[0] as f32, y: point[1] as f32 });
                        [f64::from(end.x), f64::from(end.y)]
                    }
                    None => point,
                };
                let draft = self.vector_paths.draft.get_or_insert(Draft { layer, anchors: Vec::new(), dragging: false });
                draft.anchors.push(Anchor::corner(point));
                draft.dragging = true;
            }
            PenPhase::Move | PenPhase::Up => {
                let alt = self.interaction.modifiers.alt;
                let reach = CLOSE_REACH * self.document_per_surface_pixel();
                let Some(draft) = self.vector_paths.draft.as_mut().filter(|draft| draft.dragging) else { return Ok(true) };
                draft.dragging = event.phase == PenPhase::Move;
                let last = draft.anchors.last_mut().unwrap();
                let at = last.point();
                let out = [point[0] - at[0], point[1] - at[1]];
                if out[0].hypot(out[1]) > reach * 0.5 {
                    *last = if alt { Anchor::new(at, last.in_handle(), out, AnchorKind::CORNER) } else { Anchor::new(at, [-out[0], -out[1]], out, AnchorKind::SYMMETRIC) };
                }
            }
            PenPhase::Cancel => { self.cancel_vector_path(); }
            PenPhase::Hover => {}
        }
        self.show_vector_path_preview()?;
        self.layer_interaction.changed = true;
        Ok(true)
    }
    fn drafted_path(&self, closed: bool) -> Result<Option<(OccurrenceHandle, PathItem)>, String> {
        let (Some(draft), Some(paint)) = (&self.vector_paths.draft, self.vector_path_paint()) else { return Ok(None) };
        if draft.anchors.len() < 2 { return Ok(None); }
        let geometry = PathGeometry::new(vec![Subpath::new(&draft.anchors, closed)]).map_err(str::to_string)?;
        let style = self.vector_figure_style(paint, Cap::Butt, Join::Miter)?;
        let id = self.engine.document().scene().vector_layer(draft.layer).ok_or("The vector layer was removed")?.allocate_id();
        Ok(Some((draft.layer, PathItem::new(id, Arc::new(geometry), Arc::new(style)))))
    }
    fn show_vector_path_preview(&mut self) -> Result<(), String> {
        let preview = self.drafted_path(false)?.map(|(owner, path)| {
            self.vector_paths.revision += 1;
            layer_render::VectorPreview { owner, path: Arc::new(path), revision: self.vector_paths.revision }
        });
        self.engine.backend_mut().set_vector_preview(preview);
        Ok(())
    }
    pub(super) fn finish_vector_path(&mut self, closed: bool) -> Result<(), String> {
        let drafted = self.drafted_path(closed);
        self.cancel_vector_path();
        let Some((layer, path)) = drafted? else { return Ok(()) };
        let id = path.id;
        let document = self.engine.document();
        let object = document.scene().vector_layer(layer).ok_or("The vector layer was removed")?;
        let edit = document.vector_object_edit(layer, object.insert_front(vec![Record::Path(path)])).map_err(|e| e.to_string())?;
        self.layer_edit(edit)?;
        self.select_vector_items(layer, [id].into()).map(|_| ())
    }
    pub(super) fn cancel_vector_path(&mut self) -> bool {
        if self.vector_paths.draft.take().is_none() { return false; }
        self.engine.backend_mut().set_vector_preview(None);
        self.layer_interaction.changed = true;
        true
    }
    pub(super) fn remove_vector_path_point(&mut self) -> Result<bool, String> {
        let Some(draft) = &mut self.vector_paths.draft else { return Ok(false) };
        draft.anchors.pop();
        if draft.anchors.is_empty() { self.cancel_vector_path(); } else { self.show_vector_path_preview()?; }
        self.layer_interaction.changed = true;
        Ok(true)
    }
    pub(super) fn vector_path_key(&mut self, key: &str) -> Result<bool, String> {
        if !self.vector_paths.drafting() { return Ok(false); }
        match key {
            "enter" => self.finish_vector_path(false)?,
            "backspace" | "delete" => { self.remove_vector_path_point()?; }
            "escape" => { self.cancel_vector_path(); }
            _ => return Ok(false),
        }
        self.refresh_tools();
        Ok(true)
    }
    pub(super) fn append_vector_path_overlay(&self, segments: &mut Vec<CursorSegment>) {
        let Some(draft) = &self.vector_paths.draft else { return };
        let map = self.document_to_logical();
        let [ox, oy] = self.engine.document().scene().occurrence_offset64(draft.layer);
        let point = |p: [f64; 2]| map(Point { x: (p[0] + ox) as f32, y: (p[1] + oy) as f32 });
        let half = super::operation::HANDLE_HALF_SIZE;
        for anchor in &draft.anchors {
            let at = point(anchor.point());
            for handle in [anchor.incoming(), anchor.outgoing()].into_iter().filter(|handle| *handle != anchor.point()) {
                let end = point(handle);
                segments.push(CursorSegment { from: at, to: end, distance: 0., marker: 1., scale: 1. });
                segments.push(CursorSegment { from: [end[0] - half * 0.6, end[1] - half * 0.6], to: [end[0] + half * 0.6, end[1] + half * 0.6], distance: 0., marker: 2., scale: 1. });
            }
            segments.push(CursorSegment { from: [at[0] - half, at[1] - half], to: [at[0] + half, at[1] + half], distance: 0., marker: 2., scale: 1. });
        }
        if let (Some(last), false, Some(event)) = (draft.anchors.last(), draft.dragging, self.cursor.event) {
            let to = self.state.camera.input_transform().map(event.surface_position);
            segments.push(CursorSegment { from: point(last.point()), to: map(to), distance: 0., marker: 0., scale: 1. });
        }
    }
}
