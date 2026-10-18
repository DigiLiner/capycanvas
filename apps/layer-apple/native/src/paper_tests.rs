use super::*;
use std::io::{Seek as _, Write as _};
use std::os::fd::AsRawFd;

struct PaperJob(*mut CapyPaperTask);
impl PaperJob {
    fn new(app: &App, request: u32, directory: &std::path::Path) -> Self {
        let directory = CString::new(directory.to_str().unwrap()).unwrap();
        let job = Self(unsafe { capy_apple_paper_task(app.0,request,(&*app.0).host.session.state().document_file.epoch,directory.as_ptr()) });
        assert!(!job.0.is_null()); job
    }
    fn work(&self, file: Option<&std::fs::File>) {
        let pointer = self.0 as usize;
        let fd = file.map_or(-1, |file| file.as_raw_fd());
        std::thread::spawn(move || unsafe { capy_paper_work(pointer as *const CapyPaperTask,fd,c"Scan".as_ptr()) }).join().unwrap();
        let error = unsafe { capy_paper_error(self.0) };
        assert!(error.is_null(), "{}", unsafe { CStr::from_ptr(error) }.to_string_lossy());
    }
    fn apply(&self, app: &App) { assert_eq!(unsafe { capy_apple_paper_apply(app.0,self.0) },0); }
}
impl Drop for PaperJob { fn drop(&mut self) { unsafe { capy_paper_free(self.0) }; } }
fn layer(app: &App) -> u64 {
    layer_ui::occurrence_token(layer_core::paper::paper_occurrence(unsafe { &*app.0 }.host.session.engine().document().scene(),None).unwrap())
}
fn paper_request(app: &App) -> u32 {
    unsafe { &*app.0 }.host.session.state().requests.iter().find_map(|item|
        matches!(item.kind,layer_ui::HostRequestKind::Paper { .. }).then_some(item.id)).unwrap()
}
fn checkpoint(app: &App) -> u64 {
    unsafe { &*app.0 }.host.session.engine().checkpoint()
}
fn document(app: &App) -> layer_core::Document { unsafe { &*app.0 }.host.session.engine().document().clone() }
fn choice(app: &App) -> u32 {
    let state = unsafe { &*app.0 }.host.session.state();
    let control = state.layer_properties.controls.iter().find(|control|control.key=="texture").unwrap();
    let layer_core::EffectValue::Choice(value) = &control.value else { panic!("paper choice") }; *value
}
fn effect(app: &App, action: Value) { app.action(json!({"type":"effect","action":action})); }
fn gray_png() -> std::fs::File {
    use layer_core::color::source::{SourceBuilder,SourceInterpretation,SourceChannels};
    let mut image = SourceBuilder::new([40,30],SourceInterpretation { channels:SourceChannels::Gray,
        depth:layer_core::color::SampleDepth::U8,profile:Default::default(),profile_assumed:false },1 << 24).unwrap();
    for y in 0..30u8 { image.push_row(&(0..40u8).map(|x|x.wrapping_mul(3).wrapping_add(y)).collect::<Vec<_>>()).unwrap(); }
    let mut file = fixtures::tempfile();
    file.write_all(&layer_core::paper::encode_png(&image.finish().unwrap()).unwrap()).unwrap(); file.rewind().unwrap(); file
}

