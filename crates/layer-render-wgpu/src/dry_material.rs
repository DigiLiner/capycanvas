//! Dry pages have no cross-page dependencies: dispatch their unchanged pixel
//! evaluator together instead of opening a render pass for every page.
use super::*;

pub(super) struct Job {
    pub output: wgpu::BindGroup,
    pub source: wgpu::BindGroup,
    pub display: Option<DisplayJob>,
    pub coordinate: [u32; 2],
    pub coverage: bool,
    pub record_offset: u32,
    pub dirty: PixelRect,
}

impl Job {
    fn inputs(&self) -> &wgpu::BindGroup {
        self.display.as_ref().map_or(&self.source, |display| &display.inputs)
    }
}

/// A reduced-resolution prediction first averages its page inputs into a
/// display-input set, then evaluates against those averages.
pub(super) struct DisplayJob {
    pub gather: wgpu::BindGroup,
    pub inputs: wgpu::BindGroup,
}

pub(super) const FINE_TIP_RADIUS: f32 = 8.;
pub(super) const DISPLAY_INPUT_SETS: usize = 8;
pub(super) const AGGREGATE: u32 = 8192;
pub(super) const FINE_TIP: u32 = 16384;

pub(super) fn shader(device: &PipelineDevice) -> Deferred<wgpu::ShaderModule> {
    let device = device.clone();
    Deferred::new(move || {
        device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("layer destination brush shader"),
            source: wgpu::ShaderSource::Wgsl(shader_source(device.working_space(), device.float_sampling_wgsl(), include_str!("material_brush.wgsl"))),
        })
    })
}

/// Where a dry kernel finds each texel's starting paint.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Start {
    /// In the output texel, which the kernel reads and updates in place, or
    /// zero for a display contribution. A prediction first stores its
    /// starting paint in its output.
    Stored,
    /// In a sampled page, for devices without read-write storage.
    Sampled,
}

fn destination(start: Start) -> Cow<'static, str> {
    match start {
        Start::Stored => compose_wgsl(&[include_str!("dry_destination.wgsl"), include_str!("changed_cells.wgsl"), include_str!("dry_read_write.wgsl")]),
        Start::Sampled => compose_wgsl(&[include_str!("dry_destination.wgsl"), include_str!("dry_write.wgsl")]),
    }
}

pub(super) fn shader_source(space: layer_core::color::RgbSpace, float_sampling: &str, material: &str) -> Cow<'static, str> {
    compose_wgsl(&[
        &working_color::source(space), include_str!("blend_modes.wgsl"), float_sampling, &destination(Start::Sampled), include_str!("brush_types.wgsl"), include_str!("brush_textures.wgsl"), include_str!("retouch_sample.wgsl"),
        include_str!("material_common.wgsl"), include_str!("material_clone.wgsl"),
        include_str!("material_pointwise.wgsl"), include_str!("material_transport.wgsl"),
        include_str!("material_contact.wgsl"), material, include_str!("material_compute.wgsl"),
        &coverage_source(u32::MAX), include_str!("selection_clip.wgsl"),
    ])
}

pub(super) fn coverage_source(flags:u32)->Cow<'static,str> {
    let relief_contact=if flags==u32::MAX || flags&FINE_TIP!=0 {include_str!("relief_fine.wgsl")} else {include_str!("relief_coarse.wgsl")};
    if flags!=u32::MAX && flags&64!=0 {
        compose_wgsl(&[include_str!("brush_math.wgsl"),include_str!("contact_field.wgsl"),
            include_str!("relief.wgsl"),relief_contact,include_str!("dry_footprint.wgsl"),include_str!("brush_alpha.wgsl")])
    } else {
        compose_wgsl(&[include_str!("brush_math.wgsl"),include_str!("analytic_coverage.wgsl"),
            include_str!("brush_coverage.wgsl"),include_str!("contact_field.wgsl"),
            include_str!("contact.wgsl"),include_str!("relief.wgsl"),relief_contact,include_str!("bristle.wgsl"),
            include_str!("brush_footprint.wgsl"),include_str!("brush_alpha.wgsl")])
    }
}

