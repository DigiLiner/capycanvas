use super::*;
use std::sync::atomic::AtomicU64;

struct Directory(PathBuf);
impl Directory {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "capy-saved-pipelines-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&path).unwrap();
        Self(path)
    }
    fn info(&self) -> wgpu::AdapterInfo {
        wgpu::AdapterInfo {
            vendor: 1, device: 2,
            ..wgpu::AdapterInfo::new(wgpu::DeviceType::Other, wgpu::Backend::Vulkan)
        }
    }
    fn path(&self) -> PathBuf { cache_path(&self.0, &self.info()).unwrap() }
}
impl Drop for Directory {
    fn drop(&mut self) { std::fs::remove_dir_all(&self.0).unwrap(); }
}

#[test]
fn written_bytes_roundtrip() {
    let directory = Directory::new();
    let bytes = b"compiled pipeline bytes";
    assert!(write_file(&directory.path(), bytes, &Mutex::new(None)).unwrap());
    assert_eq!(read_file(&directory.0, &directory.info()).unwrap(), bytes);
}

#[test]
fn corrupt_files_are_discarded() {
    let directory = Directory::new();
    let path = directory.path();
    write_file(&path, b"compiled pipeline bytes", &Mutex::new(None)).unwrap();
    let valid = std::fs::read(&path).unwrap();
    let mut flipped = valid.clone();
    flipped[24] ^= 1;
    let mut magic = valid.clone();
    magic[0] ^= 1;
    let mut length = valid.clone();
    length[8] ^= 1;
    for bytes in [flipped, valid[..20].to_vec(), magic, length] {
        std::fs::write(&path, bytes).unwrap();
        assert!(read_file(&directory.0, &directory.info()).is_none());
        assert!(!path.exists());
    }
}

#[test]
fn oversized_files_are_not_written_or_loaded() {
    let directory = Directory::new();
    let path = directory.path();
    std::fs::write(&path, b"old cache").unwrap();
    assert!(!write_file(&path, &vec![0; BUDGET - 23], &Mutex::new(None)).unwrap());
    assert!(!path.exists());
    std::fs::File::create(&path).unwrap().set_len((BUDGET + 1) as u64).unwrap();
    assert!(read_file(&directory.0, &directory.info()).is_none());
    assert!(!path.exists());
}

#[test]
fn cleanup_keeps_current_files_and_recent_temporaries() {
    let directory = Directory::new();
    let path = directory.path();
    write_file(&path, b"current cache", &Mutex::new(None)).unwrap();
    let obsolete = directory.0.join("pipelines-obsolete-vulkan-1-2.bin");
    let old_temp = directory.0.join(format!("pipelines-{}-vulkan-1-2.bin.tmp1", env!("CARGO_PKG_VERSION")));
    let recent_temp = directory.0.join(format!("pipelines-{}-vulkan-1-2.bin.tmp2", env!("CARGO_PKG_VERSION")));
    let unrelated = directory.0.join("other.bin");
    let subdirectory = directory.0.join("pipelines-obsolete-directory");
    std::fs::create_dir(&subdirectory).unwrap();
    for entry in [&obsolete, &old_temp, &recent_temp, &unrelated] {
        std::fs::write(entry, b"retained bytes").unwrap();
    }
    std::fs::File::options().write(true).open(&old_temp).unwrap().set_times(
        std::fs::FileTimes::new().set_modified(std::time::SystemTime::now() - std::time::Duration::from_secs(7200))
    ).unwrap();
    assert_eq!(read_file(&directory.0, &directory.info()).unwrap(), b"current cache");
    assert!(!obsolete.exists());
    assert!(!old_temp.exists());
    assert!(path.exists() && recent_temp.exists() && unrelated.exists() && subdirectory.exists());
}

#[test]
fn unsupported_backends_have_no_store() {
    let directory = Directory::new();
    let mut info = directory.info();
    for backend in [wgpu::Backend::Gl, wgpu::Backend::BrowserWebGpu, wgpu::Backend::Noop] {
        info.backend = backend;
        assert!(cache_path(&directory.0, &info).is_none());
        assert!(read_file(&directory.0, &info).is_none());
    }
}

#[test]
fn identical_cache_bytes_are_not_rewritten_but_changed_bytes_are() {
    let directory = Directory::new();
    let path = directory.path();
    let published = Mutex::new(None);
    assert!(write_file(&path, b"cache A", &published).unwrap());
    std::fs::remove_file(&path).unwrap();
    assert!(write_file(&path, b"cache A", &published).unwrap());
    assert!(!path.exists());
    assert!(write_file(&path, b"cache B", &published).unwrap());
    assert_eq!(read_file(&directory.0, &directory.info()).unwrap(), b"cache B");
}

#[test]
fn compiled_cache_loads_on_a_second_device() {
    use crate::{PipelineDevice, WgpuRasterizer};
    let directory = Directory::new();
    let instance = WgpuRasterizer::headless_instance();
    let adapter = pollster::block_on(wgpu::util::initialize_adapter_from_env_or_default(&instance, None)).unwrap();
    let info = adapter.get_info();
    assert_eq!(info.backend, wgpu::Backend::Vulkan, "saved pipeline test requires headless Vulkan");
    assert_ne!(info.device_type, wgpu::DeviceType::Cpu, "saved pipeline test requires a hardware GPU");
    assert!(adapter.features().contains(wgpu::Features::PIPELINE_CACHE), "Vulkan adapter must support PIPELINE_CACHE");
    let descriptor = wgpu::DeviceDescriptor {
        required_features: wgpu::Features::PIPELINE_CACHE, ..Default::default()
    };
    let (device, _) = pollster::block_on(adapter.request_device(&descriptor)).unwrap();
    let store = SavedPipelines::open_at(&directory.0, &device, &info, None).unwrap();
    let device = PipelineDevice::from(device).with_saved_pipelines(Some(store.clone()));
    let recipes = crate::pipeline_device::DeviceRecipes::from(&device);
    for shared in [
        device.for_recipe(),
        PipelineDevice::from((*device).clone()).with_device_recipes(&recipes),
        PipelineDevice::from((*device).clone()).with_recipes(&device),
    ] {
        assert!(Arc::ptr_eq(shared.saved_pipelines().unwrap(), &store));
    }
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("saved pipeline fixture"),
        source: wgpu::ShaderSource::Wgsl("@compute @workgroup_size(1) fn main() {}".into()),
    });
    let _pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
        label: Some("saved pipeline fixture"), layout: None, module: &shader,
        entry_point: Some("main"), compilation_options: Default::default(), cache: None,
    });
    assert_eq!(store.unsaved.load(Ordering::Relaxed), 1);
    store.save();
    assert!(store.path.exists());
    let data = read_file(&directory.0, &info).unwrap();
    assert!(!data.is_empty());
    assert_eq!(store.cache().get_data().unwrap(), data);
    std::fs::remove_file(&store.path).unwrap();
    store.compiled();
    store.saving.store(true, Ordering::Relaxed);
    store.save();
    assert!(!store.path.exists(), "identical cache bytes must not be written again");
    assert_eq!(store.unsaved.load(Ordering::Relaxed), 0);
    assert!(!store.saving.load(Ordering::Relaxed));
    let (second, _) = pollster::block_on(adapter.request_device(&descriptor)).unwrap();
    let published = (data.len(), crc32fast::hash(&data));
    let loaded = SavedPipelines::open_at(&directory.0, &second, &info, Some(data)).unwrap();
    assert_eq!(*loaded.published.lock().unwrap(), Some(published));
    assert!(!loaded.cache().get_data().unwrap().is_empty());
}
