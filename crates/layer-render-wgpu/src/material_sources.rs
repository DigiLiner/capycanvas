//! Full-resolution destination-brush samples beyond the adjacent tile window.
//! Gather disjoint source pages into two reusable 256px Float32 fields. Each
//! dispatch binds at most nine original/paint tiles; no full-photo texture or
//! CPU readback is needed, and source-cache eviction remains queue ordered.
use super::*;
use layer_core::{Point, Rect};

/// Aligned material tile uniforms retain their allocation across batches.
#[derive(Default)]
pub(super) struct TileRecords {
    buffer: Option<wgpu::Buffer>,
    stride: u64,
}
impl TileRecords {
    pub fn prepare(&mut self, device: &wgpu::Device, count: usize) -> Result<u64, GpuRasterError> {
        self.stride = u64::from(device.limits().min_uniform_buffer_offset_alignment.max(256));
        let used = (count as u64).checked_mul(self.stride)
            .filter(|size| *size <= device.limits().max_buffer_size)
            .ok_or(GpuRasterError::SizeOverflow)?;
        if used > 0 && self.buffer.as_ref().is_none_or(|buffer| buffer.size() < used) {
            self.buffer = Some(device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("reusable material tile records"),
                size: used.next_power_of_two().min(device.limits().max_buffer_size),
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            }));
        }
        Ok(used)
    }

    pub fn storage_bytes(&self) -> u64 { self.buffer.as_ref().map_or(0, wgpu::Buffer::size) }

    pub fn offset(&self, index: usize) -> u32 { (index as u64 * self.stride) as u32 }

    pub fn binding(&self) -> wgpu::BufferBinding<'_> {
        wgpu::BufferBinding {
            buffer: self.buffer.as_ref().expect("dry tile records prepared before encoding"),
            offset: 0,
            size: NonZeroU64::new(160),
        }
    }
}

/// Two reusable full-resolution sample fields that gather chunks ping-pong.
pub(super) struct Gather {
    pub(super) fields: [PageSurface; 2],
}
impl Gather {
    pub(super) fn storage_bytes(&self) -> u64 {
        self.fields.iter().map(PageSurface::storage_bytes).sum()
    }
}

pub(super) const RECORD_WORDS: usize = 40;

/// A tile record: its header, then up to nine page words.
pub(super) fn record(header: [u32; 4], pages: impl IntoIterator<Item = [u32; 4]>) -> [u32; RECORD_WORDS] {
    let mut words = [0; RECORD_WORDS];
    words[..4].copy_from_slice(&header);
    for (destination, page) in words[4..].as_chunks_mut::<4>().0.iter_mut().zip(pages) {
        *destination = page;
    }
    words
}

pub(super) fn rect_words(rect: PixelRect) -> [u32; 4] {
    [rect.min_x(), rect.min_y(), rect.max_x(), rect.max_y()]
}

/// A pointwise job's dab range and dirty rectangle, then a retouching job's
/// source mapping.
pub(super) fn dry_record(local: PixelRect, dabs: std::ops::Range<u32>, mapping: [u32; 16]) -> [u32; RECORD_WORDS] {
    record([0, 0, dabs.start, dabs.len() as u32], std::iter::once(rect_words(local)).chain(mapping.as_chunks::<4>().0.iter().copied()))
}

/// The pages a Liquify or Smudge job on page `at` samples, when they reach
/// beyond its neighborhood.
pub(super) fn distant_pages(batch: &DabBatch, dabs: &[Dab], at: [u32; 2], extent: [u32; 2]) -> Option<Vec<[u32; 2]>> {
    let pages = page_coordinates(sample_bounds(batch, dabs, at, extent)).collect::<Vec<_>>();
    pages.iter().any(|p| p[0].abs_diff(at[0]) > 1 || p[1].abs_diff(at[1]) > 1).then_some(pages)
}

