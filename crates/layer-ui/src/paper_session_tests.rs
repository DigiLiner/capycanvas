fn paper_layer(s: &UiSession<Recorder>) -> u64 {
    occurrence_token(layer_core::paper::paper_occurrence(s.engine.document().scene(), None).unwrap())
}

fn paper_control(s: &UiSession<Recorder>, key: &str) -> PropertyControl {
    s.state.layer_properties.controls.iter().find(|control| control.key == key).cloned().unwrap()
}

fn paper_scale(s: &UiSession<Recorder>) -> f32 {
    let layer_core::EffectValue::Number(scale) = paper_control(s, "scale").value else { panic!("Scale is a number") };
    scale
}

#[test]
fn choosing_and_scaling_paper_are_single_undo_steps_and_new_paper_starts_on_the_choice() {
    let mut s = session(Platform::Gtk);
    let layer = paper_layer(&s);
    s.dispatch(UiAction::SelectLayer { id: layer }).unwrap();
    assert_eq!(s.state.layer_properties.title, "Paper");
    let checkpoint = s.engine.checkpoint();
    s.dispatch(UiAction::Effect { action: EffectAction::Set { layer, key: "texture".into(), value: layer_core::EffectValue::Choice(2) } }).unwrap();
    assert!(s.state.requests.iter().any(|r| matches!(r.kind, HostRequestKind::SaveSettings { .. })), "the choice is remembered for new drawings");
    assert_eq!(s.state.settings.new_document.paper.as_deref(), Some("rough"));
    assert_eq!(s.engine.document().paper(None).texture, layer_core::paper::built_in_paper("rough").unwrap().texture);
    assert_eq!(paper_control(&s, "texture").value, layer_core::EffectValue::Choice(2));
    let gesture = |phase, value| UiAction::Effect { action: EffectAction::Gesture { phase, action: Box::new(EffectAction::Number {
        layer, key: "scale".into(), operation: NumericOperation::Value { value } }) } };
    let chosen = s.engine.checkpoint();
    assert_ne!(chosen, checkpoint);
    s.dispatch(gesture(ContactPhase::Down, 1.5)).unwrap();
    s.dispatch(gesture(ContactPhase::Move, 3.)).unwrap();
    assert!((paper_scale(&s) - 3.).abs() < 1e-4);
    s.dispatch(gesture(ContactPhase::Up, 2.)).unwrap();
    assert!((paper_scale(&s) - 2.).abs() < 1e-4);
    invoke(&mut s, CommandId::Undo);
    assert_eq!(s.engine.checkpoint(), chosen, "the drag is one step");
    assert!((paper_scale(&s) - 1.).abs() < 1e-4);
    invoke(&mut s, CommandId::Undo);
    assert_eq!(s.engine.checkpoint(), checkpoint, "the choice is one step");
    assert_eq!(s.engine.document().paper(None).texture, layer_core::paper::default_texture());
    assert_eq!(s.state.settings.new_document.paper.as_deref(), Some("rough"), "undo keeps the remembered paper");
    let menu = s.application_menu(ApplicationMenu::Layer);
    let new = menu.sections.iter().flatten().find(|item| item.label == "New").unwrap();
    let paper = new.sections.iter().flatten().find(|item| item.label == "Paper").expect("Layer › New › Paper").clone();
    s.dispatch(UiAction::Layer { action: LayerAction::Delete { id: layer } }).unwrap();
    s.dispatch(UiAction::Layer { action: LayerAction::New { group: true, clipped: false } }).unwrap();
    s.dispatch(paper.action.unwrap()).unwrap();
    let inserted = s.engine.document().working.occurrence.unwrap();
    assert_eq!(s.engine.document().scene().children(None).last(), Some(&inserted));
    assert_eq!(s.engine.document().scene().parent(inserted), None);
    let application = s.engine.document().scene().effect_application(inserted).unwrap();
    assert!(layer_core::paper::is_paper(&application.program));
    assert_eq!(s.engine.document().scene().effect(inserted).unwrap().constant_color(), Some(layer_core::color::RgbColor::WHITE));
    assert_eq!(s.engine.document().paper(s.engine.document().working.target).texture, layer_core::paper::built_in_paper("rough").unwrap().texture);
    assert!((paper_scale(&s) - 1.).abs() < 1e-4);
    assert_eq!(s.state.layer_properties.title, "Paper");
}

