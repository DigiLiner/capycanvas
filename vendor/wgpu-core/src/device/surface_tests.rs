use super::Device;
use alloc::{boxed::Box, sync::Arc, vec, vec::Vec};
use core::{any::Any, sync::atomic::{AtomicBool, Ordering}, time::Duration};
use crate::{instance::{Adapter, Surface}, lock::{rank, Mutex}};

type QueueProbe = dyn Fn(bool) -> Result<(), hal::SurfaceError> + Send + Sync;
struct SurfaceAdapter(Box<dyn hal::DynAdapter>, Option<Arc<QueueProbe>>);
impl hal::DynResource for SurfaceAdapter {
    fn as_any(&self) -> &dyn Any { self }
    fn as_any_mut(&mut self) -> &mut dyn Any { self }
}
impl hal::DynAdapter for SurfaceAdapter {
    unsafe fn open(&self, features: wgt::Features, limits: &wgt::Limits, hints: &wgt::MemoryHints) -> Result<hal::DynOpenDevice, hal::DeviceError> {
        let mut opened = unsafe { self.0.open(features, limits, hints) }?;
        if let Some(probe) = &self.1 {
            opened.queue = Box::new(ProbeQueue(opened.queue, probe.clone()));
        }
        Ok(opened)
    }
    unsafe fn texture_format_capabilities(&self, format: wgt::TextureFormat) -> hal::TextureFormatCapabilities {
        unsafe { self.0.texture_format_capabilities(format) }
    }
    unsafe fn surface_capabilities(&self, _: &dyn hal::DynSurface) -> Option<hal::SurfaceCapabilities> {
        Some(hal::SurfaceCapabilities {
            formats: vec![wgt::SurfaceFormatCapabilities { format: wgt::TextureFormat::Rgba8Unorm, color_spaces: wgt::SurfaceColorSpaces::SRGB }],
            maximum_frame_latency: 1..=3, current_extent: None,
            usage: wgt::TextureUses::COLOR_TARGET,
            present_modes: vec![wgt::PresentMode::Fifo],
            composite_alpha_modes: vec![wgt::CompositeAlphaMode::Opaque],
        })
    }
    unsafe fn surface_display_hdr_info(&self, _: &dyn hal::DynSurface) -> Option<wgt::DisplayHdrInfo> { None }
    unsafe fn get_presentation_timestamp(&self) -> wgt::PresentationTimestamp {
        unsafe { self.0.get_presentation_timestamp() }
    }
    fn get_ordered_buffer_usages(&self) -> wgt::BufferUses { self.0.get_ordered_buffer_usages() }
    fn get_ordered_texture_usages(&self) -> wgt::TextureUses { self.0.get_ordered_texture_usages() }
}

struct ProbeQueue(Box<dyn hal::DynQueue>, Arc<QueueProbe>);
impl hal::DynResource for ProbeQueue {
    fn as_any(&self) -> &dyn Any { self.0.as_any() }
    fn as_any_mut(&mut self) -> &mut dyn Any { self.0.as_any_mut() }
}
impl hal::DynQueue for ProbeQueue {
    unsafe fn submit(&self, commands: &[&dyn hal::DynCommandBuffer], textures: &[&dyn hal::DynSurfaceTexture], fence: (&dyn hal::DynFence, hal::FenceValue)) -> Result<(), hal::DeviceError> {
        match (self.1)(false) {
            Ok(()) => (),
            Err(hal::SurfaceError::Device(error)) => return Err(error),
            Err(error) => panic!("invalid submission error: {error:?}"),
        }
        unsafe { self.0.submit(commands, textures, fence) }
    }
    unsafe fn present(&self, _: &dyn hal::DynSurface, _: Box<dyn hal::DynSurfaceTexture>) -> Result<(), hal::SurfaceError> { (self.1)(true) }
    unsafe fn get_timestamp_period(&self) -> f32 { unsafe { self.0.get_timestamp_period() } }
    unsafe fn wait_for_idle(&self) -> Result<(), hal::DeviceError> { unsafe { self.0.wait_for_idle() } }
}

