#[test]
fn smoothing_controls_keep_per_brush_choices_and_restore_defaults() {
    let mut s = session(Platform::Gtk);
    let pen = DefaultBrushPreset::GPen as u32;
    s.select_brush(pen).unwrap();
    assert_eq!(s.engine.configured_brush().stabilization.strength, 0.06);

    assert!(!s.command(CommandId::SmoothingAdjustSpeed).selected);
    let control = s.state.tool_settings.iter().find(|c| c.id == "smoothing_strength").unwrap();
    assert_eq!(control.numeric.scale, 100.);
    assert_eq!((control.numeric.min, control.numeric.max), (0., 1.));
    assert_eq!(control.numeric.compact_text(control.value as f64), "6");
    for (position, strength) in [(0., 0.), (0.25, 0.02), (0.5, 0.09), (0.75, 0.3), (1., 1.)] {
        let resolved = control.numeric.resolve(0., NumericOperation::Position { position }).unwrap();
        assert!((resolved.value - strength).abs() < 1e-10);
    }
    assert!((control.numeric.resolve(0.3, NumericOperation::Format).unwrap().fill - 0.75).abs() < 1e-10);
    assert!((control.numeric.resolve(control.value as f64, NumericOperation::Format).unwrap().fill - 0.43302675).abs() < 1e-7);
    assert_eq!(control.numeric.resolve(0.3, NumericOperation::Step { steps: 1. }).unwrap().text, "31");
    assert!((control.numeric.resolve(0., NumericOperation::Expression { text: "30".into() }).unwrap().value - 0.3).abs() < 1e-10);
    let serialized = serde_json::to_string(&control.numeric).unwrap();
    let restored: NumericControl = serde_json::from_str(&serialized).unwrap();
    assert_eq!(restored, control.numeric);
    s.dispatch(UiAction::SetToolSetting { id: "smoothing_strength".into(), value: 0.5 }).unwrap();

    s.dispatch(UiAction::Invoke { command: CommandId::SmoothingAdjustSpeed }).unwrap();

    assert!(!s.engine.can_undo());
    s.select_brush(DefaultBrushPreset::Pencil as u32).unwrap();
    assert_eq!(s.engine.configured_brush().stabilization.strength, 0.06);
    assert!(!s.command(CommandId::SmoothingAdjustSpeed).selected);
    let saved = serde_json::to_vec(&s.editing_state()).unwrap();
    let mut reopened = session(Platform::Gtk);
    reopened.restore_editing(serde_json::from_slice(&saved).unwrap()).unwrap();
    reopened.select_brush(pen).unwrap();
    assert_eq!(reopened.engine.configured_brush().stabilization.strength, 0.5);

    assert!(reopened.command(CommandId::SmoothingAdjustSpeed).selected);
    reopened.dispatch(UiAction::ResetToolSetting { id: "smoothing_strength".into() }).unwrap();

    reopened.dispatch(UiAction::Invoke { command: CommandId::SmoothingAdjustSpeed }).unwrap();
    assert_eq!(reopened.engine.configured_brush().stabilization, layer_core::default_brush(DefaultBrushPreset::GPen).stabilization);
    assert!(reopened.tools.overrides.get(&pen).is_none_or(|values| !values.keys().any(|id| id.starts_with("smoothing_"))));
    reopened.dispatch(UiAction::Invoke { command: CommandId::RectangleSelect }).unwrap();
    assert!(!reopened.command(CommandId::SmoothingAdjustSpeed).enabled);
    assert!(!reopened.state.tool_settings.iter().any(|c| c.id == "smoothing_strength"));
    reopened.select_brush(pen).unwrap();
    reopened.dispatch(UiAction::Invoke { command: CommandId::QuickMask }).unwrap();

    assert!(!reopened.state.tool_settings.iter().any(|c| c.id == "smoothing_strength"));
    reopened.dispatch(UiAction::Invoke { command: CommandId::ReturnToArtwork }).unwrap();

    reopened.select_brush(DefaultBrushPreset::CloneStamp as u32).unwrap();
    assert!(reopened.state.tool_actions.iter().any(|action| action.command == CommandId::SmoothingAdjustSpeed));
}