#[test]
fn adding_paper_again_selects_the_existing_sheet_without_an_edit() {
    for platform in Platform::ALL {
        let mut s = session(platform);
        let paper = paper_layer(&s);
        let before = s.engine.checkpoint();
        s.dispatch(UiAction::Effect { action: EffectAction::Insert { effect: "paper".into() } }).unwrap();
        assert_eq!(s.state.layers.len(), 2);
        assert_eq!(s.engine.document().working.occurrence, Some(occurrence_handle(paper).unwrap()));
        assert_eq!(s.engine.checkpoint(), before);
        assert_eq!(s.state.layers.last().unwrap().id, paper);
    }
}

#[test]
fn add_paper_row_tracks_the_missing_sheet_through_undo_and_redo() {
    for platform in Platform::ALL {
        let mut s = session(platform);
        assert!(s.state.layer_tools.add_paper.is_none());
        assert!(!s.command(CommandId::LowerLayer).enabled);
        let paper = paper_layer(&s);
        s.dispatch(UiAction::Layer { action: LayerAction::Delete { id: paper } }).unwrap();
        s.dispatch(UiAction::Layer { action: LayerAction::New { group: true, clipped: false } }).unwrap();
        let row = s.state.layer_tools.add_paper.clone().unwrap();
        assert_eq!(row.label, "Add paper");
        let before = s.engine.checkpoint();
        s.dispatch(UiAction::Effect { action: row.action }).unwrap();
        let paper = s.engine.document().bottom_paper().unwrap();
        assert_eq!(s.engine.document().working.occurrence, Some(paper));
        assert!(s.state.layer_tools.add_paper.is_none());
        invoke(&mut s, CommandId::Undo);
        assert_eq!(s.engine.checkpoint(), before);
        assert!(s.state.layer_tools.add_paper.is_some());
        assert!(s.engine.document().bottom_paper().is_none());
        invoke(&mut s, CommandId::Redo);
        assert_eq!(s.engine.document().bottom_paper(), Some(paper));
        assert!(s.state.layer_tools.add_paper.is_none());
    }
}

#[test]
fn selected_fills_keep_properties_and_mark_only_an_ungrouped_layer_drawable() {
    for platform in Platform::ALL {
        for (program, title) in [("paper", "Paper"), ("solid_color", "Solid Color"), ("gradient_fill", "Gradient Fill")] {
            let mut s = session(platform);
            s.dispatch(UiAction::Effect { action: EffectAction::Insert { effect: program.into() } }).unwrap();
            let fill = occurrence_token(s.engine.document().working.occurrence.unwrap());
            s.dispatch(UiAction::Layer { action: LayerAction::New { group: false, clipped: false } }).unwrap();
            let ink = occurrence_token(s.engine.document().working.occurrence.unwrap());
            s.dispatch(UiAction::SelectLayer { id: fill }).unwrap();
            assert_eq!(s.state.layer_properties.title, title);
            assert_eq!(s.state.layers.iter().filter(|row| row.drawing).map(|row| row.id).collect::<Vec<_>>(), [ink]);
            s.dispatch(UiAction::SelectLayer { id: ink }).unwrap();
            s.dispatch(UiAction::Layer { action: LayerAction::GroupSelected }).unwrap();
            s.dispatch(UiAction::SelectLayer { id: fill }).unwrap();
            assert!(s.state.layers.iter().all(|row| !row.drawing));
            assert_eq!(s.engine.document().drawing_target(), None);
            invoke(&mut s, CommandId::Undo);
            assert_eq!(s.state.layers.iter().filter(|row| row.drawing).map(|row| row.id).collect::<Vec<_>>(), [ink]);
            s.dispatch(UiAction::Layer { action: LayerAction::AddMask { id: fill, replace: false } }).unwrap();
            s.dispatch(UiAction::SelectLayer { id: fill }).unwrap();
            assert!(s.state.layers.iter().all(|row| row.id == fill || !row.drawing));
            assert!(s.state.layers.iter().find(|row| row.id == fill).unwrap().mask_selected);
        }
    }
}

