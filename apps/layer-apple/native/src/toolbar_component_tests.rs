use super::*;

#[test]
fn apple_phone_queries_round_trip_without_a_session() {
    let dock = layer_ui::WorkspacePreset::Painter.layout(layer_ui::Platform::Gtk);
    let layout = layer_ui::PhoneLayout::resolve(&dock, [402., 874.], [0.; 4], [0.; 2], [0; 2], false, false).unwrap();
    let hit = layout.tool_hits[0];
    for request in [
        json!({"kind":"slider_stretch_distance","layout":layout}),
        json!({"kind":"bar_begin","layout":layout,"tiles":dock.phone,"point":[hit.x+hit.width/2.,hit.y+hit.height/2.]}),
    ] {
        let source = request.to_string();
        assert_eq!(stateless(capy_apple_phone_ui, source.clone()), layer_ui::phone_ui(&source).unwrap());
    }
    assert!(stateless(capy_apple_phone_ui, "{")["error"].is_string());
    let output = unsafe { capy_apple_phone_ui(std::ptr::null()) };
    let missing: Value = serde_json::from_slice(handles::take_text(output).to_bytes()).unwrap();
    assert_eq!(missing["error"], "Missing phone request");
}

#[test]
fn apple_dialog_queries_round_trip_without_a_session() {
    for request in [
        json!({"kind":"escape","subpage":true,"search_focused":true,"search_has_text":true}),
        json!({"kind":"color_editor_layout","width":320,"minimum":[128,128]}),
        json!({"kind":"phone_window","viewport":[402,874]}),
    ] {
        let source = request.to_string();
        assert_eq!(stateless(capy_apple_dialog_ui, source.clone()), layer_ui::dialog_ui(&source).unwrap());
    }
    assert!(stateless(capy_apple_dialog_ui, "{")["error"].is_string());
    let output = unsafe { capy_apple_dialog_ui(std::ptr::null()) };
    let missing: Value = serde_json::from_slice(handles::take_text(output).to_bytes()).unwrap();
    assert_eq!(missing["error"], "Missing dialog request");
}

#[test]
fn apple_device_class_admits_phone_and_tablet_finger_input() {
    for phone in [true, false] {
        let app = App::new(1);
        assert!(!app.with(|app| app.host.session.finger_drawing()));
        assert_eq!(unsafe { capy_apple_device_class(app.0, phone) }, 0);
        assert!(app.with(|app| app.host.session.finger_drawing()));
    }
    assert_eq!(unsafe { capy_apple_device_class(0, true) }, -1);
}

#[test]
fn apple_enclose_fill_flat_choices_and_source_use_native_transport() {
    for platform in [0, 1] {
        for theme in ["light", "dark"] {
            let app = App::new(platform);
            app.action(json!({"type":"set_theme","theme":theme}));
            let label = |command: &str| app.state()["commands"].as_array().unwrap().iter()
                .find(|item| item["id"] == command).unwrap()["label"].clone();
            let policy = app.borrow().host.session.state().platform;
            app.action(json!({"type":"restore_workspace","workspace":layer_ui::WorkspaceState {
                layout: layer_ui::WorkspacePreset::Illustrator.layout(policy), ..Default::default()
            }}));
            let fill = label("fill");
            let lasso = label("lasso_fill");
            let enclose = label("enclose_fill");
            let unfilled = label("paint_unfilled");
            let choose = |section: &str, label: &Value| app.action(app.state()["tool_set"][section]
                .as_array().unwrap().iter().find(|item| item["label"] == *label).unwrap()["action"].clone());
            let selected = |command: &str| app.state()["commands"].as_array().unwrap().iter()
                .find(|item| item["id"] == command).unwrap()["selected"] == true;
            app.invoke("fill");
            choose("subtools", &lasso);
            choose("subtools", &enclose);
            choose("subtools", &fill);
            assert!(selected("fill"));
            choose("subtools", &lasso);
            assert!(selected("lasso_fill"));
            choose("subtools", &enclose);
            let state = app.full_snapshot()["state"].clone();
            assert_eq!(state["tool_set"]["subtools"].as_array().unwrap().iter().map(|item| item["label"].clone()).collect::<Vec<_>>(), [fill, lasso.clone(), enclose.clone(), unfilled]);
            assert!(state["tool_set"]["groups"].as_array().unwrap().is_empty());
            assert!(selected("enclose_fill"));
            for source in ["selection_visible", "selection_editing", "selection_reference"] {
                let state = app.state();
                let choice = &state["tool_extra"].as_array().unwrap().iter()
                    .find(|item| item["Choice"]["id"] == "selection-source").unwrap()["Choice"];
                assert_eq!(choice["segmented"], true);
                assert_eq!(choice["items"].as_array().unwrap().iter().map(|item| item["label"].clone()).collect::<Vec<_>>(),
                    [label("selection_reference"), label("selection_visible"), label("selection_editing")]);
                app.action(choice["items"].as_array().unwrap().iter()
                    .find(|item| item["action"]["command"] == source).unwrap()["action"].clone());
                assert!(selected(source));
                assert!(selected("enclose_fill"));
                assert_eq!(app.state()["tool_extra"].as_array().unwrap().iter()
                    .any(|item| item["Checklist"]["id"] == "reference-layers"), source == "selection_reference");
            }
            choose("subtools", &lasso);
            assert!(selected("lasso_fill"));
            assert!(!app.state()["tool_extra"].as_array().unwrap().iter()
                .any(|item| item["Choice"]["id"] == "selection-source" || item["Checklist"]["id"] == "reference-layers"));
        }
    }
}

