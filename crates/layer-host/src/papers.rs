use layer_core::color::source::SourceImage;
use layer_core::package::session_store::atomic_publish;
use layer_ui::paper_library::{PAPER_LIBRARY_BYTES, PAPER_LIBRARY_ENTRIES, PaperEntry, PaperLibraryError, PaperRequest, paper_identity, valid_paper_id};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

static LIBRARY: Mutex<()> = Mutex::new(());
const INDEX: &str = "index.json";
const MAX_INDEX_BYTES: u64 = 1 << 20;
const MAX_PAPER_BYTES: u64 = 64 << 20;

#[derive(Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Index {
    papers: Vec<Named>,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Named {
    id: String,
    name: String,
}

fn image(directory: &Path, id: &str) -> PathBuf { directory.join(format!("{id}.png")) }
fn preview(directory: &Path, id: &str) -> PathBuf { directory.join(format!("{id}.preview")) }

fn read_bounded(path: &Path, limit: u64) -> Result<Vec<u8>, String> {
    use std::io::Read;
    let mut bytes = Vec::new();
    std::fs::File::open(path).map_err(|e| e.to_string())?.take(limit + 1).read_to_end(&mut bytes).map_err(|e| e.to_string())?;
    if bytes.len() as u64 > limit { return Err("Stored paper is too large".into()); }
    Ok(bytes)
}

fn read_index(directory: &Path) -> Result<Index, String> {
    match read_bounded(&directory.join(INDEX), MAX_INDEX_BYTES) {
        Ok(bytes) => serde_json::from_slice(&bytes).map_err(|e| e.to_string()),
        Err(_) if !directory.join(INDEX).exists() => Ok(Index::default()),
        Err(error) => Err(error),
    }
}

fn write_index(directory: &Path, index: &Index) -> Result<(), String> {
    std::fs::create_dir_all(directory).map_err(|e| e.to_string())?;
    atomic_publish(&directory.join(INDEX), |file| serde_json::to_writer(file, index).map_err(|e| e.to_string()), || Ok(()))
}

fn entries(directory: &Path, index: &Index) -> Vec<PaperEntry> {
    index.papers.iter().filter(|named| valid_paper_id(&named.id)).filter_map(|named| {
        let bytes = std::fs::metadata(image(directory, &named.id)).ok()?.len();
        let size = layer_core::paper::PREVIEW_SIZE * layer_core::paper::PREVIEW_SIZE;
        let preview = read_bounded(&preview(directory, &named.id), size as u64).ok().filter(|p| p.len() == size)
            .or_else(|| {
                let paper = layer_core::paper::decode_png(&read_bounded(&image(directory, &named.id), MAX_PAPER_BYTES).ok()?).ok()?;
                let preview = layer_core::paper::image_preview(&paper).ok()?;
                let _ = std::fs::write(self::preview(directory, &named.id), &preview);
                Some(preview)
            })?;
        Some(PaperEntry { id: named.id.clone(), name: named.name.clone(), bytes, preview: preview.into() })
    }).collect()
}

pub fn list(directory: &Path) -> Result<Vec<PaperEntry>, String> {
    let _guard = LIBRARY.lock().map_err(|_| "Paper library is unavailable")?;
    Ok(entries(directory, &read_index(directory)?))
}

pub fn store(directory: &Path, paper: &SourceImage, name: &str) -> Result<(String, Vec<PaperEntry>), PaperLibraryError> {
    let id = paper_identity(paper)?;
    let _guard = LIBRARY.lock().map_err(|_| "Paper library is unavailable".to_string())?;
    let mut index = read_index(directory)?;
    if !index.papers.iter().any(|named| named.id == id) {
        let encoded = layer_core::paper::encode_png(paper)?;
        let stored = entries(directory, &index);
        let total = stored.iter().map(|entry| entry.bytes).sum::<u64>() + encoded.len() as u64;
        if stored.len() >= PAPER_LIBRARY_ENTRIES || total > PAPER_LIBRARY_BYTES { return Err(PaperLibraryError::Full); }
        std::fs::create_dir_all(directory).map_err(|e| e.to_string())?;
        atomic_publish(&image(directory, &id), |file| std::io::Write::write_all(file, &encoded).map_err(|e| e.to_string()), || Ok(()))?;
        std::fs::write(preview(directory, &id), layer_core::paper::image_preview(paper)?).map_err(|e| e.to_string())?;
        let name = layer_ui::paper_library::paper_name(name).unwrap_or_else(|| id[..8].into());
        index.papers.push(Named { id: id.clone(), name });
        write_index(directory, &index)?;
    }
    Ok((id, entries(directory, &index)))
}

