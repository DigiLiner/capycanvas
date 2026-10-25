use std::f64::consts::{FRAC_PI_2, PI, TAU};

const HOLD: f64 = 3. * PI / 180.;
const REACH: f64 = 10. * PI / 180.;

#[derive(Clone, Copy, Debug, Default)]
pub struct RotationSnap {
    input: f64,
    output: Option<f32>,
}

impl RotationSnap {
    pub fn reset(&mut self) { *self = Self::default(); }

    pub fn rotate(&mut self, rotation: f32, delta: f32) -> Option<f32> {
        if !rotation.is_finite() || !delta.is_finite() { return None; }
        if delta == 0. { return Some(rotation); }
        let input = if self.output == Some(rotation) { self.input } else { unsnap(f64::from(rotation)) };
        self.input = wrap(input + wrap(f64::from(delta)));
        let rotation = wrap(Self::snap(self.input)) as f32;
        self.output = Some(rotation);
        Some(rotation)
    }

    pub fn snap(rotation: f64) -> f64 {
        let target = (rotation / FRAC_PI_2).round() * FRAC_PI_2;
        let offset = rotation - target;
        target + offset.signum() * response(offset.abs())
    }
}

fn wrap(angle: f64) -> f64 { (angle + PI).rem_euclid(TAU) - PI }

fn response(distance: f64) -> f64 {
    if distance <= HOLD { return 0.; }
    if distance >= REACH { return distance; }
    let t = (distance - HOLD) / (REACH - HOLD);
    t * t * ((2. * REACH + HOLD) - (REACH + HOLD) * t)
}

fn unsnap(rotation: f64) -> f64 {
    let target = (rotation / FRAC_PI_2).round() * FRAC_PI_2;
    let offset = rotation - target;
    if offset.abs() <= f64::from(f32::EPSILON) * 4. { return target; }
    if offset.abs() >= REACH { return rotation; }
    let (mut low, mut high) = (HOLD, REACH);
    for _ in 0..32 {
        let mid = (low + high) * 0.5;
        if response(mid) < offset.abs() { low = mid; } else { high = mid; }
    }
    target + offset.signum() * (low + high) * 0.5
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn absolute_angles_share_the_gesture_response_without_wrapping_slider_endpoints() {
        for quarter in -2..=2 {
            let cardinal = quarter as f64 * FRAC_PI_2;
            assert_eq!(RotationSnap::snap(cardinal), cardinal);
            for direction in [-1., 1.] {
                assert_eq!(RotationSnap::snap(cardinal + direction * 2_f64.to_radians()), cardinal);
                let free = cardinal + direction * 12_f64.to_radians();
                assert!((RotationSnap::snap(free) - free).abs() < 1e-12);
                let mut snap = RotationSnap::default();
                for step in 0..=120 {
                    let distance = direction * step as f64 * 0.1_f64.to_radians();
                    let absolute = RotationSnap::snap(cardinal + distance);
                    let gesture = snap.rotate(wrap(cardinal) as f32, distance as f32).unwrap();
                    assert!(wrap(f64::from(gesture) - absolute).abs() < 1e-6);
                    snap.reset();
                }
            }
        }
        assert_eq!(RotationSnap::snap(PI), PI);
        assert_eq!(RotationSnap::snap(-PI), -PI);
    }

    #[test]
    fn small_turns_hold_every_cardinal_and_eventually_escape() {
        for quarter in -2..=2 {
            for direction in [-1., 1.] {
                let mut snap = RotationSnap::default();
                let cardinal = wrap(quarter as f64 * FRAC_PI_2) as f32;
                let mut rotation = cardinal;
                for _ in 0..25 {
                    rotation = snap.rotate(rotation, direction * 0.1_f32.to_radians()).unwrap();
                    assert_eq!(rotation, cardinal);
                }
                for _ in 0..95 { rotation = snap.rotate(rotation, direction * 0.1_f32.to_radians()).unwrap(); }
                assert!((wrap(f64::from(rotation - cardinal)).to_degrees() - direction as f64 * 12.).abs() < 0.001);
            }
        }
    }

    #[test]
    fn response_is_monotonic_and_joins_free_rotation_smoothly() {
        let epsilon = 1e-7;
        assert_eq!(response(HOLD), 0.);
        assert!((response(HOLD + epsilon) / epsilon).abs() < 1e-4);
        assert!(((response(REACH) - response(REACH - epsilon)) / epsilon - 1.).abs() < 1e-4);
        let mut previous = 0.;
        for step in 0..=9000 {
            let distance = (step as f64 * 0.01).to_radians();
            let value = response(distance);
            assert!(value >= previous && value <= distance);
            previous = value;
        }
    }

    #[test]
    fn reversals_and_event_batching_preserve_the_same_view() {
        for direction in [-1., 1.] {
            let mut snap = RotationSnap::default();
            let mut rotation = 80_f32.to_radians();
            for _ in 0..240 { rotation = snap.rotate(rotation, direction * 0.125_f32.to_radians()).unwrap(); }
            let whole = RotationSnap::default().rotate(80_f32.to_radians(), direction * 30_f32.to_radians()).unwrap();
            assert!((rotation - whole).abs() < 1e-5);
            for _ in 0..240 { rotation = snap.rotate(rotation, -direction * 0.125_f32.to_radians()).unwrap(); }
            assert!((rotation.to_degrees() - 80.).abs() < 0.001);
        }
    }

    #[test]
    fn a_new_gesture_preserves_an_explicit_angle_and_discards_old_residual() {
        let mut snap = RotationSnap::default();
        let rotation = 89_f32.to_radians();
        assert_eq!(snap.rotate(rotation, 0.), Some(rotation));
        let moved = snap.rotate(rotation, 0.001_f32.to_radians()).unwrap();
        assert!((moved - rotation).abs() < 0.01_f32.to_radians());
        snap.reset();
        let cardinal = std::f32::consts::FRAC_PI_2;
        assert_eq!(snap.rotate(cardinal, 2_f32.to_radians()), Some(cardinal));
        snap.reset();
        assert_eq!(snap.rotate(cardinal, 2_f32.to_radians()), Some(cardinal));
    }

    #[test]
    fn rejected_input_preserves_accumulated_rotation() {
        let mut snap = RotationSnap::default();
        assert_eq!(snap.rotate(0., 2_f32.to_radians()), Some(0.));
        for (rotation, delta) in [(f32::NAN, 1.), (0., f32::INFINITY)] {
            assert_eq!(snap.rotate(rotation, delta), None);
        }
        assert!(snap.rotate(0., 2_f32.to_radians()).unwrap() > 0.);
    }
}