#[test]
fn apple_paper_library_import_reload_rename_delete_and_add_preserve_authored_history() {
    for platform in [0,1] {
        let directory = std::env::temp_dir().join(format!("capy-apple-paper-{}",layer_workspace::new_id()));
        let app = App::new(platform);
        app.action(json!({"type":"select_layer","id":layer(&app)}));
        let initial = document(&app); let initial_checkpoint = checkpoint(&app);
        let load = PaperJob::new(&app,0,&directory); load.work(None); load.apply(&app);
        assert_project_document(&document(&app),&initial); assert_eq!(checkpoint(&app),initial_checkpoint);
        assert!(!app.state()["document_file"]["modified"].as_bool().unwrap());
        assert!(app.state()["layer_properties"]["controls"].as_array().unwrap().iter()
            .find(|control|control["key"]=="texture").unwrap()["paper"]["import"].is_object());
        effect(&app,json!({"op":"import_paper","layer":layer(&app)}));
        let id = paper_request(&app); let job = PaperJob::new(&app,id,&directory); let input = gray_png();
        job.work(Some(&input));
        assert_project_document(&document(&app),&initial); assert_eq!(checkpoint(&app),initial_checkpoint);
        assert!(unsafe { &*app.0 }.host.session.state().requests.iter().any(|item|item.id==id));
        job.apply(&app);
        assert!(!unsafe { &*app.0 }.host.session.state().requests.iter().any(|item|item.id==id));
        let imported = document(&app); let imported_checkpoint = checkpoint(&app); assert_ne!(imported_checkpoint,initial_checkpoint);
        assert!(app.state()["document_file"]["modified"].as_bool().unwrap());
        let entries = layer_host::papers::list(&directory).unwrap(); assert_eq!(entries.len(),1); let paper_id = entries[0].id.clone();
        let original = unsafe { &*app.0 }.host.session.paper_image(layer(&app)).unwrap();
        assert_source_samples(&original,&layer_host::papers::load(&directory,&paper_id).unwrap());
        app.invoke("undo"); assert_eq!(checkpoint(&app),initial_checkpoint); assert_project_document(&document(&app),&initial);
        app.invoke("redo"); assert_eq!(checkpoint(&app),imported_checkpoint); assert_project_document(&document(&app),&imported);
        for (operation,action) in [json!({"op":"rename_paper","id":paper_id,"name":"Cotton rag"}),json!({"op":"delete_paper","id":paper_id}),
            json!({"op":"add_paper","layer":layer(&app)})].into_iter().enumerate() {
            effect(&app,action); let id = paper_request(&app); let job = PaperJob::new(&app,id,&directory);job.work(None);job.apply(&app);
            assert_eq!(checkpoint(&app),imported_checkpoint); assert_project_document(&document(&app),&imported);
            assert!(app.state()["document_file"]["modified"].as_bool().unwrap());
            assert!(!unsafe { &*app.0 }.host.session.state().requests.iter().any(|item|item.id==id));
            let entries=layer_host::papers::list(&directory).unwrap();
            match operation {
                0 => { assert_eq!(entries.len(),1);assert_eq!(entries[0].id,paper_id);assert_eq!(entries[0].name,"Cotton rag"); }
                1 => assert!(entries.is_empty()),
                2 => { assert_eq!(entries.len(),1);assert_eq!(entries[0].id,paper_id);assert_source_samples(&layer_host::papers::load(&directory,&paper_id).unwrap(),&original); }
                _ => unreachable!(),
            }
        }
        for _ in 0..2 {
            let pending=unsafe { &*app.0 }.host.session.state().requests.iter().find_map(|item|
                matches!(item.kind,layer_ui::HostRequestKind::Paper { .. }).then_some(item.id));
            let Some(id)=pending else { break };
            let job=PaperJob::new(&app,id,&directory);job.work(None);job.apply(&app);
        }
        assert!(!unsafe { &*app.0 }.host.session.state().requests.iter().any(|item|matches!(item.kind,layer_ui::HostRequestKind::Paper { .. })));
        assert_eq!(checkpoint(&app),imported_checkpoint);assert_project_document(&document(&app),&imported);
        assert_eq!(layer_host::papers::list(&directory).unwrap().len(),1);
        effect(&app,json!({"op":"reset","layer":layer(&app),"key":"texture"}));
        let reset=checkpoint(&app);
        effect(&app,json!({"op":"set","layer":layer(&app),"key":"texture","value":{"kind":"choice","value":13}}));
        let pending=unsafe { &*app.0 }.host.session.state().requests.iter().find_map(|item|
            matches!(item.kind,layer_ui::HostRequestKind::Paper { .. }).then_some(item.id));
        if let Some(id)=pending {let job=PaperJob::new(&app,id,&directory);job.work(None);job.apply(&app);}
        assert_project_document(&document(&app),&imported);
        app.invoke("undo");assert_eq!(checkpoint(&app),reset);
        app.invoke("redo");assert_project_document(&document(&app),&imported);
        let reopened = App::new(platform); reopened.action(json!({"type":"select_layer","id":layer(&reopened)}));
        let load = PaperJob::new(&reopened,0,&directory);load.work(None);load.apply(&reopened);
        let before = checkpoint(&reopened);
        let index = unsafe { &*reopened.0 }.host.session.state().layer_properties.controls.iter()
            .find(|control|control.key=="texture").unwrap().paper.as_ref().unwrap().tiles.len()-1;
        effect(&reopened,json!({"op":"set","layer":layer(&reopened),"key":"texture","value":{"kind":"choice","value":index}}));
        assert_eq!(checkpoint(&reopened),before);
        let job = PaperJob::new(&reopened,paper_request(&reopened),&directory);job.work(None);job.apply(&reopened);
        assert_eq!(choice(&reopened),index as u32);
        assert_source_samples(&unsafe { &*reopened.0 }.host.session.paper_image(layer(&reopened)).unwrap(),&original);
        assert!(matches!(unsafe { &*reopened.0 }.host.session.new_drawing_paper(),layer_core::TextureSource::Image(_)));
        reopened.invoke("undo");assert_eq!(checkpoint(&reopened),before);
        std::fs::remove_dir_all(directory).unwrap();
    }
}