pub fn import(directory: &Path, source: &SourceImage, name: &str, cancelled: &dyn Fn() -> bool) -> Result<(String, SourceImage, Vec<PaperEntry>), PaperLibraryError> {
    let paper = layer_core::paper::paper_image(source, cancelled).map_err(|error|
        if error == layer_core::paper::CMYK { PaperLibraryError::Unsupported } else { PaperLibraryError::Failed(error) })?;
    let (id, entries) = store(directory, &paper, name)?;
    Ok((id, paper, entries))
}

#[derive(Clone)]
pub struct PreparedPaper {
    pub texture: layer_core::TextureSource,
    pub identity: Option<String>,
    pub warning: Option<String>,
}
impl PreparedPaper {
    pub fn apply<R: layer_render::CanvasRenderer>(self, session: &mut layer_ui::UiSession<R>) -> layer_ui::UiChange {
        session.initialize_paper(self.identity, self.texture, self.warning)
    }
}
pub fn resolve(material: &layer_ui::paper_library::PaperMaterial, directory: Option<&Path>, localization: &layer_ui::Localizer) -> PreparedPaper {
    use layer_ui::paper_library::PaperMaterial;
    match material {
        PaperMaterial::Ready(texture) => PreparedPaper { texture: texture.clone(), identity: None, warning: None },
        PaperMaterial::Stored { id, fallback } => {
            let loaded = directory.and_then(|directory| load(directory, id).ok());
            let texture = loaded.map(|image| layer_core::TextureSource::Image(layer_core::authored::Image::with_id(
                layer_ui::paper_library::paper_image_id(id).unwrap(), std::sync::Arc::new(image))));
            let warning = texture.is_none().then(|| layer_ui::file_open_failure(localization, &format!("{id}.png")));
            PreparedPaper { texture: texture.unwrap_or_else(|| fallback.clone()), identity: Some(id.clone()), warning }
        }
    }
}

pub fn load(directory: &Path, id: &str) -> Result<SourceImage, String> {
    if !valid_paper_id(id) { return Err("Unknown paper".into()); }
    let paper = layer_core::paper::decode_png(&read_bounded(&image(directory, id), MAX_PAPER_BYTES)?)?;
    if paper_identity(&paper)? != id { return Err("The stored paper changed".into()); }
    Ok(paper)
}

pub fn canonical_source(id: &str, source: SourceImage, retained: &[layer_core::Image]) -> Result<SourceImage, String> {
    let parent=layer_ui::paper_library::paper_image_id(id).ok_or("Unknown paper")?;
    if let Some(image)=retained.iter().find(|image|image.id()==parent) {
        if image.extent != source.extent || layer_core::paper::paper_ppi(image.resolution) != layer_core::paper::paper_ppi(source.resolution)
            || image.interpretation != source.interpretation || paper_identity(image.storage())? != id {return Err("The stored paper changed".into());}
        return Ok(image.storage().as_ref().clone());
    }
    Ok(source)
}

pub fn rename(directory: &Path, id: &str, name: &str) -> Result<Vec<PaperEntry>, String> {
    let name = layer_ui::paper_library::paper_name(name).ok_or("Name the paper")?;
    let _guard = LIBRARY.lock().map_err(|_| "Paper library is unavailable")?;
    let mut index = read_index(directory)?;
    index.papers.iter_mut().find(|named| named.id == id).ok_or("Unknown paper")?.name = name;
    write_index(directory, &index)?;
    Ok(entries(directory, &index))
}