pub(super) fn specialize_contact(source: Cow<'static,str>, flags:u32)->Cow<'static,str> {
    if flags==u32::MAX {return source;}
    source.replace("override CONTACT_FLAGS: u32 = 4294967295u;",
        &format!("const CONTACT_FLAGS: u32 = {flags}u;")).into()
}

pub(super) fn kernel_source(space:layer_core::color::RgbSpace,float_sampling:&str,start:Start,flags:u32,operation:MaterialOperation)->Cow<'static,str> {
    let leaf=if flags!=u32::MAX && flags&64!=0 {include_str!("dry_material_contact.wgsl")} else {include_str!("material_contact.wgsl")};
    let source=compose_wgsl(&[&working_color::source(space),include_str!("blend_modes.wgsl"),float_sampling,
        &destination(start),include_str!("brush_types.wgsl"),include_str!("brush_textures.wgsl"),
        include_str!("retouch_sample.wgsl"),include_str!("material_common.wgsl"),include_str!("material_clone.wgsl"),
        include_str!("material_pointwise.wgsl"),leaf,
        "fn material_result(p:vec4<f32>)->MaterialOutput {return pointwise_material_result(p);}",
        include_str!("material_compute.wgsl"),&coverage_source(flags),include_str!("selection_clip.wgsl")]);
    specialize_contact(source,flags)
        .replace("override MATERIAL_OPERATION: u32;",&format!("const MATERIAL_OPERATION: u32 = {}u;",operation as u32))
        .replace("override MATERIAL_NORMAL_BLEND: bool = false;","const MATERIAL_NORMAL_BLEND: bool = true;").into()
}

/// One reduced-resolution page of averaged original paint, stroke coverage
/// and starting paint.
pub(super) struct DisplayInputs {
    views: [wgpu::TextureView; 3],
    inputs: bindings::MaterialInput,
    gather: bindings::CachedBinding<(wgpu::Buffer, wgpu::TextureView)>,
}

impl DisplayInputs {
    const FORMATS: [wgpu::TextureFormat; 3] = [wgpu::TextureFormat::Rgba32Float, wgpu::TextureFormat::R32Float, wgpu::TextureFormat::Rgba32Float];

    pub fn new(device: &wgpu::Device) -> Self {
        Self {
            views: Self::FORMATS.map(|format| create_target(device, [PAGE_SIZE / 2; 2], format, "dry display inputs").1),
            inputs: Default::default(),
            gather: Default::default(),
        }
    }

    pub fn storage_bytes() -> u64 {
        Self::FORMATS.iter().map(|f| u64::from(f.block_copy_size(None).unwrap())).sum::<u64>() * u64::from(PAGE_SIZE / 2).pow(2)
    }

    /// The evaluator's inputs: the job's own neighborhood around the averaged
    /// page, coverage and starting paint.
    pub fn inputs(&self, r: &WgpuRasterizer, neighborhood: [&wgpu::TextureView; 9]) -> wgpu::BindGroup {
        let mut views = neighborhood;
        views[4] = &self.views[0];
        let bound = std::array::from_fn(|i| match i { 9 => &self.views[1], 10 => &self.views[2], _ => views[i] }.clone());
        self.inputs.get(([r.dab_buffer.clone(), r.dry_records.binding().buffer.clone()], bound), || create_material_bind_group(
            &r.device, &r.material_layout, &views, &r.dab_buffer,
            &self.views[1], &self.views[2], wgpu::BindingResource::Buffer(r.dry_records.binding())))
    }

    /// The gather pass's outputs. Where the start is stored, the averaged
    /// starting paint goes straight to the prediction's output.
    pub fn gather(&self, r: &WgpuRasterizer, output: &PageSurface) -> wgpu::BindGroup {
        let original = if r.pipelines.dry_material.in_place { &output.view } else { &self.views[0] };
        self.gather.get((r.style_buffer.clone(), original.clone()), || {
            let entries = [wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                    buffer: &r.style_buffer, offset: 0, size: NonZeroU64::new(mem::size_of::<StyleGpu>() as u64),
                }),
            }].into_iter().chain([original, &self.views[1], &self.views[2]].into_iter().zip(4..).map(|(view, binding)| wgpu::BindGroupEntry {
                binding, resource: wgpu::BindingResource::TextureView(view),
            })).collect::<Vec<_>>();
            r.device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("dry display input gather"),
                layout: &r.pipelines.dry_material.gather_layout,
                entries: &entries,
            })
        })
    }
}

