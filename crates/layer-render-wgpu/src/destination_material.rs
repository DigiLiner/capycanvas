//! Smudge, Wet and watercolor read only the pre-batch pages around
//! each page they write, so one compute pass dispatches every page job of a
//! batch. A job writes its whole inactive page: the material result inside its
//! dirty rectangle and the unchanged texel elsewhere, so no page copy precedes
//! it. Watercolor wetness is stored only inside the rectangle.
use super::*;
use dry_material::{Entries, MATERIAL_BLENDS, MATERIAL_SLOTS};
use material_sources::{elapsed, record, rect_words};

pub(super) const OPERATIONS: [MaterialOperation; 3] = [MaterialOperation::Smudge,
    MaterialOperation::Wet, MaterialOperation::Watercolor];

/// The workgroup of an operation's page and reservoir kernels.
pub(super) fn workgroup(operation: MaterialOperation) -> [u32; 2] {
    if operation == MaterialOperation::Smudge { [16, 8] } else { [8, 8] }
}

pub(super) fn transported(style: &layer_render::DabStyle) -> bool {
    OPERATIONS.contains(&BrushPassPlan::for_style(style).material)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(super) enum Entry {
    Color,
    Coverage,
    Reservoir,
    Watercolor,
}

impl Entry {
    pub(super) fn name(self) -> &'static str {
        match self {
            Self::Color => "transport_color",
            Self::Coverage => "transport_coverage",
            Self::Reservoir => "reservoir_exchange",
            Self::Watercolor => "transport_watercolor",
        }
    }

    /// The output layout: color, with coverage, or with coverage and wetness.
    fn outputs(self) -> usize {
        match self {
            Self::Coverage => 1,
            Self::Watercolor => 2,
            Self::Color | Self::Reservoir => 0,
        }
    }
}

pub(super) fn entries(operation: MaterialOperation) -> &'static [Entry] {
    match operation {
        MaterialOperation::Wet => &[Entry::Color, Entry::Coverage, Entry::Reservoir],
        MaterialOperation::Watercolor => &[Entry::Watercolor],
        MaterialOperation::Smudge => &[Entry::Color, Entry::Coverage],
        _ => unreachable!("operation uses its own pipeline"),
    }
}

/// A blender kind and whether its kernel reads the paper.
pub(super) type Blender = (layer_core::BlenderKind, bool);

/// The blenders an operation compiles a module for: for Smudge, every kind
/// without the paper and the kinds that touch it with the paper too.
pub(super) fn blenders(operation: MaterialOperation) -> Vec<Blender> {
    if operation != MaterialOperation::Smudge { return vec![(layer_core::BlenderKind::Soft, false)]; }
    layer_core::BlenderKind::ALL.into_iter().map(|kind| (kind, false))
        .chain(layer_core::BlenderKind::ALL.into_iter().filter(|kind| kind.touches_paper()).map(|kind| (kind, true)))
        .collect()
}

/// The blender kernel a style needs: it reads the paper when the kind
/// touches paper and the brush has Texture strength.
pub(super) fn blender(style: &layer_render::DabStyle) -> Blender {
    (style.blender.kind, style.blender.kind.touches_paper() && style.grain.is_some_and(|grain| grain.depth > 0.0))
}

/// The module that provides `entry` of `operation` for one blend code and blender.
pub(super) fn module(operation: MaterialOperation, blend: u32, (kind, paper): Blender) -> Entries {
    match operation {
        MaterialOperation::Smudge => Entries::Blend(kind, paper),
        _ => Entries::Transport(blend),
    }
}

/// The kernels a brush selection prepares for committed and predicted pages.
pub(super) fn selected(plan: BrushPassPlan, preview: bool) -> impl Iterator<Item = Entry> {
    entries(plan.material).iter().copied().filter(move |&entry| match entry {
        Entry::Color => preview || !plan.state.coverage,
        Entry::Coverage => plan.state.coverage,
        Entry::Reservoir | Entry::Watercolor => true,
    })
}

/// Wet and watercolor deposit through the brush blend. Smudge only
/// move paint, so every blend shares the normal slot of their contact.
pub(super) fn blended(operation: MaterialOperation) -> bool {
    matches!(operation, MaterialOperation::Wet | MaterialOperation::Watercolor)
}

