use super::{test_support::*, *};
use layer_core::TiltAxes;

fn device(id:u64,key:&str,turn:u8)->PenDeviceInfo {
    PenDeviceInfo{id,key:key.into(),label:format!("Pen {key}"),native_to_surface:TiltAxes::quarter_turns(turn)}
}
fn hover(s:&mut UiSession<Recorder>,id:u64,tilt:[f32;2]) {
    s.cursor_input(Some(PenEvent{device_id:id,tilt_radians:tilt,..event(s,1,PenPhase::Hover,0.)}));
}
fn panel(s:&mut UiSession<Recorder>,action:PressureCalibrationAction) {
    s.dispatch(UiAction::TiltCalibration{action:TiltCalibrationAction::Panel{action}}).unwrap();
}
fn axes(s:&mut UiSession<Recorder>,axes:TiltAxes) {
    s.dispatch(UiAction::TiltCalibration{action:TiltCalibrationAction::Axes{axes}}).unwrap();
}
fn edit(s:&mut UiSession<Recorder>,action:CurveEditorAction) {
    s.dispatch(UiAction::CurveEditor{target:CurveEditorTarget::Tilt,action}).unwrap();
}
fn open(s:&mut UiSession<Recorder>) {invoke(s,CommandId::PenTilt);}
fn view(s:&UiSession<Recorder>)->&TiltCalibrationView {s.state.tilt_calibration.as_ref().unwrap()}

#[test]
fn tilt_editor_waits_for_a_pen_and_keeps_its_target_until_closed() {
    let mut s=session(Platform::Gtk);open(&mut s);
    assert!(!view(&s).ready);panel(&mut s,PressureCalibrationAction::Apply);assert!(s.state.tilt_calibration.is_some());
    s.register_pen_device(&device(2,"first",0));hover(&mut s,2,[0.5,0.]);assert!(view(&s).ready);
    panel(&mut s,PressureCalibrationAction::Sensitivity{lighter:true});
    let draft=view(&s).panel.editor.points.clone();
    s.register_pen_device(&device(3,"second",0));hover(&mut s,3,[0.,0.6]);
    assert_eq!(view(&s).device,"Pen first");assert_eq!(view(&s).panel.editor.points,draft);
    assert_eq!(s.engine.configured_tilt_curve(),&layer_engine::TiltCurve::default());
    s.dispatch(UiAction::OpenSettings{page:SettingsPage::Input}).unwrap();
    open(&mut s);assert!(!s.state.settings_open);assert_eq!(view(&s).panel.editor.points,draft,"reopening retains the draft");
    panel(&mut s,PressureCalibrationAction::Apply);assert!(s.state.settings.tilt_calibrations.contains_key("first"));
    assert!(!s.state.settings.tilt_calibrations.contains_key("second"));
    open(&mut s);assert_eq!(view(&s).device,"Pen second");assert_eq!(view(&s).panel.editor.points,[[0.,0.],[1.,1.]]);
}

#[test]
fn tilt_preview_applies_cancels_resets_and_restores_per_device_settings() {
    let mut s=session(Platform::Gtk);s.register_pen_device(&device(2,"pen",0));hover(&mut s,2,[0.5,0.]);
    let workspace=serde_json::to_value(&s.state.workspace).unwrap();let revision=s.engine.document().revision;
    open(&mut s);hover(&mut s,2,[0.5,0.]);panel(&mut s,PressureCalibrationAction::Sensitivity{lighter:false});
    axes(&mut s,TiltAxes{reverse_horizontal:true,..Default::default()});
    assert_eq!(view(&s).panel.editor.controls.axes[0].maximum,"90°");
    assert_eq!(view(&s).panel.editor.points,[[0.,0.],[1.,0.975]]);
    let direction=view(&s).adjusted_direction.unwrap();assert!(direction[0]<0.);
    assert!(direction[0].abs()<view(&s).input_direction.unwrap()[0]);
    assert!(s.state.settings.tilt_calibrations.is_empty());
    panel(&mut s,PressureCalibrationAction::Cancel);assert_eq!(s.engine.configured_tilt_curve(),&layer_engine::TiltCurve::default());
    open(&mut s);panel(&mut s,PressureCalibrationAction::Sensitivity{lighter:true});
    axes(&mut s,TiltAxes{swap:true,..Default::default()});panel(&mut s,PressureCalibrationAction::Apply);
    let settings:Settings=serde_json::from_slice(&serde_json::to_vec(&s.state.settings).unwrap()).unwrap();
    let saved=settings.tilt_calibrations["pen"].clone();assert!(saved.axes.swap);
    open(&mut s);edit(&mut s,CurveEditorAction::Reset);assert_eq!(view(&s).axes,TiltAxes::default());
    assert_eq!(view(&s).panel.editor.points,[[0.,0.],[1.,1.]]);panel(&mut s,PressureCalibrationAction::Cancel);
    assert_eq!(s.state.settings.tilt_calibrations["pen"],saved);
    assert_eq!(s.engine.document().revision,revision);assert_eq!(serde_json::to_value(&s.state.workspace).unwrap(),workspace);
    let mut restarted=session(Platform::Gtk);restarted.apply_settings(settings).unwrap();
    restarted.register_pen_device(&device(99,"pen",0));hover(&mut restarted,99,[0.5,0.]);open(&mut restarted);
    assert_eq!(view(&restarted).axes,saved.axes);assert_eq!(view(&restarted).panel.editor.points,saved.response.points());
}