struct ProbeSurface {
    configure: Box<dyn Fn() -> Result<(), hal::SurfaceError> + Send + Sync>,
    acquire: Box<dyn Fn() -> Result<hal::DynAcquiredSurfaceTexture, hal::SurfaceError> + Send + Sync>,
}
impl hal::DynResource for ProbeSurface {
    fn as_any(&self) -> &dyn Any { self }
    fn as_any_mut(&mut self) -> &mut dyn Any { self }
}
impl hal::DynSurface for ProbeSurface {
    unsafe fn configure(&self, _: &dyn hal::DynDevice, _: &hal::SurfaceConfiguration) -> Result<(), hal::SurfaceError> { (self.configure)() }
    unsafe fn unconfigure(&self, _: &dyn hal::DynDevice) {}
    unsafe fn acquire_texture(&self, _: Option<Duration>, _: &dyn hal::DynFence) -> Result<hal::DynAcquiredSurfaceTexture, hal::SurfaceError> { (self.acquire)() }
    unsafe fn discard_texture(&self, _: Box<dyn hal::DynSurfaceTexture>) {}
}

fn device() -> (Arc<Device>, Arc<crate::device::queue::Queue>) {
    device_with_probe(None)
}

fn device_with_probe(probe: Option<Arc<QueueProbe>>) -> (Arc<Device>, Arc<crate::device::queue::Queue>) {
    let mut options = wgt::BackendOptions::default();
    options.noop.enable = true;
    let instance = unsafe { <hal::noop::Context as hal::Instance>::init(&hal::InstanceDescriptor {
        name: "surface admission test", flags: wgt::InstanceFlags::empty(),
        memory_budget_thresholds: Default::default(), backend_options: options,
        telemetry: None, display: None,
    }) }.unwrap();
    let exposed = unsafe { <hal::noop::Context as hal::Instance>::enumerate_adapters(&instance, None) }.remove(0);
    let mut exposed: hal::DynExposedAdapter = exposed.into();
    exposed.adapter = Box::new(SurfaceAdapter(exposed.adapter, probe));
    Arc::new(Adapter::new(exposed)).create_device_and_queue(&Default::default(), wgt::InstanceFlags::empty()).unwrap()
}

fn configuration() -> wgt::SurfaceConfiguration<Vec<wgt::TextureFormat>> {
    wgt::SurfaceConfiguration {
        usage: wgt::TextureUsages::RENDER_ATTACHMENT, format: wgt::TextureFormat::Rgba8Unorm,
        width: 16, height: 16, present_mode: wgt::PresentMode::Fifo,
        desired_maximum_frame_latency: 2, alpha_mode: wgt::CompositeAlphaMode::Opaque,
        view_formats: vec![], color_space: wgt::SurfaceColorSpace::Srgb,
    }
}

fn surface(probe: impl Fn() -> Result<(), hal::SurfaceError> + Send + Sync + 'static) -> Arc<Surface> {
    surface_with_acquire(probe, || Err(hal::SurfaceError::Timeout))
}

fn surface_with_acquire(
    configure: impl Fn() -> Result<(), hal::SurfaceError> + Send + Sync + 'static,
    acquire: impl Fn() -> Result<hal::DynAcquiredSurfaceTexture, hal::SurfaceError> + Send + Sync + 'static,
) -> Arc<Surface> {
    let raw: Box<dyn hal::DynSurface> = Box::new(ProbeSurface { configure: Box::new(configure), acquire: Box::new(acquire) });
    Arc::new(Surface {
        presentation: Mutex::new(rank::SURFACE_PRESENTATION, None),
        surface_per_backend: [(wgt::Backend::Noop, raw)].into_iter().collect(),
    })
}

#[test]
fn configure_excludes_background_submission_and_fires_reentrant_callbacks_on_both_outcomes() {
    for fail in [false, true] {
        let (device, queue) = device();
        queue.submit(&[]).unwrap();
        let called = Arc::new(AtomicBool::new(false));
        let probe_device = device.clone();
        let probe_called = called.clone();
        let surface = surface(move || {
                assert!(!probe_called.load(Ordering::Acquire));
                let concurrent = probe_device.clone();
                let admitted = std::thread::spawn(move || concurrent.command_indices.try_write().is_some()).join().unwrap();
                assert!(!admitted, "a background submission can enter during HAL surface configuration");
                if fail { Err(hal::SurfaceError::Lost) } else { Ok(()) }
        });
        let callback_queue = queue.clone();
        let callback_surface = surface.clone();
        let callback_called = called.clone();
        queue.on_submitted_work_done(Box::new(move || {
            drop(callback_surface.presentation.lock());
            callback_queue.submit(&[]).unwrap();
            callback_called.store(true, Ordering::Release);
        }));
        let result = device.configure_surface(&surface, &configuration());
        assert_eq!(result.is_some(), fail);
        assert!(called.load(Ordering::Acquire), "configure dropped a pending work-done callback");
        assert!(device.command_indices.try_write().is_some());
    }
}

