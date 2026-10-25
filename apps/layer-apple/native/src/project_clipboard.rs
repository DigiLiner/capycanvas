//! Pixel copies: frozen on the serial owner, composed and encoded on a file
//! worker. The application keeps the clip; the pasteboard carries its PNG and nonce.
use super::*;

/// # Safety
/// The task stays alive. Returns owned progress text for a large copy, or NULL.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn capy_project_clip_progress(task: u64) -> u64 {
    let Some(lease_task)=handles::shared::<CapyProjectTask>(task) else { return 0 };
    let task=&*lease_task;
    let state = task.state.lock().unwrap_or_else(|e| e.into_inner());
    let Payload::Clip { task: Some(clip), operation, .. } = &state.payload else { return 0 };
    let details = clip.capture_details();
    if !details.large { return 0; }
    CString::new(operation.title(&task.localization).to_string()).map_or(0, handles::text)
}

/// # Safety
/// Worker only. Composes and encodes the copy; `nonce` is NUL-terminated UTF-8.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn capy_project_clip_run(task: u64, nonce: *const c_char) -> i32 {
    let Some(lease_task)=handles::shared::<CapyProjectTask>(task) else { return -1 };
    let task=&*lease_task;
    let nonce = unsafe { read_title(nonce) };
    task.perform(|payload| {
        let nonce = nonce?;
        let Payload::Clip { task: capture, clip, .. } = payload else { return Err("Not a copy task".into()) };
        let capture = capture.take().ok_or("This copy has already run")?;
        *clip = Some(Box::new(capture.run(nonce, task.control.clone())?));
        Ok(())
    })
}

/// # Safety
/// Worker only, after `capy_project_clip_run`. The PNG stays borrowed from the
/// task until it is adopted or freed. Outputs are aligned and exclusively writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn capy_project_clip_png(task: u64, count: *mut usize) -> i32 {
    if count.is_null() {return -1;}
    unsafe {count.write(0);}
    let Some(lease_task)=handles::shared::<CapyProjectTask>(task) else { return -1 };
    let task=&*lease_task;
    let state = task.state.lock().unwrap_or_else(|e| e.into_inner());
    let Payload::Clip { clip: Some(clip), .. } = &state.payload else { return -1 };
    unsafe { count.write(clip.png.len()); }
    0
}

/// # Safety
/// Nonempty output is writable and exclusive for length bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn capy_project_clip_png_copy(task:u64,offset:usize,output:*mut u8,length:usize)->bool {
    let Some(task)=handles::shared::<CapyProjectTask>(task) else {return false;};
    let state=task.state.lock().unwrap_or_else(|e|e.into_inner());
    let Payload::Clip {clip:Some(clip),..}=&state.payload else {return false;};
    unsafe {layer_host::ffi::copy_bytes(&clip.png,offset,output,length)}
}

/// # Safety
/// The prepared task stays alive; called on the pasteboard thread after writing.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn capy_project_clip_published(task: u64) {
    let Some(lease_task)=handles::shared::<CapyProjectTask>(task) else { return };
    let task=&*lease_task;
    let mut state = task.state.lock().unwrap_or_else(|e| e.into_inner());
    if let Payload::Clip { publication, clip: Some(_), .. } = &mut state.payload {
        publication.get_or_insert_with(layer_ui::RetainedClipboard::publication);
    }
}

pub(super) fn adopt_clip(app: &mut CapyApple, task: &CapyProjectTask, clip: &mut Option<Box<PixelClip>>, publication: u64, request: u32) -> Result<(), String> {
    let clip = clip.take().ok_or("The copy did not finish")?;
    if task.begin_commit() < 0 { return Err("Document operation cancelled".into()); }
    app.window.documents.clip.set_published(*clip, publication);
    let previous = app.host.session.state().revision;
    let mut change = app.host.session.complete_document_request(request, Ok(true))?;
    change.canvas_wake = true;
    app.host.apply_change(previous, change);
    Ok(())
}

/// # Safety
/// Session owner only. Returns the nonce of the retained copy, or NULL.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn capy_apple_clip_nonce(app: u64) -> u64 {
    let Some(mut lease_app)=handles::app(app) else { return 0 };
    let app=&mut *lease_app;
    app.window.documents.clip.get()
        .and_then(|clip| CString::new(clip.nonce.as_str()).ok())
        .map_or(0, handles::text)
}
