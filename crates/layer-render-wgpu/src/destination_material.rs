//! Liquify, Smudge and Wet read only the pre-batch pages around each page they
//! write, so one compute pass dispatches every page job of a batch. A job
//! writes its whole inactive page: the material result inside its dirty
//! rectangle and the unchanged texel elsewhere, so no page copy precedes it.
use super::*;
use dry_material::{Entries, MATERIAL_BLENDS, MATERIAL_SLOTS};
use material_sources::{elapsed, record, rect_words};

pub(super) const OPERATIONS: [MaterialOperation; 3] = [MaterialOperation::Liquify, MaterialOperation::Smudge, MaterialOperation::Wet];
const WORKGROUP: u32 = 8;

pub(super) fn transported(style: &layer_render::DabStyle) -> bool {
    OPERATIONS.contains(&BrushPassPlan::for_style(style).material)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(super) enum Entry {
    Gather,
    Color,
    Coverage,
    Reservoir,
}

impl Entry {
    pub(super) fn name(self) -> &'static str {
        match self {
            Self::Gather => "gather_samples",
            Self::Color => "transport_color",
            Self::Coverage => "transport_coverage",
            Self::Reservoir => "reservoir_exchange",
        }
    }
}

/// Liquify and Smudge gather distant samples; Wet exchanges its reservoir
/// with the canvas after each committed batch.
pub(super) fn entries(operation: MaterialOperation) -> &'static [Entry] {
    if operation == MaterialOperation::Wet { &[Entry::Color, Entry::Coverage, Entry::Reservoir] }
    else { &[Entry::Gather, Entry::Color, Entry::Coverage] }
}

/// The kernels a brush selection prepares for committed and predicted pages.
pub(super) fn selected(plan: BrushPassPlan, preview: bool) -> impl Iterator<Item = Entry> {
    entries(plan.material).iter().copied().filter(move |&entry| match entry {
        Entry::Color => preview || !plan.state.coverage,
        Entry::Coverage => plan.state.coverage,
        Entry::Gather | Entry::Reservoir => true,
    })
}

/// Wet deposits through the brush blend. Liquify and Smudge only move paint,
/// so every blend shares the normal slot of their contact.
pub(super) fn blended(operation: MaterialOperation) -> bool {
    operation == MaterialOperation::Wet
}

pub(super) struct Pipelines {
    layouts: [wgpu::BindGroupLayout; 2],
    kernels: std::collections::BTreeMap<(u32, usize, Entry), Deferred<wgpu::ComputePipeline>>,
}