#[test]
fn image_commands_carry_the_paper_and_scale_reports_it() {
    let mut s = session(Platform::Gtk);
    let layer = paper_layer(&s);
    s.dispatch(UiAction::SelectLayer { id: layer }).unwrap();
    invoke(&mut s, CommandId::ImageSize);
    assert_eq!(image_size_view(&s).resolution, 300.);
    image_size(&mut s, ImageSizeAction::Width { value: 2000. });
    image_size(&mut s, ImageSizeAction::Apply);
    assert!((paper_scale(&s) - 2.).abs() < 1e-4, "the paper grows with the marks");
    invoke(&mut s, CommandId::ImageSize);
    image_size(&mut s, ImageSizeAction::Resample { resample: ImageResample::Automatic });
    image_size(&mut s, ImageSizeAction::Resolution { value: 600. });
    image_size(&mut s, ImageSizeAction::Width { value: 2000. });
    image_size(&mut s, ImageSizeAction::Apply);
    assert!((paper_scale(&s) - 1.).abs() < 1e-4, "the same pixels against a finer print");
    s.dispatch(UiAction::Effect { action: EffectAction::Reset { layer, key: "scale".into() } }).unwrap();
    assert!((paper_scale(&s) - 1.).abs() < 1e-4);
    let mapping = s.engine.document().paper(None).mapping;
    assert!((mapping.0[0] - 600. / 25.4).abs() < 1e-9, "100% is real size at the drawing's resolution");
}

#[test]
fn paper_relief_shows_while_scaling_and_briefly_after_a_choice() {
    let mut s = session(Platform::Gtk);
    let layer = paper_layer(&s);
    s.dispatch(UiAction::SelectLayer { id: layer }).unwrap();
    let opacity = |s: &mut UiSession<Recorder>| s.renderer_mut().paper_overlay.as_ref().map(|overlay| overlay.opacity);
    s.frame(1_000_000_000, 1_000_000_000).unwrap();
    assert_eq!(opacity(&mut s), None);
    s.dispatch(UiAction::Effect { action: EffectAction::Set { layer, key: "texture".into(), value: layer_core::EffectValue::Choice(2) } }).unwrap();
    assert!(s.wants_continuous_frames());
    s.frame(2_000_000_000, 2_000_000_000).unwrap();
    assert_eq!(opacity(&mut s), Some(1.));
    assert_eq!(s.renderer_mut().paper_overlay.as_ref().unwrap().paper.surface.texture, layer_core::paper::built_in_paper("rough").unwrap().texture);
    s.frame(3_050_000_000, 3_050_000_000).unwrap();
    assert!(opacity(&mut s).is_some_and(|o| (o - 0.5).abs() < 1e-3), "fades out");
    s.frame(3_300_000_000, 3_300_000_000).unwrap();
    assert_eq!(opacity(&mut s), None);
    assert!(!s.wants_continuous_frames());
    let gesture = |phase, value| UiAction::Effect { action: EffectAction::Gesture { phase, action: Box::new(EffectAction::Number {
        layer, key: "scale".into(), operation: NumericOperation::Value { value } }) } };
    s.dispatch(gesture(ContactPhase::Down, 1.5)).unwrap();
    for (step, now) in [4_000_000_000u64, 6_000_000_000, 9_000_000_000].into_iter().enumerate() {
        s.dispatch(gesture(ContactPhase::Move, 1.5 + step as f64 * 0.1)).unwrap();
        s.frame(now, now).unwrap();
        assert_eq!(opacity(&mut s), Some(1.), "shown for the whole drag");
        let mapping = s.renderer_mut().paper_overlay.as_ref().unwrap().paper.surface.mapping;
        assert!((mapping.0[0] - (1.5 + step as f64 * 0.1) * 300. / 25.4).abs() < 1e-3, "shows the scale being set");
    }
    s.dispatch(gesture(ContactPhase::Up, 2.)).unwrap();
    s.frame(9_400_000_000, 9_400_000_000).unwrap();
    assert!(opacity(&mut s).is_some());
    s.frame(9_700_000_000, 9_700_000_000).unwrap();
    assert_eq!(opacity(&mut s), None, "hidden shortly after release");
}

