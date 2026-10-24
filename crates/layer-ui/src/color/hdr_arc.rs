//! Shared geometry for the intensity arc; hosts own input capture and timing.
use super::{ColorPanelLayout, ColorWheelGeometry};

#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct HdrIntensityArc {
    pub center: [f32; 2],
    pub radius: f32,
    pub width: f32,
    pub marker_radius: f32,
    end_angle: f32,
}
impl HdrIntensityArc {
    pub fn range(stops: f32, depth: layer_core::color::SampleDepth) -> [f32; 2] {
        [(-2f32).min(stops.floor()), 6f32.max(stops.ceil()).min(depth.max_linear().log2())]
    }
    pub fn value(&self, point: [f32; 2], [minimum, maximum]: [f32; 2]) -> f32 {
        (((minimum + self.fraction(point) * (maximum - minimum)) * 100.).round() / 100.).clamp(minimum, maximum)
    }
    pub fn new(size: f32) -> Option<Self> {
        let layout = ColorPanelLayout::new(size)?;
        let wheel = ColorWheelGeometry::new(layout.wheel[2])?;
        let width = wheel.outer - wheel.inner;
        let gap = wheel.inner - wheel.disc_radius();
        let radius = wheel.outer + gap + width * 0.5;
        // Keep the round caps inside the panel at large widths as the gap grows.
        let end_angle = 43f32
            .to_radians()
            .max(((size - width) * 0.5 / radius).clamp(0., 1.).acos());
        Some(Self {
            center: [size * 0.5; 2],
            radius,
            width,
            marker_radius: wheel.marker_radius(),
            end_angle,
        })
    }
    pub fn point(&self, fraction: f32) -> [f32; 2] {
        let a = std::f32::consts::PI
            - self.end_angle
            - fraction.clamp(0., 1.) * (std::f32::consts::PI - self.end_angle * 2.);
        [
            self.center[0] + self.radius * a.cos(),
            self.center[1] + self.radius * a.sin(),
        ]
    }
    pub fn zero_tick(&self, [minimum, maximum]: [f32; 2]) -> [[f32; 2]; 2] {
        let point = self.point(-minimum / (maximum - minimum));
        [2., 5.].map(|outset| std::array::from_fn(|axis| {
            point[axis] + (point[axis] - self.center[axis]) / self.radius * (self.width * 0.5 + outset)
        }))
    }
    /// Clockwise along the lower arc, clamped to the nearest endpoint off-track.
    pub fn fraction(&self, point: [f32; 2]) -> f32 {
        let x = point[0] - self.center[0];
        let y = point[1] - self.center[1];
        let a = y.max(0.).atan2(x);
        ((std::f32::consts::PI - self.end_angle - a) / (std::f32::consts::PI - self.end_angle * 2.))
            .clamp(0., 1.)
    }
    pub fn contains(&self, point: [f32; 2]) -> bool {
        let p = self.point(self.fraction(point));
        (point[0] - p[0]).hypot(point[1] - p[1]) <= self.width * 0.5 + 3.
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn zero_tick_follows_expanded_ranges_and_translated_centers() {
        for size in [128., 230., 358.] {
            for center in [[0., 0.], [size * 0.5, size * 0.5]] {
                let arc = HdrIntensityArc { center, ..HdrIntensityArc::new(size).unwrap() };
                for range in [[-2., 6.], [-12., 6.], [-2., 12.]] {
                    let tick = arc.zero_tick(range);
                    assert!((arc.fraction(tick[0]) + range[0] / (range[1] - range[0])).abs() < 1e-5);
                    for (point, outset) in tick.into_iter().zip([2., 5.]) {
                        assert!(((point[0] - center[0]).hypot(point[1] - center[1]) - arc.radius - arc.width * 0.5 - outset).abs() < 1e-4);
                    }
                    assert!(((tick[1][0] - tick[0][0]).hypot(tick[1][1] - tick[0][1]) - 3.).abs() < 1e-4);
                }
            }
        }
    }
    #[test]
    fn rounded_intensity_endpoints_stay_inside_the_document_range() {
        use layer_core::color::SampleDepth;
        let arc = HdrIntensityArc::new(226.).unwrap();
        for depth in [SampleDepth::F16, SampleDepth::F32] {
            for stops in [-12., 0., depth.max_linear().log2()] {
                let range = HdrIntensityArc::range(stops, depth);
                for i in 0..=100 {
                    let value = arc.value(arc.point(i as f32 / 100.), range);
                    assert!(value >= range[0] && value <= range[1]);
                }
                assert_eq!(arc.value(arc.point(1.), range), range[1]);
            }
        }
    }
    #[test]
    fn arc_hits_follow_angles_and_leave_wheel_and_footer_controls_clear() {
        for size in [128., 144., 226., 264., 400., 1024.] {
            let arc = HdrIntensityArc::new(size).unwrap();
            let layout = ColorPanelLayout::with_hdr(size).unwrap();
            let sdr = ColorPanelLayout::new(size).unwrap();
            let wheel = ColorWheelGeometry::new(sdr.wheel[2]).unwrap();
            assert_eq!(arc.width, wheel.outer - wheel.inner);
            assert!(
                (arc.radius - arc.width * 0.5 - wheel.outer - (wheel.inner - wheel.disc_radius()))
                    .abs()
                    < 1e-4
            );
            assert_eq!(arc.marker_radius, wheel.marker_radius());
            let clearance = |b: [f32; 4], outer: f32| {
                (b[0] + b[2] * 0.5 - arc.center[0]).hypot(b[1] + b[3] * 0.5 - arc.center[1])
                    - b[2] * 0.5
                    - outer
            };
            for (old, new) in [sdr.foreground, sdr.background, sdr.transparent]
                .into_iter()
                .zip([layout.foreground, layout.background, layout.transparent])
            {
                assert!(
                    (clearance(old, wheel.outer) - clearance(new, arc.radius + arc.width * 0.5))
                        .abs()
                        < 1e-4
                );
                assert!(new[1] + new[3] <= layout.height());
            }
            assert!(!arc.contains(arc.center));
            for i in 0..=100 {
                let t = i as f32 / 100.;
                let p = arc.point(t);
                assert!((arc.fraction(p) - t).abs() < 1e-5);
                assert!(arc.contains(p));
                assert!(p[0] >= arc.width / 2. - 1e-4 && p[0] + arc.width / 2. <= size + 1e-4);
                for b in [
                    layout.foreground,
                    layout.background,
                    layout.transparent,
                    layout.swap,
                ] {
                    let distance = (p[0] - b[0] - b[2] / 2.).hypot(p[1] - b[1] - b[3] / 2.);
                    assert!(
                        distance > (arc.width + b[2]) / 2.,
                        "size={size} {p:?} {b:?}"
                    );
                }
            }
        }
    }

    #[test]
    fn arc_caps_accept_contacts_at_docked_panel_sizes() {
        for size in [128., 144., 176., 226., 264., 400.] {
            let arc = HdrIntensityArc::new(size).unwrap();
            let layout = ColorPanelLayout::with_hdr(size).unwrap();
            for fraction in [0., 1.] {
                let point = arc.point(fraction);
                assert!(arc.contains(point), "size={size} fraction={fraction}");
                assert!((arc.fraction(point) - fraction).abs() < 1e-5);
                assert!(point[0] >= 0. && point[0] <= size);
                assert!(point[1] >= 0. && point[1] <= layout.height());
                assert!(point[0] >= layout.wheel[0] && point[0] <= layout.wheel[0] + layout.wheel[2]);
                assert!(point[1] >= layout.wheel[1] && point[1] <= layout.wheel[1] + layout.wheel[3]);
            }
        }
    }
}