#[test]
fn apple_paper_retired_cancelled_and_replaced_requests_do_not_edit_the_current_document() {
    let directory = std::env::temp_dir().join(format!("capy-apple-paper-stale-{}",layer_workspace::new_id()));
    let app = App::new(1);app.action(json!({"type":"select_layer","id":layer(&app)}));
    let load=PaperJob::new(&app,0,&directory);load.work(None);load.apply(&app);
    for cancel in [false,true] {
        effect(&app,json!({"op":"import_paper","layer":layer(&app)}));
        let id=paper_request(&app);let job=PaperJob::new(&app,id,&directory);let input=gray_png();job.work(Some(&input));
        let before=document(&app);let mark=checkpoint(&app);let dirty=app.state()["document_file"]["modified"].clone();
        if cancel { unsafe { capy_paper_cancel(job.0) }; }
        else { app.action(json!({"type":"complete_request","id":id,"error":null})); }
        job.apply(&app);assert_project_document(&document(&app),&before);assert_eq!(checkpoint(&app),mark);
        assert_eq!(app.state()["document_file"]["modified"],dirty);
        assert!(!unsafe { &*app.0 }.host.session.state().requests.iter().any(|item|item.id==id));
    }
    effect(&app,json!({"op":"import_paper","layer":layer(&app)}));
    let id=paper_request(&app);let job=PaperJob::new(&app,id,&directory);let input=gray_png();job.work(Some(&input));
    app.action(json!({"type":"complete_request","id":id,"error":null}));
    unsafe { &mut *app.0 }.host.session.renderer_mut().0=Some(native_renderer());app.draw_until_idle();
    let old=document(&app).artwork.id;
    let replacement=ProjectJob::new(&app,true);assert_eq!(replacement.create([64,64]),0);
    assert_eq!(unsafe { capy_apple_project_adopt(app.0,replacement.0,c"Replacement".as_ptr(),c"".as_ptr()) },0,"{:?}",unsafe { &*app.0 }.error);
    app.action(json!({"type":"select_layer","id":layer(&app)}));
    effect(&app,json!({"op":"import_paper","layer":layer(&app)}));
    let current=paper_request(&app);assert!(current>id);
    let before=document(&app);let mark=checkpoint(&app);assert_ne!(before.artwork.id,old);
    let state=app.state();let requests=state["requests"].clone();let file=state["document_file"].clone();
    let history={let engine=unsafe { &*app.0 }.host.session.engine();(engine.can_undo(),engine.can_redo())};
    job.apply(&app);
    assert_project_document(&document(&app),&before);assert_eq!(checkpoint(&app),mark);
    assert_eq!(document(&app).artwork.id,before.artwork.id);
    assert_eq!(app.state()["requests"],requests);assert_eq!(paper_request(&app),current);
    assert_eq!(app.state()["document_file"],file);
    let engine=unsafe { &*app.0 }.host.session.engine();assert_eq!((engine.can_undo(),engine.can_redo()),history);
    std::fs::remove_dir_all(directory).unwrap();
}

