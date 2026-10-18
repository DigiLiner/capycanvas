use super::*;
use super::effects::{PropertyControl, PropertyKind};
use layer_core::{EffectValue, authored::OccurrenceHandle, authored::vector::{Cap, Fill, Join, Paint, PaintEntry, Position, Record, Stroke, Style}};
use std::{collections::HashMap, sync::Arc};

const FILL: &str = "path_fill";
const FILL_COLOR: &str = "path_fill_color";
const OUTLINE: &str = "path_outline";
const OUTLINE_COLOR: &str = "path_outline_color";
const WIDTH: &str = "path_width";
const ENDS: &str = "path_ends";
const CORNERS: &str = "path_corners";
const CAPS: [Cap; 3] = [Cap::Butt, Cap::Round, Cap::Square];
const JOINS: [Join; 3] = [Join::Miter, Join::Round, Join::Bevel];

pub(super) fn is_path_property(key: &str) -> bool { key.starts_with("path_") }

pub(super) fn controls(doc: &Document, layer: OccurrenceHandle, l: &Localizer) -> Vec<PropertyControl> {
    let Some(selection) = doc.working.vector_items.as_ref().filter(|selection| selection.layer == layer) else { return Vec::new() };
    let Some(path) = doc.scene().vector_layer(layer).and_then(|object| object.selected(&selection.items).into_iter().next()) else { return Vec::new() };
    let fill = path.style.entries.iter().find_map(|entry| if let PaintEntry::Fill(fill) = entry { fill.paint.rgb() } else { None });
    let outline = path.style.strokes().last();
    let unchanged = |key: &str, label: MessageId, kind: PropertyKind, value: EffectValue| PropertyControl::new(key, &l.text(label), kind, value.clone(), value);
    let mut controls = vec![unchanged(FILL, MessageId::RESOURCES_PATH_FILL, PropertyKind::Toggle, EffectValue::Toggle(fill.is_some()))];
    controls.extend(fill.map(|color| unchanged(FILL_COLOR, MessageId::RESOURCES_PATH_FILL_COLOR, PropertyKind::Color { opaque: false }, EffectValue::Color(color))));
    controls.push(unchanged(OUTLINE, MessageId::RESOURCES_PATH_OUTLINE, PropertyKind::Toggle, EffectValue::Toggle(outline.is_some())));
    if let Some(stroke) = outline {
        controls.extend(stroke.paint.rgb().map(|color| unchanged(OUTLINE_COLOR, MessageId::RESOURCES_PATH_OUTLINE_COLOR, PropertyKind::Color { opaque: false }, EffectValue::Color(color))));
        controls.push(unchanged(WIDTH, MessageId::RESOURCES_PATH_WIDTH, PropertyKind::Number { numeric: crate::NumericControl::number(0.1, 4096., 0.1, 1).unit("px") }, EffectValue::Number(stroke.width as f32)));
        let ends = [MessageId::RESOURCES_PATH_ENDS_FLAT, MessageId::RESOURCES_PATH_ENDS_ROUND, MessageId::RESOURCES_PATH_ENDS_SQUARE].map(|id| l.text(id)).into();
        controls.push(unchanged(ENDS, MessageId::RESOURCES_PATH_ENDS, PropertyKind::Choice { options: ends }, EffectValue::Choice(CAPS.iter().position(|cap| *cap == stroke.cap).unwrap_or(0) as u32)));
        let corners = [MessageId::RESOURCES_PATH_CORNERS_SHARP, MessageId::RESOURCES_PATH_CORNERS_ROUND, MessageId::RESOURCES_PATH_CORNERS_BEVELED].map(|id| l.text(id)).into();
        controls.push(unchanged(CORNERS, MessageId::RESOURCES_PATH_CORNERS, PropertyKind::Choice { options: corners }, EffectValue::Choice(JOINS.iter().position(|join| *join == stroke.join).unwrap_or(0) as u32)));
    }
    controls
}

