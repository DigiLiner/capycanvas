//! Validated wire records. Positions/revisions are captured by the OS adapter,
//! never reconstructed from the render owner's newer camera.
use layer_engine::{PenEvent, PenPhase, SampleFlags, ToolKind};
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct CapyPointer {
    pub id: u64,
    pub timestamp_ns: u64,
    pub sequence: u64,
    pub view_revision: u64,
    pub x: f32,
    pub y: f32,
    pub pressure: f32,
    pub tilt_x: f32,
    pub tilt_y: f32,
    pub twist: f32,
    pub distance: f32,
    pub phase: u32,
    pub tool: u32,
    pub button: u32,
    pub flags: u32,
}
impl CapyPointer {
    /// Preserve capture metadata without routing timestamps through JSON/f64.
    pub fn event(&self) -> PenEvent {
        PenEvent {
            device_id: self.id,
            sequence: self.sequence,
            timestamp_ns: self.timestamp_ns,
            view_revision: self.view_revision,
            surface_position: layer_core::Point {
                x: self.x,
                y: self.y,
            },
            pressure: if (0.0..=1.0).contains(&self.pressure) {self.pressure} else {f32::NAN},
            tilt_radians: [self.tilt_x, self.tilt_y],
            twist_radians: self.twist,
            distance: if (0.0..=1.0).contains(&self.distance) {self.distance} else {f32::NAN},
            phase: self.phase(),
            tool: self.tool(),
            flags: SampleFlags(self.flags as u16),
        }
    }