/// Dry dabs carry their color in the space they lay over paint in: the
/// document's encoded values in a Perceptual document, so the shader does not
/// convert it for every pixel.
pub(super) fn prepare_colors(style: &layer_render::DabStyle, space: layer_core::color::RgbSpace, dabs: &mut [DabGpu]) {
    if style.execution != BrushExecution::Dry
        || style.mode != DabMode::Paint
        || style.blend_space != layer_core::BlendSpace::Perceptual
    {
        return;
    }
    for dab in dabs {
        for channel in &mut dab.dab.color_rgba_linear[..3] {
            *channel = space.encode(f64::from(*channel)) as f32;
        }
    }
}

pub(super) fn prepare_geometry(style: &layer_render::DabStyle, dabs: &mut [DabGpu]) {
    let Some(dry)=style.contact.and_then(|c|c.dry) else {return};
    for d in dabs {
        let m=d.metric;
        let v=[d.dab.motion[0]*m[0]+d.dab.motion[1]*m[1],d.dab.motion[0]*m[2]+d.dab.motion[1]*m[3]];
        if dry.corner==0. {
            let offset = dry.relief_offset * d.dab.material[3] / (m[0]*m[0]+m[1]*m[1]);
            let tip = [m[0]*offset,m[1]*offset];
            let speed=v[0].hypot(v[1]);
            let inverse=if speed*speed>=0.00000001 {speed.recip()} else {0.};
            if speed>0. {
                let (c,s)=(v[0]/speed,v[1]/speed);
                d.metric=[c*m[0]+s*m[2],c*m[1]+s*m[3],-s*m[0]+c*m[2],-s*m[1]+c*m[3]];
            }
            d.invariants=[speed,inverse,speed*speed,inverse*inverse];
            d.dab.material[1]=d.metric[0]*tip[0]+d.metric[1]*tip[1];
            d.dab.material[2]=d.metric[2]*tip[0]+d.metric[3]*tip[1];
        } else {
            let axes=[(d.dab.radii[0]+d.dab.previous[0])*0.5,(d.dab.radii[1]+d.dab.previous[1])*0.5].map(|x|x.max(0.005));
            let rim=dry.corner*axes[0].min(axes[1]);
            d.invariants=[-v[0],-v[1],rim/axes[0],rim/axes[1]];
            let scaled=[-v[0]/d.invariants[2],-v[1]/d.invariants[3]];
            d.dab.material=[-v[0],-v[1],scaled[0]+scaled[1],scaled[0]-scaled[1]]
                .map(|v|if v.abs()<0.000001 {0.} else {v.recip()});
        }
    }
}

/// The pointwise operations with page kernels, each with and without a
/// coverage output.
const OPERATIONS: [MaterialOperation; 3] = [MaterialOperation::Deposit, MaterialOperation::Coverage, MaterialOperation::Clone];
type Kernels = [Deferred<wgpu::ComputePipeline>; 2 * OPERATIONS.len()];

pub(super) struct Pipelines {
    in_place: bool,
    layouts: [wgpu::BindGroupLayout; 2],
    gather_layout: wgpu::BindGroupLayout,
    pub kernels: Kernels,
    copy: Deferred<wgpu::ComputePipeline>,
    gather: Deferred<wgpu::ComputePipeline>,
    variants: std::collections::BTreeMap<u32, Kernels>,
}

pub(super) fn contact_flags(contact: Option<layer_core::BrushContact>) -> u32 {
    let Some(c) = contact else {
        return 0;
    };
    1 | (u32::from(c.paper > 0.) << 1)
        | (u32::from(c.edge_roughness > 0.) << 2)
        | (u32::from(c.fiber_strength > 0.) << 3)
        | (u32::from(c.pooling > 0.) << 4)
        | (u32::from(c.depletion > 0.) << 5)
        | (u32::from(c.dry.is_some()) << 6)
        | (u32::from(c.linear_edge) << 8)
        | (u32::from(c.bristles.is_some()) << 9)
        | (u32::from(c.dry.is_some_and(|d|d.poses.iter().any(|p|p.relief[2]>0.000001))) << 11)
        | (u32::from(c.dry.is_some_and(|d|d.corner>0.)) << 12)
        | (u32::from(c.dry.is_some_and(|d|d.relief_offset!=0.)) << 15)
}