pub(super) struct Pipelines {
    layouts: [wgpu::BindGroupLayout; 3],
    kernels: std::collections::BTreeMap<(u32, usize, Entry, u8, bool), Deferred<wgpu::ComputePipeline>>,
}

impl Pipelines {
    /// `shared` holds the target, material source and texture layouts;
    /// `layouts` the write-only color, color-with-coverage and watercolor outputs.
    pub fn new(device: &PipelineDevice, shared: [&wgpu::BindGroupLayout; 3], layouts: [wgpu::BindGroupLayout; 3]) -> Self {
        let pipeline_layouts: [_; 3] = std::array::from_fn(|outputs| device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("destination material pages"),
            bind_group_layouts: &[Some(&layouts[outputs]), Some(shared[0]), Some(shared[1]), Some(shared[2])],
            immediate_size: 0,
        }));
        let mut kernels = std::collections::BTreeMap::new();
        for (slot, operation) in (0..MATERIAL_SLOTS).flat_map(|slot| OPERATIONS.map(|operation| (slot, operation))) {
            if !blended(operation) && slot % MATERIAL_BLENDS.len() != 0 { continue; }
            let (flags, blend) = dry_material::material_entries(slot);
            for blender in blenders(operation) {
                let transport = dry_material::material_shader(device, module(operation, blend, blender), flags, operation);
                for &entry in entries(operation) {
                    kernels.insert((operation as u32, slot, entry, blender.0 as u8, blender.1), Deferred::compute(device,
                        format!("destination material {} operation={} contact={flags:#x} blend={blend} blender={:?} paper={}", entry.name(), operation as u32, blender.0, blender.1),
                        &pipeline_layouts[entry.outputs()], &transport, entry.name()));
                }
            }
        }
        Self { layouts, kernels }
    }

    pub fn kernel(&self, style: &layer_render::DabStyle, entry: Entry) -> &Deferred<wgpu::ComputePipeline> {
        let operation = BrushPassPlan::for_style(style).material;
        let slot = if blended(operation) { dry_material::material_slot(style) }
            else { dry_material::material_contact_slot(style) * MATERIAL_BLENDS.len() };
        let (kind, paper) = if operation == MaterialOperation::Smudge { blender(style) } else { (layer_core::BlenderKind::Soft, false) };
        &self.kernels[&(operation as u32, slot, entry, kind as u8, paper)]
    }

    fn output(&self, r: &WgpuRasterizer, color: &PageSurface, coverage: Option<&PageSurface>, wetness: Option<&PageSurface>) -> wgpu::BindGroup {
        let layout = &self.layouts[usize::from(coverage.is_some()) + usize::from(wetness.is_some())];
        dry_material::output_binding(r, layout, color, coverage, wetness, None)
    }
}

pub(super) struct Dispatch {
    entry: Entry,
    output: wgpu::BindGroup,
    sources: wgpu::BindGroup,
    coordinate: [u32; 2],
    record_offset: u32,
}