#[test]
fn paper_relief_waits_for_preparation_and_survives_renderer_replacement() {
    let mut s = session(Platform::Windows);
    let layer = paper_layer(&s);
    s.dispatch(UiAction::SelectLayer { id: layer }).unwrap();
    s.renderer_mut().paper_overlay_pending = true;
    s.dispatch(UiAction::Effect { action: EffectAction::Set { layer, key: "texture".into(), value: layer_core::EffectValue::Choice(2) } }).unwrap();
    for now in [1_000_000_000, 4_000_000_000] {
        s.frame(now, now).unwrap();
        assert_eq!(s.renderer_mut().paper_overlay.as_ref().map(|o| o.opacity), Some(1.));
        assert!(s.wants_continuous_frames());
    }
    let color = s.engine.backend().document_color();
    s.replace_renderer(Recorder { color, paper_overlay_pending: true, ..Default::default() }).unwrap();
    assert!(s.wants_continuous_frames());
    s.frame(8_000_000_000, 8_000_000_000).unwrap();
    assert_eq!(s.renderer_mut().paper_overlay.as_ref().map(|o| o.opacity), Some(1.));
    s.renderer_mut().paper_overlay_pending = false;
    s.frame(12_000_000_000, 12_000_000_000).unwrap();
    assert_eq!(s.renderer_mut().paper_overlay.as_ref().map(|o| o.opacity), Some(1.));
    s.frame(13_050_000_000, 13_050_000_000).unwrap();
    assert!((s.renderer_mut().paper_overlay.as_ref().unwrap().opacity - 0.5).abs() < 1e-3);
    s.frame(13_300_000_000, 13_300_000_000).unwrap();
    assert!(s.renderer_mut().paper_overlay.is_none());
    assert!(!s.wants_continuous_frames());
}

fn gray_paper(value: u8) -> layer_core::color::source::SourceImage {
    use layer_core::color::source::{SourceBuilder, SourceChannels, SourceInterpretation};
    let interpretation = SourceInterpretation { channels: SourceChannels::Gray, depth: layer_core::color::SampleDepth::U8, profile: Default::default(), profile_assumed: false };
    let mut builder = SourceBuilder::new([8, 8], interpretation, 1 << 20).unwrap();
    for y in 0..8u8 { builder.push_row(&(0..8u8).map(|x| value.wrapping_add(x * 9 + y * 5)).collect::<Vec<_>>()).unwrap(); }
    let mut image = builder.finish().unwrap();
    image.resolution = Some(layer_core::ImageResolution::ppi(600));
    image
}

fn paper_requests(s: &UiSession<Recorder>) -> Vec<crate::paper_library::PaperRequest> {
    s.state.requests.iter().filter_map(|r| match &r.kind { HostRequestKind::Paper { request } => Some(request.clone()), _ => None }).collect()
}

