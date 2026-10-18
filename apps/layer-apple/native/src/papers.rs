use super::*;
use layer_core::color::source::SourceImage;
use layer_ui::paper_library::{PaperEntry, PaperRequest};
use std::{fs::File, mem::ManuallyDrop, os::fd::FromRawFd, path::PathBuf,
    sync::{Arc, Mutex, atomic::{AtomicBool, Ordering}}};

enum PaperResult {
    Entries(Vec<PaperEntry>),
    Loaded(String, SourceImage),
    Imported(u64, String, SourceImage, Vec<PaperEntry>),
}
pub struct CapyPaperTask {
    request: u32,
    action: Option<PaperRequest>,
    directory: PathBuf,
    epoch: u64,
    document: layer_core::authored::PortableId,
    image: Option<Arc<SourceImage>>,
    retained: Vec<layer_core::Image>,
    localization: Arc<layer_ui::Localizer>,
    cancelled: AtomicBool,
    result: Mutex<Option<Result<PaperResult, String>>>,
}
impl CapyPaperTask {
    fn capture(app: &CapyApple, request: u32, expected_epoch: u64, directory: PathBuf) -> Result<Self, String> {
        let session = &app.host.session;
        if request != 0 && expected_epoch != session.state().document_file.epoch {
            return Err(layer_ui::DocumentDeliveryCopy::new(session.localization()).change_in_progress.to_string());
        }
        let action = if request == 0 { None } else {
            Some(session.state().requests.iter().find_map(|item| match &item.kind {
                layer_ui::HostRequestKind::Paper { request: action } if item.id == request => Some(action.clone()),
                _ => None,
            }).ok_or_else(|| layer_ui::paper_library::PaperLibraryError::Unsupported.message(session.localization()))?)
        };
        let image = match &action {
            Some(PaperRequest::Add { layer, .. }) => session.paper_image(*layer),
            _ => None,
        };
        let retained=if matches!(&action,Some(PaperRequest::Load { .. } | PaperRequest::Import { .. })) {session.paper_sources()}else{Vec::new()};
        Ok(Self { request, action, directory, epoch: session.state().document_file.epoch,
            document: session.engine().document().artwork.id, image, retained,
            localization: session.localization().clone(), cancelled: AtomicBool::new(false), result: Mutex::new(None) })
    }
    fn work(&self, fd: i32, name: &str) -> Result<PaperResult, String> {
        use layer_host::papers;
        let failed = |error: layer_ui::paper_library::PaperLibraryError| error.message(&self.localization);
        if self.cancelled.load(Ordering::Relaxed) { return Ok(PaperResult::Entries(papers::list(&self.directory)?)); }
        Ok(match &self.action {
            None => PaperResult::Entries(papers::list(&self.directory)?),
            Some(PaperRequest::Load { id }) => PaperResult::Loaded(id.clone(), papers::canonical_source(id,papers::load(&self.directory,id)?,&self.retained)?),
            Some(PaperRequest::Import { layer }) => {
                if fd < 0 { return Err(failed(layer_ui::paper_library::PaperLibraryError::Unsupported)); }
                let file = ManuallyDrop::new(unsafe { File::from_raw_fd(fd) });
                let mut images = layer_ui::ImageImportBatch::new(Default::default(), layer_core::color::RgbSpace::ProPhoto, Default::default());
                images.read(layer_core::Cancellable { inner: &*file, cancelled: || self.cancelled.load(Ordering::Relaxed) },
                    name, &self.cancelled).map_err(|e| e.to_string())?;
                let (_, source) = images.take_sources(false)?.into_iter().next()
                    .ok_or_else(|| failed(layer_ui::paper_library::PaperLibraryError::Unsupported))?;
                let (id, image, entries) = papers::import(&self.directory, &source, name,
                    &|| self.cancelled.load(Ordering::Relaxed)).map_err(failed)?;
                let image=papers::canonical_source(&id,image,&self.retained)?;
                PaperResult::Imported(*layer, id, image, entries)
            }
            Some(PaperRequest::Add { name, .. }) => {
                let image = self.image.as_ref().ok_or_else(|| failed(layer_ui::paper_library::PaperLibraryError::Unsupported))?;
                PaperResult::Entries(papers::store(&self.directory, image, name).map_err(failed)?.1)
            }
            Some(PaperRequest::Rename { id, name }) => PaperResult::Entries(papers::rename(&self.directory, id, name)?),
            Some(PaperRequest::Delete { id }) => PaperResult::Entries(papers::delete(&self.directory, id)?),
        })
    }
    fn apply(&self, app: &mut CapyApple) -> Result<(), String> {
        let mut result = self.result.lock().unwrap_or_else(|e| e.into_inner());
        let result = result.take().ok_or_else(|| layer_ui::paper_library::PaperLibraryError::Unsupported.message(&self.localization))?;
        let session = &mut app.host.session;
        if self.request != 0 && !session.state().requests.iter().any(|item| item.id == self.request) { return Ok(()); }
        let matches = self.epoch == session.state().document_file.epoch && self.document == session.engine().document().artwork.id;
        let cancelled = self.cancelled.load(Ordering::Relaxed);
        let previous = session.state().revision;
        let applied = match result {
            Ok(PaperResult::Entries(entries)) => session.set_paper_library(entries).map(Some),
            Ok(PaperResult::Loaded(id, image)) if !cancelled => session.paper_loaded(&id, image).map(Some),
            Ok(PaperResult::Imported(layer, id, image, entries)) => {
                if matches && !cancelled { session.paper_imported(layer, entries, &id, image).map(Some) }
                else { session.set_paper_library(entries).map(Some) }
            }
            Ok(PaperResult::Loaded(..)) => Ok(None),
            Err(_) if cancelled => Ok(None),
            Err(error) => Err(error),
        };
        let error = match applied {
            Ok(Some(change)) => { app.host.apply_change(previous, change); None },
            Ok(None) => None,
            Err(error) => Some(error),
        };
        if self.request != 0 {
            app.host.dispatch(layer_ui::UiAction::CompleteRequest { id: self.request, error })?;
        } else if let Some(error) = error { return Err(error); }
        Ok(())
    }
}

