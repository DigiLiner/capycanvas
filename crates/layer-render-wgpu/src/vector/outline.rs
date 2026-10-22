use super::tiles::{Frame, TOLERANCE};
use layer_core::authored::vector::{PathItem, Stroke};

pub(crate) fn outline(path: &PathItem, stroke: &Stroke, frame: &Frame) -> Vec<Vec<[f64; 2]>> {
    path.stroke_outline(stroke, TOLERANCE / frame.scale).into_iter().map(|ring| ring.into_iter()
        .map(|p| [(p[0] + frame.origin[0]) * frame.scale, (p[1] + frame.origin[1]) * frame.scale]).collect()).collect()
}
