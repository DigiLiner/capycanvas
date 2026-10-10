#[test]
fn layer_drawer_growth_is_bounded_and_holds_properties_and_contacts() {
    let measurement = crate::PanelScrollMeasurement {fixed_height:100., unit_height:40.};
    let mut sizing = LayerDrawerSizing::default();
    assert!(sizing.update(measurement, 2, 1000., false, true));
    let minimum = sizing.height();
    assert!(!sizing.update(measurement, 6, 1000., false, true));
    assert!(sizing.update(measurement, 12, 1000., false, true));
    let expanded = sizing.height();
    assert!(expanded > minimum);
    assert!(!sizing.update(measurement, 1000, 1000., true, true));
    assert!(!sizing.update(measurement, 1000, 1000., false, false));
    assert_eq!(sizing.height(), expanded);
    assert!(sizing.update(measurement, 1000, 1000., false, true));
    assert_eq!(sizing.height(), 850.);
    assert!(!sizing.update(measurement, 1001, 1000., false, true));
    assert!(sizing.update(measurement, 1000, 320., true, false));
    assert_eq!(sizing.height(), 272.);
    assert!(sizing.update(measurement, 2, 1000., false, true));
    assert_eq!(sizing.height(), minimum);

    let mut s = session(Platform::Gtk);
    s.state.workspace.layout = WorkspacePreset::Painter.layout(Platform::Gtk);
    let id = s.state.workspace.layout.header.entries().find(|e| e.item == HeaderItem::Tool {control:ToolbarControl::Panel {panel:Panel::Layers}}).unwrap().id;
    s.dispatch(UiAction::MeasureHeader {height:60., items:vec![HeaderItemBounds {id, bounds:Bounds {x:800., y:0., width:40., height:60.}}]}).unwrap();
    customize(&mut s, CustomizationAction::ToggleHeaderDrawer {id});
    let drawer = s.state.customization.drawer.as_ref().unwrap();
    let large = drawer.placement(&s.state.workspace.layout, [1000., 1000.], &[10000.]).unwrap();
    assert_eq!(large.bounds.height, 850.);
    let short = drawer.placement(&s.state.workspace.layout, [1000., 200.], &[10000.]).unwrap();
    assert!(short.bounds.y + short.bounds.height <= 200. - WORKSPACE_SPACING);
    let mut ordinary = drawer.clone(); ordinary.layer_properties = None;
    assert!(ordinary.placement(&s.state.workspace.layout, [1000., 1000.], &[10000.]).unwrap().bounds.height > large.bounds.height);
}

