//! Figure tools use the same GPU paint operation and selection path as fills.
use crate::*;
use layer_core::{Figure, RasterOperationKind, Point};
use layer_render::CanvasRenderer;
use std::sync::Arc;

pub(crate) fn modes(shape: FigureShape, paint: FigurePaint, localizer: &crate::localization::Localizer) -> Vec<ToolSetItem> {
    let item = |label, icon, shape, paint, selected| ToolSetItem { enabled: true,
        label,
        icon,
        selected,
        preview: None,
        action: UiAction::Layer {
            action: LayerAction::Tool {
                tool: LayerCanvasTool::Figure { shape, paint },
            },
        },
    };
    let icon = ToolVariant::Figure { shape }.icon();
    [
        (FigurePaint::Outline, localizer.text(crate::localization::MessageId::TOOL_FIGURES_OUTLINE)),
        (FigurePaint::Fill, localizer.text(crate::localization::MessageId::TOOL_FIGURES_FILL)),
        (FigurePaint::Both, localizer.text(crate::localization::MessageId::TOOL_FIGURES_OUTLINE_AND_FILL)),
    ]
        .into_iter()
        .filter(|(p, _)| shape != FigureShape::Line || *p == FigurePaint::Outline)
        .map(|(p, label)| {
            let icon = match (shape, p) {
                (FigureShape::Rectangle, FigurePaint::Fill) => "rectangle-fill",
                (FigureShape::Rectangle, FigurePaint::Both) => "rectangle-both",
                (FigureShape::Ellipse, FigurePaint::Fill) => "ellipse-fill",
                (FigureShape::Ellipse, FigurePaint::Both) => "ellipse-both",
                (FigureShape::Path, FigurePaint::Fill) => "path-fill",
                (FigureShape::Path, FigurePaint::Both) => "path-both",
                _ => icon,
            };
            item(label, icon, shape, p, p == paint)
        })
        .collect()
}