#[test]
fn apple_paper_preview_leases_outlive_the_editor_and_match_shared_pixels() {
    let app=App::new(1);app.action(json!({"type":"select_layer","id":layer(&app)}));
    let state=unsafe { &*app.0 }.host.session.state();
    let mut ids:Vec<String>=state.layer_properties.controls.iter().find_map(|control|control.paper.as_ref()).unwrap()
        .tiles.iter().filter_map(|tile|tile.id.as_deref().map(str::to_string)).collect();
    ids.push("missing".into());
    let expected:Vec<_>=ids.iter().map(|id|unsafe { &*app.0 }.host.session.paper_preview(id)).collect();
    assert_eq!(expected.iter().filter(|image|image.is_some()).count(),13);
    let json=std::ffi::CString::new(serde_json::to_string(&ids).unwrap()).unwrap();
    assert!(unsafe { capy_apple_paper_previews(app.0,std::ptr::null()) }.is_null());
    let previews=unsafe { capy_apple_paper_previews(app.0,json.as_ptr()) };assert!(!previews.is_null());drop(app);
    assert_eq!(unsafe { capy_paper_previews_count(previews) },expected.len());
    for (index,image) in expected.iter().enumerate() {
        let mut bytes=std::ptr::null();let mut count=0;
        let side=unsafe { capy_paper_previews_read(previews,index,&mut bytes,&mut count) };
        if let Some(image)=image { assert_eq!(count,(side * side) as usize);assert_eq!(unsafe { std::slice::from_raw_parts(bytes,count) },image.as_ref()); }
        else { assert_eq!(side,0); }
    }
    unsafe { capy_paper_previews_free(previews) };
}

#[test]
fn apple_paper_worker_failure_retires_its_request_without_authored_changes() {
    let directory=std::env::temp_dir().join(format!("capy-apple-paper-error-{}",layer_workspace::new_id()));
    let app=App::new(1);app.action(json!({"type":"select_layer","id":layer(&app)}));
    effect(&app,json!({"op":"import_paper","layer":layer(&app)}));
    let id=paper_request(&app);let job=PaperJob::new(&app,id,&directory);
    let before=document(&app);let mark=checkpoint(&app);let dirty=app.state()["document_file"]["modified"].clone();
    unsafe { capy_paper_work(job.0,-1,c"Invalid input".as_ptr()) };
    let error=unsafe { capy_paper_error(job.0) };assert!(!error.is_null());unsafe { capy_apple_string_free(error) };
    job.apply(&app);assert_project_document(&document(&app),&before);assert_eq!(checkpoint(&app),mark);
    assert_eq!(app.state()["document_file"]["modified"],dirty);
    assert!(!unsafe { &*app.0 }.host.session.state().requests.iter().any(|item|item.id==id));
    assert!(app.state()["host_error"].is_string());
}

fn remembered_launch(directory: &std::path::Path, id: &str) -> String {
    let mut settings=layer_ui::Settings::default();settings.new_document.paper=Some(id.into());
    settings.new_document.defaults.extent=[513,257];settings.new_document.defaults.resolution=150;
    json!({"saved":serde_json::to_string(&settings).unwrap(),"preferred_languages":["en"],"paper_directory":directory}).to_string()
}

