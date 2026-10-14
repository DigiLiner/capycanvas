use layer_core::color::source::SourceImage;
use layer_ui::paper_library::{PAPER_LIBRARY_BYTES, PAPER_LIBRARY_ENTRIES, PaperEntry, PaperLibraryError, paper_identity, valid_paper_id};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

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
    layer_core::atomic_write(&directory.join(INDEX), |file| serde_json::to_writer(file, index).map_err(|e| e.to_string()))
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
        layer_core::atomic_write(&image(directory, &id), |file| std::io::Write::write_all(file, &encoded).map_err(|e| e.to_string()))?;
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

pub fn load(directory: &Path, id: &str) -> Result<SourceImage, String> {
    if !valid_paper_id(id) { return Err("Unknown paper".into()); }
    let paper = layer_core::paper::decode_png(&read_bounded(&image(directory, id), MAX_PAPER_BYTES)?)?;
    if paper_identity(&paper)? != id { return Err("The stored paper changed".into()); }
    Ok(paper)
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
}