#[test]
fn library_papers_load_when_chosen_and_new_drawings_remember_them() {
    use crate::paper_library::{PaperEntry, PaperRequest, paper_identity};
    let mut s = session(Platform::Gtk);
    let layer = paper_layer(&s);
    s.dispatch(UiAction::SelectLayer { id: layer }).unwrap();
    let picker = |s: &UiSession<Recorder>| paper_control(s, "texture").paper.unwrap();
    assert!(picker(&s).import.is_none(), "no Import without paper storage");
    let image = gray_paper(10);
    let id = paper_identity(&image).unwrap();
    let entry = PaperEntry { id: id.clone(), name: "scan_0042".into(), bytes: 64, preview: vec![128; 96 * 96].into() };
    s.set_paper_library(vec![entry.clone()]).unwrap();
    let view = picker(&s);
    assert_eq!(view.tiles.len(), 14);
    assert_eq!(view.tiles[13].label.as_ref(), "scan_0042");
    assert_eq!(view.tiles[13].actions.iter().map(|a| a.label.as_str()).collect::<Vec<_>>(), ["Rename…", "Delete"]);
    assert_eq!(view.import.as_ref().map(|a| a.action.clone()), Some(EffectAction::ImportPaper { layer }));
    let checkpoint = s.engine.checkpoint();
    s.dispatch(UiAction::Effect { action: EffectAction::Set { layer, key: "texture".into(), value: layer_core::EffectValue::Choice(13) } }).unwrap();
    assert_eq!(paper_requests(&s), [PaperRequest::Load { id: id.clone() }]);
    assert_eq!(s.engine.checkpoint(), checkpoint, "nothing changes until the paper is read");
    s.paper_loaded(&id, image.clone()).unwrap();
    let layer_core::TextureSource::Image(applied) = s.engine.document().paper(None).texture else { panic!("an image paper") };
    assert_eq!(*applied.storage().as_ref(), image);
    assert_eq!(applied.id(), crate::paper_library::paper_image_id(&id).unwrap());
    assert_eq!(paper_control(&s, "texture").value, layer_core::EffectValue::Choice(13));
    assert_eq!(s.state.settings.new_document.paper.as_deref(), Some(id.as_str()));
    assert_eq!(s.new_drawing_paper(), layer_core::TextureSource::Image(applied.clone()));
    invoke(&mut s, CommandId::Undo);
    assert_eq!(s.engine.checkpoint(), checkpoint, "one step");

    s.dispatch(UiAction::Effect { action: EffectAction::RenamePaper { id: id.clone(), name: " Cotton rag ".into() } }).unwrap();
    assert_eq!(paper_requests(&s).last(), Some(&PaperRequest::Rename { id: id.clone(), name: "Cotton rag".into() }));
    assert!(s.dispatch(UiAction::Effect { action: EffectAction::RenamePaper { id: id.clone(), name: " ".into() } }).is_err());
    s.dispatch(UiAction::Effect { action: EffectAction::ImportPaper { layer } }).unwrap();
    assert_eq!(paper_requests(&s).last(), Some(&PaperRequest::Import { layer }));
    let imported = gray_paper(90);
    let imported_id = paper_identity(&imported).unwrap();
    let entries = vec![entry, PaperEntry { id: imported_id.clone(), name: "linen scan".into(), bytes: 64, preview: vec![90; 96 * 96].into() }];
    s.paper_imported(layer, entries.clone(), &imported_id, imported).unwrap();
    assert_eq!(paper_control(&s, "texture").value, layer_core::EffectValue::Choice(14));
    assert_eq!(s.state.settings.new_document.paper.as_deref(), Some(imported_id.as_str()));
    assert!(matches!(s.new_drawing_paper(), layer_core::TextureSource::Image(_)));
    s.set_paper_library(entries[..1].to_vec()).unwrap();
    assert_eq!(s.new_drawing_paper(), layer_core::paper::default_texture(), "a deleted paper is not remembered");
    assert!(matches!(s.engine.document().paper(None).texture, layer_core::TextureSource::Image(_)), "the drawing keeps its copy");

    let mut reopened = UiSession::new(Recorder::default(), s.engine.document().clone(), [1000, 1000], Platform::Gtk).unwrap();
    let paper = paper_layer(&reopened);
    reopened.dispatch(UiAction::SelectLayer { id: paper }).unwrap();
    let own = paper_control(&reopened, "texture").paper.unwrap();
    assert_eq!(own.tiles[0].label.as_ref(), "This drawing’s paper", "another library does not know the image");
    assert!(own.tiles[0].actions.is_empty(), "nothing to add to without paper storage");
    reopened.set_paper_library(Vec::new()).unwrap();
    let own = paper_control(&reopened, "texture").paper.unwrap();
    assert_eq!(own.tiles[0].actions.iter().map(|a| a.action.clone()).collect::<Vec<_>>(), [EffectAction::AddPaper { layer: paper }]);
    reopened.dispatch(UiAction::Effect { action: own.tiles[0].actions[0].action.clone() }).unwrap();
    assert!(matches!(paper_requests(&reopened).last(), Some(PaperRequest::Add { layer, .. }) if *layer == paper));
    assert!(reopened.paper_image(paper).is_some());
}

