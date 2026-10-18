use super::*;
use layer_core::{TextureSource, paper::{PaperBinding, PaperSurface}};
use std::sync::atomic::{AtomicU8, Ordering};

const PROBE_SIZE: u32 = 256;
const PAGE_SIZE: u32 = 256;
const PAGES_PER_STEP: u32 = 4;
const MAX_TEXELS: u32 = 4096;
const CACHED_BAKES: usize = 6;
const REFERENCE_RELIEF_UM: f32 = 100.;
const LOW_PERCENTILE: f64 = 0.005;
const HIGH_PERCENTILE: f64 = 0.995;
const PROGRAM_JITTER: f64 = 0.5;
const IMAGE_PROGRAM: &str = "image";
const MAPPING: u8 = 1;
const MAPPED: u8 = 2;
const FAILED: u8 = 3;

pub(super) struct PaperTile {
    pixels: [f64; 2],
    origin: [i64; 2],
    fraction: [f64; 2],
    inverse: [f64; 4],
    texels: [u32; 2],
    period: [f64; 2],
}
impl PaperTile {
    pub fn new(surface: &PaperSurface) -> Result<Self, GpuRasterError> {
        let period = surface.texture.period().ok_or(GpuRasterError::Effect("Paper texture without a physical size".into()))?;
        let [a, b, c, d, x, y] = surface.mapping.0;
        let raw = [period[0] * a.abs() + period[1] * c.abs(), period[0] * b.abs() + period[1] * d.abs()];
        let pixels = raw.map(|v| v.round().max(1.));
        let stretch = [pixels[0] / raw[0], pixels[1] / raw[1]];
        let inverse = layer_core::Affine64([a * stretch[0], b * stretch[1], c * stretch[0], d * stretch[1], 0., 0.])
            .inverse().ok_or(GpuRasterError::InvalidTransform("Singular paper mapping"))?.0;
        Ok(Self {
            pixels,
            origin: [x.floor() as i64, y.floor() as i64],
            fraction: [x - x.floor(), y - y.floor()],
            inverse: [inverse[0], inverse[1], inverse[2], inverse[3]],
            texels: pixels.map(|v| (v as u32).min(MAX_TEXELS)),
            period,
        })
    }
    pub fn id_for(binding: &PaperBinding) -> Option<AssetId> {
        Self::new(&binding.surface).ok().map(|tile| tile.id(&binding.surface))
    }
    pub fn id(&self, surface: &PaperSurface) -> AssetId {
        use std::hash::{Hash, Hasher};
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        let name = match &surface.texture {
            TextureSource::Program(instance) => {
                for value in &instance.values {
                    match value {
                        layer_core::EffectValue::Number(v) => v.to_bits().hash(&mut hasher),
                        layer_core::EffectValue::Toggle(v) => v.hash(&mut hasher),
                        layer_core::EffectValue::Choice(v) => v.hash(&mut hasher),
                        _ => {}
                    }
                }
                instance.program.id.to_string()
            }
            TextureSource::Image(image) => {
                image.id().to_string().hash(&mut hasher);
                IMAGE_PROGRAM.into()
            }
        };
        for value in self.inverse.iter().chain(&self.fraction).chain(&self.pixels) {
            value.to_bits().hash(&mut hasher);
        }
        AssetId::from(format!("paper:{name}:{:016x}", hasher.finish()).as_str())
    }
    fn texel_mm(&self) -> f64 {
        (self.period[0] * self.period[1] / f64::from(self.texels[0]) / f64::from(self.texels[1])).sqrt()
    }
    fn image_shrink(&self, surface: &PaperSurface) -> Option<f64> {
        let TextureSource::Image(image) = &surface.texture else { return None };
        Some((f64::from(image.extent[0]) * f64::from(image.extent[1]) / f64::from(self.texels[0]) / f64::from(self.texels[1])).sqrt())
    }
    fn parameters(&self, surface: &PaperSurface, low: f32, high: f32) -> [f32; 20] {
        let (jitter, nearest) = self.image_shrink(surface)
            .map_or((PROGRAM_JITTER, 0.), |shrink| (0.5 * (shrink - 1.).clamp(0., 1.), f64::from(u8::from(shrink >= 1.))));
        let [ia, ib, ic, id] = self.inverse.map(|v| v as f32);
        [ia, ib, ic, id,
            self.fraction[0] as f32, self.fraction[1] as f32, self.pixels[0] as f32, self.pixels[1] as f32,
            self.texels[0] as f32, self.texels[1] as f32, low, high,
            self.period[0] as f32, self.period[1] as f32, self.texel_mm() as f32, jitter as f32,
            nearest as f32, 0., 0., 0.]
    }
    pub fn grain(&self, binding: &PaperBinding) -> [f32; 4] {
        let offset = [0, 1].map(|axis| ((binding.target_offset[axis] - self.origin[axis]) as f64).rem_euclid(self.pixels[axis]));
        [self.pixels[0] as f32, self.pixels[1] as f32, offset[0] as f32, offset[1] as f32]
    }
}