impl Pipelines {
    /// `shared` holds the target, material source and texture layouts.
    pub fn new(device: &PipelineDevice, shared: [&wgpu::BindGroupLayout; 3], in_place: bool) -> Self {
        let style = || crate::bindings::buffer(0, wgpu::ShaderStages::COMPUTE, wgpu::BufferBindingType::Uniform, true,
            NonZeroU64::new(mem::size_of::<StyleGpu>() as u64));
        let storage = |binding, format| crate::bindings::storage_texture(binding, wgpu::ShaderStages::COMPUTE, format,
            if in_place && binding == 1 { wgpu::StorageTextureAccess::ReadWrite } else { wgpu::StorageTextureAccess::WriteOnly });
        let layouts = std::array::from_fn(|coverage| {
            let mut entries = vec![style(), storage(1, wgpu::TextureFormat::Rgba32Float)];
            if coverage == 1 { entries.push(storage(2, wgpu::TextureFormat::R32Float)); }
            if in_place {
                entries.push(crate::bindings::buffer(3, wgpu::ShaderStages::COMPUTE,
                    wgpu::BufferBindingType::Storage { read_only: false }, false, NonZeroU64::new(20)));
            }
            crate::bindings::layout(device, "dry material outputs", &entries)
        });
        let gather_layout = crate::bindings::layout(device, "dry display inputs", &[style(),
            storage(4, wgpu::TextureFormat::Rgba32Float), storage(5, wgpu::TextureFormat::R32Float), storage(6, wgpu::TextureFormat::Rgba32Float)]);
        let pipeline_layout = |group: &wgpu::BindGroupLayout| device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("dry material pages"),
            bind_group_layouts: &[Some(group), Some(shared[0]), Some(shared[1]), Some(shared[2])],
            immediate_size: 0,
        });
        let pipeline_layouts: [_; 2] = std::array::from_fn(|coverage| pipeline_layout(&layouts[coverage]));
        let start = if in_place { Start::Stored } else { Start::Sampled };
        let make_kernels = |flags: u32| -> Kernels {
            let shaders=OPERATIONS.map(|operation| {
                let device=device.for_recipe();
                Deferred::new(move||device.create_shader_module(wgpu::ShaderModuleDescriptor {
                    label:Some("layer specialized contact shader"),
                    source:wgpu::ShaderSource::Wgsl(kernel_source(device.working_space(),device.float_sampling_wgsl(),start,flags,operation)),
                }))
            });
            std::array::from_fn(|index| {
                let layout = pipeline_layouts[index % 2].clone();
                let entry = if index % 2 == 0 { "compute_color" } else { "compute_coverage" };
                let label = format!("dry material {entry} operation={} contact={flags:#x}", OPERATIONS[index / 2] as u32);
                let (device, shader) = (device.for_recipe(), shaders[index/2].clone());
                Deferred::pipeline(move |mode| {
                    mode.compute(
                        &device,
                        &wgpu::ComputePipelineDescriptor {
                            label: Some(&label),
                            layout: Some(&layout),
                            module: &shader,
                            entry_point: Some(entry),
                            compilation_options: Default::default(),
                            cache: None,
                        },
                    )
                })
            })
        };
        let pages = Deferred::wgsl(device, "dry material pages",
            kernel_source(device.working_space(), device.float_sampling_wgsl(), start, u32::MAX, MaterialOperation::Deposit));
        let copy = Deferred::compute(device, "dry unchanged pixels", &pipeline_layouts[0], &pages, "copy_color");
        let gather = Deferred::compute(device, "dry display inputs", &pipeline_layout(&gather_layout), &pages, "gather_display");
        let kernels = make_kernels(u32::MAX);
        let retouching = [
            layer_core::DefaultBrushPreset::CloneStamp,
            layer_core::DefaultBrushPreset::HealingBrush,
            layer_core::DefaultBrushPreset::SpotHealingBrush,
        ];
        let mut flags = layer_core::CONTACT_BRUSH_PRESETS
            .into_iter()
            .chain(retouching)
            .map(|p| contact_flags(layer_core::default_brush(p).contact))
            .collect::<std::collections::BTreeSet<_>>();
        for paper in [0,2] {for quartic in [0,2048] {for corner in [0,4096] {
            flags.insert(1|64|paper|quartic|corner);
        }}}
        flags.extend(flags.clone().into_iter().filter(|f|f&64!=0 && f&4096==0).map(|f|f|32768));
        flags.insert(0);
        flags.extend(flags.clone().into_iter().map(|flags| flags | 128));
        flags.extend(flags.clone().into_iter().filter(|f|f&64!=0 && f&128==0).map(|f|f|AGGREGATE));
        flags.extend(flags.clone().into_iter().filter(|f|f&64!=0 && f&4096==0).map(|f|f|FINE_TIP));
        let variants = flags.into_iter().map(|f| (f, make_kernels(f))).collect();
        Self { in_place, layouts, gather_layout, kernels, copy, gather, variants }
    }

    pub(super) fn in_place(&self) -> bool {
        self.in_place
    }

    pub(super) fn copy_kernel(&self) -> &Deferred<wgpu::ComputePipeline> {
        &self.copy
    }

    pub(super) fn gather_kernel(&self) -> &Deferred<wgpu::ComputePipeline> {
        &self.gather
    }

    pub(super) fn selected_kernel(&self,style:&layer_render::DabStyle,operation:MaterialOperation,coverage:bool,extra:u32)->&Deferred<wgpu::ComputePipeline> {
        // Flow integration and maximum-film deposition have different kernels.
        // Resolve that uniform branch at compilation, including its register
        // requirements, rather than carrying both models through every pixel.
        let flags = extra | contact_flags(style.contact)
            | if style.rendering.accumulation == BrushAccumulation::Uniform { 128 } else { 0 };
        let kernels = self.variants.get(&flags).unwrap_or(&self.kernels);
        let slot = OPERATIONS.iter().position(|o| *o == operation).expect("a pointwise operation");
        &kernels[slot * 2 + usize::from(coverage)]
    }

    /// Changed cells are marked only for a tracked page; every other in-place
    /// destination binds the disabled set.
    pub fn output(&self, r: &WgpuRasterizer, color: &PageSurface, coverage: Option<&PageSurface>,
        tracked: Option<(SourceTarget, [u32; 2])>) -> wgpu::BindGroup {
        let changed = self.in_place.then(|| {
            let cells = r.changed_cells.as_ref().unwrap();
            tracked.and_then(|(target, coordinate)| cells.buffer(target, coordinate)).unwrap_or(&cells.disabled)
        });
        let mut entries = vec![
            wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                    buffer: &r.style_buffer,
                    offset: 0,
                    size: NonZeroU64::new(mem::size_of::<StyleGpu>() as u64),
                }),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::TextureView(&color.view),
            },
        ];
        if let Some(coverage) = coverage {
            entries.push(wgpu::BindGroupEntry { binding: 2, resource: wgpu::BindingResource::TextureView(&coverage.view) });
        }
        if let Some(changed) = changed {
            entries.push(wgpu::BindGroupEntry { binding: 3, resource: changed.as_entire_binding() });
        }
        coverage.unwrap_or(color).material_output.get(
            (r.style_buffer.clone(), color.view.clone(), coverage.map(|p| p.view.clone()), changed.cloned()),
            || {
                r.device.create_bind_group(&wgpu::BindGroupDescriptor {
                    label: Some("dry material page output"),
                    layout: &self.layouts[usize::from(coverage.is_some())],
                    entries: &entries,
                })
            },
        )
    }
}