pub fn delete(directory: &Path, id: &str) -> Result<Vec<PaperEntry>, String> {
    if !valid_paper_id(id) { return Err("Unknown paper".into()); }
    let _guard = LIBRARY.lock().map_err(|_| "Paper library is unavailable")?;
    let mut index = read_index(directory)?;
    index.papers.retain(|named| named.id != id);
    write_index(directory, &index)?;
    for path in [image(directory, id), preview(directory, id)] {
        match std::fs::remove_file(path) {
            Err(error) if error.kind() != std::io::ErrorKind::NotFound => return Err(error.to_string()),
            _ => {}
        }
    }
    Ok(entries(directory, &index))
}

pub enum PaperResult {
    Entries(Vec<PaperEntry>),
    Loaded(String, SourceImage),
    Imported(u64, String, SourceImage, Vec<PaperEntry>),
}

/// One library request, captured on the session owner, run on a file worker
/// and adopted back on the owner. A request of `None` lists the library.
pub struct PaperTask {
    request: Option<u32>,
    action: Option<PaperRequest>,
    directory: PathBuf,
    epoch: u64,
    document: layer_core::authored::PortableId,
    image: Option<Arc<SourceImage>>,
    retained: Vec<layer_core::Image>,
    localization: Arc<layer_ui::Localizer>,
}
impl PaperTask {
    pub fn capture<R: layer_render::CanvasRenderer>(session: &layer_ui::UiSession<R>, request: Option<u32>, directory: PathBuf) -> Result<Self, String> {
        let action = request.map(|request| session.state().requests.iter().find_map(|item| match &item.kind {
            layer_ui::HostRequestKind::Paper { request: action } if item.id == request => Some(action.clone()),
            _ => None,
        }).ok_or_else(|| PaperLibraryError::Unsupported.message(session.localization()))).transpose()?;
        let image = match &action { Some(PaperRequest::Add { layer, .. }) => session.paper_image(*layer), _ => None };
        let retained = if matches!(&action, Some(PaperRequest::Load { .. } | PaperRequest::Import { .. })) { session.paper_sources() } else { Vec::new() };
        Ok(Self { request, action, directory, epoch: session.state().document_file.epoch,
            document: session.engine().document().artwork.id, image, retained, localization: session.localization().clone() })
    }
    pub fn imports(&self) -> bool { matches!(self.action, Some(PaperRequest::Import { .. })) }
    pub fn work(&self, file: Option<(impl std::io::Read + std::io::Seek, &str)>, cancelled: &std::sync::atomic::AtomicBool) -> Result<PaperResult, String> {
        let failed = |error: PaperLibraryError| error.message(&self.localization);
        let stopped = || cancelled.load(std::sync::atomic::Ordering::Relaxed);
        if stopped() { return Ok(PaperResult::Entries(list(&self.directory)?)); }
        Ok(match &self.action {
            None => PaperResult::Entries(list(&self.directory)?),
            Some(PaperRequest::Load { id }) => PaperResult::Loaded(id.clone(), canonical_source(id, load(&self.directory, id)?, &self.retained)?),
            Some(PaperRequest::Import { layer }) => {
                let (file, name) = file.ok_or_else(|| failed(PaperLibraryError::Unsupported))?;
                let mut images = layer_ui::ImageImportBatch::new(Default::default(), layer_core::color::RgbSpace::ProPhoto, Default::default());
                images.read(layer_core::Cancellable { inner: file, cancelled: stopped }, name, cancelled).map_err(|e| e.to_string())?;
                let (_, source) = images.take_sources(false)?.into_iter().next().ok_or_else(|| failed(PaperLibraryError::Unsupported))?;
                let (id, image, entries) = import(&self.directory, &source, name, &stopped).map_err(failed)?;
                PaperResult::Imported(*layer, id.clone(), canonical_source(&id, image, &self.retained)?, entries)
            }
            Some(PaperRequest::Add { name, .. }) => {
                let image = self.image.as_ref().ok_or_else(|| failed(PaperLibraryError::Unsupported))?;
                PaperResult::Entries(store(&self.directory, image, name).map_err(failed)?.1)
            }
            Some(PaperRequest::Rename { id, name }) => PaperResult::Entries(rename(&self.directory, id, name)?),
            Some(PaperRequest::Delete { id }) => PaperResult::Entries(delete(&self.directory, id)?),
        })
    }
    pub fn apply(&self, host: &mut crate::NativeHost, result: Result<PaperResult, String>, cancelled: bool) -> Result<(), String> {
        let session = &mut host.session;
        if let Some(request) = self.request && !session.state().requests.iter().any(|item| item.id == request) { return Ok(()); }
        let current = self.epoch == session.state().document_file.epoch && self.document == session.engine().document().artwork.id;
        let previous = session.state().revision;
        let applied = match result {
            Ok(PaperResult::Entries(entries)) => session.set_paper_library(entries).map(Some),
            Ok(PaperResult::Loaded(id, image)) if !cancelled => session.paper_loaded(&id, image).map(Some),
            Ok(PaperResult::Loaded(..)) => Ok(None),
            Ok(PaperResult::Imported(layer, id, image, entries)) if current && !cancelled => session.paper_imported(layer, entries, &id, image).map(Some),
            Ok(PaperResult::Imported(_, _, _, entries)) => session.set_paper_library(entries).map(Some),
            Err(_) if cancelled => Ok(None),
            Err(error) => Err(error),
        };
        let error = match applied {
            Ok(Some(change)) => { host.apply_change(previous, change); None }
            Ok(None) => None,
            Err(error) => Some(error),
        };
        match self.request {
            Some(id) => host.dispatch(layer_ui::UiAction::CompleteRequest { id, error }),
            None => error.map_or(Ok(()), Err),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use layer_core::color::source::{SourceBuilder, SourceChannels, SourceInterpretation};

    fn photo(seed: u8) -> SourceImage {
        let interpretation = SourceInterpretation { channels: SourceChannels::Rgb, depth: layer_core::color::SampleDepth::U8, profile: Default::default(), profile_assumed: false };
        let mut builder = SourceBuilder::new([40, 30], interpretation, 1 << 24).unwrap();
        for y in 0..30u8 { builder.push_row(&(0..40u8).flat_map(|x| [seed.wrapping_add(x * 3), y * 4, x ^ y]).collect::<Vec<_>>()).unwrap(); }
        builder.finish().unwrap()
    }

    #[test]
    fn paper_tasks_list_import_rename_and_delete_through_the_session() {
        use layer_ui::{EffectAction, HostRequestKind, UiAction};
        let directory = std::env::temp_dir().join(format!("capy-paper-tasks-{}", layer_core::PortableId::random()));
        let mut host = crate::NativeHost::new(layer_ui::Platform::Android).unwrap();
        let layer = layer_ui::occurrence_token(layer_core::paper::paper_occurrence(host.session.engine().document().scene(), None).unwrap());
        host.dispatch(UiAction::SelectLayer { id: layer }).unwrap();
        let picker = |host: &crate::NativeHost| host.session.state().layer_properties.controls.iter().find_map(|control| control.paper.clone()).unwrap();
        let pending = |host: &crate::NativeHost| host.session.state().requests.iter()
            .find_map(|item| matches!(item.kind, HostRequestKind::Paper { .. }).then_some(item.id));
        let run = |host: &mut crate::NativeHost, request: Option<u32>, file: Option<&std::fs::File>| {
            let task = PaperTask::capture(&host.session, request, directory.clone()).unwrap();
            assert_eq!(task.imports(), file.is_some());
            let result = task.work(file.map(|file| (file, "Scan")), &std::sync::atomic::AtomicBool::new(false));
            task.apply(host, result, false).unwrap();
            assert_eq!(pending(host), None);
        };
        assert!(picker(&host).import.is_none());
        run(&mut host, None, None);
        assert!(picker(&host).import.is_some());
        assert!(!host.session.state().document_file.modified);
        let mut file = tempfile(&directory);
        std::io::Write::write_all(&mut file, &layer_core::paper::encode_png(&photo_paper()).unwrap()).unwrap();
        std::io::Seek::rewind(&mut file).unwrap();
        host.dispatch(UiAction::Effect { action: EffectAction::ImportPaper { layer } }).unwrap();
        let request = pending(&host);
        run(&mut host, request, Some(&file));
        let entries = list(&directory).unwrap();
        assert_eq!(entries.iter().map(|entry| entry.name.as_str()).collect::<Vec<_>>(), ["Scan"]);
        assert!(host.session.state().document_file.modified);
        let id = entries[0].id.clone();
        assert_eq!(paper_identity(&host.session.paper_image(layer).unwrap()).unwrap(), id);
        for (action, names) in [
            (EffectAction::RenamePaper { id: id.clone(), name: "Cotton rag".into() }, vec!["Cotton rag"]),
            (EffectAction::DeletePaper { id: id.clone() }, vec![]),
        ] {
            host.dispatch(UiAction::Effect { action }).unwrap();
            let request = pending(&host);
            run(&mut host, request, None);
            assert_eq!(list(&directory).unwrap().iter().map(|entry| entry.name.as_str()).collect::<Vec<_>>(), names);
        }
        assert_eq!(paper_identity(&host.session.paper_image(layer).unwrap()).unwrap(), id, "deleting an entry keeps the drawing's paper");
        std::fs::remove_dir_all(&directory).unwrap();
    }

    fn photo_paper() -> SourceImage {
        layer_core::paper::paper_image(&photo(7), &|| false).unwrap()
    }

    fn tempfile(directory: &Path) -> std::fs::File {
        std::fs::create_dir_all(directory).unwrap();
        std::fs::OpenOptions::new().read(true).write(true).create_new(true).open(directory.join("scan.png")).unwrap()
    }

    #[test]
    fn imported_papers_are_stored_once_named_renamed_and_deleted() {
        let directory = std::env::temp_dir().join(format!("capy-papers-{}", layer_core::PortableId::random()));
        assert!(list(&directory).unwrap().is_empty());
        let (id, paper, entries) = import(&directory, &photo(7), "scan_0042", &|| false).unwrap();
        assert_eq!(entries.iter().map(|e| (e.id.as_str(), e.name.as_str())).collect::<Vec<_>>(), [(id.as_str(), "scan_0042")]);
        assert_eq!(entries[0].preview.len(), layer_core::paper::PREVIEW_SIZE * layer_core::paper::PREVIEW_SIZE);
        assert_eq!(paper_identity(&load(&directory, &id).unwrap()).unwrap(), paper_identity(&paper).unwrap());
        let (again, entries) = store(&directory, &paper, "copy").unwrap();
        assert_eq!((again.as_str(), entries.len(), entries[0].name.as_str()), (id.as_str(), 1, "scan_0042"), "an equal image is stored once");
        let (second, _, _) = import(&directory, &photo(90), "linen", &|| false).unwrap();
        assert_eq!(rename(&directory, &id, " Cotton rag ").unwrap().iter().map(|e| e.name.as_str()).collect::<Vec<_>>(), ["Cotton rag", "linen"]);
        std::fs::remove_file(preview(&directory, &second)).unwrap();
        assert_eq!(list(&directory).unwrap()[1].preview.len(), layer_core::paper::PREVIEW_SIZE * layer_core::paper::PREVIEW_SIZE, "a lost preview is rebuilt");
        assert_eq!(delete(&directory, &id).unwrap().iter().map(|e| e.id.as_str()).collect::<Vec<_>>(), [second.as_str()]);
        assert!(load(&directory, &id).is_err());
        assert!(load(&directory, "../index").is_err());
        std::fs::remove_dir_all(&directory).unwrap();
    }

    #[cfg(windows)]
    #[test]
    fn locked_index_preserves_paper_names_until_publication_succeeds() {
        use std::os::windows::fs::OpenOptionsExt;
        let directory = std::env::temp_dir().join(format!("capy-papers-{}", layer_core::PortableId::random()));
        let (id, _, _) = import(&directory, &photo(7), "scan", &|| false).unwrap();
        let path = directory.join(INDEX);
        let original = std::fs::read(&path).unwrap();
        let locked = std::fs::OpenOptions::new().read(true).share_mode(1).open(&path).unwrap();
        assert!(rename(&directory, &id, "Cotton rag").is_err());
        assert_eq!(std::fs::read(&path).unwrap(), original);
        assert_eq!(list(&directory).unwrap()[0].name, "scan");
        drop(locked);
        assert_eq!(rename(&directory, &id, "Cotton rag").unwrap()[0].name, "Cotton rag");
        std::fs::remove_dir_all(&directory).unwrap();
    }

    #[test]
    fn remembered_material_prepares_initial_source_defaults_and_clean_history_before_construction() {
        let directory=std::env::temp_dir().join(format!("capy-paper-launch-{}",layer_core::PortableId::random()));
        let (id,image,_)=import(&directory,&photo(37),"Cotton",&||false).unwrap();
        let mut settings=layer_ui::Settings::default();settings.new_document.paper=Some(id.clone());
        settings.new_document.defaults.extent=[513,257];settings.new_document.defaults.resolution=150;
        let material=layer_ui::paper_library::PaperMaterial::capture(&settings.new_document,None);
        assert!(matches!(material,layer_ui::paper_library::PaperMaterial::Stored {..}));
        let localization=layer_ui::Localizer::shared(layer_ui::UiLanguage::English);
        let prepared=std::thread::spawn({let directory=directory.clone();let localization=localization.clone();
            move || resolve(&material,Some(&directory),&localization)}).join().unwrap();
        assert!(prepared.warning.is_none());assert_eq!(prepared.identity.as_deref(),Some(id.as_str()));
        std::fs::write(super::image(&directory,&id),b"changed after worker preparation").unwrap();
        for platform in [layer_ui::Platform::Mac,layer_ui::Platform::Ios] {
            let host=crate::NativeHost::launch_prepared(platform,settings.clone(),localization.clone(),prepared.clone()).unwrap();
            let document=host.session.engine().document();
            let layer_core::TextureSource::Image(source)=document.paper(None).texture else {panic!("prepared image paper")};
            assert_eq!(document.paper(None).texture,prepared.texture);
            let actual=source.storage();
            assert_eq!((actual.extent,actual.resolution,actual.interpretation.channels,actual.interpretation.depth,actual.interpretation.profile_assumed),
                (image.extent,image.resolution,image.interpretation.channels,image.interpretation.depth,image.interpretation.profile_assumed));
            assert_eq!(layer_color::profile_bytes(&actual.interpretation.profile).unwrap(),layer_color::profile_bytes(&image.interpretation.profile).unwrap());
            assert!(actual.tiles.keys().eq(image.tiles.keys()));
            for (coordinate,tile) in &image.tiles {
                let found=&actual.tiles[coordinate];
                assert_eq!(found.descriptor,tile.descriptor);
                assert_eq!(found.decode().unwrap(),tile.decode().unwrap());
            }
            assert_eq!(document.composition().size,[513,257]);
            assert_eq!(document.composition().resolution,Some(layer_core::ImageResolution::ppi(150)));
            assert!(!host.session.state().document_file.modified);
            assert!(!host.session.engine().can_undo());assert!(!host.session.engine().can_redo());
            assert_eq!(host.session.state().settings.new_document.paper.as_deref(),Some(id.as_str()));
            assert!(!host.session.state().requests.iter().any(|request|matches!(request.kind,layer_ui::HostRequestKind::SaveSettings {..})));
            assert_eq!(host.session.new_drawing_paper(),layer_core::TextureSource::Image(source));
        }
        std::fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn missing_corrupt_and_unavailable_remembered_material_falls_back_once_without_forgetting_or_retry_loop() {
        for failure in 0..3 {
            let directory=std::env::temp_dir().join(format!("capy-paper-failure-{}",layer_core::PortableId::random()));
            let (id,_,entries)=import(&directory,&photo(77),"Cotton",&||false).unwrap();
            if failure==0 {std::fs::remove_file(image(&directory,&id)).unwrap();}
            if failure==1 {std::fs::write(image(&directory,&id),b"invalid PNG").unwrap();}
            let mut settings=layer_ui::Settings::default();settings.new_document.paper=Some(id.clone());
            let localization=layer_ui::Localizer::shared(layer_ui::UiLanguage::Japanese);
            let material=layer_ui::paper_library::PaperMaterial::capture(&settings.new_document,None);
            let prepared=resolve(&material,(failure!=2).then_some(directory.as_path()),&localization);
            let expected=layer_ui::file_open_failure(&localization,&format!("{id}.png"));
            assert_eq!(prepared.warning.as_deref(),Some(expected.as_str()));assert_eq!(prepared.texture,settings.new_document.paper());
            let mut host=crate::NativeHost::launch_prepared(layer_ui::Platform::Mac,settings.clone(),localization,prepared).unwrap();
            let document=host.session.engine().document().clone();let mark=host.session.engine().checkpoint();
            let notice=host.session.state().notice.as_ref().unwrap().id;
            for _ in 0..2 {host.session.set_paper_library(entries.clone()).unwrap();}
            assert_eq!(host.session.engine().document(),&document);assert_eq!(host.session.engine().checkpoint(),mark);
            assert_eq!(host.session.state().notice.as_ref().unwrap().id,notice);
            assert_eq!(host.session.state().notice.as_ref().unwrap().text,expected);
            assert_eq!(host.session.state().settings.new_document.paper,settings.new_document.paper);
            assert!(!host.session.state().requests.iter().any(|request|matches!(request.kind,
                layer_ui::HostRequestKind::Paper {..}|layer_ui::HostRequestKind::SaveSettings {..})));
            assert!(!host.session.state().document_file.modified);assert!(!host.session.engine().can_undo());
            let layer=layer_ui::occurrence_token(layer_core::paper::paper_occurrence(host.session.engine().document().scene(),None).unwrap());
            host.dispatch(layer_ui::UiAction::SelectLayer {id:layer}).unwrap();
            host.dispatch(layer_ui::UiAction::Effect {action:layer_ui::EffectAction::Set {layer,key:"texture".into(),value:layer_core::EffectValue::Choice(13)}}).unwrap();
            assert_eq!(host.session.state().requests.iter().filter(|request|matches!(&request.kind,
                layer_ui::HostRequestKind::Paper {request:layer_ui::paper_library::PaperRequest::Load {id:requested}} if requested==&id)).count(),1,
                "an explicit user retry must remain available");
            std::fs::remove_dir_all(directory).unwrap();
        }
    }


    #[test]
    fn verified_paper_worker_reuses_retained_tiles_and_rejects_forged_full_keys_and_profiles() {
        use layer_core::Image;
        use std::sync::Arc;
        let source=|seed|layer_core::paper::paper_image(&photo(seed),&||false).unwrap();
        let original=source(42);let id=paper_identity(&original).unwrap();
        let parent=layer_ui::paper_library::paper_image_id(&id).unwrap();
        let retained=Image::with_id(parent,Arc::new(original));
        let decoded=layer_core::paper::decode_png(&layer_core::paper::encode_png(retained.storage()).unwrap()).unwrap();
        let actual=canonical_source(&id,decoded,&[retained.clone()]).unwrap();
        for (coordinate,tile) in &actual.tiles {assert!(Arc::ptr_eq(tile,&retained.tiles[coordinate]));}
        let wrong=Image::with_id(parent,Arc::new(source(91)));
        assert!(canonical_source(&id,source(42),&[wrong]).is_err());
        let mut changed=retained.storage().as_ref().clone();changed.interpretation.profile_assumed=true;
        assert_eq!(paper_identity(&changed).unwrap(),id,"sample hash does not establish interpretation");
        assert!(canonical_source(&id,source(42),&[Image::with_id(parent,Arc::new(changed))]).is_err());
        let mut forged=id.clone();let byte=if &id[63..]=="0" {"1"}else{"0"};forged.replace_range(63..64,byte);
        assert_eq!(layer_ui::paper_library::paper_image_id(&forged),Some(parent));
        assert!(canonical_source(&forged,source(42),&[retained]).is_err(),"the whole256-bit Paper key is verified on the worker");
    }
}