struct Programs {
    id: Arc<str>,
    probe: Deferred<wgpu::RenderPipeline>,
    bake: Deferred<wgpu::RenderPipeline>,
}

enum Stage {
    Waiting,
    Probing { readback: wgpu::Buffer, state: Arc<AtomicU8>, image: wgpu::TextureView },
    Baking { texture: wgpu::Texture, group: wgpu::BindGroup, next: u32, relief: f32 },
    Ready { view: wgpu::TextureView, relief: f32 },
}

struct Bake {
    id: AssetId,
    program: Arc<str>,
    surface: Arc<PaperSurface>,
    tile: PaperTile,
    data: wgpu::Buffer,
    priority: u8,
    used: u64,
    stage: Stage,
}

pub(super) struct Papers {
    layout: wgpu::BindGroupLayout,
    pipeline_layout: wgpu::PipelineLayout,
    sampler: wgpu::Sampler,
    empty_image: wgpu::TextureView,
    programs: Vec<Programs>,
    bakes: Vec<Bake>,
    clock: u64,
}

impl Papers {
    pub fn new(device: &PipelineDevice) -> Self {
        let layout = bindings::layout(device, "paper bake", &[
            bindings::buffer(0, wgpu::ShaderStages::FRAGMENT, wgpu::BufferBindingType::Uniform, false, None),
            bindings::buffer(1, wgpu::ShaderStages::FRAGMENT, wgpu::BufferBindingType::Storage { read_only: true }, false, None),
            bindings::texture(2, wgpu::ShaderStages::FRAGMENT, true),
            bindings::sampler(3, wgpu::ShaderStages::FRAGMENT, wgpu::SamplerBindingType::Filtering),
        ]);
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("paper bake"),
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("paper image"),
            address_mode_u: wgpu::AddressMode::Repeat,
            address_mode_v: wgpu::AddressMode::Repeat,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        let empty_image = Self::image_texture(device, [1, 1]).create_view(&Default::default());
        Self { layout, pipeline_layout, sampler, empty_image, programs: Vec::new(), bakes: Vec::new(), clock: 0 }
    }