impl WgpuRasterizer {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn encode_transport_jobs(
        &mut self,
        encoder: &mut crate::submission::CommandEncoder,
        batch_index: usize,
        batch: &DabBatch,
        dabs: &[Dab],
        jobs: &[MaterialJob],
        layer_index: usize,
        preview: bool,
        sources_preview: bool,
    ) -> Result<(), GpuRasterError> {
        let started = self.telemetry.enabled.then(web_time::Instant::now);
        let extent = self.target_extent(batch.target);
        let operation = BrushPassPlan::for_style(&batch.style).material;
        let exchange = dabs.first().zip(dabs.last()).filter(|_| !preview && operation == MaterialOperation::Wet);
        let seeds: Vec<_> = jobs.iter().map(|job| operation == MaterialOperation::Watercolor && !self.watercolor_written.contains(&job.coordinate)).collect();
        self.watercolor_written.extend(jobs.iter().zip(&seeds).filter(|(_, seed)| **seed).map(|(job, _)| job.coordinate));
        self.metrics.material_cpu_ms[0] += elapsed(started);
        let mut first_record = self.prepare_records(jobs.iter().zip(&seeds).map(|(job, &seed)| {
            record([0, 0, 0, u32::from(seed)], [rect_words(job.local)])
        }).chain(exchange.map(|(first, _)| {
            let color = first.color_rgba_linear;
            record([0; 4], [[color[0], color[1], color[2], batch.style.wet_mix.amount_of_paint * first.material[2]].map(f32::to_bits)])
        })), encoder)?;
        let mut dispatches = Vec::with_capacity(jobs.len());
        for job in jobs {
            let layer = &self.paint_layers[layer_index];
            let (color_pages, coverage_pages, wetness_pages) = if preview {
                (&self.preview_pages, &self.preview_coverage_pages, &self.preview_watercolor_wetness_pages)
            } else {
                (&layer.pages, &layer.coverage_pages, &layer.watercolor_wetness_pages)
            };
            let destination = color_pages[job.page_index].surface(job.destination_secondary);
            let coverage = job.coverage_index.zip(job.coverage_destination_secondary)
                .map(|(i, secondary)| if secondary { &coverage_pages[i].secondary } else { &coverage_pages[i].primary });
            let wetness = (operation == MaterialOperation::Watercolor).then(|| wetness_pages.iter().find(|p| p.coordinate == job.coordinate)
                .expect("watercolor wetness page is prepared before encoding").inactive());
            destination.preview.set(None);
            let output = self.pipelines.destination_material.output(self, destination, coverage, wetness);
            let entry = if wetness.is_some() { Entry::Watercolor } else if coverage.is_some() { Entry::Coverage } else { Entry::Color };
            self.add_transport_job(encoder, batch_index, batch, job.coordinate, first_record, sources_preview, entry, output, &mut dispatches)?;
            first_record += 1;
        }
        if let Some((_, last)) = exchange {
            let coordinate = std::array::from_fn(|i| ([last.center.x, last.center.y][i].max(0.) as u32 / PAGE_SIZE).min(extent[i].saturating_sub(1) / PAGE_SIZE));
            let output = self.pipelines.destination_material.output(self, self.reservoir.inactive(), None, None);
            self.add_transport_job(encoder, batch_index, batch, coordinate, first_record, false, Entry::Reservoir, output, &mut dispatches)?;
        }
        self.encode_transport(encoder, batch_index, batch, &dispatches);
        if exchange.is_some() { self.reservoir.active_secondary = !self.reservoir.active_secondary; }
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    fn add_transport_job(
        &mut self,
        encoder: &mut crate::submission::CommandEncoder,
        batch_index: usize,
        batch: &DabBatch,
        coordinate: [u32; 2],
        record_index: usize,
        preview: bool,
        entry: Entry,
        output: wgpu::BindGroup,
        dispatches: &mut Vec<Dispatch>,
    ) -> Result<(), GpuRasterError> {
        let timing = self.telemetry.enabled;
        let offsets = self.material_offsets(batch);
        self.encode_before_borrowing(encoder, batch_index, batch, coordinate, offsets, preview, dispatches);
        let started = timing.then(web_time::Instant::now);
        let (sources, _) = self.material_bind_group(batch, coordinate, preview, MaterialInputs::Neighborhood, None, encoder)?;
        self.metrics.material_cpu_ms[4] += elapsed(started);
        dispatches.push(Dispatch { entry, output, sources, coordinate, record_offset: self.material_records.offset(record_index) });
        Ok(())
    }

    /// The decoded-tile cache may reuse a slot that a pending dispatch reads,
    /// so those dispatches are encoded before such a preparation.
    #[allow(clippy::too_many_arguments)]
    fn encode_before_borrowing(
        &mut self,
        encoder: &mut crate::submission::CommandEncoder,
        batch_index: usize,
        batch: &DabBatch,
        coordinate: [u32; 2],
        offsets: [Option<[i32; 2]>; 9],
        preview: bool,
        dispatches: &mut Vec<Dispatch>,
    ) {
        if self.borrows_decoded_tiles(batch.target, coordinate, offsets, preview) {
            self.encode_transport(encoder, batch_index, batch, dispatches);
            dispatches.clear();
        }
    }

    fn encode_transport(&mut self, encoder: &mut crate::submission::CommandEncoder, batch_index: usize, batch: &DabBatch, dispatches: &[Dispatch]) {
        if dispatches.is_empty() {
            return;
        }
        let started = self.telemetry.enabled.then(web_time::Instant::now);
        let texture_key = Self::texture_set_key(&batch.style);
        let textures = &self.texture_sets.iter().find(|set| set.key == texture_key).unwrap().bind_group;
        let style_offset = batch_index as u32 * self.style_stride as u32;
        let [width, height] = workgroup(BrushPassPlan::for_style(&batch.style).material);
        let pass = encoder.compute_pass();
        pass.set_bind_group(3, textures, &[]);
        for dispatch in dispatches {
            pass.set_pipeline(self.pipelines.destination_material.kernel(&batch.style, dispatch.entry));
            pass.set_bind_group(0, &dispatch.output, &[style_offset]);
            pass.set_bind_group(1, self.paint_target_binding(&batch.style), &[self.layer_target_offset(batch.target, dispatch.coordinate)]);
            pass.set_bind_group(2, &dispatch.sources, &[dispatch.record_offset]);
            let size = if dispatch.entry == Entry::Reservoir { RESERVOIR_SIZE } else { PAGE_SIZE };
            pass.dispatch_workgroups(size / width, size / height, 1);
        }
        self.metrics.material_cpu_ms[3] += elapsed(started);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use layer_core::DefaultBrushPreset as Preset;

    #[test]
    fn selecting_a_transported_brush_compiles_only_its_page_and_reservoir_kernels() {
        let reference = WgpuRasterizer::new_native_headless(Default::default()).unwrap();
        let document = layer_core::Document::new(layer_core::authored::PortableId::random(), 128, 128,
            layer_core::DocumentNames { paint: "Current ink".into(), paper: "Paper".into() });
        for (preset, expected) in [(Preset::Smudge, 1), (Preset::NaturalBlender, 1), (Preset::WetRound, 2),
            (Preset::OpaqueGouache, 3), (Preset::LoadedOil, 3), (Preset::PaletteKnife, 3), (Preset::WatercolorWash, 1), (Preset::WetWatercolor, 1)] {
            let mut renderer = crate::test_support::staged_renderer(&reference, Default::default());
            renderer.shader_idle(false, false);
            let mut brush = layer_core::default_brush(preset);
            renderer.prepare_startup(&document, &brush, false).unwrap();
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(60);
            crate::test_support::wait_startup(&mut renderer, deadline, |progress| progress.brush_ready, format_args!("{preset:?} preparation timed out"));
            let pipelines = &renderer.pipelines;
            let transport = &pipelines.destination_material;
            let ready = transport.kernels.values().filter(|p| p.ready()).count();
            assert_eq!(ready, expected, "{preset:?} compiles its commit, prediction and reservoir kernels");
            let normal = layer_render::DabStyle::for_brush(&brush, layer_core::StrokeTool::Brush);
            brush.rendering.blend_mode = BrushBlendMode::Multiply;
            let multiply = layer_render::DabStyle::for_brush(&brush, layer_core::StrokeTool::Brush);
            let operation = BrushPassPlan::for_style(&normal).material;
            for &entry in entries(operation) {
                assert_eq!(std::ptr::eq(transport.kernel(&normal, entry), transport.kernel(&multiply, entry)), !blended(operation),
                    "{preset:?} {entry:?} has a module per blend only when it deposits through the blend");
            }
        }
    }

    #[test]
    fn transport_kernels_declare_the_workgroup_they_are_dispatched_with() {
        for operation in OPERATIONS {
            for (&entry, blender) in entries(operation).iter().flat_map(|entry| blenders(operation).into_iter().map(move |blender| (entry, blender))) {
                let source = dry_material::material_source(Default::default(), crate::pipeline_device::float_sampling_wgsl(true),
                    module(operation, 0, blender), 0, operation);
                let module = naga::front::wgsl::parse_str(&source).unwrap();
                let kernel = module.entry_points.iter().find(|e| e.name == entry.name()).unwrap();
                let [width, height] = workgroup(operation);
                assert_eq!(kernel.workgroup_size, [width, height, 1], "{operation:?} {entry:?} {blender:?}");
                assert_eq!([PAGE_SIZE % width, PAGE_SIZE % height, RESERVOIR_SIZE % width, RESERVOIR_SIZE % height], [0; 4]);
            }
        }
    }
}