impl WgpuRasterizer {
    pub(super) fn dry_display(&self, batch: &DabBatch) -> bool {
        batch.kind == DabBatchKind::Preview && (self.preview_level > 0 || self.compact_preview_contribution(batch))
    }

    pub(super) fn in_place_dry_material(&self, batch: &DabBatch) -> bool {
        batch.kind == DabBatchKind::Persistent && self.compute_dry_material(batch)
            && BrushPassPlan::for_device(&batch.style, &self.device).requires_destination()
            && self.pipelines.dry_material.in_place()
    }

    /// Display blocks average their inputs in a separate pass. A contribution
    /// with no stroke coverage has only zero inputs and skips it.
    pub(super) fn dry_display_gathers(&self, batch: &DabBatch) -> bool {
        self.dry_display(batch) && self.dry_material_block(batch) > 1
            && (!self.compact_preview_contribution(batch) || BrushPassPlan::for_device(&batch.style, &self.device).state.coverage)
    }

    pub(super) fn compute_dry_material(&self, batch: &DabBatch) -> bool {
        // Adreno's compute destination-blend specialization corrupts predicted
        // Multiply batches. Non-normal blends use the ordered fragment path.
        dry_material_compute_eligible(&batch.style)
    }

    pub(super) fn dry_material_block(&self, batch: &DabBatch) -> u32 {
        if batch.kind == DabBatchKind::Preview {
            if self.preview_level > 0 { 1 << self.preview_level } else { 1 }
        } else { 1 }
    }