#[test]
fn apple_initial_paper_is_prepared_before_publication_and_later_library_reload_does_not_enqueue_another_load() {
    let directory=std::env::temp_dir().join(format!("capy-apple-paper-initial-{}",layer_workspace::new_id()));
    let mut input=gray_png();let mut encoded=Vec::new();std::io::Read::read_to_end(&mut input,&mut encoded).unwrap();
    let image=layer_core::paper::decode_png(&encoded).unwrap();
    let (id,paper,entries)=layer_host::papers::import(&directory,&image,"Cotton",&||false).unwrap();
    for platform in [0,1] {
        let configuration=CString::new(remembered_launch(&directory,&id)).unwrap();
        let app=App(unsafe {launch_json(platform,configuration.as_ptr(),std::ptr::null_mut(),std::ptr::null_mut())});assert!(!app.0.is_null());
        let document=document(&app);
        let layer_core::TextureSource::Image(actual)=document.paper(None).texture else {panic!("initial remembered image must exist before publication")};
        assert_source_samples(actual.storage(),&paper);
        assert_eq!(document.composition().size,[513,257]);assert_eq!(document.composition().resolution,Some(layer_core::ImageResolution::ppi(150)));
        assert!(!app.state()["document_file"]["modified"].as_bool().unwrap());
        assert!(!unsafe { &*app.0 }.host.session.engine().can_undo());assert!(!unsafe { &*app.0 }.host.session.engine().can_redo());
        let job=PaperJob::new(&app,0,&directory);job.work(None);job.apply(&app);
        assert!(!unsafe { &*app.0 }.host.session.state().requests.iter().any(|request|matches!(request.kind,
            layer_ui::HostRequestKind::Paper {..}|layer_ui::HostRequestKind::SaveSettings {..})));
        assert_eq!(layer_host::papers::list(&directory).unwrap(),entries);
    }
    std::fs::remove_dir_all(directory).unwrap();
}

#[test]
fn apple_explicit_new_resolves_the_captured_uncached_paper_on_its_existing_project_worker() {
    let directory=std::env::temp_dir().join(format!("capy-apple-paper-new-{}",layer_workspace::new_id()));
    let mut input=gray_png();let mut encoded=Vec::new();std::io::Read::read_to_end(&mut input,&mut encoded).unwrap();
    let image=layer_core::paper::decode_png(&encoded).unwrap();
    let (id,paper,_)=layer_host::papers::import(&directory,&image,"Cotton",&||false).unwrap();
    let configuration=CString::new(json!({"saved":"","preferred_languages":["en"],"paper_directory":directory}).to_string()).unwrap();
    let app=App(unsafe {launch_json(1,configuration.as_ptr(),std::ptr::null_mut(),std::ptr::null_mut())});assert!(!app.0.is_null());
    unsafe { &mut *app.0 }.host.session.renderer_mut().0=Some(native_renderer());
    assert_eq!(unsafe {capy_apple_resize(app.0,1200,900,1.)},0);app.draw_frame();
    let mut settings=unsafe { &*app.0 }.host.session.state().settings.clone();settings.new_document.paper=Some(id.clone());
    app.action(json!({"type":"restore_saved_settings","saved":serde_json::to_string(&settings).unwrap()}));
    assert!(!matches!(unsafe { &*app.0 }.host.session.new_drawing_paper(),layer_core::TextureSource::Image(_)),"the capture begins uncached");
    let before=document(&app);let mark=checkpoint(&app);
    let job=ProjectJob::new(&app,true);let pointer=job.0 as usize;
    assert_eq!(std::thread::spawn(move ||unsafe {capy_project_read(pointer as *const CapyProjectTask,-1,c"Untitled".as_ptr())}).join().unwrap(),0);
    assert_project_document(&document(&app),&before);assert_eq!(checkpoint(&app),mark);
    assert_eq!(unsafe {capy_apple_project_adopt(app.0,job.0,c"Untitled".as_ptr(),c"".as_ptr())},0);
    let layer_core::TextureSource::Image(actual)=document(&app).paper(None).texture else {panic!("New must use the worker-resolved remembered image")};
    assert_source_samples(actual.storage(),&paper);
    assert!(!app.state()["document_file"]["modified"].as_bool().unwrap());
    assert!(!unsafe { &*app.0 }.host.session.engine().can_undo());assert!(!unsafe { &*app.0 }.host.session.engine().can_redo());
    assert_eq!(unsafe { &*app.0 }.host.session.state().settings.new_document.paper.as_deref(),Some(id.as_str()));
    assert!(!unsafe { &*app.0 }.host.session.state().requests.iter().any(|request|matches!(request.kind,layer_ui::HostRequestKind::SaveSettings {..})));
    std::fs::remove_dir_all(directory).unwrap();
}