impl Pipelines {
    /// `shared` holds the target, material source and texture layouts;
    /// `layouts` the write-only color and color-with-coverage outputs.
    pub fn new(device: &PipelineDevice, shared: [&wgpu::BindGroupLayout; 3], layouts: [wgpu::BindGroupLayout; 2]) -> Self {
        let pipeline_layouts: [_; 2] = std::array::from_fn(|coverage| device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("destination material pages"),
            bind_group_layouts: &[Some(&layouts[coverage]), Some(shared[0]), Some(shared[1]), Some(shared[2])],
            immediate_size: 0,
        }));
        let mut kernels = std::collections::BTreeMap::new();
        for (slot, operation) in (0..MATERIAL_SLOTS).flat_map(|slot| OPERATIONS.map(|operation| (slot, operation))) {
            if !blended(operation) && slot % MATERIAL_BLENDS.len() != 0 { continue; }
            let (flags, blend) = dry_material::material_entries(slot);
            let transport = dry_material::material_shader(device, Entries::Transport(blend), flags, operation);
            for &entry in entries(operation) {
                let shader = if entry == Entry::Gather { dry_material::material_shader(device, Entries::Gather, flags, operation) } else { transport.clone() };
                kernels.insert((operation as u32, slot, entry), Deferred::compute(device,
                    format!("destination material {} operation={} contact={flags:#x} blend={blend}", entry.name(), operation as u32),
                    &pipeline_layouts[usize::from(entry == Entry::Coverage)], &shader, entry.name()));
            }
        }
        Self { layouts, kernels }
    }

    pub fn kernel(&self, style: &layer_render::DabStyle, entry: Entry) -> &Deferred<wgpu::ComputePipeline> {
        let operation = BrushPassPlan::for_style(style).material;
        let slot = if blended(operation) { dry_material::material_slot(style) }
            else { dry_material::material_contact_slot(style) * MATERIAL_BLENDS.len() };
        &self.kernels[&(operation as u32, slot, entry)]
    }

    fn output(&self, r: &WgpuRasterizer, color: &PageSurface, coverage: Option<&PageSurface>) -> wgpu::BindGroup {
        dry_material::output_binding(r, &self.layouts[usize::from(coverage.is_some())], color, coverage, None)
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
    /// A job's distant samples accumulate chunk by chunk into a gather field
    /// that its kernel reads. Records hold each job's chunk page lists, then
    /// its dirty rectangle. A committed Wet batch then exchanges its reservoir
    /// with the pre-batch pages at its last dab; its record holds the starting
    /// reservoir of a new stroke.
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
        let gathers = entries(operation).contains(&Entry::Gather);
        let distant = jobs.iter().map(|job| gathers.then(|| material_sources::distant_pages(batch, dabs, job.coordinate, extent)).flatten())
            .collect::<Vec<_>>();
        let exchange = dabs.first().zip(dabs.last()).filter(|_| !preview && operation == MaterialOperation::Wet);
        self.metrics.material_cpu_ms[0] += elapsed(started);
        self.prepare_records(jobs.iter().zip(&distant).flat_map(|(job, pages)| {
            pages.iter().flat_map(|pages| pages.chunks(9)).enumerate()
                .map(|(chunk, pages)| record([1, pages.len() as u32, u32::from(chunk > 0), 0], pages.iter().map(|p| [p[0], p[1], 0, 0])))
                .chain(std::iter::once(record([if pages.is_some() { 2 } else { 0 }, 0, 0, 0], [rect_words(job.local)])))
        }).chain(exchange.map(|(first, _)| {
            let color = first.color_rgba_linear;
            record([0; 4], [[color[0], color[1], color[2], batch.style.wet_mix.amount_of_paint * first.material[2]].map(f32::to_bits)])
        })), encoder)?;
        let mut dispatches = Vec::with_capacity(jobs.len());
        let mut first_record = 0;
        for (job, pages) in jobs.iter().zip(&distant) {
            let (color_pages, coverage_pages) = if preview {
                (&self.preview_pages, &self.preview_coverage_pages)
            } else {
                (&self.paint_layers[layer_index].pages, &self.paint_layers[layer_index].coverage_pages)
            };
            let destination = color_pages[job.page_index].surface(job.destination_secondary);
            let coverage = job.coverage_index.zip(job.coverage_destination_secondary)
                .map(|(i, secondary)| if secondary { &coverage_pages[i].secondary } else { &coverage_pages[i].primary });
            destination.preview.set(None);
            let output = self.pipelines.destination_material.output(self, destination, coverage);
            let entry = if coverage.is_some() { Entry::Coverage } else { Entry::Color };
            self.add_transport_job(encoder, batch_index, batch, job.coordinate, pages.as_deref(), first_record, sources_preview, entry, output, &mut dispatches)?;
            first_record += pages.as_ref().map_or(0, |pages| pages.len().div_ceil(9)) + 1;
        }
        if let Some((_, last)) = exchange {
            let coordinate = std::array::from_fn(|i| ([last.center.x, last.center.y][i].max(0.) as u32 / PAGE_SIZE).min(extent[i].saturating_sub(1) / PAGE_SIZE));
            let output = self.pipelines.destination_material.output(self, self.reservoir.inactive(), None);
            self.add_transport_job(encoder, batch_index, batch, coordinate, None, first_record, false, Entry::Reservoir, output, &mut dispatches)?;
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
        distant: Option<&[[u32; 2]]>,
        mut record_index: usize,
        preview: bool,
        entry: Entry,
        output: wgpu::BindGroup,
        dispatches: &mut Vec<Dispatch>,
    ) -> Result<(), GpuRasterError> {
        let timing = self.telemetry.enabled;
        let mut field = None;
        if let Some(pages) = distant {
            self.ensure_material_gather();
            self.metrics.material_sample_jobs += 1;
            for (chunk, pages) in pages.chunks(9).enumerate() {
                let started = timing.then(web_time::Instant::now);
                self.metrics.material_sample_passes += 1;
                let offsets = std::array::from_fn::<_, 9, _>(|i| {
                    pages.get(i).map(|p| [p[0] as i32 - coordinate[0] as i32, p[1] as i32 - coordinate[1] as i32])
                });
                self.encode_before_borrowing(encoder, batch_index, batch, coordinate, offsets, preview, dispatches);
                self.prepare_raw_neighborhood(batch.target, coordinate, offsets, preview, encoder)?;
                self.metrics.material_cpu_ms[1] += elapsed(started);
                let started = timing.then(web_time::Instant::now);
                let layer = self.paint_layers.iter().find(|l| l.id == batch.target)
                    .ok_or(GpuRasterError::MissingPaintLayer(batch.target))?;
                let sources = self.source_tiles.borrow();
                let views = self.raw_layer_neighborhood(&sources, layer, coordinate, offsets, preview);
                let fields = &self.material_gather.as_ref().unwrap().fields;
                let accumulated = if chunk == 0 { &self.empty_view } else { &fields[1 - chunk % 2].view };
                dispatches.push(Dispatch {
                    entry: Entry::Gather,
                    output: self.pipelines.destination_material.output(self, &fields[chunk % 2], None),
                    sources: create_material_bind_group(&self.device, &self.material_layout, &views, &self.dab_buffer,
                        &self.empty_scalar_view, accumulated, wgpu::BindingResource::Buffer(self.material_records.binding())),
                    coordinate,
                    record_offset: self.material_records.offset(record_index),
                });
                drop(sources);
                record_index += 1;
                field = Some(fields[chunk % 2].view.clone());
                self.metrics.material_cpu_ms[2] += elapsed(started);
            }
        }
        let offsets = self.material_offsets(batch, field.is_some());
        self.encode_before_borrowing(encoder, batch_index, batch, coordinate, offsets, preview, dispatches);
        let started = timing.then(web_time::Instant::now);
        let inputs = field.as_ref().map_or(MaterialInputs::Neighborhood, MaterialInputs::Gathered);
        let (sources, _) = self.material_bind_group(batch, coordinate, preview, inputs, None, encoder)?;
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
        let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
            label: Some("destination material pages"),
            timestamp_writes: None,
        });
        pass.set_bind_group(3, textures, &[]);
        for dispatch in dispatches {
            pass.set_pipeline(self.pipelines.destination_material.kernel(&batch.style, dispatch.entry));
            pass.set_bind_group(0, &dispatch.output, &[style_offset]);
            pass.set_bind_group(1, self.paint_target_binding(&batch.style), &[self.layer_target_offset(batch.target, dispatch.coordinate)]);
            pass.set_bind_group(2, &dispatch.sources, &[dispatch.record_offset]);
            let size = if dispatch.entry == Entry::Reservoir { RESERVOIR_SIZE } else { PAGE_SIZE } / WORKGROUP;
            pass.dispatch_workgroups(size, size, 1);
        }
        drop(pass);
        self.metrics.material_cpu_ms[3] += elapsed(started);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use layer_core::DefaultBrushPreset as Preset;

    #[test]
    fn selecting_a_transported_brush_compiles_only_its_page_and_gather_kernels() {
        let reference = WgpuRasterizer::new_native_headless(Default::default()).unwrap();
        let document = layer_core::Document::new(layer_core::authored::PortableId::random(), 128, 128,
            layer_core::DocumentNames { paint: "Current ink".into(), paper: "Paper".into() });
        for (preset, expected) in [(Preset::LiquifyPush, 2), (Preset::Smudge, 2), (Preset::NaturalBlender, 3), (Preset::WetRound, 2),
            (Preset::OpaqueGouache, 3), (Preset::LoadedOil, 3), (Preset::PaletteKnife, 3)] {
            let mut renderer = crate::test_support::staged_renderer(&reference, Default::default());
            renderer.shader_idle(false, false);
            let mut brush = layer_core::default_brush(preset);
            renderer.prepare_startup(&document, &brush, false).unwrap();
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(60);
            crate::test_support::wait_startup(&mut renderer, deadline, |progress| progress.brush_ready, format_args!("{preset:?} preparation timed out"));
            let pipelines = &renderer.pipelines;
            let transport = &pipelines.destination_material;
            let ready = transport.kernels.values().filter(|p| p.ready()).count();
            assert_eq!(ready, expected, "{preset:?} compiles its commit, prediction and gather or reservoir kernels");
            assert!(pipelines.material.iter().flatten().all(|p| !p.ready()), "{preset:?} compiles no material fragment");
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
}