    pub(super) fn compact_preview_contribution(&self, batch: &DabBatch) -> bool {
        batch.kind == DabBatchKind::Preview && self.preview_contribution
            && batch.style.mode == DabMode::Paint && batch.style.opacity == 1.
            && ((self.preview_level > 0 && display_preview_eligible(&batch.style)) || native_dry_preview_eligible(&batch.style))
    }

    pub(super) fn encode_dry_material_jobs(
        &self,
        encoder: &mut crate::submission::CommandEncoder,
        batch_index: usize,
        batch: &DabBatch,
        jobs: &[Job],
    ) {
        if jobs.is_empty() {
            return;
        }
        let operation = BrushPassPlan::for_device(&batch.style, &self.device).material;
        let pipelines = &self.pipelines.dry_material;
        let in_place = self.in_place_dry_material(batch);
        crate::performance_trace::counter(c"Capy dry material in place", u64::from(in_place));
        let texture_key = Self::texture_set_key(&batch.style);
        let textures = self
            .texture_sets
            .iter()
            .find(|set| set.key == texture_key)
            .unwrap();
        let style_offset = batch_index as u32 * self.style_stride as u32;
        let block = self.dry_material_block(batch);
        let display_size = [PAGE_SIZE.div_ceil(block).div_ceil(32), PAGE_SIZE.div_ceil(block).div_ceil(2), 1];
        if jobs.iter().any(|job| job.display.is_some()) {
            let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("dry material display inputs"), timestamp_writes: None,
            });
            pass.set_bind_group(3, &textures.bind_group, &[]);
            pass.set_pipeline(pipelines.gather_kernel());
            for job in jobs {
                let Some(display) = &job.display else { continue };
                pass.set_bind_group(0, &display.gather, &[style_offset]);
                pass.set_bind_group(1, self.paint_target_binding(&batch.style), &[self.layer_target_offset(batch.target, job.coordinate)]);
                pass.set_bind_group(2, &job.source, &[job.record_offset]);
                pass.dispatch_workgroups(display_size[0], display_size[1], display_size[2]);
            }
        }
        let cropped = operation == MaterialOperation::Deposit && dry_flow(&batch.style);
        let stored_start = pipelines.in_place() && !self.compact_preview_contribution(batch);
        let copied = |job: &&Job| !job.coverage && !in_place && if stored_start { job.display.is_none() } else { cropped };
        if jobs.iter().any(|job| copied(&job)) {
            let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("dry material copy pages"), timestamp_writes: None,
            });
            pass.set_bind_group(3, &textures.bind_group, &[]);
            pass.set_pipeline(pipelines.copy_kernel());
            for job in jobs.iter().filter(copied) {
                pass.set_bind_group(0, &job.output, &[style_offset]);
                pass.set_bind_group(1, self.paint_target_binding(&batch.style), &[self.layer_target_offset(batch.target, job.coordinate)]);
                pass.set_bind_group(2, job.inputs(), &[job.record_offset]);
                pass.dispatch_workgroups(display_size[0], display_size[1], display_size[2]);
            }
        }
        let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
            label: Some("dry material pages"),
            timestamp_writes: None,
        });
        pass.set_bind_group(3, &textures.bind_group, &[]);
        let marker=batch_index*self.style_stride as usize+mem::offset_of!(StyleGpu,material_a)+12;
        let aggregate=f32::from_ne_bytes(self.style_upload[marker..marker+4].try_into().unwrap())>0.5
            && aggregate_kernel(&batch.style);
        let fine=fine_tip_capable(&batch.style)
            && self.dab_upload[batch.first_dab as usize..(batch.first_dab+batch.dab_count) as usize]
                .iter().any(|d|(0..2).any(|i|0.5*(d.dab.radii[i]+d.dab.previous[i])<FINE_TIP_RADIUS));
        let extra=if aggregate {AGGREGATE} else {0} | if fine {FINE_TIP} else {0};
        for job in jobs {
            pass.set_bind_group(0, &job.output, &[style_offset]);
            pass.set_bind_group(
                1,
                self.paint_target_binding(&batch.style),
                &[self.layer_target_offset(batch.target, job.coordinate)],
            );
            pass.set_bind_group(2, job.inputs(), &[job.record_offset]);
            let cropped=!job.coverage && cropped;
            let kernel=pipelines.selected_kernel(&batch.style,operation,job.coverage,extra);
            debug_assert!(kernel.ready() || !self.startup.as_ref().is_some_and(|s| s.current.covers(&batch.style)), "dry kernel {extra:#x} was not prepared for its brush");
            pass.set_pipeline(kernel);
            let dirty=job.dirty;
            let size=if cropped {[dirty.max_x().div_ceil(block)-dirty.min_x()/block,
                dirty.max_y().div_ceil(block)-dirty.min_y()/block]} else {[PAGE_SIZE.div_ceil(block);2]};
            pass.dispatch_workgroups(size[0].div_ceil(32),size[1].div_ceil(2),1);
        }
    }
}

