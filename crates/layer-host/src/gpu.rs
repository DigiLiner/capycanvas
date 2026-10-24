//! One GPU context, renderer factory and device-failure watch for native hosts.
use layer_core::color::{DocumentColor, RgbSpace};
use layer_render_wgpu::WgpuRasterizer;
use std::sync::{Arc, OnceLock};

/// UI preview colors: mapped through the SDR rendition, or tagged in a display space.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum UiColor {
    #[default]
    Mapped,
    Tagged(RgbSpace),
}
impl UiColor {
    pub fn preview_space(self) -> RgbSpace {
        match self {
            Self::Mapped => RgbSpace::Srgb,
            Self::Tagged(space) => space,
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct RendererOptions {
    pub ui_color: UiColor,
}

#[derive(Clone)]
pub struct GpuContext {
    pub adapter: wgpu::Adapter,
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
}
impl GpuContext {
    pub fn of(gpu: &WgpuRasterizer) -> Self {
        Self {
            adapter: gpu.adapter().clone(),
            device: gpu.device().clone(),
            queue: gpu.queue().clone(),
        }
    }
    /// Secondary renderers (tabs, open, color candidates) pass `finish_catalog = true`;
    /// the window renderer passes false and calls `finish_startup_catalog` after its catalog.
    pub fn rasterizer(
        &self,
        color: DocumentColor,
        options: &RendererOptions,
        finish_catalog: bool,
    ) -> Result<Box<WgpuRasterizer>, String> {
        let (adapter, device, queue) = (
            self.adapter.clone(),
            self.device.clone(),
            self.queue.clone(),
        );
        let mut gpu = WgpuRasterizer::from_wgpu_native_staged_boxed(adapter, device, queue, color)
            .map_err(|e| e.to_string())?;
        gpu.configure_ui_previews(options.ui_color.preview_space())
            .map_err(|e| e.to_string())?;
        if finish_catalog {
            gpu.finish_startup_catalog();
        }
        Ok(gpu)
    }
}

pub fn unsupported_adapter(adapter: &wgpu::Adapter, limits: &wgpu::Limits) -> Option<String> {
    let info = adapter.get_info();
    if info.device_type == wgpu::DeviceType::Cpu && !layer_render_wgpu::software_adapter_tests() {
        return Some(format!("{} is a software renderer, not a hardware GPU", info.name));
    }
    let mut short = Vec::new();
    limits.check_limits_with_fail_fn(&adapter.limits(), false, |name, required, allowed| {
        short.push(format!("{name} is {allowed}, needs {required}"))
    });
    (!short.is_empty()).then(|| format!("{} does not meet the canvas limits: {}", info.name, short.join(", ")))
}

pub fn admit_adapter(localization: &layer_ui::Localizer, adapter: &wgpu::Adapter, limits: &wgpu::Limits) -> Result<(), String> {
    unsupported_adapter(adapter, limits).map_or(Ok(()), |cause| Err(layer_ui::gpu_unsupported(localization, &cause)))
}

pub async fn request_device(adapter: &wgpu::Adapter, descriptor: &wgpu::DeviceDescriptor<'_>)
    -> Result<(wgpu::Device, wgpu::Queue, DeviceWatch), wgpu::RequestDeviceError> {
    let (device, queue) = adapter.request_device(descriptor).await?;
    let watch = DeviceWatch::observe(&device);
    Ok((device, queue, watch))
}

#[derive(Clone, Default)]
pub struct DeviceWatch {
    lost: Arc<OnceLock<String>>,
    error: Arc<OnceLock<String>>,
}
impl DeviceWatch {
    pub fn observe(device: &wgpu::Device) -> Self {
        let watch = Self::default();
        let lost = watch.lost.clone();
        device.set_device_lost_callback(move |reason, message| {
            lost.get_or_init(|| format!("Canvas GPU stopped ({reason:?}): {message}"));
        });
        let error = watch.error.clone();
        device.on_uncaptured_error(Arc::new(move |e: wgpu::Error| {
            error.get_or_init(|| e.to_string());
        }));
        watch
    }
    pub fn lost(&self) -> Option<&str> {
        self.lost.get().map(String::as_str)
    }
    pub fn error(&self) -> Option<&str> {
        self.error.get().map(String::as_str)
    }
    pub fn failure(&self) -> Option<String> {
        self.lost().or(self.error()).map(str::to_owned)
    }
    pub fn report_lost(&self, message: impl Into<String>) {
        self.lost.get_or_init(|| message.into());
    }
    pub fn report_error(&self, message: impl Into<String>) {
        self.error.get_or_init(|| message.into());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use layer_render::CanvasRenderer;

    #[test]
    fn native_renderer_factory_and_frames_leave_stack_for_their_caller() {
        let color = DocumentColor::default();
        let reference = WgpuRasterizer::new_native_headless(color).unwrap();
        let context = GpuContext::of(&reference);
        let renderer = std::thread::Builder::new()
            .name("native-renderer-factory".into())
            .stack_size(640 * 1024)
            .spawn(move || {
                let renderer = context.rasterizer(color, &RendererOptions::default(), false).unwrap();
                assert_eq!(renderer.document_color(), color);
                assert!(renderer.device() == &context.device);
                renderer
            })
            .unwrap()
            .join()
            .unwrap();
        let mut host = Box::new(crate::NativeHost::new(layer_ui::Platform::Mac).unwrap());
        host.replace_renderer(crate::Renderer(Some(renderer))).unwrap();
        host.resize(640, 480, 1.).unwrap();
        host.startup = Default::default();
        let mut host = std::thread::Builder::new()
            .name("native-renderer-backdrop".into())
            .stack_size(384 * 1024)
            .spawn(move || {
                let submitted = host.session.engine().backend().0.as_ref().unwrap().submitted_updates();
                host.prepare_canvas_frame(0, 0, false).unwrap();
                assert!(host.session.engine().backend().0.as_ref().unwrap().submitted_updates() > submitted);
                host.session.submit_backdrop_frame().unwrap();
                assert!(host.session.engine().backend().0.as_ref().unwrap().submitted_updates() > submitted);
                host
            })
            .unwrap()
            .join()
            .unwrap();
        host.session.renderer_mut().0.as_mut().unwrap().finish_startup_catalog();
        host.startup = Default::default();
        let clock = std::time::Instant::now();
        while !host.startup.canvas_ready || !host.startup.brush_ready {
            let now = clock.elapsed().as_nanos() as u64;
            host.prepare_canvas_frame(now, now, true).unwrap();
            assert!(clock.elapsed() < std::time::Duration::from_secs(30), "Blank canvas startup timed out");
            std::thread::sleep(std::time::Duration::from_millis(2));
        }
        std::thread::Builder::new()
            .name("native-renderer-frame".into())
            .stack_size(384 * 1024)
            .spawn(move || {
                let submitted = host.session.engine().backend().0.as_ref().unwrap().submitted_updates();
                host.resize(641, 480, 1.).unwrap();
                let now = clock.elapsed().as_nanos() as u64;
                host.prepare_canvas_frame(now, now, true).unwrap();
                assert!(host.startup.canvas_ready);
                assert!(host.session.engine().backend().0.as_ref().unwrap().submitted_updates() > submitted);
            })
            .unwrap()
            .join()
            .unwrap();
    }

    #[test]
    fn renderer_factory_admits_input_and_follows_host_ui_color() {
        let color = DocumentColor::default();
        let headless = WgpuRasterizer::new_native_headless(color).unwrap();
        let mut host = crate::NativeHost::new(layer_ui::Platform::Mac).unwrap();
        assert_eq!(
            host.renderer_options().ui_color.preview_space(),
            RgbSpace::Srgb
        );
        host.ui_color = UiColor::Tagged(RgbSpace::DisplayP3);
        let options = host.renderer_options();
        assert_eq!(options.ui_color.preview_space(), RgbSpace::DisplayP3);
        let gpu = GpuContext::of(&headless)
            .rasterizer(color, &options, true)
            .unwrap();
        assert_eq!(gpu.document_color(), color);
        assert!(gpu.device() == headless.device());
        gpu.shader_input();
        assert!(gpu.shader_wait_ms() > 0.);
    }

    #[test]
    fn adapters_short_of_canvas_limits_are_refused_with_a_plain_reason() {
        let gpu = WgpuRasterizer::new_native_headless(DocumentColor::default()).unwrap();
        let adapter = gpu.adapter();
        let limits = wgpu::Limits::downlevel_defaults().using_resolution(adapter.limits());
        assert_eq!(unsupported_adapter(adapter, &limits), None);
        let beyond = wgpu::Limits { max_bind_groups: adapter.limits().max_bind_groups + 1, ..limits.clone() };
        let cause = unsupported_adapter(adapter, &beyond).unwrap();
        assert!(cause.contains("max_bind_groups"), "{cause}");
        let localization = layer_ui::Localizer::shared(layer_ui::UiLanguage::English);
        let message = layer_ui::gpu_unsupported(&localization, &cause);
        assert!(message.starts_with("This device’s graphics processor (GPU) can’t run Capy Canvas."), "{message}");
        assert!(message.ends_with(&cause));
        for language in [layer_ui::UiLanguage::English, layer_ui::UiLanguage::Japanese] {
            let localization = layer_ui::Localizer::shared(language);
            assert_eq!(admit_adapter(&localization, adapter, &limits), Ok(()));
            assert_eq!(admit_adapter(&localization, adapter, &beyond), Err(layer_ui::gpu_unsupported(&localization, &cause)));
        }
    }

    #[test]
    fn a_restarted_gpu_is_never_mistaken_for_the_retired_one() {
        let color = DocumentColor::default();
        let retired = WgpuRasterizer::new_native_headless(color).unwrap();
        let restarted = WgpuRasterizer::new_native_headless(color).unwrap();
        assert!(retired.device() != restarted.device());
        assert!(retired.device() == retired.device());
    }
}