    fn image_texture(device: &PipelineDevice, extent: [u32; 2]) -> wgpu::Texture {
        device.create_texture(&wgpu::TextureDescriptor {
            label: Some("paper image"),
            size: wgpu::Extent3d { width: extent[0], height: extent[1], depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::R8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        })
    }

    fn programs(&mut self, device: &PipelineDevice, texture: &TextureSource) -> Result<Arc<str>, GpuRasterError> {
        let id: Arc<str> = texture.program().map_or(IMAGE_PROGRAM.into(), |instance| instance.program.id.clone());
        if self.programs.iter().any(|p| p.id == id) {
            return Ok(id);
        }
        let mut source = String::from(include_str!("paper_bake.wgsl"));
        match texture.program() {
            Some(instance) => {
                for part in instance.program.wgsl.sources().map_err(|e| GpuRasterError::Effect(e.into()))? {
                    source.push('\n');
                    source.push_str(part);
                }
                source.push_str(&format!("\nfn paper_height(p: vec2<f32>) -> f32 {{ return {}(p, 0u); }}\n", instance.program.entry));
            }
            None => source.push_str(include_str!("paper_image.wgsl")),
        }
        let module = Deferred::wgsl(device, "paper texture", source);
        let pipeline = |entry: &'static str, format: wgpu::TextureFormat| {
            let (device, layout, module) = (device.for_recipe(), self.pipeline_layout.clone(), module.clone());
            Deferred::pipeline(move |mode| fullscreen_pipeline_recipe(mode, &device, &layout, &module, entry, None, format, "paper texture"))
        };
        self.programs.push(Programs {
            id: id.clone(),
            probe: pipeline("paper_probe", wgpu::TextureFormat::R32Float),
            bake: pipeline("paper_bake", wgpu::TextureFormat::R8Unorm),
        });
        Ok(id)
    }

    pub fn require(&mut self, device: &PipelineDevice, compiler: Option<&startup::Compiler>, binding: &PaperBinding, priority: u8) -> Result<AssetId, GpuRasterError> {
        let tile = PaperTile::new(&binding.surface)?;
        let id = tile.id(&binding.surface);
        self.clock += 1;
        let clock = self.clock;
        let program = self.programs(device, &binding.surface.texture)?;
        if let Some(compiler) = compiler {
            let programs = self.programs.iter().find(|p| p.id == program).expect("prepared paper program");
            compiler.require([&programs.probe, &programs.bake], priority);
        }
        if let Some(bake) = self.bakes.iter_mut().find(|b| b.id == id) {
            bake.priority = bake.priority.min(priority);
            bake.used = clock;
            return Ok(id);
        }
        let parameters = match &binding.surface.texture {
            TextureSource::Program(instance) => instance.gpu_parameters(layer_core::color::RgbSpace::Srgb).map_err(GpuRasterError::Effect)?,
            TextureSource::Image(_) => vec![[0.; 4]],
        };
        let data = initialized_buffer(device, &wgpu::util::BufferInitDescriptor {
            label: Some("paper texture parameters"),
            contents: &parameters.iter().flatten().flat_map(|v| v.to_ne_bytes()).collect::<Vec<_>>(),
            usage: wgpu::BufferUsages::STORAGE,
        })?;
        self.bakes.push(Bake { id: id.clone(), program, surface: binding.surface.clone(), tile, data, priority, used: clock, stage: Stage::Waiting });
        while self.bakes.len() > CACHED_BAKES {
            let Some(oldest) = self.bakes.iter().enumerate().filter(|(_, b)| matches!(b.stage, Stage::Ready { .. } | Stage::Waiting))
                .min_by_key(|(_, b)| b.used).map(|(i, _)| i) else { break };
            self.bakes.remove(oldest);
        }
        Ok(id)
    }

    pub fn ready_through(&self, priority: u8) -> bool {
        self.bakes.iter().all(|b| b.priority > priority || matches!(b.stage, Stage::Ready { .. }))
    }

    pub fn view(&self, id: &AssetId) -> Option<&wgpu::TextureView> {
        self.bakes.iter().find_map(|b| match &b.stage { Stage::Ready { view, .. } if b.id == *id => Some(view), _ => None })
    }

    pub fn relief(&self, id: &AssetId) -> Option<f32> {
        self.bakes.iter().find_map(|b| match &b.stage { Stage::Ready { relief, .. } if b.id == *id => Some(*relief), _ => None })
    }

    pub fn advance(&mut self, device: &PipelineDevice, queue: &wgpu::Queue, wait: bool) -> Result<(), GpuRasterError> {
        loop {
            let mut progressed = false;
            for index in 0..self.bakes.len() {
                progressed |= self.step(index, device, queue, wait)?;
            }
            if !wait || self.bakes.iter().all(|b| matches!(b.stage, Stage::Ready { .. })) {
                return Ok(());
            }
            if !progressed {
                device.poll(wgpu::PollType::wait_indefinitely()).map_err(|e| GpuRasterError::WaitFailed(e.to_string()))?;
            }
        }
    }