/// Conservative bounds of every backtrace and blur tap from this output page.
/// Coverage lies in [0,1], so each step includes both zero and full displacement.
/// The shader still computes the exact nonlinear coordinate for every pixel.
fn sample_bounds(batch: &DabBatch, dabs: &[Dab], at: [u32; 2], extent: [u32; 2]) -> PixelRect {
    let mut bounds = Rect {
        min: Point {
            x: (at[0] * PAGE_SIZE) as f32,
            y: (at[1] * PAGE_SIZE) as f32,
        },
        max: Point {
            x: ((at[0] + 1) * PAGE_SIZE) as f32,
            y: ((at[1] + 1) * PAGE_SIZE) as f32,
        },
    };
    for dab in dabs.iter().rev() {
        let shift = if batch.style.execution == BrushExecution::Smudge {
            let pull = dab.material[1].clamp(0., 1.);
            [-dab.motion[0] * pull, -dab.motion[1] * pull]
        } else {
            let amount = dab.material[3].clamp(0., 1.);
            let motion = dab.motion.map(|v| v * (1. + batch.style.deform.momentum));
            match batch.style.deform.mode {
                LiquifyMode::Push => [-motion[0] * amount, -motion[1] * amount],
                LiquifyMode::TwirlClockwise | LiquifyMode::TwirlCounterClockwise => {
                    let direction = if batch.style.deform.mode == LiquifyMode::TwirlClockwise {
                        1.
                    } else {
                        -1.
                    };
                    bounds = swept_rotation(bounds, dab.center, direction * amount * 0.75);
                    [0.; 2]
                }
                LiquifyMode::Pinch | LiquifyMode::Expand => {
                    let direction = if batch.style.deform.mode == LiquifyMode::Pinch {
                        1.
                    } else {
                        -1.
                    };
                    let scale = 1. + direction * amount * 0.35;
                    let scaled = Rect {
                        min: Point {
                            x: dab.center.x + (bounds.min.x - dab.center.x) * scale,
                            y: dab.center.y + (bounds.min.y - dab.center.y) * scale,
                        },
                        max: Point {
                            x: dab.center.x + (bounds.max.x - dab.center.x) * scale,
                            y: dab.center.y + (bounds.max.y - dab.center.y) * scale,
                        },
                    };
                    bounds = bounds.union(scaled);
                    [0.; 2]
                }
                LiquifyMode::Crystals => {
                    bounds = expand(bounds, 12. * amount * batch.style.deform.distortion.abs());
                    [0.; 2]
                }
                LiquifyMode::Edge | LiquifyMode::Reconstruct => {
                    bounds = expand(bounds, motion[0].hypot(motion[1]) * amount);
                    [0.; 2]
                }
            }
        };
        bounds.min.x += shift[0].min(0.);
        bounds.max.x += shift[0].max(0.);
        bounds.min.y += shift[1].min(0.);
        bounds.max.y += shift[1].max(0.);
    }
    if batch.style.execution == BrushExecution::Smudge {
        bounds = expand(bounds, batch.style.wet_mix.blur.clamp(0., 1.) * 4.);
    }
    let local = expand(bounds, 1.); // Manual bilinear neighbors.
    if [local.min.x, local.min.y, local.max.x, local.max.y]
        .into_iter()
        .any(|v| !v.is_finite())
    {
        PixelRect::full(extent)
    } else {
        pixel_rect(local, extent)
    }
}
fn expand(r: Rect, amount: f32) -> Rect {
    Rect {
        min: Point {
            x: r.min.x - amount,
            y: r.min.y - amount,
        },
        max: Point {
            x: r.max.x + amount,
            y: r.max.y + amount,
        },
    }
}

