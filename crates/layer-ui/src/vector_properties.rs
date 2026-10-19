use super::*;
use super::effects::{PropertyControl, PropertyKind};
use layer_core::{EffectValue, authored::OccurrenceHandle, authored::vector::{Cap, Fill, FillRule, Join, Paint, PaintEntry, PathItem, Position, Record, Stroke, Style}};
use std::{collections::HashMap, sync::Arc};

const FILL: &str = "path_fill";
const FILL_COLOR: &str = "path_fill_color";
const OUTLINE: &str = "path_outline";
const OUTLINE_COLOR: &str = "path_outline_color";
const WIDTH: &str = "path_width";
const ENDS: &str = "path_ends";
const CORNERS: &str = "path_corners";
const MITER: &str = "path_miter_limit";
const OVERLAPS: &str = "path_overlaps";
const KEYS: [&str; 9] = [FILL, FILL_COLOR, OUTLINE, OUTLINE_COLOR, WIDTH, ENDS, CORNERS, MITER, OVERLAPS];
const RULES: [FillRule; 2] = [FillRule::NonZero, FillRule::EvenOdd];
const CAPS: [Cap; 3] = [Cap::Butt, Cap::Round, Cap::Square];
const JOINS: [Join; 3] = [Join::Miter, Join::Round, Join::Bevel];

pub(super) fn is_path_property(key: &str) -> bool { key.starts_with("path_") }

fn values(path: &PathItem) -> [Option<EffectValue>; 9] {
    let style = &path.style;
    let fill = style.entries.iter().find_map(|entry| if let PaintEntry::Fill(fill) = entry { Some(fill) } else { None });
    let outline = style.strokes().last();
    [
        Some(EffectValue::Toggle(fill.is_some())),
        fill.and_then(|fill| fill.paint.rgb()).map(EffectValue::Color),
        Some(EffectValue::Toggle(outline.is_some())),
        outline.and_then(|stroke| stroke.paint.rgb()).map(EffectValue::Color),
        outline.map(|stroke| EffectValue::Number(stroke.width as f32)),
        outline.map(|stroke| EffectValue::Choice(CAPS.iter().position(|cap| *cap == stroke.cap).unwrap_or(0) as u32)),
        outline.map(|stroke| EffectValue::Choice(JOINS.iter().position(|join| *join == stroke.join).unwrap_or(0) as u32)),
        outline.map(|stroke| EffectValue::Number(stroke.miter_limit as f32)),
        Some(EffectValue::Choice(RULES.iter().position(|rule| *rule == path.fill_rule).unwrap_or(0) as u32)),
    ]
}