pub(super) fn dry_material_compute_eligible(style: &layer_render::DabStyle) -> bool {
    pointwise(style) && style.rendering.blend_mode == BrushBlendMode::Normal
}

pub(super) fn single_pigment_marked(style: &layer_render::DabStyle) -> bool {
    style.execution == BrushExecution::Dry && !style.contact.is_some_and(|c| c.bristles.is_some())
        && style.rendering.blend_mode == BrushBlendMode::Normal && !style.alpha_locked && style.selection.is_none()
}

pub(super) fn dry_flow(style: &layer_render::DabStyle) -> bool {
    style.contact.is_some_and(|c| c.dry.is_some()) && style.rendering.accumulation == BrushAccumulation::Flow
}

fn aggregate_kernel(style: &layer_render::DabStyle) -> bool {
    single_pigment_marked(style) && dry_flow(style)
}

fn fine_tip_capable(style: &layer_render::DabStyle) -> bool {
    style.contact.is_some_and(|c| c.dry.is_some_and(|d| d.corner == 0.))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Reach {
    coarse_tip: bool,
    varying_color: bool,
}

impl Reach {
    pub fn of(brush: &layer_core::BrushSnapshot) -> Self {
        Self {
            coarse_tip: brush.contact.and_then(|c| c.dry).is_none_or(|d| d.corner > 0.)
                || brush.conservative_minor_radius() >= FINE_TIP_RADIUS,
            varying_color: brush.dab_color_varies(),
        }
    }

    pub fn any_size(self) -> Self {
        Self { coarse_tip: true, ..self }
    }

    pub fn batch_variants(self, style: &layer_render::DabStyle) -> impl Iterator<Item = u32> {
        let aggregate = aggregate_kernel(style);
        let pigments = [aggregate.then_some(AGGREGATE),
            (!aggregate || (style.mode == DabMode::Paint && self.varying_color)).then_some(0)];
        let fine = fine_tip_capable(style);
        let tips = [fine.then_some(FINE_TIP), (!fine || self.coarse_tip).then_some(0)];
        pigments.into_iter().flatten().flat_map(move |pigment| tips.into_iter().flatten().map(move |tip| pigment | tip))
    }
}

/// Basic analytic contacts (including G-Pen, eraser and airbrush) have no
/// unresolved paper, fiber, bristle or texture frequencies at display scale.
pub(super) fn display_preview_eligible(style: &layer_render::DabStyle) -> bool {
    style.execution == BrushExecution::Dry && dry_material_compute_eligible(style) && !style.rendering.edge_after_stroke
        && !style.alpha_locked && style.selection.is_none() && style.grain.is_none()
        && matches!(style.tip, BrushTip::AnalyticEllipse) && contact_flags(style.contact) <= 1
}

pub(super) fn native_dry_preview_eligible(style: &layer_render::DabStyle) -> bool {
    style.execution == BrushExecution::Dry && style.opacity == 1. && dry_material_compute_eligible(style)
        && style.contact.is_some_and(|c| c.dry.is_some()) && contact_flags(style.contact) & !(67|2048|4096|32768) == 0
        && style.rendering.accumulation == BrushAccumulation::Flow && !style.rendering.edge_after_stroke
        && !style.alpha_locked && style.selection.is_none()
}

/// Predictions of these contacts evaluate at the reduced display level when
/// the view is zoomed out; the committed stroke replaces them at full resolution.
pub(super) fn reduced_preview_eligible(style: &layer_render::DabStyle) -> bool {
    display_preview_eligible(style) || native_dry_preview_eligible(style)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(target_os = "windows")]
    #[test]
    #[ignore = "Requires an explicitly selected hardware GPU"]
    fn fine_aggregate_pencil_color_pipeline_prepares_on_hardware() {
        let dxc = std::env::current_exe().unwrap().with_file_name("dxcompiler.dll");
        assert!(dxc.is_file(), "bundled DXC required: {dxc:?}");
        let mut descriptor = wgpu::InstanceDescriptor::new_without_display_handle();
        descriptor.backends = wgpu::Backends::DX12;
        descriptor.flags.remove(wgpu::InstanceFlags::DEBUG);
        descriptor.backend_options.dx12.shader_compiler = wgpu::Dx12Compiler::DynamicDxc {
            dxc_path: dxc.to_str().unwrap().to_owned(),
        };
        let instance = wgpu::Instance::new(descriptor);
        let adapter = pollster::block_on(wgpu::util::initialize_adapter_from_env(&instance, None)).unwrap();
        let features = (adapter.features() & (wgpu::Features::TIMESTAMP_QUERY
            | wgpu::Features::TIMESTAMP_QUERY_INSIDE_ENCODERS | wgpu::Features::FLOAT32_FILTERABLE
            | wgpu::Features::FLOAT32_BLENDABLE)) | native_tiles::native_in_place_features(&adapter);
        let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
            required_features: features,
            required_limits: wgpu::Limits::downlevel_defaults().using_resolution(adapter.limits()),
            memory_hints: memory_hints(),
            ..Default::default()
        })).unwrap();
        let renderer = WgpuRasterizer::from_wgpu_inner(adapter, device.into(), queue, Initialization::Snapshot).unwrap();
        let info = renderer.adapter.get_info();
        assert_ne!(info.device_type, wgpu::DeviceType::Cpu, "{info:?}");
        let material = &renderer.pipelines.dry_material;
        let pipeline = &material.variants.get(&0x6843).expect("fine aggregate Pencil recipe")[0];
        assert_eq!(OPERATIONS[0], MaterialOperation::Deposit);
        assert!(!pipeline.ready());
        eprintln!("adapter={info:?} features={:?} in_place={} filterable={} contact=0x6843 operation=Deposit entry=compute_color cache=none",
            renderer.device.features(), material.in_place(), renderer.device.features().contains(wgpu::Features::FLOAT32_FILTERABLE));
        let internal_scope = renderer.device.push_error_scope(wgpu::ErrorFilter::Internal);
        let validation_scope = renderer.device.push_error_scope(wgpu::ErrorFilter::Validation);
        pollster::block_on(pipeline.prepare()).unwrap();
        let validation = pollster::block_on(validation_scope.pop());
        let internal = pollster::block_on(internal_scope.pop());
        assert!(validation.is_none() && internal.is_none(), "validation={validation:?}; internal={internal:?}");
        assert!(pipeline.ready());
    }

    #[test]
    fn every_kernel_entry_resolves_its_overrides() {
        let validate = |module: &naga::Module| {
            naga::valid::Validator::new(naga::valid::ValidationFlags::all(), naga::valid::Capabilities::all())
                .validate(module).unwrap();
        };
        let mut families=std::collections::BTreeSet::from([0,u32::MAX]);
        for preset in layer_core::CONTACT_BRUSH_PRESETS {
            let flags=contact_flags(layer_core::default_brush(preset).contact);
            families.extend([flags,flags|128,flags|8192,flags|16384,flags|8192|16384]);
        }
        for &flags in &families {for operation in OPERATIONS {for filterable in [true, false] {
            for start in [Start::Stored, Start::Sampled] {
            let source = kernel_source(Default::default(),crate::pipeline_device::float_sampling_wgsl(filterable),start,flags,operation);
            let module = naga::front::wgsl::parse_str(&source).unwrap();
            assert_eq!(module.overrides.iter().any(|(_,v)|v.name.as_deref()==Some("CONTACT_FLAGS")),flags==u32::MAX);
            if flags!=u32::MAX {assert!(source.contains(&format!("const CONTACT_FLAGS: u32 = {flags}u;")));}
            assert!(!module.overrides.iter().any(|(_,v)|v.name.as_deref()==Some("MATERIAL_OPERATION")));
            assert!(source.contains(&format!("const MATERIAL_OPERATION: u32 = {}u;",operation as u32)));
            validate(&module);
            for entry in ["compute_color", "compute_coverage", "copy_color", "gather_display"] {
                let mut used = module.clone();
                used.entry_points.retain(|e| e.name == entry);
                assert_eq!(used.entry_points.len(), 1);
                naga::compact::compact(&mut used, naga::compact::KeepUnused::No);
                assert!(used.overrides.iter().all(|(_, value)| value.init.is_some()),
                    "{entry}/{operation:?}/{flags:#x} needs an override no pipeline supplies");
            }
        }}}}
    }
}