#[test]
fn rejected_live_output_and_invalid_configuration_preserve_presentation_and_callbacks() {
    let (device, queue) = device();
    let surface = surface(|| panic!("invalid configure reached HAL"));
    let (texture, _) = device.create_texture(&wgt::TextureDescriptor {
        label: None, size: wgt::Extent3d { width: 16, height: 16, depth_or_array_layers: 1 },
        mip_level_count: 1, sample_count: 1, dimension: wgt::TextureDimension::D2,
        format: wgt::TextureFormat::Rgba8Unorm, usage: wgt::TextureUsages::COPY_DST,
        view_formats: vec![],
    });
    *surface.presentation.lock() = Some(crate::present::Presentation {
        device: device.clone(), config: configuration(), acquired_texture: Some(texture.clone()),
    });
    for invalid_size in [false, true] {
        let called = Arc::new(AtomicBool::new(false));
        let callback_called = called.clone();
        queue.on_submitted_work_done(Box::new(move || callback_called.store(true, Ordering::Release)));
        let mut config = configuration();
        if invalid_size { config.width = 0; }
        let result = device.configure_surface(&surface, &config);
        assert!(result.is_some());
        assert!(called.load(Ordering::Acquire));
        let presentation = surface.presentation.lock();
        let retained = presentation.as_ref().unwrap();
        assert!(Arc::ptr_eq(retained.acquired_texture.as_ref().unwrap(), &texture));
        assert_eq!(retained.config.width, 16);
    }
}

#[test]
fn configure_device_error_releases_guards_before_synchronous_device_lost_callback() {
    let (device, queue) = device();
    let surface = surface(|| Err(hal::SurfaceError::Device(hal::DeviceError::Lost)));
    let called = Arc::new(AtomicBool::new(false));
    let callback_called = called.clone();
    let callback_device = device.clone();
    let callback_surface = surface.clone();
    *device.device_lost_closure.lock() = Some(Box::new(move |_, _| {
        assert!(callback_device.command_indices.try_write().is_some());
        drop(callback_surface.presentation.lock());
        callback_called.store(true, Ordering::Release);
    }));
    let submitted = Arc::new(AtomicBool::new(false));
    let callback_submitted = submitted.clone();
    queue.on_submitted_work_done(Box::new(move || callback_submitted.store(true, Ordering::Release)));
    assert!(device.configure_surface(&surface, &configuration()).is_some());
    assert!(called.load(Ordering::Acquire));
    assert!(submitted.load(Ordering::Acquire));
    assert!(surface.presentation.lock().is_none());
}

#[test]
fn configure_preserves_ranked_presentation_snatch_submission_order() {
    let (device, _queue) = device();
    let probe_device = device.clone();
    let surface = surface(move || {
        let concurrent = probe_device.clone();
        assert!(!std::thread::spawn(move || concurrent.command_indices.try_write().is_some()).join().unwrap());
        Ok(())
    });
    assert!(device.configure_surface(&surface, &configuration()).is_none());
    assert!(surface.presentation.lock().is_some());
}

fn acquired_texture() -> hal::DynAcquiredSurfaceTexture {
    hal::DynAcquiredSurfaceTexture { texture: Box::new(hal::noop::Resource), suboptimal: false }
}

fn presentation_is_locked(surface: &Arc<Surface>) -> bool {
    let surface = surface.clone();
    std::thread::spawn(move || surface.presentation.try_lock().is_none()).join().unwrap()
}