#[test]
fn apple_failed_initial_material_notifies_once_and_reload_does_not_repeat_the_failed_default_request() {
    let directory=std::env::temp_dir().join(format!("capy-apple-paper-initial-failure-{}",layer_workspace::new_id()));
    let mut input=gray_png();let mut encoded=Vec::new();std::io::Read::read_to_end(&mut input,&mut encoded).unwrap();
    let image=layer_core::paper::decode_png(&encoded).unwrap();
    let (id,_,_)=layer_host::papers::import(&directory,&image,"Cotton",&||false).unwrap();
    std::fs::write(directory.join(format!("{id}.png")),b"invalid PNG").unwrap();
    let configuration=CString::new(remembered_launch(&directory,&id)).unwrap();
    let app=App(unsafe {launch_json(1,configuration.as_ptr(),std::ptr::null_mut(),std::ptr::null_mut())});assert!(!app.0.is_null());
    let before=document(&app);let mark=checkpoint(&app);let notice=app.state()["notice"]["id"].clone();assert!(!notice.is_null());
    assert_eq!(before.paper(None).texture,layer_core::paper::default_texture());
    for _ in 0..2 {let job=PaperJob::new(&app,0,&directory);job.work(None);job.apply(&app);}
    assert_project_document(&document(&app),&before);assert_eq!(checkpoint(&app),mark);
    assert_eq!(app.state()["notice"]["id"],notice);
    assert_eq!(unsafe { &*app.0 }.host.session.state().settings.new_document.paper.as_deref(),Some(id.as_str()));
    assert!(!unsafe { &*app.0 }.host.session.state().requests.iter().any(|request|matches!(request.kind,
        layer_ui::HostRequestKind::Paper {..}|layer_ui::HostRequestKind::SaveSettings {..})));
    app.action(json!({"type":"select_layer","id":layer(&app)}));
    effect(&app,json!({"op":"set","layer":layer(&app),"key":"texture","value":{"kind":"choice","value":13}}));
    let request=paper_request(&app);let job=PaperJob::new(&app,request,&directory);
    unsafe {capy_paper_work(job.0,-1,c"Cotton".as_ptr())};
    let error=unsafe {capy_paper_error(job.0)};assert!(!error.is_null());unsafe {capy_apple_string_free(error)};job.apply(&app);
    assert!(!unsafe { &*app.0 }.host.session.state().requests.iter().any(|item|item.id==request));
    assert!(app.state()["host_error"].is_string());assert_project_document(&document(&app),&before);
    let job=PaperJob::new(&app,0,&directory);job.work(None);job.apply(&app);
    assert!(!unsafe { &*app.0 }.host.session.state().requests.iter().any(|request|matches!(request.kind,layer_ui::HostRequestKind::Paper {..})));
    std::fs::remove_dir_all(directory).unwrap();
}
