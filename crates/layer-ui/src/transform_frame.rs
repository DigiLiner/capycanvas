use layer_core::authored::Affine64;
use layer_render::CursorSegment;

const UNIT_HANDLES: [[f64; 2]; 8] = [[0., 0.], [0.5, 0.], [1., 0.], [1., 0.5], [1., 1.], [0.5, 1.], [0., 1.], [0., 0.5]];

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) enum FrameHandle { Move, Scale([f64; 2]), Rotate, Pivot }

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct Frame { pub unit: Affine64, pub pivot: [f64; 2] }
impl Frame {
    pub fn moved(self, delta: Affine64) -> Self { Self { unit: delta.compose(self.unit), pivot: delta.map(self.pivot) } }
    pub fn handles(self, reach: f64) -> Vec<(FrameHandle, [f64; 2])> {
        let top = self.unit.map([0.5, 0.]);
        let centre = self.unit.map([0.5, 0.5]);
        let outward = [top[0] - centre[0], top[1] - centre[1]];
        let length = outward[0].hypot(outward[1]).max(f64::MIN_POSITIVE);
        let rotate = [top[0] + outward[0] / length * reach * 2.5, top[1] + outward[1] / length * reach * 2.5];
        UNIT_HANDLES.into_iter().map(|unit| (FrameHandle::Scale(unit), self.unit.map(unit)))
            .chain([(FrameHandle::Rotate, rotate), (FrameHandle::Pivot, self.pivot)]).collect()
    }
    pub fn hit(self, point: [f64; 2], reach: f64) -> Option<FrameHandle> {
        self.handles(reach).into_iter().map(|(handle, at)| ((at[0] - point[0]).hypot(at[1] - point[1]), handle))
            .filter(|(distance, _)| *distance <= reach).min_by(|a, b| a.0.total_cmp(&b.0)).map(|(_, handle)| handle)
    }
    pub fn delta(self, handle: FrameHandle, press: [f64; 2], point: [f64; 2], shift: bool, aspect: bool, alt: bool) -> Option<Affine64> {
        Some(match handle {
            FrameHandle::Rotate => {
                let pivot = self.pivot;
                let mut angle = (point[1] - pivot[1]).atan2(point[0] - pivot[0]) - (press[1] - pivot[1]).atan2(press[0] - pivot[0]);
                if shift { let step = std::f64::consts::PI / 12.; angle = (angle / step).round() * step; }
                let (sin, cos) = angle.sin_cos();
                about(pivot, [cos, sin, -sin, cos])
            }
            FrameHandle::Scale(handle) => {
                let inverse = self.unit.inverse()?;
                let anchor = if alt { inverse.map(self.pivot) } else { handle.map(|v| 1. - v) };
                let q = inverse.map(point);
                let mut scale = [0, 1].map(|axis| {
                    let span = handle[axis] - anchor[axis];
                    if handle[axis] == 0.5 || span == 0. { 1. } else { (q[axis] - anchor[axis]) / span }
                });
                if (aspect || shift) && handle[0] != 0.5 && handle[1] != 0.5 {
                    let factor = if (scale[0] - 1.).abs() >= (scale[1] - 1.).abs() { scale[0] } else { scale[1] };
                    scale = [factor; 2];
                }
                if scale.iter().any(|v| !v.is_finite()) { return None; }
                let [a, b, c, d, _, _] = self.unit.0;
                let sides = [a.hypot(b), c.hypot(d)];
                let scale = [0, 1].map(|axis| {
                    let least = (1. / sides[axis]).min(1.);
                    if scale[axis].abs() >= least { scale[axis] } else if scale[axis] < 0. { -least } else { least }
                });
                self.unit.compose(about(anchor, [scale[0], 0., 0., scale[1]])).compose(inverse)
            }
            FrameHandle::Move | FrameHandle::Pivot => return None,
        })
    }
    pub fn append_handles(self, reach: f64, point: impl Fn([f64; 2]) -> [f32; 2], pivot: bool, segments: &mut Vec<CursorSegment>) {
        let handles = self.handles(reach);
        if let Some((_, rotate)) = handles.iter().find(|(handle, _)| *handle == FrameHandle::Rotate) {
            segments.push(CursorSegment { from: point(self.unit.map([0.5, 0.])), to: point(*rotate), distance: 0., marker: 1., scale: 1. });
        }
        for (handle, at) in handles.into_iter().filter(|(handle, _)| pivot || *handle != FrameHandle::Pivot) {
            let [x, y] = point(at);
            let half = super::operation::HANDLE_HALF_SIZE;
            segments.push(CursorSegment { from: [x - half, y - half], to: [x + half, y + half], distance: 0., marker: if handle == FrameHandle::Pivot { 5. } else { 2. }, scale: 1. });
        }
    }
}

pub(super) fn translation(delta: [f64; 2]) -> Affine64 { Affine64([1., 0., 0., 1., delta[0], delta[1]]) }
pub(super) fn about(pivot: [f64; 2], linear: [f64; 4]) -> Affine64 {
    translation(pivot).compose(Affine64([linear[0], linear[1], linear[2], linear[3], 0., 0.])).compose(translation([-pivot[0], -pivot[1]]))
}
