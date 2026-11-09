use std::{
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::{Arc, Mutex, OnceLock, atomic::{AtomicBool, AtomicUsize, Ordering}},
    time::Duration,
};

const BUDGET: usize = 64 * 1024 * 1024;
const MAGIC: [u8; 8] = *b"CAPYPLC1";
static DIRECTORY: OnceLock<PathBuf> = OnceLock::new();

pub struct SavedPipelines {
    cache: wgpu::PipelineCache,
    path: PathBuf,
    compiled: AtomicUsize,
    saved: AtomicUsize,
    saving: AtomicBool,
    full: AtomicBool,
    published: Mutex<Option<(usize, u32)>>,
}

impl SavedPipelines {
    pub fn register(directory: PathBuf) { let _ = DIRECTORY.set(directory); }

    pub fn read_file(info: &wgpu::AdapterInfo) -> Option<Vec<u8>> {
        read_file(DIRECTORY.get()?, info)
    }

    pub fn open(device: &wgpu::Device, info: &wgpu::AdapterInfo, data: Option<Vec<u8>>) -> Option<Arc<Self>> {
        Self::open_at(DIRECTORY.get()?, device, info, data)
    }

    fn open_at(directory: &Path, device: &wgpu::Device, info: &wgpu::AdapterInfo, data: Option<Vec<u8>>) -> Option<Arc<Self>> {
        if !device.features().contains(wgpu::Features::PIPELINE_CACHE) { return None; }
        let path = cache_path(directory, info)?;
        let mut published = data.as_ref().map(|data| (data.len(), crc32fast::hash(data)));
        let create = |data: Option<&[u8]>| {
            let scope = device.push_error_scope(wgpu::ErrorFilter::Validation);
            // SAFETY: These bytes came from get_data of this app for this adapter
            // key and passed length and CRC checks. wgpu validates adapter and driver
            // keys and falls back to an empty cache.
            let cache = unsafe { device.create_pipeline_cache(&wgpu::PipelineCacheDescriptor {
                label: Some("Capy saved pipelines"), data, fallback: true,
            }) };
            (cache, pollster::block_on(scope.pop()))
        };
        let (cache, error) = create(data.as_deref());
        let cache = if error.is_some() {
            drop(cache);
            published = None;
            let _ = std::fs::remove_file(&path);
            let (cache, error) = create(None);
            if error.is_some() { return None; }
            cache
        } else { cache };
        Some(Arc::new(Self { cache, path, compiled: AtomicUsize::new(0), saved: AtomicUsize::new(0), saving: AtomicBool::new(false), full: AtomicBool::new(false), published: Mutex::new(published) }))
    }

    pub(crate) fn cache(&self) -> &wgpu::PipelineCache { &self.cache }
    pub(crate) fn compiled(&self) { self.compiled.fetch_add(1, Ordering::Release); }

    pub(crate) fn claim_save(self: &Arc<Self>) -> Option<Save> {
        (
        self.compiled.load(Ordering::Acquire) != self.saved.load(Ordering::Acquire) && !self.full.load(Ordering::Relaxed)
            && !self.saving.swap(true, Ordering::AcqRel)).then(||Save(self.clone()))
    }

    fn save(&self) {
        let epoch = self.compiled.load(Ordering::Acquire);
        if let Some(data) = self.cache.get_data() && let Ok(false) = write_file(&self.path, &data, &self.published) {
            self.full.store(true, Ordering::Relaxed);
        }
        self.saved.store(epoch, Ordering::Release);
    }
}

pub(crate) struct Save(Arc<SavedPipelines>);
impl Save {pub(crate) fn run(self) {self.0.save();}}
impl Drop for Save {fn drop(&mut self) {self.0.saving.store(false,Ordering::Release);}}

fn cache_path(directory: &Path, info: &wgpu::AdapterInfo) -> Option<PathBuf> {
    let backend = match info.backend {
        wgpu::Backend::Vulkan => "vulkan", wgpu::Backend::Dx12 => "dx12", wgpu::Backend::Metal => "metal",
        _ => return None,
    };
    let mut driver = crc32fast::Hasher::new();
    driver.update(info.driver.as_bytes());
    driver.update(&[0]);
    driver.update(info.driver_info.as_bytes());
    Some(directory.join(format!("pipelines-{}-{:x}-{:x}-{:08x}.bin", backend, info.vendor, info.device, driver.finalize())))
}

fn read_file(directory: &Path, info: &wgpu::AdapterInfo) -> Option<Vec<u8>> {
    let path = cache_path(directory, info)?;
    let (gpu, driver) = path.file_stem()?.to_str()?.rsplit_once('-')?;
    let prefix = format!("{gpu}-");
    if let Ok(entries) = std::fs::read_dir(directory) {
        for entry in entries.flatten() {
            if !entry.file_type().is_ok_and(|kind| kind.is_file()) { continue; }
            let name = entry.file_name();
            let name = name.to_string_lossy();
            if (name.starts_with("pipelines-") && !matches!(name.split('-').nth(1), Some("vulkan" | "dx12" | "metal")))
                || name.strip_prefix(&prefix).is_some_and(|file| file.split('.').next() != Some(driver))
                || (name.contains(".tmp") && entry.metadata().and_then(|metadata| metadata.modified()).ok()
                    .and_then(|modified| modified.elapsed().ok()).is_some_and(|age| age > Duration::from_secs(3600))) {
                let _ = std::fs::remove_file(entry.path());
            }
        }
    }
    let file = std::fs::File::open(&path).ok()?;
    let mut bytes = Vec::new();
    let valid = file.take((BUDGET + 1) as u64).read_to_end(&mut bytes).is_ok()
        && bytes.len() >= 24 && bytes.len() <= BUDGET && bytes[..8] == MAGIC
        && u64::from_le_bytes(bytes[8..16].try_into().unwrap()) == (bytes.len() - 24) as u64
        && u32::from_le_bytes(bytes[16..20].try_into().unwrap()) == crc32fast::hash(&bytes[24..]);
    if !valid { let _ = std::fs::remove_file(&path); return None; }
    Some(bytes.split_off(24))
}

fn write_file(path: &Path, data: &[u8], published: &Mutex<Option<(usize, u32)>>) -> std::io::Result<bool> {
    if data.len() > BUDGET - 24 {
        let _ = std::fs::remove_file(path);
        return Ok(false);
    }
    let identity = (data.len(), crc32fast::hash(data));
    let mut published = published.lock().unwrap();
    if *published == Some(identity) { return Ok(true); }
    let mut temp = path.as_os_str().to_os_string();
    temp.push(format!(".tmp{}", std::process::id()));
    let temp = PathBuf::from(temp);
    let result = (|| {
        if let Some(parent) = path.parent() { std::fs::create_dir_all(parent)?; }
        let mut file = std::fs::File::create(&temp)?;
        file.write_all(&MAGIC)?;
        file.write_all(&(data.len() as u64).to_le_bytes())?;
        file.write_all(&identity.1.to_le_bytes())?;
        file.write_all(&0u32.to_le_bytes())?;
        file.write_all(data)?;
        file.sync_data()?;
        std::fs::rename(&temp, path)?;
        Ok(true)
    })();
    if result.is_err() { let _ = std::fs::remove_file(temp); }
    else { *published = Some(identity); }
    result
}

#[cfg(test)]
mod tests;