/// Each pixel rotates by an angle between zero and the contact's maximum.
/// At a fixed angle, a rectangle's extrema occur at its corners. Over this
/// partial turn, each corner reaches extrema at endpoints or cardinal angles.
/// Keeping that swept rectangle avoids fetching a full circle for a small twist.
fn swept_rotation(bounds: Rect, center: Point, angle: f32) -> Rect {
    if angle == 0. {
        return bounds;
    }
    use std::f64::consts::{FRAC_PI_2, PI, TAU};
    let angle = f64::from(angle);
    let (lo, hi) = (angle.min(0.), angle.max(0.));
    let mut result = bounds;
    let mut include = |x: f64, y: f64| {
        let x = (f64::from(center.x) + x) as f32;
        let y = (f64::from(center.y) + y) as f32;
        result.min.x = result.min.x.min(x);
        result.max.x = result.max.x.max(x);
        result.min.y = result.min.y.min(y);
        result.max.y = result.max.y.max(y);
    };
    let (sin, cos) = angle.sin_cos();
    for x in [bounds.min.x, bounds.max.x] {
        for y in [bounds.min.y, bounds.max.y] {
            let (x, y) = (
                f64::from(x) - f64::from(center.x),
                f64::from(y) - f64::from(center.y),
            );
            include(x * cos - y * sin, x * sin + y * cos);
            let direction = y.atan2(x);
            let radius = x.hypot(y);
            for (i, (x, y)) in [(radius, 0.), (0., radius), (-radius, 0.), (0., -radius)]
                .into_iter()
                .enumerate()
            {
                let turn = (i as f64 * FRAC_PI_2 - direction + PI).rem_euclid(TAU) - PI;
                if turn >= lo && turn <= hi {
                    include(x, y);
                }
            }
        }
    }
    result
}

impl WgpuRasterizer {
    /// Upload one batch's tile records into pooled mapped storage, with no
    /// intermediate byte vector or second CPU copy on the drawing path.
    pub(super) fn prepare_records(
        &mut self,
        records: impl Iterator<Item = [u32; RECORD_WORDS]> + Clone,
        encoder: &mut crate::submission::CommandEncoder,
    ) -> Result<(), GpuRasterError> {
        let records_buffer = &mut self.material_records;
        let used = records_buffer.prepare(&self.device, records.clone().count())?;
        if used == 0 { return Ok(()); }
        self.uploads.write_mapped(encoder, records_buffer.buffer.as_ref().unwrap(), 0, used, |mapped| {
            mapped.slice(..).fill(0);
            for (index, words) in records.enumerate() {
                let offset = index * records_buffer.stride as usize;
                for (i, word) in words.into_iter().enumerate() {
                    mapped.slice(offset + i * 4..offset + i * 4 + 4).copy_from_slice(&word.to_le_bytes());
                }
            }
        })
    }

    pub(super) fn material_source_binding(
        &mut self,
        batch: &DabBatch,
        coordinate: [u32; 2],
        preview: bool,
        display: Option<usize>,
        encoder: &mut crate::submission::CommandEncoder,
    ) -> Result<(wgpu::BindGroup, Option<wgpu::BindGroup>), GpuRasterError> {
        let started = self.telemetry.enabled.then(web_time::Instant::now);
        let result = self.material_bind_group(batch, coordinate, preview, MaterialInputs::Neighborhood, display, encoder);
        self.metrics.material_cpu_ms[4] += elapsed(started);
        result
    }

    pub(super) fn ensure_material_gather(&mut self) {
        if self.material_gather.is_none() {
            let gather = Gather { fields: std::array::from_fn(|_| self.create_page_surface("full-resolution material sample field")) };
            self.metrics.material_sample_storage_bytes = gather.storage_bytes();
            self.material_gather = Some(gather);
        }
    }

    /// Bind what a retouching batch copies onto page `coordinate`: the pages
    /// `mapping` reads, around the destination.
    pub(super) fn clone_source_binding(
        &mut self,
        batch: &DabBatch,
        coordinate: [u32; 2],
        mapping: &crate::retouch_sources::Mapping,
        preview: bool,
        display: Option<usize>,
        encoder: &mut crate::submission::CommandEncoder,
    ) -> Result<(wgpu::BindGroup, Option<wgpu::BindGroup>), GpuRasterError> {
        let pages = self.retouch_pages(mapping, encoder)?;
        self.material_bind_group(batch, coordinate, preview, MaterialInputs::Retouch(&pages), display, encoder)
    }
}

pub(super) fn elapsed(started: Option<web_time::Instant>) -> f64 {
    started.map_or(0., |at| at.elapsed().as_secs_f64() * 1000.)
}