#[test]
fn acquisition_excludes_reconfiguration_and_rejects_duplicate_before_hal() {
    use core::sync::atomic::AtomicUsize;
    let (device, _queue) = device();
    let slot = Arc::new(std::sync::Mutex::new(alloc::sync::Weak::<Surface>::new()));
    let probe_slot = slot.clone();
    let acquisitions = Arc::new(AtomicUsize::new(0));
    let probe_acquisitions = acquisitions.clone();
    let surface = surface_with_acquire(|| Ok(()), move || {
        let surface = probe_slot.lock().unwrap().upgrade().unwrap();
        assert!(presentation_is_locked(&surface), "configuration can enter during HAL acquisition");
        probe_acquisitions.fetch_add(1, Ordering::Relaxed);
        Ok(acquired_texture())
    });
    *slot.lock().unwrap() = Arc::downgrade(&surface);
    assert!(device.configure_surface(&surface, &configuration()).is_none());
    let output = surface.get_current_texture().unwrap();
    assert!(matches!(surface.get_current_texture(), Err(crate::present::SurfaceError::AlreadyAcquired)));
    assert_eq!(acquisitions.load(Ordering::Relaxed), 1);
    surface.discard().unwrap();
    drop(output);
}

#[test]
fn acquisition_device_error_releases_presentation_before_callback() {
    let (device, _queue) = device();
    let surface = surface_with_acquire(|| Ok(()), || Err(hal::SurfaceError::Device(hal::DeviceError::Lost)));
    assert!(device.configure_surface(&surface, &configuration()).is_none());
    let called = Arc::new(AtomicBool::new(false));
    let callback_called = called.clone();
    let callback_surface = surface.clone();
    *device.device_lost_closure.lock() = Some(Box::new(move |_, _| {
        assert!(!presentation_is_locked(&callback_surface));
        callback_called.store(true, Ordering::Release);
    }));
    assert!(surface.get_current_texture().is_err());
    assert!(called.load(Ordering::Acquire));
}

#[test]
fn presentation_retains_live_output_through_preparation_and_serializes_hal_handoff() {
    for failure in [None, Some(false), Some(true)] {
        let slot = Arc::new(std::sync::Mutex::new(alloc::sync::Weak::<Surface>::new()));
        let probe_slot = slot.clone();
        let submitted = Arc::new(AtomicBool::new(false));
        let probe_submitted = submitted.clone();
        let presented = Arc::new(AtomicBool::new(false));
        let probe_presented = presented.clone();
        let (device, queue) = device_with_probe(Some(Arc::new(move |is_present| {
            let Some(surface) = probe_slot.lock().unwrap().upgrade() else { return Ok(()); };
            if is_present {
                assert!(presentation_is_locked(&surface), "configuration can enter during HAL presentation");
                probe_presented.store(true, Ordering::Release);
            } else {
                let observer = surface.clone();
                let retained = std::thread::spawn(move || observer.presentation.lock().as_ref().unwrap().acquired_texture.is_some()).join().unwrap();
                assert!(retained, "configuration can retire the output during presentation preparation");
                probe_submitted.store(true, Ordering::Release);
            }
            if failure == Some(is_present) { return Err(hal::SurfaceError::Device(hal::DeviceError::Lost)); }
            Ok(())
        })));
        let surface = surface_with_acquire(|| Ok(()), || Ok(acquired_texture()));
        *slot.lock().unwrap() = Arc::downgrade(&surface);
        assert!(device.configure_surface(&surface, &configuration()).is_none());
        let output = surface.get_current_texture().unwrap();
        let called = Arc::new(AtomicBool::new(false));
        let callback_called = called.clone();
        let callback_surface = surface.clone();
        let callback_device = device.clone();
        let callback_queue = queue.clone();
        *device.device_lost_closure.lock() = Some(Box::new(move |_, _| {
            assert!(!presentation_is_locked(&callback_surface));
            assert!(callback_device.command_indices.try_write().is_some());
            assert!(callback_queue.pending_writes.try_lock().is_some());
            callback_called.store(true, Ordering::Release);
        }));
        assert_eq!(queue.present(&surface).is_err(), failure.is_some());
        assert!(submitted.load(Ordering::Acquire));
        assert_eq!(presented.load(Ordering::Acquire), failure != Some(false));
        assert_eq!(called.load(Ordering::Acquire), failure.is_some());
        if failure == Some(false) {
            surface.release().unwrap();
        }
        assert!(surface.presentation.lock().as_ref().unwrap().acquired_texture.is_none());
        drop(device.device_lost_closure.lock().take());
        drop(output);
    }
}