    pub fn validate(&self) -> Result<(), &'static str> {
        if self.phase > 4 || self.tool > 3 || self.button > 2
            || self.flags & !(0x0f | u32::from(SampleFlags::INDIRECT_POINTER.0 | SampleFlags::BARREL_TWIST.0)) != 0 {
            return Err("Invalid pointer enum or flags");
        }
        if self.sequence == 0 {
            return Err("Pointer sequence must be nonzero");
        }
        Ok(())
    }
    pub fn phase(&self) -> PenPhase {
        match self.phase {
            0 => PenPhase::Hover,
            1 => PenPhase::Down,
            2 => PenPhase::Move,
            3 => PenPhase::Up,
            _ => PenPhase::Cancel,
        }
    }
    pub fn tool(&self) -> ToolKind {
        match self.tool {
            1 => ToolKind::Mouse,
            2 => ToolKind::Eraser,
            3 => ToolKind::Finger,
            _ => ToolKind::Pen,
        }
    }
}
/// Validate the whole ingress batch before any session actions can run.
#[cfg(any(target_os = "windows", test))]
pub fn validate_batch(records: &[CapyPointer]) -> Result<(), &'static str> {
    for sample in records {
        sample.validate()?;
    }
    for pair in records.windows(2) {
        if pair[1].sequence <= pair[0].sequence || pair[1].timestamp_ns < pair[0].timestamp_ns {
            return Err("Pointer history must be chronological");
        }
    }
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    fn point(sequence: u64) -> CapyPointer {
        CapyPointer {
            id: 17,
            timestamp_ns: sequence * 1_000_000,
            sequence,
            view_revision: 7,
            x: 45.0,
            y: 60.0,
            pressure: 0.4,
            tilt_x: 0.1,
            tilt_y: 0.2,
            twist: 0.0,
            distance: 0.0,
            phase: 2,
            tool: 0,
            button: 0,
            flags: 2,
        }
    }
    #[test]
    fn predicted_future_does_not_reject_next_real_history_batch() {
        let mut predicted=point(2);predicted.timestamp_ns=20_000_000;predicted.flags|=1;
        validate_batch(&[point(1),predicted]).unwrap();
        validate_batch(&[point(3)]).unwrap();
        assert!(predicted.event().flags.contains(SampleFlags::PREDICTED));
        assert!(!point(3).event().flags.contains(SampleFlags::PREDICTED));
    }
    #[test]
    fn malformed_structure_rejects_the_batch_but_numeric_samples_reach_admission() {
        let mut batch = [point(1), point(2), point(3)];
        batch[2].pressure = f32::NAN;
        validate_batch(&batch).unwrap();
        batch[2].pressure = 0.4;
        batch[2].phase = 900;
        assert!(validate_batch(&batch).is_err());
        batch[2].phase = 3;
        assert!(validate_batch(&batch).is_ok());
    }
    #[test]
    fn history_cannot_reverse_or_repeat_sequence() {
        assert!(validate_batch(&[point(2), point(1)]).is_err());
        assert!(validate_batch(&[point(1), point(1)]).is_err());
        let mut last = point(3);
        last.timestamp_ns = 0;
        assert!(validate_batch(&[point(1), last]).is_err());
    }
    #[test]
    fn indirect_tablet_and_barrel_twist_flags_preserve_pen_identity() {
        let mut record = point(1);
        record.flags |= u32::from(SampleFlags::INDIRECT_POINTER.0);
        record.validate().unwrap();
        assert_eq!(record.event().tool, ToolKind::Pen);
        assert!(record.event().flags.contains(SampleFlags::INDIRECT_POINTER));
        record.flags |= u32::from(SampleFlags::BARREL_TWIST.0);
        record.validate().unwrap();
        assert!(record.event().flags.contains(SampleFlags::BARREL_TWIST));
        record.flags |= 0x100;
        assert!(record.validate().is_err());
    }

    #[test]
    fn typed_conversion_preserves_capture_precision_and_all_axes() {
        let mut record = point(31);
        record.timestamp_ns = (1u64 << 54) + 1;
        record.view_revision = (1u64 << 54) + 3;
        record.tilt_x = -0.4;
        record.tilt_y = 0.8;
        record.twist = 2.5;
        record.distance = 0.25;
        record.flags = 0x0f;
        record.tool = 2;
        validate_batch(&[record]).unwrap();
        let event = record.event();
        assert_eq!(event.timestamp_ns, record.timestamp_ns);
        assert_eq!(event.view_revision, record.view_revision);
        assert_eq!(event.sequence, record.sequence);
        assert_eq!(event.device_id, record.id);
        assert_eq!(
            [event.surface_position.x, event.surface_position.y],
            [record.x, record.y]
        );
        assert_eq!(event.pressure, record.pressure);
        assert_eq!(event.tilt_radians, [record.tilt_x, record.tilt_y]);
        assert_eq!(event.twist_radians, record.twist);
        assert_eq!(event.distance, record.distance);
        assert_eq!(event.flags.0, 0x0f);
        assert_eq!(event.tool, ToolKind::Eraser);
    }
    #[test]
    fn capture_revision_and_eraser_survive_validation() {
        let mut record = point(1);
        record.view_revision = 123;
        record.tool = 2;
        record.phase = 4;
        validate_batch(&[record]).unwrap();
        assert_eq!(record.view_revision, 123);
        assert_eq!(record.tool(), ToolKind::Eraser);
        assert_eq!(record.phase(), PenPhase::Cancel);
        assert_eq!(std::mem::size_of::<CapyPointer>(), 80);
    }

    fn route_typed(host:&mut layer_host::NativeHost,records:&[CapyPointer]) {
        validate_batch(records).unwrap();
        for record in records {host.pointer_event(record.event(),layer_ui::PointerButton::Primary).unwrap();}
    }

    fn typed_host()->layer_host::NativeHost {
        let mut host=layer_host::NativeHost::new(layer_ui::Platform::Windows).unwrap();
        host.resize(800,600,1.).unwrap();host
    }

    #[test]
    fn typed_numeric_damage_keeps_valid_hover_siblings() {
        for axis in 0..7 {
            let mut host=typed_host();let mut control=typed_host();
            let mut records=[point(1),point(2),point(3)];
            for record in &mut records {record.phase=0;record.view_revision=host.session.state().camera.revision;}
            records[2].x=180.;
            match axis {0=>records[1].x=f32::NAN,1=>records[1].y=f32::INFINITY,2=>records[1].pressure=f32::NAN,
                3=>records[1].tilt_x=f32::NAN,4=>records[1].tilt_y=f32::INFINITY,5=>records[1].twist=f32::NAN,_=>records[1].distance=f32::NAN}
            route_typed(&mut host,&records);route_typed(&mut control,&records[2..]);
            let actual=host.session.canvas_cursor().unwrap();let expected=control.session.canvas_cursor().unwrap();
            assert_eq!(actual.center,expected.center);assert_eq!(actual.mode,expected.mode);assert!(host.error.is_none());
        }
    }

    #[test]
    fn typed_damaged_release_retires_only_the_matching_contact() {
        for axis in 0..9 {
            let mut host=typed_host();let revision=host.session.state().camera.revision;
            let mut records=[point(1),point(2),point(3)];
            for record in &mut records {record.view_revision=revision;}
            records[0].phase=1;records[1].x=140.;records[2].phase=3;
            match axis {0=>records[2].x=f32::NAN,1=>records[2].y=f32::INFINITY,2=>records[2].pressure=f32::NAN,
                3=>records[2].tilt_x=f32::NAN,4=>records[2].tilt_y=f32::INFINITY,5=>records[2].twist=f32::NAN,
                6=>records[2].distance=f32::NAN,7=>records[2].pressure=1.5,_=>records[2].distance = -1.}
            let mut other=point(4);other.id=81;other.phase=1;other.view_revision=revision;
            route_typed(&mut host,&[other]);route_typed(&mut host,&records);
            assert!(host.session.take_pen_cancel(17,ToolKind::Pen).is_none());
            assert!(host.session.take_pen_cancel(81,ToolKind::Pen).is_some());
            let mut next=point(5);next.phase=1;next.view_revision=revision;
            route_typed(&mut host,&[next]);
            let cancel=host.session.take_pen_cancel(17,ToolKind::Pen).expect("a new real contact remains admissible");
            assert_eq!(cancel.phase,PenPhase::Cancel);assert_eq!(cancel.surface_position,next.event().surface_position);
            host.pointer_event(cancel,layer_ui::PointerButton::Primary).unwrap();assert!(host.error.is_none());
        }
    }

    #[test]
    fn typed_damaged_prediction_cannot_retire_a_real_contact() {
        let mut host=typed_host();let revision=host.session.state().camera.revision;
        let mut down=point(1);down.phase=1;down.view_revision=revision;
        let mut predicted=down;predicted.phase=3;predicted.sequence=2;predicted.timestamp_ns+=1;predicted.flags|=1;predicted.x=f32::NAN;
        route_typed(&mut host,&[down,predicted]);
        let cancel=host.session.take_pen_cancel(17,ToolKind::Pen).expect("invalid optional data keeps the real contact");
        assert_eq!(cancel.surface_position,down.event().surface_position);
        host.pointer_event(cancel,layer_ui::PointerButton::Primary).unwrap();assert!(host.error.is_none());
    }

}
