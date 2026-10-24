use super::{ColorPanelLayout, ColorWheelGeometry, HdrIntensityArc};
use crate::{Bounds, RING_RIM};

#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ColorBubbleLayout {
    pub diameter: f32,
    pub stage: f32,
    pub intensity: Option<HdrIntensityArc>,
}

#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize)]
pub struct ColorBubbleArc {
    pub center: [f32; 2],
    pub radius: f32,
    pub start: f32,
    pub sweep: f32,
}

impl ColorBubbleArc {
    pub fn point(self, fraction: f32) -> [f32; 2] {
        let a = self.start + self.sweep * fraction;
        [self.center[0] + self.radius * a.cos(), self.center[1] + self.radius * a.sin()]
    }
}

impl ColorBubbleLayout {
    pub fn new(diameter: f32, hdr: bool) -> Self {
        let stage = ColorPanelLayout::stage_for_wheel(diameter - 2. * RING_RIM).expect("valid bubble diameter");
        let intensity = hdr.then(|| { let mut arc = HdrIntensityArc::new(stage).unwrap(); arc.center = [0.; 2]; arc });
        Self { diameter, stage, intensity }
    }

    pub fn wheel_size(self) -> f32 { self.diameter - 2. * RING_RIM }

    pub fn margin(self) -> f32 {
        self.diameter * 0.5 - ColorWheelGeometry::new(self.wheel_size()).unwrap().outer
    }

    fn junction(self, outset: f32) -> Option<(f32, f32, f32, f32, f32)> {
        let arc = self.intensity?;
        let radius = self.diameter * 0.5 + outset;
        let cap = arc.width * 0.5 + self.margin() + outset;
        let end = arc.point(1.);
        let angle = end[1].atan2(end[0]);
        let ring_join = angle - ((radius * radius + arc.radius * arc.radius - cap * cap)
            / (2. * radius * arc.radius)).clamp(-1., 1.).acos();
        let cap_join = (radius * ring_join.sin() - end[1]).atan2(radius * ring_join.cos() - end[0]);
        Some((radius, cap, angle, ring_join, cap_join))
    }

    pub fn outline(self, outset: f32) -> Vec<ColorBubbleArc> {
        use std::f32::consts::{PI, TAU};
        let Some((radius, cap, angle, ring_join, cap_join)) = self.junction(outset) else {
            return vec![ColorBubbleArc { center: [0.; 2], radius: self.diameter * 0.5 + outset, start: 0., sweep: TAU }];
        };
        let arc = self.intensity.unwrap();
        vec![
            ColorBubbleArc { center: [0.; 2], radius, start: PI - ring_join, sweep: PI + 2. * ring_join },
            ColorBubbleArc { center: arc.point(1.), radius: cap, start: cap_join, sweep: angle - cap_join },
            ColorBubbleArc { center: [0.; 2], radius: arc.radius + cap, start: angle, sweep: PI - 2. * angle },
            ColorBubbleArc { center: arc.point(0.), radius: cap, start: PI - angle, sweep: angle - cap_join },
        ]
    }

    pub fn radius_at(self, point: [f32; 2], outset: f32) -> f32 {
        let radius = self.diameter * 0.5 + outset;
        let Some(arc) = self.intensity else { return radius; };
        let cap = arc.width * 0.5 + self.margin() + outset;
        let end = arc.point(1.);
        let angle = end[1].atan2(end[0]);
        let a = point[1].atan2(point[0].abs());
        if a >= angle { return arc.radius + cap; }
        let delta = a - angle;
        let along = arc.radius * delta.cos();
        let discriminant = cap * cap - (arc.radius * delta.sin()).powi(2);
        if along > 0. && discriminant >= 0. { radius.max(along + discriminant.sqrt()) } else { radius }
    }

    pub fn contains(self, point: [f32; 2]) -> bool {
        point[0].hypot(point[1]) <= self.diameter * 0.5 || self.intensity.is_some_and(|arc| {
            let nearest = arc.point(arc.fraction(point));
            (point[0] - nearest[0]).hypot(point[1] - nearest[1]) <= arc.width * 0.5 + self.margin()
        })
    }

    pub fn orbit(self, point: [f32; 2]) -> [f32; 2] {
        let distance = point[0].hypot(point[1]);
        let scale = self.radius_at(point, distance - self.diameter * 0.5) / distance.max(f32::EPSILON);
        point.map(|v| v * scale)
    }

    pub fn orbit_from_bottom(self, distance: f32, outset: f32) -> [f32; 2] {
        let sign = if distance < 0. { 1. } else { -1. };
        let mut left = distance.abs();
        let Some((radius, cap, angle, ring_join, cap_join)) = self.junction(outset) else {
            let a = std::f32::consts::FRAC_PI_2 - left / (self.diameter * 0.5 + outset);
            return [sign * (self.diameter * 0.5 + outset) * a.cos(), (self.diameter * 0.5 + outset) * a.sin()];
        };
        let arc = self.intensity.unwrap();
        let outer = arc.radius + cap;
        let bottom = (std::f32::consts::FRAC_PI_2 - angle) * outer;
        if left <= bottom {
            let a = std::f32::consts::FRAC_PI_2 - left / outer;
            return [sign * outer * a.cos(), outer * a.sin()];
        }
        left -= bottom;
        let end = arc.point(1.);
        let shoulder = (angle - cap_join) * cap;
        if left <= shoulder {
            let a = angle - left / cap;
            return [sign * (end[0] + cap * a.cos()), end[1] + cap * a.sin()];
        }
        left -= shoulder;
        let a = ring_join - left / radius;
        [sign * radius * a.cos(), radius * a.sin()]
    }

    pub fn bounds(self) -> Bounds {
        let radius = self.diameter * 0.5;
        let width = self.intensity.map_or(radius, |arc| radius.max(arc.point(1.)[0] + arc.width * 0.5 + self.margin()));
        let bottom = self.radius_at([0., 1.], 0.);
        Bounds { x: -width, y: -radius, width: width * 2., height: radius + bottom }
    }

    pub fn panel(self) -> ColorPanelLayout {
        if self.intensity.is_some() { ColorPanelLayout::with_hdr(self.stage) } else { ColorPanelLayout::new(self.stage) }.unwrap()
    }

    pub fn palette_bounds(self) -> Bounds {
        let mut bounds = crate::palette_pill(self.diameter);
        bounds.y += self.bounds().height - self.diameter;
        bounds
    }

    pub fn collision_circles(self) -> Vec<[f32; 3]> {
        let radius = self.diameter * 0.5;
        let mut circles = vec![[0., 0., radius]];
        if let Some(arc) = self.intensity {
            let cap = arc.width * 0.5 + self.margin();
            circles.extend((0..=12).map(|i| { let p = arc.point(i as f32 / 12.); [p[0], p[1], cap] }));
        }
        circles
    }
}