impl<B: CanvasRenderer> UiSession<B> {
    pub(super) fn vector_figure_layer(&self) -> Option<layer_core::authored::OccurrenceHandle> {
        let doc = self.engine.document();
        let layer = doc.working.occurrence?;
        doc.scene().vector_layer(layer)?;
        (!self.state.colors.transparent() && self.selection_masks.target().is_none()).then_some(layer)
    }
    fn figure_colors(&self, paint: FigurePaint) -> Option<[[f32; 4]; 2]> {
        let definitions = [if paint == FigurePaint::Both { self.state.colors.foreground } else { self.state.colors.definition() }, self.state.colors.background];
        let mut colors = [[0.; 4]; 2];
        for (value, definition) in colors.iter_mut().zip(definitions) {
            *value = definition.linear_in(self.engine.document().composition().color.space).ok()?;
            value[3] *= self.state.brush.opacity;
        }
        Some(colors)
    }
    pub(super) fn vector_figure_style(&self, paint: FigurePaint, cap: layer_core::authored::vector::Cap, join: layer_core::authored::vector::Join) -> Result<layer_core::authored::vector::Style, String> {
        use layer_core::authored::vector::*;
        let colors = self.figure_colors(paint).ok_or("Choose a color")?;
        let space = self.engine.document().composition().color.space;
        let color = |rgba: [f32; 4]| layer_core::color::RgbColor::from_linear(space, rgba).map(Paint::color).map_err(|e| e.to_string());
        let outline = |rgba: [f32; 4], cap: Cap| -> Result<PaintEntry, String> {
            let mut stroke = Stroke::new(color(rgba)?, f64::from(self.state.brush.diameter));
            (stroke.cap, stroke.join) = (cap, join);
            Ok(PaintEntry::Stroke(stroke))
        };
        let fill = |rgba: [f32; 4]| -> Result<PaintEntry, String> { Ok(PaintEntry::Fill(Fill { paint: color(rgba)?, unknown: None })) };
        Ok(Style::new(match paint {
            FigurePaint::Outline => vec![outline(colors[0], cap)?],
            FigurePaint::Fill => vec![fill(colors[0])?],
            FigurePaint::Both => vec![fill(colors[1])?, outline(colors[0], Cap::Butt)?],
        }))
    }
    fn commit_vector_figure(&mut self, layer: layer_core::authored::OccurrenceHandle, figure: &Figure) -> Result<(), String> {
        use layer_core::authored::vector::*;
        if self.engine.document().is_locked(layer) { return Err(self.localization().text(MessageId::COMMANDS_THE_ACTIVE_LAYER_IS_LOCKED).to_string()); }
        let (start, end) = ([f64::from(figure.start.x), f64::from(figure.start.y)], [f64::from(figure.end.x), f64::from(figure.end.y)]);
        let join = if figure.shape == FigureShape::Rectangle { Join::Miter } else { Join::Round };
        let style = self.vector_figure_style(figure.paint, if figure.shape == FigureShape::Line { Cap::Round } else { Cap::Butt }, join)?;
        let doc = self.engine.document();
        let object = doc.scene().vector_layer(layer).ok_or("The vector layer was removed")?;
        let id = object.allocate_id();
        let line = PathGeometry::new(vec![Subpath::new(&[Anchor::corner(start), Anchor::corner(end)], false)]).map_err(str::to_string)?;
        let path = PathItem::new(id, Arc::new(line), Arc::new(style));
        let shape = |kind| LiveShape { kind, center: [(start[0] + end[0]) * 0.5, (start[1] + end[1]) * 0.5], size: [(end[0] - start[0]).abs(), (end[1] - start[1]).abs()], rotation: 0. };
        let path = match figure.shape {
            FigureShape::Line | FigureShape::Path => path,
            FigureShape::Rectangle => path.with_shape(shape(ShapeKind::Rectangle { radii: [0.; 4] })).map_err(str::to_string)?,
            FigureShape::Ellipse => path.with_shape(shape(ShapeKind::Ellipse)).map_err(str::to_string)?,
        };
        let edit = doc.vector_object_edit(layer, object.insert_front(vec![Record::Path(path)])).map_err(|e| e.to_string())?;
        self.layer_edit(edit)?;
        self.select_vector_items(layer, [id].into()).map(|_| ())
    }
    /// Local-space operation snapshot; pointer moves only update the guide.
    pub(super) fn current_figure(&self) -> Option<Figure> {
        let LayerCanvasTool::Figure { shape, paint } = self.layer_interaction.tool else {
            return None;
        };
        let [start, end] = self.layer_interaction.path.as_slice() else {
            return None;
        };
        let end = if self.interaction.modifiers.shift {
            shape.constrained_end(*start, *end)
        } else {
            *end
        };
        let doc = self.engine.document();
        let (alpha_locked, offset) = match self.vector_figure_layer() {
            Some(layer) => { let [x, y] = doc.scene().occurrence_offset64(layer); (false, Point { x: x as f32, y: y as f32 }) }
            None => {
                let id = doc.drawing_content()?;
                (doc.scene().occurrence(doc.target_owner(id)?)?.alpha_locked, layer_core::offsets::point(doc.target_offset(id)))
            }
        };
        let local = |p: Point| Point {
            x: p.x - offset.x,
            y: p.y - offset.y,
        };
        let colors = self.figure_colors(paint)?;
        Some(Figure {
            shape,
            paint,
            start: local(*start),
            end: local(end),
            width: self.state.brush.diameter,
            colors,
            alpha_locked,
            erase: self.state.colors.transparent(),
        })
    }

    pub(super) fn commit_figure(&mut self) -> Result<(), String> {
        let Some(figure) = self.current_figure() else {
            return Ok(());
        };
        let dx = (figure.end.x - figure.start.x).abs();
        let dy = (figure.end.y - figure.start.y).abs();
        let threshold = (0.5 / self.state.camera.zoom).max(0.001);
        if (figure.shape == FigureShape::Line && dx.hypot(dy) < threshold)
            || (figure.shape != FigureShape::Line && dx.min(dy) < threshold)
        {
            return Ok(());
        }
        if let Some(layer) = self.vector_figure_layer() { return self.commit_vector_figure(layer, &figure); }
        let both = figure.paint == FigurePaint::Both;
        let colors = if both {
            [self.state.colors.foreground, self.state.colors.background]
        } else {
            [self.state.colors.definition(); 2]
        };
        self.paint_operation(
            self.engine.document().working.selection.clone(),
            RasterOperationKind::Figure(figure),
            &colors[..if both { 2 } else { 1 }],
        )
    }
}