#[test]
fn sketch_layer_drawer_properties_scope_and_dismissal() {
    for platform in [Platform::Gtk, Platform::Web, Platform::Android, Platform::Mac, Platform::Ios, Platform::Windows] {
        let mut s = session(platform);
        s.state.workspace.layout = WorkspacePreset::Painter.layout(platform);
        let id = s.state.workspace.layout.header.entries().find(|e| e.item == HeaderItem::Tool {control:ToolbarControl::Panel {panel:Panel::Layers}}).unwrap().id;
        s.dispatch(UiAction::MeasureHeader {height:60.,items:vec![HeaderItemBounds {id,bounds:Bounds {x:800.,y:0.,width:40.,height:60.}}]}).unwrap();
        customize(&mut s, CustomizationAction::ToggleHeaderDrawer {id});
        assert_eq!(s.state.customization.drawer.as_ref().unwrap().layer_properties, (platform == Platform::Gtk).then_some(false));
        if platform != Platform::Gtk { continue }
        let layout = s.state.workspace.clone();
        let history = s.capture_workspace();
        customize(&mut s, CustomizationAction::ToggleLayerDrawerProperties);
        assert_eq!(s.state.customization.drawer.as_ref().unwrap().layer_properties, Some(true));
        assert!(s.panel_is_presented(Panel::Properties));
        assert!(!s.panel_is_presented(Panel::Layers));
        let reply = key(&mut s, "Escape", true, false, false);
        assert!(reply.handled);
        assert_eq!(s.state.customization.drawer.as_ref().unwrap().layer_properties, Some(false));
        assert!(s.panel_is_presented(Panel::Layers));
        assert!(!s.panel_is_presented(Panel::Properties));
        assert_eq!(s.state.workspace, layout);
        assert_eq!(s.capture_workspace(), history);
        s.dispatch(UiAction::Effect {action:EffectAction::Insert {effect:"gaussian_blur".into()}}).unwrap();
        assert_eq!(s.state.customization.drawer.as_ref().unwrap().layer_properties, Some(true));
        assert_eq!(s.state.workspace, layout);
        customize(&mut s, CustomizationAction::ToggleLayerDrawerProperties);
        key(&mut s, "Escape", false, false, false);
        key(&mut s, "Escape", true, false, false);
        assert!(s.state.customization.drawer.is_none());
    }
    for preset in [WorkspacePreset::Illustrator, WorkspacePreset::Photographer] {
        let mut s = session(Platform::Gtk);
        s.state.workspace.layout = preset.layout(Platform::Gtk);
        s.state.workspace.layout.header = HeaderLayout::painter_for_platform(Platform::Gtk);
        let id = s.state.workspace.layout.header.entries().find(|e| e.item == HeaderItem::Tool {control:ToolbarControl::Panel {panel:Panel::Layers}}).unwrap().id;
        s.dispatch(UiAction::MeasureHeader {height:60.,items:vec![HeaderItemBounds {id,bounds:Bounds {x:800.,y:0.,width:40.,height:60.}}]}).unwrap();
        customize(&mut s, CustomizationAction::ToggleHeaderDrawer {id});
        assert_eq!(s.state.customization.drawer.as_ref().unwrap().layer_properties, None);
    }
}

#[test]
fn layer_drawer_disclosure_uses_types_and_nonredundant_authored_values() {
    let mut s = session(Platform::Gtk);
    assert_eq!(s.state.layer_properties.disclosure.label, "Layer properties");
    assert!(s.state.layer_properties.disclosure.summary.is_empty());
    s.dispatch(UiAction::Layer {action:LayerAction::ColorMode {id:1, epoch:s.state.document_file.epoch, mode:layer_core::color::LayerColorMode::Grayscale}}).unwrap();
    assert!(s.state.layers.iter().find(|l| l.id == 1).unwrap().description.contains("Grayscale"));
    assert!(s.state.layer_properties.disclosure.summary.is_empty());
    s.dispatch(UiAction::Effect {action:EffectAction::Insert {effect:"gaussian_blur".into()}}).unwrap();
    let id = s.state.layer_properties.layer.unwrap();
    assert_eq!(s.state.layer_properties.disclosure.label, "Gaussian Blur");
    assert!(s.state.layer_properties.disclosure.summary.contains("Radius"));
    s.dispatch(UiAction::Layer {action:LayerAction::Rename {id,name:"Soft edges".into()}}).unwrap();
    s.dispatch(UiAction::Effect {action:EffectAction::Set {layer:id,key:"sigma".into(),value:layer_core::EffectValue::Number(8.)}}).unwrap();
    assert_eq!(s.state.layer_properties.disclosure.label, "Gaussian Blur");
    assert!(s.state.layer_properties.disclosure.summary.contains("8 px"));
    assert!(!s.state.layer_properties.disclosure.summary.contains("Gaussian Blur"));
    s.dispatch(UiAction::Invoke {command:CommandId::Undo}).unwrap();
    assert!(!s.state.layer_properties.disclosure.summary.contains("8 px"));
    s.set_localization(Localizer::shared(UiLanguage::French));
    assert_eq!(s.state.layer_properties.disclosure.label, s.localization().text(MessageId::RESOURCES_FILTER_GAUSSIAN_BLUR).as_ref());
    assert_eq!(s.state.layer_properties.name, "Soft edges");
    assert!(!s.state.layer_properties.disclosure.summary.contains("Radius"));
    s.dispatch(UiAction::Effect {action:EffectAction::Insert {effect:"curves".into()}}).unwrap();
    assert!(s.state.layer_properties.disclosure.summary.is_empty());
}
