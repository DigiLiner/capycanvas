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
fn cache_paths_are_stable_across_app_updates() {
    let directory = Directory::new();
    let mut info = directory.info();
    for (backend, name) in [(wgpu::Backend::Vulkan, "vulkan"), (wgpu::Backend::Dx12, "dx12"), (wgpu::Backend::Metal, "metal")] {
        info.backend = backend;
        assert_eq!(cache_path(&directory.0, &info).unwrap(), directory.0.join(format!("pipelines-{name}-1-2-d202ef8d.bin")));
    }
}

#[test]
fn driver_paths_use_crc32_with_a_zero_separator() {
    let directory = Directory::new();
    let mut info = directory.info();
    for (driver, driver_info, hash) in [("driver", "version", "f6e556c9"), ("ab", "c", "3d3660d6"), ("a", "bc", "21ae76bd"), ("driver10", "version", "0a44e71b")] {
        info.driver = driver.into(); info.driver_info = driver_info.into();
        assert_eq!(cache_path(&directory.0, &info).unwrap(), directory.0.join(format!("pipelines-vulkan-1-2-{hash}.bin")));
    }
}

#[test]
fn different_gpus_keep_their_caches_when_either_adapter_is_read() {
    for (vendor, device) in [(3, 4), (1, 3), (3, 2)] {
        let directory = Directory::new();
        let first = directory.info();
        let second = wgpu::AdapterInfo { vendor, device, driver: "other driver".into(), ..first.clone() };
        let first_path = cache_path(&directory.0, &first).unwrap();
        let second_path = cache_path(&directory.0, &second).unwrap();
        assert_ne!(first_path, second_path);
        write_file(&first_path, b"first GPU", &Mutex::new(None)).unwrap();
        write_file(&second_path, b"second GPU", &Mutex::new(None)).unwrap();
        for (info, bytes) in [(&first, b"first GPU".as_slice()), (&second, b"second GPU".as_slice())] {
            assert_eq!(read_file(&directory.0, info).unwrap(), bytes);
            assert!(first_path.exists() && second_path.exists());
        }
    }
}

#[test]
fn a_driver_change_removes_only_the_same_gpus_stale_driver_cache() {
    let directory = Directory::new();
    let old = directory.info();
    for (driver, driver_info) in [("new driver", ""), ("", "new version")] {
        let new = wgpu::AdapterInfo { driver: driver.into(), driver_info: driver_info.into(), ..old.clone() };
        let old_path = cache_path(&directory.0, &old).unwrap();
        let new_path = cache_path(&directory.0, &new).unwrap();
        assert_ne!(old_path, new_path);
        write_file(&old_path, b"old driver", &Mutex::new(None)).unwrap();
        write_file(&new_path, b"new driver", &Mutex::new(None)).unwrap();
        assert_eq!(read_file(&directory.0, &new).unwrap(), b"new driver");
        assert!(!old_path.exists());
        assert!(new_path.exists());
    }
}

#[test]
fn legacy_versioned_caches_are_removed_and_other_adapters_survive() {
    let directory = Directory::new();
    write_file(&directory.path(), b"current cache", &Mutex::new(None)).unwrap();
    let legacy = directory.0.join("pipelines-1.0.15-vulkan-13b5-90930010.bin");
    let same_version = directory.0.join(format!("pipelines-{}-dx12-1-2.bin", env!("CARGO_PKG_VERSION")));
    let others = ["pipelines-vulkan-3-4-d202ef8d.bin", "pipelines-dx12-1-2-d202ef8d.bin", "pipelines-metal-1-2-d202ef8d.bin"].map(|name| directory.0.join(name));
    for path in [&legacy, &same_version].into_iter().chain(others.iter()) {
        std::fs::write(path, b"other cache").unwrap();
    }
    assert_eq!(read_file(&directory.0, &directory.info()).unwrap(), b"current cache");
    assert!(!legacy.exists() && !same_version.exists());
    assert!(others.iter().all(|path| std::fs::read(path).unwrap() == b"other cache"));
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
    let old_temp = path.with_extension("bin.tmp1");
    let recent_temp = path.with_extension("bin.tmp2");
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
    assert_ne!(info.device_type, wgpu::DeviceType::Cpu, "saved pipeline test requires a hardware GPU");
    if !adapter.features().contains(wgpu::Features::PIPELINE_CACHE) {
        let (device, _) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default())).unwrap();
        assert!(SavedPipelines::open_at(&directory.0, &device, &info, None).is_none());
        return;
    }
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