#[test]
fn a_late_library_read_cannot_replace_a_newer_paper_choice() {
    use crate::paper_library::{PaperEntry, paper_identity};
    for platform in [Platform::Gtk, Platform::Mac, Platform::Ios, Platform::Windows] {
        for different in [false,true] {
            let mut s = session(platform);
            let layer = paper_layer(&s);
            s.dispatch(UiAction::SelectLayer { id: layer }).unwrap();
            let current = paper_control(&s,"texture").value;
            let image = gray_paper(90);
            let id = paper_identity(&image).unwrap();
            s.set_paper_library(vec![PaperEntry { id: id.clone(), name: "linen scan".into(), bytes: 64,
                preview: vec![90;96 * 96].into() }]).unwrap();
            s.dispatch(UiAction::Effect { action: EffectAction::Set { layer, key: "texture".into(), value: layer_core::EffectValue::Choice(13) } }).unwrap();
            let request = s.state.requests.iter().find(|request|matches!(&request.kind,HostRequestKind::Paper { .. })).unwrap().id;
            let value = if different { layer_core::EffectValue::Choice(2) } else { current };
            s.dispatch(UiAction::Effect { action: EffectAction::Set { layer, key: "texture".into(), value: value.clone() } }).unwrap();
            let document = s.engine.document().clone();
            let mark = s.engine.checkpoint();
            let modified = s.state.document_file.modified;
            s.paper_loaded(&id,image).unwrap();
            s.dispatch(UiAction::CompleteRequest { id:request,error:None }).unwrap();
            assert_live_artwork_eq(s.engine.document(),&document);
            assert_eq!(s.engine.checkpoint(),mark,"the stale read must not add history");
            assert_eq!(s.state.document_file.modified,modified);
            assert_eq!(paper_control(&s,"texture").value,value);
            assert!(!s.state.requests.iter().any(|item|item.id==request));
        }
    }
}

#[test]
fn scaling_paper_preserves_the_pending_library_choice() {
    use crate::paper_library::{PaperEntry, paper_identity};
    for platform in [Platform::Mac, Platform::Windows] {
        let mut s = session(platform);
        let layer = paper_layer(&s);
        s.dispatch(UiAction::SelectLayer { id:layer }).unwrap();
        let image = gray_paper(90);let id = paper_identity(&image).unwrap();
        s.set_paper_library(vec![PaperEntry { id:id.clone(),name:"linen scan".into(),bytes:64,preview:vec![90;96 * 96].into() }]).unwrap();
        s.dispatch(UiAction::Effect { action:EffectAction::Set {layer,key:"texture".into(),value:layer_core::EffectValue::Choice(13)} }).unwrap();
        s.dispatch(UiAction::Effect { action:EffectAction::Number {layer,key:"scale".into(),operation:NumericOperation::Value {value:1.5}} }).unwrap();
        s.paper_loaded(&id,image.clone()).unwrap();
        assert_eq!(*s.paper_image(layer).unwrap(),image);
        assert!((paper_scale(&s)-1.5).abs()<1e-4);
    }
}
