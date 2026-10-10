#[test]
fn layer_drawer_sizes_each_page_immediately_within_the_viewport() {
    let measurement = crate::PanelScrollMeasurement {fixed_height:100., unit_height:40.};
    let mut sizing = LayerDrawerSizing::default();
    assert!(sizing.update(measurement, 2, 1000., None));
    let minimum = sizing.height();
    assert!(!sizing.update(measurement, 6, 1000., None));
    assert!(sizing.update(measurement, 7, 1000., None));
    assert_eq!(sizing.height(), minimum + 40.);
    assert!(sizing.update(measurement, 6, 1000., None));
    assert_eq!(sizing.height(), minimum);
    assert!(sizing.update(measurement, 12, 1000., None));
    let expanded = sizing.height();
    assert!(expanded > minimum);
    assert!(sizing.update(measurement, 1000, 1000., None));
    assert_eq!(sizing.height(), 850.);
    assert!(!sizing.update(measurement, 1001, 1000., None));
    assert!(sizing.update(measurement, 1000, 1000., Some(120.)));
    assert_eq!(sizing.height(), minimum);
    assert!(sizing.update(measurement, 1000, 1000., Some(600.)));
    assert_eq!(sizing.height(), 649.);
    assert!(sizing.update(measurement, 1000, 320., Some(600.)));
    assert_eq!(sizing.height(), 272.);
    assert!(sizing.update(measurement, 1000, 1000., Some(600.)));
    assert_eq!(sizing.height(), 649.);
    assert!(sizing.update(measurement, 12, 1000., None));
    assert_eq!(sizing.height(), expanded);
    assert!(sizing.update(measurement, 2, 1000., None));
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
fn layer_drawer_bridges_centered_header_buttons() {
    let mut s = session(Platform::Gtk);
    s.state.workspace.layout = WorkspacePreset::Painter.layout(Platform::Gtk);
    let id = s.state.workspace.layout.header.entries().find(|e| e.item == HeaderItem::Tool {control:ToolbarControl::Panel {panel:Panel::Layers}}).unwrap().id;
    for height in [36., 60., 84.] {
        let anchor = Bounds {x:800., y:(height-36.)/2., width:36., height:36.};
        s.dispatch(UiAction::MeasureHeader {height, items:vec![HeaderItemBounds {id, bounds:anchor}]}).unwrap();
        let drawer = crate::ContentDrawer::for_header(&s.state.workspace.layout, id).unwrap();
        let placement = drawer.placement(&s.state.workspace.layout, [1000., 1000.], &[395.]).unwrap();
        let bridge = placement.connection().unwrap();
        assert_eq!(bridge.bounds.y, anchor.y + anchor.height);
        assert_eq!(bridge.bounds.y + bridge.bounds.height, placement.bounds.y);
        assert!(bridge.glass().iter().all(|(bounds, _)| bounds[2] > 0. && bounds[3] > 0.));
    }
}

#[test]
fn layer_drawer_reveals_changed_targets_and_allows_return_from_empty_properties() {
    let mut s = session(Platform::Gtk);
    let epoch = s.state.document_file.epoch;
    let id = s.state.layer_tools.editing_layer.as_ref().unwrap().id;
    let previous = Some((epoch, Some(id)));
    assert!(s.state.layer_tools.drawer_reveal_row(&s.state.layers, epoch, None).is_some());
    assert_eq!(s.state.layer_tools.drawer_reveal_row(&s.state.layers, epoch, previous), None);
    assert!(s.state.layer_tools.drawer_reveal_row(&s.state.layers, epoch + 1, previous).is_some());
    s.dispatch(UiAction::Layer {action:LayerAction::New {group:false, clipped:false}}).unwrap();
    assert!(s.state.layer_tools.drawer_reveal_row(&s.state.layers, epoch, previous).is_some());
    assert_eq!(s.state.layer_tools.drawer_reveal_row(&[], epoch, previous), None);
    let empty = LayerPropertiesView::default();
    assert!(!empty.drawer_toggle_enabled(false));
    assert!(empty.drawer_toggle_enabled(true));
    assert!(s.state.layer_properties.drawer_toggle_enabled(false));
    s.state.workspace.layout = WorkspacePreset::Painter.layout(Platform::Gtk);
    let id = s.state.workspace.layout.header.entries().find(|e| e.item == HeaderItem::Tool {control:ToolbarControl::Panel {panel:Panel::Layers}}).unwrap().id;
    s.dispatch(UiAction::MeasureHeader {height:60., items:vec![HeaderItemBounds {id, bounds:Bounds {x:800., y:12., width:36., height:36.}}]}).unwrap();
    customize(&mut s, CustomizationAction::ToggleHeaderDrawer {id});
    customize(&mut s, CustomizationAction::ToggleLayerDrawerProperties);
    s.dispatch(UiAction::Layer {action:LayerAction::SelectAllLayers {selected:true}}).unwrap();
    s.dispatch(UiAction::Layer {action:LayerAction::DeleteSelected}).unwrap();
    assert!(s.state.layer_properties.layer.is_none());
    assert_eq!(s.state.customization.drawer.as_ref().unwrap().layer_properties, Some(true));
    customize(&mut s, CustomizationAction::ToggleLayerDrawerProperties);
    assert_eq!(s.state.customization.drawer.as_ref().unwrap().layer_properties, Some(false));
    customize(&mut s, CustomizationAction::ToggleLayerDrawerProperties);
    assert_eq!(s.state.customization.drawer.as_ref().unwrap().layer_properties, Some(false));
}

#[test]
fn layer_properties_offer_add_filter_only_on_raster_and_object_layers() {
    let mut s = session(Platform::Gtk);
    assert!(s.state.layer_properties.add_filter.is_some());
    s.dispatch(UiAction::Layer {action:LayerAction::AddMask {id:1, replace:false}}).unwrap();
    s.dispatch(UiAction::Layer {action:LayerAction::Select {id:1, mask:true}}).unwrap();
    assert!(s.state.layer_properties.add_filter.is_none());
    s.dispatch(UiAction::Layer {action:LayerAction::Select {id:1, mask:false}}).unwrap();
    assert!(s.state.layer_properties.add_filter.is_some());
    s.dispatch(UiAction::Effect {action:EffectAction::InsertAttached {effect:"gaussian_blur".into(), owner:1, epoch:s.state.document_file.epoch}}).unwrap();
    assert!(s.state.layer_properties.add_filter.is_none());
    assert!(s.state.layer_tools.add_filter.is_some());
    invoke(&mut s, CommandId::Undo);
    assert!(s.state.layer_properties.add_filter.is_some());
    s.dispatch(UiAction::Layer {action:LayerAction::New {group:true, clipped:false}}).unwrap();
    assert!(s.state.layer_properties.add_filter.is_none());
    let source = layer_core::color::source::rgba8_source([8;2], |_,_| [17,33,65,255]);
    let mut doc = Document::from_artwork(layer_core::authored::Artwork::new([64;2]).unwrap()).unwrap();
    let (owner, edit) = doc.create_object_layer_edit("Photo", layer_core::authored::ImageObject::new(source.into()), None, 0).unwrap();
    doc.apply(edit).unwrap();
    let mut s = UiSession::new(Recorder {tiled_sources:true, ..Default::default()}, doc, [64;2], Platform::Gtk).unwrap();
    s.dispatch(UiAction::SelectLayer {id:occurrence_token(owner)}).unwrap();
    assert!(s.state.layer_properties.add_filter.is_some());
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