#[test]
fn tilt_axis_choices_follow_native_rotation_and_ignore_canvas_rotation() {
    let mut s=session(Platform::Gtk);let mut info=device(2,"pen",0);s.register_pen_device(&info);hover(&mut s,2,[0.5,0.]);open(&mut s);
    let correction=TiltAxes{reverse_horizontal:true,..Default::default()};axes(&mut s,correction);
    for turn in 0..4 {
        info.native_to_surface=TiltAxes::quarter_turns(turn);s.register_pen_device(&info);
        let expected=correction.in_frame(info.native_to_surface);assert_eq!(view(&s).axes,expected);
        let v=info.native_to_surface.apply([0.5,0.]);let output=s.engine.configured_tilt_curve().map(v);
        assert_eq!(output,info.native_to_surface.apply([-0.5,0.]));
        axes(&mut s,expected);
    }
    s.state.camera.rotation=1.;s.sync_camera();assert_eq!(view(&s).axes,correction.in_frame(info.native_to_surface));
    panel(&mut s,PressureCalibrationAction::Apply);assert_eq!(s.state.settings.tilt_calibrations["pen"].axes,correction);
}

#[test]
fn tilt_curve_reuses_point_editing_and_protects_its_origin_and_endpoint() {
    let mut s=session(Platform::Gtk);s.register_pen_device(&device(2,"pen",0));hover(&mut s,2,[0.4,0.]);open(&mut s);
    let epoch=view(&s).panel.editor.controls.epoch;
    let contact=|phase,point|CurveEditorAction::Contact{epoch,phase,point,extent:[100.,100.]};
    edit(&mut s,contact(ContactPhase::Down,[50.,50.]));edit(&mut s,contact(ContactPhase::Up,[50.,25.]));
    assert_eq!(view(&s).panel.editor.points,[[0.,0.],[0.5,0.75],[1.,1.]]);
    edit(&mut s,contact(ContactPhase::Down,[50.,25.]));edit(&mut s,contact(ContactPhase::Move,[50.,-40.]));
    assert_eq!(view(&s).panel.editor.points.len(),2);edit(&mut s,contact(ContactPhase::Cancel,[50.,-40.]));
    assert_eq!(view(&s).panel.editor.points.len(),3);
    edit(&mut s,contact(ContactPhase::Down,[100.,0.]));edit(&mut s,contact(ContactPhase::Up,[60.,0.]));
    assert_eq!(view(&s).panel.editor.points,[[0.,0.],[0.3,0.75],[0.6,1.]]);
    edit(&mut s,contact(ContactPhase::Down,[0.,100.]));edit(&mut s,contact(ContactPhase::Up,[30.,70.]));
    assert_eq!(view(&s).panel.editor.points[0],[0.,0.]);
    edit(&mut s,CurveEditorAction::Key{epoch,key_event:"Escape".into(),pressed:true,repeat:false,modifiers:Modifiers::default()});
    assert!(s.state.tilt_calibration.is_none());assert!(s.state.settings.tilt_calibrations.is_empty());
}

#[test]
fn tilt_and_pressure_share_one_utility_and_only_gtk_exposes_tilt() {
    for platform in Platform::ALL {
        let mut s=session(platform);s.dispatch(UiAction::OpenSettings{page:SettingsPage::Input}).unwrap();
        let has=s.preferences().unwrap().pages.iter().flat_map(|p|&p.groups).flat_map(|g|&g.rows).any(|r|r.id==PreferenceId::PenTilt);
        assert_eq!(has,platform==Platform::Gtk);
    }
    let mut s=session(Platform::Gtk);invoke(&mut s,CommandId::PenPressure);open(&mut s);assert!(s.state.pressure_calibration.is_none());
    invoke(&mut s,CommandId::PenPressure);assert!(s.state.tilt_calibration.is_none());
}

#[test]
fn tilt_window_measurement_clamps_short_viewports_without_rebuilding_curve_data() {
    let mut s=session(Platform::Gtk);s.register_pen_device(&device(2,"pen",0));hover(&mut s,2,[0.4,0.]);open(&mut s);
    let plot=view(&s).panel.editor.plot.as_ptr();let points=view(&s).panel.editor.points.as_ptr();
    panel(&mut s,PressureCalibrationAction::Measure{extent:[336.,640.],viewport:[640.,400.]});
    let bounds=view(&s).panel.bounds;assert_eq!(bounds.height,400.);assert_eq!(bounds.y,0.);
    for (phase,position) in [(ContactPhase::Down,[100.,100.]),(ContactPhase::Move,[-200.,700.]),(ContactPhase::Up,[-200.,700.])] {
        panel(&mut s,PressureCalibrationAction::Drag{phase,position,viewport:[640.,400.]});
    }
    let bounds=view(&s).panel.bounds;assert_eq!(bounds.x,0.);assert_eq!(bounds.y,0.);
    assert_eq!(view(&s).panel.editor.plot.as_ptr(),plot);assert_eq!(view(&s).panel.editor.points.as_ptr(),points);
}
