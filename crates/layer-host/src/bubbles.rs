use layer_ui::{BubbleGrab, BubbleHitGeometry, UiAction};
use serde::Deserialize;
use serde_json::{Value, json};

#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum Request {
    Hit { view: BubbleHitGeometry, point: [f32; 2] },
    PinchedSize { size: f32, scale: f32 },
    ColorOutline { size: f32, outset: f32 },
    ColorDragOut { size: f32, point: [f32; 2] },
    Choice { action: UiAction },
    CountBadge { text: [f32; 2] },
    ChooserTag { center: [f32; 2], width: f32 },
}

pub fn query(request: Value) -> Result<Value, String> {
    Ok(match serde_json::from_value(request).map_err(|error| error.to_string())? {
        Request::Hit { view, point } => json!(view.hit(point)),
        Request::PinchedSize { size, scale } => json!(layer_ui::pinched_size(size, scale)),
        Request::ColorOutline { size, outset } => {
            if !size.is_finite() || !(layer_ui::COLOR_RING_RANGE[0]..=layer_ui::COLOR_RING_RANGE[1]).contains(&size) || !outset.is_finite() || !(0. ..=32.).contains(&outset) { return Err("Invalid color bubble outline".into()); }
            json!(layer_ui::ColorBubbleLayout::new(size, true).outline(outset))
        },
        Request::ColorDragOut { size, point } => json!(layer_ui::color_drag_out(size, point)),
        Request::ChooserTag { center, width } => json!(layer_ui::chooser_tag(center, width)),
        Request::CountBadge { text } => json!(layer_ui::count_badge(text)),
        Request::Choice { action } => json!(BubbleGrab::for_choice(&action)),
    })
}
