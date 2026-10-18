use layer_core::authored::{Image, PortableId};
use layer_core::color::source::SourceImage;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{collections::BTreeMap, sync::{Arc, Weak}};

pub const PAPER_LIBRARY_ENTRIES: usize = 64;
pub const PAPER_LIBRARY_BYTES: u64 = 256 * 1024 * 1024;
const LOADED_IMAGES: usize = 3;
const MAX_NAME_CHARS: usize = 64;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PaperEntry {
    pub id: String,
    pub name: String,
    pub bytes: u64,
    #[serde(skip)]
    pub preview: Arc<[u8]>,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum PaperRequest {
    Import { layer: u64 },
    Add { layer: u64, name: String },
    Load { id: String },
    Rename { id: String, name: String },
    Delete { id: String },
}

#[derive(Clone, Debug, PartialEq)]
pub enum PaperMaterial {
    Ready(layer_core::TextureSource),
    Stored { id: String, fallback: layer_core::TextureSource },
}
impl PaperMaterial {
    pub fn capture(settings: &crate::NewDocumentSettings, cached: Option<Image>) -> Self {
        if let Some(image) = cached { return Self::Ready(layer_core::TextureSource::Image(image)); }
        let fallback = settings.paper();
        match settings.paper.as_ref().filter(|id| valid_paper_id(id)) {
            Some(id) => Self::Stored { id: id.clone(), fallback },
            None => Self::Ready(fallback),
        }
    }
    pub fn fallback(&self) -> layer_core::TextureSource {
        match self { Self::Ready(texture) | Self::Stored { fallback: texture, .. } => texture.clone() }
    }
}

pub fn paper_identity(image: &SourceImage) -> Result<String, String> {
    let mut hash = Sha256::new();
    for v in image.extent { hash.update(v.to_le_bytes()); }
    for v in layer_core::paper::paper_ppi(image.resolution) { hash.update(v.to_le_bytes()); }
    let mut rows = image.rows();
    let mut row = vec![0; image.row_bytes()];
    for y in 0..image.extent[1] {
        rows.read(y, &mut row)?;
        hash.update(&row);
    }
    Ok(hash.finalize().iter().map(|b| format!("{b:02x}")).collect())
}

pub fn valid_paper_id(id: &str) -> bool {
    id.len() == 64 && id.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

pub fn paper_image_id(id: &str) -> Option<PortableId> {
    if !valid_paper_id(id) { return None; }
    let bytes: Vec<u8> = (0..16).map(|i| u8::from_str_radix(&id[2 * i..2 * i + 2], 16).unwrap()).collect();
    Some(PortableId::from_bytes(bytes.try_into().unwrap()))
}

pub fn paper_name(name: &str) -> Option<String> {
    let name = name.trim();
    (!name.is_empty() && !name.chars().any(char::is_control)).then(|| name.chars().take(MAX_NAME_CHARS).collect())
}

#[derive(Clone, Debug, PartialEq)]
pub enum PaperLibraryError {
    Full,
    Unsupported,
    Failed(String),
}
impl PaperLibraryError {
    pub fn message(&self, localization: &crate::Localizer) -> String {
        match self {
            Self::Full => localization.text(crate::MessageId::RESOURCES_PAPER_LIBRARY_FULL).to_string(),
            Self::Unsupported => localization.text(crate::MessageId::RESOURCES_PAPER_UNSUPPORTED).to_string(),
            Self::Failed(detail) => detail.clone(),
        }
    }
}
impl From<String> for PaperLibraryError {
    fn from(detail: String) -> Self { Self::Failed(detail) }
}

pub(crate) struct PendingPaper {
    pub id: String,
    pub layer: u64,
    pub epoch: u64,
}

#[derive(Default)]
pub(crate) struct PaperLibrary {
    pub entries: Option<Vec<PaperEntry>>,
    images: Vec<(String, Image)>,
    sources: BTreeMap<String, Weak<SourceImage>>,
    pub pending: Option<PendingPaper>,
    pub prepared_default: Option<String>,
}
impl PaperLibrary {
    pub fn inherit(&mut self, previous: &Self) {
        self.entries.clone_from(&previous.entries);
        self.images.clone_from(&previous.images);
        self.sources.retain(|_, source| source.strong_count() != 0);
        for (id,source) in previous.sources.iter().filter(|(_,source)|source.strong_count()!=0) {self.sources.entry(id.clone()).or_insert_with(||source.clone());}
        for (id,image) in &mut self.images {
            if let Some(source)=self.sources.get(id).and_then(Weak::upgrade) {*image=Image::with_id(image.id(),source);}
        }
        self.prepared_default.clone_from(&previous.prepared_default);
    }
    pub fn image(&self, id: &str) -> Option<&Image> {
        self.images.iter().find(|(entry, _)| entry == id).map(|(_, image)| image)
    }
    pub fn set_entries(&mut self, entries: Vec<PaperEntry>) {
        self.images.retain(|(id, _)| entries.iter().any(|entry| entry.id == *id));
        self.sources.retain(|_, source| source.strong_count() != 0);
        self.entries = Some(entries);
    }
    pub fn retain_source(&mut self, id: &str, image: &Image) {
        if Some(image.id()) == paper_image_id(id) {self.sources.insert(id.to_string(),Arc::downgrade(image.storage()));}
    }
    pub fn keep(&mut self, id: &str, image: SourceImage) -> Option<Image> {
        self.keep_image(id, Image::with_id(paper_image_id(id)?, Arc::new(image)))
    }
    pub fn keep_image(&mut self, id: &str, image: Image) -> Option<Image> {
        if Some(image.id()) != paper_image_id(id) { return None; }
        self.sources.retain(|_, source| source.strong_count() != 0);
        let image = match self.sources.get(id).and_then(Weak::upgrade) {
            Some(source) => Image::with_id(image.id(), source),
            None => image,
        };
        self.sources.insert(id.to_string(), Arc::downgrade(image.storage()));
        self.images.retain(|(entry, _)| entry != id);
        self.images.push((id.to_string(), image.clone()));
        if self.images.len() > LOADED_IMAGES { self.images.remove(0); }
        Some(image)
    }
    pub fn entry_of(&self, image: &Image) -> Option<&PaperEntry> {
        self.entries.iter().flatten().find(|entry| paper_image_id(&entry.id) == Some(image.id()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn imported_papers_keep_one_identity_and_names_are_plain_text() {
        let id = "0123456789abcdef".repeat(4);
        assert!(valid_paper_id(&id));
        assert!(!valid_paper_id("../escape"));
        assert_eq!(paper_image_id(&id).unwrap().bytes(), [0x01, 0x23, 0x45, 0x67, 0x89, 0xab, 0xcd, 0xef, 0x01, 0x23, 0x45, 0x67, 0x89, 0xab, 0xcd, 0xef]);
        assert_eq!(paper_name("  scan_0042 "), Some("scan_0042".into()));
        assert_eq!(paper_name(" \n"), None);
        assert_eq!(paper_name(&"x".repeat(100)).unwrap().len(), 64);
    }

    #[test]
    fn paper_source_registry_does_not_retain_pixels_after_owners_retire() {
        let mut builder = layer_core::color::source::SourceBuilder::new([1,1],
            layer_core::color::source::SourceInterpretation { channels:layer_core::color::source::SourceChannels::Gray,
                depth:layer_core::color::SampleDepth::U8,profile:Default::default(),profile_assumed:false },1 << 20).unwrap();
        builder.push_row(&[127]).unwrap();
        let mut library = PaperLibrary::default();
        let source = builder.finish().unwrap();let id = paper_identity(&source).unwrap();
        let image = library.keep(&id,source).unwrap();
        let source = Arc::downgrade(image.storage());
        library.set_entries(Vec::new());
        assert!(source.upgrade().is_some());
        let mut inherited = PaperLibrary::default(); inherited.inherit(&library);
        assert!(inherited.sources[&id].upgrade().is_some());
        drop(image);
        assert!(source.upgrade().is_none(),"weak canonical owners must not retain pixels");
        library.set_entries(Vec::new()); inherited.set_entries(Vec::new());
        assert!(library.sources.is_empty() && inherited.sources.is_empty());
        assert!(library.images.is_empty() && inherited.images.is_empty());
    }
}