#[test]
fn apple_geometry_metrics_do_not_require_a_prepared_language() {
    let request = CString::new(r#"{"language":"th","request":{"type":"metrics"}}"#).unwrap();
    let output = unsafe { capy_apple_toolbar_ui(request.as_ptr()) };
    let metrics: Value = serde_json::from_slice(handles::take_text(output).to_bytes()).unwrap();
    assert_eq!(metrics, layer_ui::toolbar_metrics());
    assert!(metrics["surface_radius"].as_f64().unwrap() > 0.);
    assert!(metrics["layer_edge_inset"].as_f64().unwrap() > 0.);
}

#[test]
fn apple_toolbar_queries_match_the_shared_transport_without_a_session() {
    let launch = CString::new(json!({"saved":"", "preferred_languages":["en"]}).to_string()).unwrap();
    let prepared = App::from_handle(unsafe { launch_json(0, launch.as_ptr(), std::ptr::null_mut(), std::ptr::null_mut()) });
    assert!(!(prepared.0==0));
    for request in [
        json!({"type":"slider_layout","width":44,"height":176,"axis":"vertical","style":"small","slider_style":{"show_value":false,"expand":false}}),
        json!({"type":"slider_layout","width":54,"height":400,"axis":"vertical","style":"medium","slider_style":{"show_value":true,"expand":true}}),
        json!({"type":"slider_readout","style":"medium","axis":"vertical","value":430,
            "control":layer_ui::ToolbarNumericBinding::BrushSize.numeric()}),
        json!({"type":"slider_spec","control":{"kind":"brush_size_slider"}}),
        json!({"type":"style","style":"medium"}),
        json!({"type":"choice_style","labeled":true}),
        json!({"type":"choice_style","labeled":true,"style":"large"}),
        json!({"type":"choice_style","labeled":false,"style":"large"}),
        json!({"type":"slider_preview","control":{"kind":"brush_strength_slider"},"style":"medium","value":0.5,"length":176,"extent":64}),
    ] {
        let expected = layer_ui::toolbar_ui(serde_json::from_value(request.clone()).unwrap(), &prepared.with(|native|native.host.session.localization().clone())).unwrap();
        assert_eq!(localized_stateless(capy_apple_toolbar_ui, request.to_string()), expected, "{request}");
    }
    assert!(localized_stateless(capy_apple_toolbar_ui, "{").get("error").is_some());
    assert!(localized_stateless(capy_apple_toolbar_ui, r#"{"type":"slider_spec","control":{"kind":"color"}}"#).get("error").is_some());
    let output = unsafe { capy_apple_toolbar_ui(std::ptr::null()) };
    let missing: Value = serde_json::from_slice(handles::take_text(output).to_bytes()).unwrap();

    assert_eq!(missing["error"], "Missing toolbar request");
}