pub(super) fn controls(doc: &Document, layer: OccurrenceHandle, l: &Localizer) -> Vec<PropertyControl> {
    let Some(selection) = doc.working.vector_items.as_ref().filter(|selection| selection.layer == layer) else { return Vec::new() };
    let paths = doc.scene().vector_layer(layer).map(|object| object.selected(&selection.items)).unwrap_or_default();
    let Some(path) = paths.first() else { return Vec::new() };
    let front = values(path);
    let mut mixed = [false; 9];
    let mut seen: Vec<*const Style> = vec![Arc::as_ptr(&path.style)];
    for other in &paths[1..] {
        mixed[8] |= other.fill_rule != path.fill_rule;
        if seen.contains(&Arc::as_ptr(&other.style)) { continue; }
        if seen.len() < 64 { seen.push(Arc::as_ptr(&other.style)); }
        for (flag, (a, b)) in mixed.iter_mut().zip(front.iter().zip(values(other))).take(8) { *flag |= *a != b; }
    }
    let fill = path.style.entries.iter().find_map(|entry| if let PaintEntry::Fill(fill) = entry { fill.paint.rgb() } else { None });
    let outline = path.style.strokes().last();
    let unchanged = |key: &str, label: MessageId, kind: PropertyKind, value: EffectValue| {
        let at = KEYS.iter().position(|k| *k == key).unwrap();
        let label = if mixed[at] {
            let mut args = FluentArgs::new();
            args.set("label", l.text(label).to_string());
            l.format(MessageId::RESOURCES_PATH_MIXED, &args)
        } else { l.text(label).to_string() };
        PropertyControl::new(key, &label, kind, value.clone(), value)
    };
    let mut controls = vec![unchanged(FILL, MessageId::RESOURCES_PATH_FILL, PropertyKind::Toggle, EffectValue::Toggle(fill.is_some()))];
    controls.extend(fill.map(|color| unchanged(FILL_COLOR, MessageId::RESOURCES_PATH_FILL_COLOR, PropertyKind::Color { opaque: false }, EffectValue::Color(color))));
    if fill.is_some() {
        let rules = [MessageId::RESOURCES_PATH_OVERLAPS_FILLED, MessageId::RESOURCES_PATH_OVERLAPS_HOLES].map(|id| l.text(id)).into();
        controls.push(unchanged(OVERLAPS, MessageId::RESOURCES_PATH_OVERLAPS, PropertyKind::Choice { options: rules }, EffectValue::Choice(RULES.iter().position(|rule| *rule == path.fill_rule).unwrap_or(0) as u32)));
    }
    controls.push(unchanged(OUTLINE, MessageId::RESOURCES_PATH_OUTLINE, PropertyKind::Toggle, EffectValue::Toggle(outline.is_some())));
    if let Some(stroke) = outline {
        controls.extend(stroke.paint.rgb().map(|color| unchanged(OUTLINE_COLOR, MessageId::RESOURCES_PATH_OUTLINE_COLOR, PropertyKind::Color { opaque: false }, EffectValue::Color(color))));
        controls.push(unchanged(WIDTH, MessageId::RESOURCES_PATH_WIDTH, PropertyKind::Number { numeric: crate::NumericControl::number(0.1, 4096., 0.1, 1).unit("px") }, EffectValue::Number(stroke.width as f32)));
        let ends = [MessageId::RESOURCES_PATH_ENDS_FLAT, MessageId::RESOURCES_PATH_ENDS_ROUND, MessageId::RESOURCES_PATH_ENDS_SQUARE].map(|id| l.text(id)).into();
        controls.push(unchanged(ENDS, MessageId::RESOURCES_PATH_ENDS, PropertyKind::Choice { options: ends }, EffectValue::Choice(CAPS.iter().position(|cap| *cap == stroke.cap).unwrap_or(0) as u32)));
        let corners = [MessageId::RESOURCES_PATH_CORNERS_SHARP, MessageId::RESOURCES_PATH_CORNERS_ROUND, MessageId::RESOURCES_PATH_CORNERS_BEVELED].map(|id| l.text(id)).into();
        controls.push(unchanged(CORNERS, MessageId::RESOURCES_PATH_CORNERS, PropertyKind::Choice { options: corners }, EffectValue::Choice(JOINS.iter().position(|join| *join == stroke.join).unwrap_or(0) as u32)));
        if stroke.join == Join::Miter {
            controls.push(unchanged(MITER, MessageId::RESOURCES_PATH_MITER_LIMIT, PropertyKind::Number { numeric: crate::NumericControl::number(1., 20., 0.5, 1) }, EffectValue::Number(stroke.miter_limit as f32)));
        }
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
        (WIDTH | ENDS | CORNERS | MITER, _) => {
            let PaintEntry::Stroke(stroke) = &mut entries[last_stroke?] else { return None };
            match (key, value) {
                (WIDTH, EffectValue::Number(width)) if width.is_finite() && *width > 0. => stroke.width = f64::from(*width),
                (ENDS, EffectValue::Choice(index)) => stroke.cap = *CAPS.get(*index as usize)?,
                (CORNERS, EffectValue::Choice(index)) => stroke.join = *JOINS.get(*index as usize)?,
                (MITER, EffectValue::Number(limit)) if limit.is_finite() && *limit >= 1. => stroke.miter_limit = f64::from(*limit),
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
        let rule = match (key, value) { (OVERLAPS, EffectValue::Choice(index)) => Some(*RULES.get(*index as usize).ok_or("Unknown choice")?), (OVERLAPS, _) => return Err("Unknown choice".into()), _ => None };
        let mut styles: HashMap<usize, Option<Arc<Style>>> = HashMap::new();
        let changes: Vec<(Position, Option<Record>)> = object.positions(&selection.items).into_iter()
            .flat_map(|(chunk, indices)| indices.into_iter().map(move |index| Position { chunk: chunk as u32, index: index as u32 }))
            .filter_map(|position| {
                let path = object.record(position)?.path()?;
                if let Some(rule) = rule { return (path.fill_rule != rule).then(|| (position, Some(Record::Path(PathItem { fill_rule: rule, ..path.clone() })))); }
                let style = styles.entry(Arc::as_ptr(&path.style) as usize)
                    .or_insert_with(|| restyled(&path.style, key, value, (&defaults.0, &defaults.1, defaults.2)).map(Arc::new)).clone()?;
                Some((position, Some(Record::Path(path.with_style(style)))))
            }).collect();
        if changes.is_empty() { return Ok(()); }
        let edit = document.vector_object_edit(layer, object.replace(changes)).map_err(|e| e.to_string())?;
        if self.effect_gesture.is_some() { self.engine.preview_edit(edit).map_err(|e| e.to_string()) } else { self.layer_edit(edit) }
    }
}