/// # Safety
/// app is exclusive on its serial owner; directory is readable NUL-terminated UTF-8. Return/free one independent job.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn capy_apple_paper_task(app: *mut CapyApple, request: u32, epoch: u64, directory: *const c_char) -> *mut CapyPaperTask {
    let Some(app) = (unsafe { app.as_mut() }) else { return std::ptr::null_mut() };
    app.perform(|app| CapyPaperTask::capture(app, request, epoch, PathBuf::from(unsafe { project::read_title(directory) }?))
        .map(|task| Box::into_raw(Box::new(task)))).unwrap_or(std::ptr::null_mut())
}
/// # Safety
/// task is live on one file worker; fd and NUL-terminated name remain readable through this call. Do not run twice.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn capy_paper_work(task: *const CapyPaperTask, fd: i32, name: *const c_char) {
    let Some(task) = (unsafe { task.as_ref() }) else { return };
    let result = catch_unwind(AssertUnwindSafe(|| task.work(fd, unsafe { project::read_title(name) }?)))
        .unwrap_or_else(|error| Err(panic_diagnostic(error)));
    *task.result.lock().unwrap_or_else(|e| e.into_inner()) = Some(result);
}
/// # Safety
/// app is exclusive on its serial owner; task work finished. Adopt a job at most once.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn capy_apple_paper_apply(app: *mut CapyApple, task: *const CapyPaperTask) -> i32 {
    let (Some(app), Some(task)) = (unsafe { app.as_mut() }, unsafe { task.as_ref() }) else { return -1 };
    app.perform(|app| task.apply(app).map(|_| 0)).unwrap_or(-1)
}
/// # Safety
/// task remains live throughout this thread-safe cancellation call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn capy_paper_cancel(task: *const CapyPaperTask) {
    if let Some(task) = unsafe { task.as_ref() } { task.cancelled.store(true, Ordering::Relaxed); }
}
/// # Safety
/// task is live on its file worker; message is readable NUL-terminated UTF-8.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn capy_paper_failed(task: *const CapyPaperTask, message: *const c_char) {
    if let Some(task) = unsafe { task.as_ref() } {
        *task.result.lock().unwrap_or_else(|e| e.into_inner()) = Some(Err(unsafe { project::read_title(message) }.unwrap_or_default().to_string()));
    }
}
/// # Safety
/// task work finished and task remains live; free returned owned text with capy_apple_string_free.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn capy_paper_error(task: *const CapyPaperTask) -> *mut c_char {
    let Some(task) = (unsafe { task.as_ref() }) else { return std::ptr::null_mut() };
    if task.cancelled.load(Ordering::Relaxed) { return std::ptr::null_mut(); }
    let result = task.result.lock().unwrap_or_else(|e| e.into_inner());
    result.as_ref().and_then(|result| result.as_ref().err())
        .and_then(|error| CString::new(error.as_str()).ok()).map_or(std::ptr::null_mut(), CString::into_raw)
}
/// # Safety
/// task is owned exactly once and no calls/borrowed data remain; drop on the file worker.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn capy_paper_free(task: *mut CapyPaperTask) {
    if !task.is_null() { unsafe { drop(Box::from_raw(task)) }; }
}

pub struct CapyPaperPreviews { images: Vec<Option<Arc<[u8]>>> }
/// # Safety
/// app is exclusive on its serial owner. Returned previews own immutable allocations independent of app.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn capy_apple_paper_previews(app: *mut CapyApple, layer: u64, epoch: u64) -> *mut CapyPaperPreviews {
    let Some(app) = (unsafe { app.as_mut() }) else { return std::ptr::null_mut() };
    let state = app.host.session.state();
    if state.document_file.epoch != epoch || state.layer_properties.layer != Some(layer) { return std::ptr::null_mut(); }
    let Some(picker) = state.layer_properties.controls.iter().find_map(|control| control.paper.as_ref()) else { return std::ptr::null_mut() };
    Box::into_raw(Box::new(CapyPaperPreviews { images: picker.tiles.iter().map(|tile| tile.preview.clone()).collect() }))
}
/// # Safety
/// previews remains live throughout this call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn capy_paper_previews_count(previews: *const CapyPaperPreviews) -> usize {
    unsafe { previews.as_ref() }.map_or(0, |previews| previews.images.len())
}
/// # Safety
/// previews is live; output pointers are writable. Borrowed bytes remain valid until previews_free.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn capy_paper_previews_read(previews: *const CapyPaperPreviews, index: usize, bytes: *mut *const u8, count: *mut usize) -> u32 {
    let (Some(previews), Some(bytes), Some(count)) = (unsafe { previews.as_ref() }, unsafe { bytes.as_mut() }, unsafe { count.as_mut() }) else { return 0 };
    let Some(Some(image)) = previews.images.get(index) else { return 0 };
    *bytes = image.as_ptr(); *count = image.len(); layer_core::paper::PREVIEW_SIZE as u32
}
/// # Safety
/// previews is owned exactly once; all readers finished. Drop on the image/file worker.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn capy_paper_previews_free(previews: *mut CapyPaperPreviews) {
    if !previews.is_null() { unsafe { drop(Box::from_raw(previews)) }; }
}