fn restyled(style: &Style, key: &str, value: &EffectValue, defaults: (&Paint, &Paint, f64)) -> Option<Style> {
    let (fill_paint, outline_paint, width) = defaults;
    let mut entries = style.entries.to_vec();
    let first_fill = entries.iter().position(|entry| matches!(entry, PaintEntry::Fill(_)));
    let last_stroke = entries.iter().rposition(|entry| matches!(entry, PaintEntry::Stroke(_)));
    let new_fill = |paint: Paint| PaintEntry::Fill(Fill { paint, unknown: None });
    let new_stroke = |paint: Paint| PaintEntry::Stroke(Stroke::new(paint, width));
    match (key, value) {
        (FILL, EffectValue::Toggle(true)) if first_fill.is_none() => entries.insert(0, new_fill(fill_paint.clone())),
        (FILL, EffectValue::Toggle(false)) => entries.retain(|entry| !matches!(entry, PaintEntry::Fill(_))),
        (FILL_COLOR, EffectValue::Color(color)) => match first_fill {
            Some(at) => if let PaintEntry::Fill(fill) = &mut entries[at] { fill.paint = Paint::color(*color); },
            None => entries.insert(0, new_fill(Paint::color(*color))),
        },
        (OUTLINE, EffectValue::Toggle(true)) if last_stroke.is_none() => entries.push(new_stroke(outline_paint.clone())),
        (OUTLINE, EffectValue::Toggle(false)) => entries.retain(|entry| !matches!(entry, PaintEntry::Stroke(_))),
        (OUTLINE_COLOR, EffectValue::Color(color)) => match last_stroke {
            Some(at) => if let PaintEntry::Stroke(stroke) = &mut entries[at] { stroke.paint = Paint::color(*color); },
            None => entries.push(new_stroke(Paint::color(*color))),
        },
        (WIDTH | ENDS | CORNERS, _) => {
            let PaintEntry::Stroke(stroke) = &mut entries[last_stroke?] else { return None };
            match (key, value) {
                (WIDTH, EffectValue::Number(width)) if width.is_finite() && *width > 0. => stroke.width = f64::from(*width),
                (ENDS, EffectValue::Choice(index)) => stroke.cap = *CAPS.get(*index as usize)?,
                (CORNERS, EffectValue::Choice(index)) => stroke.join = *JOINS.get(*index as usize)?,
                _ => return None,
            }
        }
        _ => return None,
    }
    let edited = Style::new(entries);
    (edited != *style).then_some(edited)
}

impl<R: CanvasRenderer> UiSession<R> {
    pub(super) fn set_path_property(&mut self, layer: OccurrenceHandle, key: &str, value: &EffectValue) -> Result<(), String> {
        let document = self.engine.document();
        if document.is_locked(layer) { return Err(self.state.localization.text(MessageId::RESOURCES_ERROR_LAYER_LOCKED).to_string()); }
        let selection = document.working.vector_items.clone().filter(|selection| selection.layer == layer).ok_or("Select paths first")?;
        let object = document.scene().vector_layer(layer).ok_or("The vector layer was removed")?;
        let defaults = (Paint::color(self.state.colors.background), Paint::color(self.state.colors.foreground), f64::from(self.state.brush.diameter));
        let mut styles: HashMap<usize, Option<Arc<Style>>> = HashMap::new();
        let changes: Vec<(Position, Option<Record>)> = object.positions(&selection.items).into_iter()
            .flat_map(|(chunk, indices)| indices.into_iter().map(move |index| Position { chunk: chunk as u32, index: index as u32 }))
            .filter_map(|position| {
                let path = object.record(position)?.path()?;
                let style = styles.entry(Arc::as_ptr(&path.style) as usize)
                    .or_insert_with(|| restyled(&path.style, key, value, (&defaults.0, &defaults.1, defaults.2)).map(Arc::new)).clone()?;
                Some((position, Some(Record::Path(path.with_style(style)))))
            }).collect();
        if changes.is_empty() { return Ok(()); }
        let edit = document.vector_object_edit(layer, object.replace(changes)).map_err(|e| e.to_string())?;
        if self.effect_gesture.is_some() { self.engine.preview_edit(edit).map_err(|e| e.to_string()) } else { self.layer_edit(edit) }
    }
}