    fn step(&mut self, index: usize, device: &PipelineDevice, queue: &wgpu::Queue, wait: bool) -> Result<bool, GpuRasterError> {
        let programs = self.programs.iter().find(|p| p.id == self.bakes[index].program).expect("prepared paper program");
        if !wait && !(programs.probe.ready() && programs.bake.ready()) {
            return Ok(false);
        }
        let bake = &mut self.bakes[index];
        let next = match &mut bake.stage {
            Stage::Waiting => {
                let image = match &bake.surface.texture {
                    TextureSource::Image(source) => {
                        let texture = Self::image_texture(device, source.extent);
                        let mut rows = source.rows();
                        let mut bytes = vec![0; source.row_bytes() * source.extent[1] as usize];
                        for (y, row) in bytes.chunks_mut(source.row_bytes()).enumerate() {
                            rows.read(y as u32, row).map_err(GpuRasterError::Effect)?;
                        }
                        queue.write_texture(texture.as_image_copy(), &bytes,
                            wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(source.extent[0]), rows_per_image: Some(source.extent[1]) },
                            wgpu::Extent3d { width: source.extent[0], height: source.extent[1], depth_or_array_layers: 1 });
                        texture.create_view(&Default::default())
                    }
                    TextureSource::Program(_) => self.empty_image.clone(),
                };
                let probe = device.create_texture(&wgpu::TextureDescriptor {
                    label: Some("paper probe"),
                    size: wgpu::Extent3d { width: PROBE_SIZE, height: PROBE_SIZE, depth_or_array_layers: 1 },
                    mip_level_count: 1,
                    sample_count: 1,
                    dimension: wgpu::TextureDimension::D2,
                    format: wgpu::TextureFormat::R32Float,
                    usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
                    view_formats: &[],
                });
                let readback = device.create_buffer(&wgpu::BufferDescriptor {
                    label: Some("paper probe readback"),
                    size: u64::from(PROBE_SIZE * PROBE_SIZE * 4),
                    usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
                    mapped_at_creation: false,
                });
                let group = Self::group(device, &self.layout, &bake.tile.parameters(&bake.surface, 0., 1.), &bake.data, &image, &self.sampler)?;
                let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("paper probe") });
                {
                    let view = probe.create_view(&Default::default());
                    let mut pass = Self::pass(&mut encoder, &view, wgpu::LoadOp::Clear(wgpu::Color::BLACK));
                    pass.set_pipeline(&programs.probe);
                    pass.set_bind_group(0, &group, &[]);
                    pass.draw(0..3, 0..1);
                }
                encoder.copy_texture_to_buffer(probe.as_image_copy(), wgpu::TexelCopyBufferInfo {
                    buffer: &readback,
                    layout: wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(PROBE_SIZE * 4), rows_per_image: Some(PROBE_SIZE) },
                }, wgpu::Extent3d { width: PROBE_SIZE, height: PROBE_SIZE, depth_or_array_layers: 1 });
                queue.submit([encoder.finish()]);
                let state = Arc::new(AtomicU8::new(MAPPING));
                let signal = state.clone();
                readback.slice(..).map_async(wgpu::MapMode::Read, move |result| {
                    signal.store(if result.is_ok() { MAPPED } else { FAILED }, Ordering::Release);
                });
                Stage::Probing { readback, state, image }
            }
            Stage::Probing { readback, state, image } => {
                if !wait {
                    let _ = device.poll(wgpu::PollType::Poll);
                }
                match state.load(Ordering::Acquire) {
                    MAPPED => {}
                    FAILED => return Err(GpuRasterError::MapFailed("Paper probe readback failed".into())),
                    _ => return Ok(false),
                }
                let mut heights: Vec<f32> = readback.slice(..).get_mapped_range().map_err(|e| GpuRasterError::MapFailed(e.to_string()))?
                    .as_chunks::<4>().0.iter().map(|b| f32::from_ne_bytes(*b)).filter(|v| v.is_finite()).collect();
                readback.unmap();
                if heights.is_empty() {
                    return Err(GpuRasterError::Effect("Paper texture produced no finite heights".into()));
                }
                let mut percentile = |p: f64| {
                    let index = ((heights.len() - 1) as f64 * p).round() as usize;
                    *heights.select_nth_unstable_by(index, f32::total_cmp).1
                };
                let low = percentile(LOW_PERCENTILE);
                let high = percentile(HIGH_PERCENTILE).max(low + 1e-3);
                let relief = match bake.surface.texture {
                    TextureSource::Program(_) => (high - low) / REFERENCE_RELIEF_UM,
                    TextureSource::Image(_) => 1.,
                };
                let texture = device.create_texture(&wgpu::TextureDescriptor {
                    label: Some("paper texture"),
                    size: wgpu::Extent3d { width: bake.tile.texels[0], height: bake.tile.texels[1], depth_or_array_layers: 1 },
                    mip_level_count: 1,
                    sample_count: 1,
                    dimension: wgpu::TextureDimension::D2,
                    format: wgpu::TextureFormat::R8Unorm,
                    usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_SRC,
                    view_formats: &[],
                });
                let group = Self::group(device, &self.layout, &bake.tile.parameters(&bake.surface, low, high), &bake.data, image, &self.sampler)?;
                Stage::Baking { texture, group, next: 0, relief }
            }
            Stage::Baking { texture, group, next, relief } => {
                let columns = bake.tile.texels[0].div_ceil(PAGE_SIZE);
                let total = columns * bake.tile.texels[1].div_ceil(PAGE_SIZE);
                let end = if wait { total } else { (*next + PAGES_PER_STEP).min(total) };
                let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("paper bake") });
                {
                    let view = texture.create_view(&Default::default());
                    let load = if *next == 0 { wgpu::LoadOp::Clear(wgpu::Color::BLACK) } else { wgpu::LoadOp::Load };
                    let mut pass = Self::pass(&mut encoder, &view, load);
                    pass.set_pipeline(&programs.bake);
                    pass.set_bind_group(0, &*group, &[]);
                    for page in *next..end {
                        let [x, y] = [page % columns * PAGE_SIZE, page / columns * PAGE_SIZE];
                        pass.set_scissor_rect(x, y, PAGE_SIZE.min(bake.tile.texels[0] - x), PAGE_SIZE.min(bake.tile.texels[1] - y));
                        pass.draw(0..3, 0..1);
                    }
                }
                queue.submit([encoder.finish()]);
                *next = end;
                if end < total {
                    return Ok(true);
                }
                Stage::Ready { view: texture.create_view(&Default::default()), relief: *relief }
            }
            Stage::Ready { .. } => return Ok(false),
        };
        bake.stage = next;
        Ok(true)
    }

    fn group(device: &PipelineDevice, layout: &wgpu::BindGroupLayout, parameters: &[f32; 20], data: &wgpu::Buffer,
        image: &wgpu::TextureView, sampler: &wgpu::Sampler) -> Result<wgpu::BindGroup, GpuRasterError> {
        let uniform = initialized_buffer(device, &wgpu::util::BufferInitDescriptor {
            label: Some("paper bake parameters"),
            contents: &parameters.iter().flat_map(|v| v.to_ne_bytes()).collect::<Vec<_>>(),
            usage: wgpu::BufferUsages::UNIFORM,
        })?;
        Ok(bindings::group(device, "paper bake", layout, [uniform.as_entire_binding(), data.as_entire_binding(),
            wgpu::BindingResource::TextureView(image), wgpu::BindingResource::Sampler(sampler)]))
    }

    fn pass<'a>(encoder: &'a mut wgpu::CommandEncoder, view: &'a wgpu::TextureView, load: wgpu::LoadOp<wgpu::Color>) -> wgpu::RenderPass<'a> {
        encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("paper texture"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations { load, store: wgpu::StoreOp::Store },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use layer_core::paper::{reference, PaperBinding, PaperSurface};

    #[test]
    fn device_loss_before_paper_parameters_returns_an_error() {
        let mut renderer = WgpuRasterizer::new_native_headless(Default::default()).unwrap();
        let surface = PaperSurface { texture: layer_core::paper::default_texture(), mapping: reference([64, 64], [300.; 2]).mapping };
        let binding = PaperBinding { surface: Arc::new(surface.clone()), target_offset: [0; 2] };
        let previous = renderer.papers.require(&renderer.device, None, &binding, startup::BRUSH).unwrap();
        let mut changed = surface;
        changed.mapping.0[4] += 0.5;
        let binding = PaperBinding { surface: Arc::new(changed), target_offset: [0; 2] };
        assert_ne!(Some(previous), PaperTile::id_for(&binding));
        renderer.device.destroy();
        let result = renderer.papers.require(&renderer.device, None, &binding, startup::BRUSH);
        assert!(matches!(result, Err(GpuRasterError::MapFailed(_))));
    }

    #[test]
    fn device_loss_before_paper_bake_uniform_does_not_panic() {
        let mut renderer = WgpuRasterizer::new_native_headless(Default::default()).unwrap();
        let surface = PaperSurface { texture: layer_core::paper::default_texture(), mapping: reference([64, 64], [300.; 2]).mapping };
        let binding = PaperBinding { surface: Arc::new(surface), target_offset: [0; 2] };
        let id = renderer.papers.require(&renderer.device, None, &binding, startup::BRUSH).unwrap();
        let bake = renderer.papers.bakes.iter().find(|bake| bake.id == id).unwrap();
        let parameters = bake.tile.parameters(&bake.surface, 0., 1.);
        renderer.device.destroy();
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            Papers::group(&renderer.device, &renderer.papers.layout, &parameters, &bake.data,
                &renderer.papers.empty_image, &renderer.papers.sampler)
        }));
        assert!(result.is_ok(), "Paper bake uniform initialization must report device loss without panicking");
        assert!(matches!(result.unwrap(), Err(GpuRasterError::MapFailed(_))));
    }

    fn bake(renderer: &mut WgpuRasterizer, texture: TextureSource) -> (Vec<u8>, [u32; 2], f32) {
        bake_binding(renderer, PaperBinding { surface: Arc::new(PaperSurface { texture, mapping: reference([64, 64], [300.; 2]).mapping }), target_offset: [0; 2] })
    }

    fn bake_binding(renderer: &mut WgpuRasterizer, binding: PaperBinding) -> (Vec<u8>, [u32; 2], f32) {
        let id = renderer.papers.require(&renderer.device, None, &binding, startup::BRUSH).unwrap();
        renderer.papers.advance(&renderer.device, &renderer.queue, true).unwrap();
        let bake = renderer.papers.bakes.iter().find(|b| b.id == id).unwrap();
        let Stage::Ready { view, relief } = &bake.stage else { panic!("unfinished bake") };
        let texture = view.texture();
        let [width, height] = bake.tile.texels;
        let row = width.div_ceil(256) * 256;
        let readback = renderer.device.create_buffer(&wgpu::BufferDescriptor { label: None, size: u64::from(row * height),
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ, mapped_at_creation: false });
        let mut encoder = renderer.device.create_command_encoder(&Default::default());
        encoder.copy_texture_to_buffer(texture.as_image_copy(), wgpu::TexelCopyBufferInfo { buffer: &readback,
            layout: wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(row), rows_per_image: Some(height) } },
            wgpu::Extent3d { width, height, depth_or_array_layers: 1 });
        renderer.queue.submit([encoder.finish()]);
        readback.slice(..).map_async(wgpu::MapMode::Read, |_| {});
        renderer.device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
        let data = readback.slice(..).get_mapped_range().unwrap();
        let pixels = data.chunks(row as usize).flat_map(|r| r[..width as usize].to_vec()).collect();
        (pixels, [width, height], *relief)
    }

    fn preview(renderer: &mut WgpuRasterizer, texture: &TextureSource) -> Vec<u8> {
        let size = 2 * layer_core::paper::PREVIEW_SIZE;
        let ppi = size as f64 / layer_core::paper::PREVIEW_MM * 25.4;
        let binding = PaperBinding { surface: Arc::new(PaperSurface { texture: texture.clone(), mapping: reference([size as u32; 2], [ppi; 2]).mapping }), target_offset: [0; 2] };
        let (pixels, [width, _], relief) = bake_binding(renderer, binding);
        let heights: Vec<f32> = (0..size * size).map(|i| f32::from(pixels[(i / size) * width as usize + i % size]) / 255. * relief * REFERENCE_RELIEF_UM).collect();
        layer_core::paper::preview(&heights)
    }

    #[test]
    #[ignore = "writes assets/filters/paper_previews.gray"]
    fn write_built_in_paper_previews() {
        let mut renderer = WgpuRasterizer::new_native_headless(Default::default()).unwrap();
        let atlas: Vec<u8> = layer_core::paper::built_in_papers().iter().flat_map(|paper| preview(&mut renderer, &paper.texture)).collect();
        std::fs::write(concat!(env!("CARGO_MANIFEST_DIR"), "/../../assets/filters/paper_previews.gray"), atlas).unwrap();
    }

    #[test]
    fn built_in_paper_previews_match_their_bakes() {
        let mut renderer = WgpuRasterizer::new_native_headless(Default::default()).unwrap();
        let papers = layer_core::paper::built_in_papers();
        assert_eq!(papers.len(), layer_core::bundled_texture_catalog().filters().iter().map(|d| d.presets.len()).sum::<usize>());
        for paper in papers {
            let fresh = preview(&mut renderer, &paper.texture);
            let difference = fresh.iter().zip(paper.preview.iter()).map(|(a, b)| f64::from(a.abs_diff(*b))).sum::<f64>() / fresh.len() as f64;
            assert!(difference < 1., "{}: preview is {difference:.2} codes from its bake; run write_built_in_paper_previews", paper.id);
        }
    }

    fn seam_ratio(pixels: &[u8], [width, height]: [u32; 2]) -> f64 {
        let at = |x: u32, y: u32| f64::from(pixels[(y * width + x) as usize]);
        let (mut seam, mut interior) = (0., 0.);
        for y in 0..height {
            seam += (at(width - 1, y) - at(0, y)).abs();
            interior += (at(width / 2, y) - at(width / 2 - 1, y)).abs();
        }
        seam / interior.max(1.)
    }

    #[test]
    fn every_built_in_paper_bakes_a_seamless_tile_with_its_relief() {
        let mut renderer = WgpuRasterizer::new_native_headless(Default::default()).unwrap();
        let mut relief = std::collections::BTreeMap::new();
        for definition in layer_core::bundled_texture_catalog().filters() {
            for preset in &definition.presets {
                let instance = definition.preset(preset).unwrap();
                let (pixels, extent, measured) = bake(&mut renderer, TextureSource::Program(instance));
                let period = f64::from(definition.program.period.unwrap()) * 300. / 25.4;
                assert_eq!(extent, [period.round() as u32; 2], "{}", preset.id);
                let mean = pixels.iter().map(|v| f64::from(*v)).sum::<f64>() / pixels.len() as f64;
                let spread = (pixels.iter().map(|v| (f64::from(*v) - mean).powi(2)).sum::<f64>() / pixels.len() as f64).sqrt();
                assert!(spread > 20., "{} has no visible tooth: {spread}", preset.id);
                assert!(seam_ratio(&pixels, extent) < 2.5, "{} shows a seam", preset.id);
                relief.insert(preset.id.to_string(), measured);
            }
        }
        assert!(relief["hot_press"] < relief["cold_press"] && relief["cold_press"] < relief["rough"], "{relief:?}");
        assert!(relief["bristol"] < relief["drawing_paper"], "{relief:?}");
    }

    #[test]
    fn turned_and_flipped_papers_bake_the_turned_tile() {
        use layer_core::color::source::{SourceBuilder, SourceChannels, SourceInterpretation};
        let interpretation = SourceInterpretation { channels: SourceChannels::Gray, depth: layer_core::color::SampleDepth::U8,
            profile: Default::default(), profile_assumed: false };
        let mut builder = SourceBuilder::new([24, 17], interpretation, 1 << 20).unwrap();
        for y in 0..17 { builder.push_row(&(0..24).map(|x| ((x * 37 + y * 101) % 251) as u8).collect::<Vec<_>>()).unwrap(); }
        let mut source = builder.finish().unwrap();
        source.resolution = Some(layer_core::ImageResolution::ppi(300));
        let texture = TextureSource::Image(layer_core::Image::new(Arc::new(source)));
        let mut renderer = WgpuRasterizer::new_native_headless(Default::default()).unwrap();
        let k = 300. / 25.4;
        let binding = |mapping: [f64; 6]| PaperBinding { surface: Arc::new(PaperSurface { texture: texture.clone(), mapping: layer_core::Affine64(mapping) }), target_offset: [0; 2] };
        let (straight, _, _) = bake_binding(&mut renderer, binding([k, 0., 0., k, 0., 0.]));
        for mapping in [[0., k, -k, 0., 17., 0.], [-k, 0., 0., k, 24., 0.], [0., -k, -k, 0., 17., 24.]] {
            let tile = PaperTile::new(&binding(mapping).surface).unwrap();
            let (turned, [width, height], _) = bake_binding(&mut renderer, binding(mapping));
            let [a, b, c, d, x, y] = mapping;
            let det = a * d - b * c;
            for j in 0..height {
                for i in 0..width {
                    let p = [(tile.origin[0] + i64::from(i)) as f64 + 0.5 - x, (tile.origin[1] + i64::from(j)) as f64 + 0.5 - y];
                    let mm = [(d * p[0] - c * p[1]) / det, (a * p[1] - b * p[0]) / det];
                    let source = [(mm[0] * k).floor().rem_euclid(24.) as usize, (mm[1] * k).floor().rem_euclid(17.) as usize];
                    assert_eq!(turned[(j * width + i) as usize], straight[source[1] * 24 + source[0]], "{mapping:?} texel {i},{j}");
                }
            }
        }
    }

    #[test]
    fn image_papers_at_their_own_resolution_are_copied_exactly() {
        use layer_core::color::source::{SourceBuilder, SourceChannels, SourceInterpretation};
        let interpretation = SourceInterpretation { channels: SourceChannels::Gray, depth: layer_core::color::SampleDepth::U8,
            profile: Default::default(), profile_assumed: false };
        let mut builder = SourceBuilder::new([24, 17], interpretation, 1 << 20).unwrap();
        let sample = |x: u32, y: u32| ((x * 37 + y * 101) % 251) as u8;
        for y in 0..17 { builder.push_row(&(0..24).map(|x| sample(x, y)).collect::<Vec<_>>()).unwrap(); }
        let mut source = builder.finish().unwrap();
        source.resolution = Some(layer_core::ImageResolution::ppi(300));
        let mut renderer = WgpuRasterizer::new_native_headless(Default::default()).unwrap();
        let (pixels, extent, relief) = bake(&mut renderer, TextureSource::Image(layer_core::Image::new(Arc::new(source))));
        assert_eq!((extent, relief), ([24, 17], 1.));
        let (low, high) = (0., 250.);
        for y in 0..17 {
            for x in 0..24 {
                let expected = ((f64::from(sample(x, y)) - low) / (high - low) * 255.).round().clamp(0., 255.);
                assert!((f64::from(pixels[(y * 24 + x) as usize]) - expected).abs() <= 1., "texel {x},{y}");
            }
        }
    }
}
