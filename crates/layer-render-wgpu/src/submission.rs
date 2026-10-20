//! Bound the native command buffers materialized by a large document replay.
//! wgpu can expand each render pass into multiple Metal command buffers at
//! finish. Keep chunks unfinalized until all uploads are closed, then finish
//! and submit each chunk in order before materializing the next one.
use std::ops::{Deref, DerefMut};

/// A cached GPU value is usable in queue order while its producing commands
/// are pending. Discarding those commands must invalidate the CPU cache key.
pub(crate) struct CacheWrite {
    valid: std::sync::Arc<std::sync::atomic::AtomicBool>,
    tracked: std::sync::atomic::AtomicBool,
}
impl CacheWrite {
    pub fn new() -> Self {
        Self::shared(std::sync::Arc::new(std::sync::atomic::AtomicBool::new(true)))
    }
    pub fn shared(valid: std::sync::Arc<std::sync::atomic::AtomicBool>) -> Self {
        Self {
            valid,
            tracked: std::sync::atomic::AtomicBool::new(false),
        }
    }
    pub fn validity(&self) -> std::sync::Arc<std::sync::atomic::AtomicBool> {
        self.valid.clone()
    }
    pub fn track(&self, encoder: &CommandEncoder) {
        use std::sync::atomic::Ordering;
        assert!(!self.tracked.swap(true, Ordering::Relaxed));
        let guard = DiscardedCacheWrite(Some(self.valid.clone()));
        encoder.on_submitted_work_done(move || guard.complete());
    }
}
impl Drop for CacheWrite {
    fn drop(&mut self) {
        use std::sync::atomic::Ordering;
        if !self.tracked.load(Ordering::Relaxed) {
            self.valid.store(false, Ordering::Release);
        }
    }
}
struct DiscardedCacheWrite(Option<std::sync::Arc<std::sync::atomic::AtomicBool>>);
impl DiscardedCacheWrite {
    fn complete(mut self) {
        self.0 = None;
    }
}
impl Drop for DiscardedCacheWrite {
    fn drop(&mut self) {
        if let Some(valid) = &self.0 {
            valid.store(false, std::sync::atomic::Ordering::Release);
        }
    }
}

pub(crate) struct CommandEncoder {
    compute: Option<wgpu::ComputePass<'static>>,
    device: wgpu::Device,
    prologue: Option<wgpu::CommandEncoder>,
    current: wgpu::CommandEncoder,
    earlier: Vec<wgpu::CommandEncoder>,
    passes: usize,
    total_passes: u64,
}

impl CommandEncoder {
    /// wgpu records each pass and the barriers before it into about two
    /// command buffers, and Android destroys a command pool that held more
    /// than 128 once its work completes (`vendor/README.md`), so chunks this
    /// size keep reusing their pools.
    pub(crate) const PASSES_PER_SUBMISSION: usize = 56;

    pub fn new(device: &wgpu::Device, descriptor: &wgpu::CommandEncoderDescriptor<'_>) -> Self {
        Self {
            compute: None,
            device: device.clone(),
            prologue: None,
            current: device.create_command_encoder(descriptor),
            earlier: Vec::new(),
            passes: 0,
            total_passes: 0,
        }
    }
    fn begin_pass(&mut self) {
        self.reserve_passes(1);
    }
    /// Account for a bounded helper that records raw wgpu passes. Rotate before
    /// the helper so its complete batch fits the same command-buffer ceiling.
    pub fn reserve_passes(&mut self, count: usize) {
        assert!(count <= Self::PASSES_PER_SUBMISSION);
        self.compute = None;
        if self.passes + count > Self::PASSES_PER_SUBMISSION {
            let next = self
                .device
                .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                    label: Some("continued document submission"),
                });
            self.earlier
                .push(std::mem::replace(&mut self.current, next));
            self.passes = 0;
        }
        self.passes += count;
        self.total_passes += count as u64;
    }
    pub fn pass_count(&self) -> u64 { self.total_passes }
    pub fn can_fit_passes(&self, count: usize) -> bool {
        count <= Self::PASSES_PER_SUBMISSION - self.passes
    }
    pub fn begin_render_pass<'a>(
        &'a mut self,
        descriptor: &wgpu::RenderPassDescriptor<'_>,
    ) -> wgpu::RenderPass<'a> {
        self.begin_pass();
        self.current.begin_render_pass(descriptor)
    }
    pub fn begin_compute_pass<'a>(
        &'a mut self,
        descriptor: &wgpu::ComputePassDescriptor<'_>,
    ) -> wgpu::ComputePass<'a> {
        self.begin_pass();
        self.current.begin_compute_pass(descriptor)
    }
    /// A compute pass that consecutive dispatches share until any other pass,
    /// copy or command ends it.
    pub fn compute_pass(&mut self) -> &mut wgpu::ComputePass<'static> {
        if self.compute.is_none() {
            self.begin_pass();
            self.compute = Some(self.current.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("shared compute"),
                timestamp_writes: None,
            }).forget_lifetime());
        }
        self.compute.as_mut().unwrap()
    }
    /// Commands that run before every other command of this encoder, such as
    /// uploads to storage that nothing earlier in it reads.
    pub fn prologue(&mut self) -> &mut wgpu::CommandEncoder {
        let device = &self.device;
        self.prologue.get_or_insert_with(|| device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("submission prologue"),
        }))
    }
    /// Finish staging uploads before calling this. Their completion callbacks
    /// belong to the final encoder, after every earlier chunk on this queue.
    pub fn submit(mut self, queue: &wgpu::Queue) -> wgpu::SubmissionIndex {
        self.compute = None;
        let mut chunks = self.earlier.into_iter().chain([self.current]);
        let mut index = submit_traced(queue, self.prologue.take().into_iter().chain(chunks.next()));
        for encoder in chunks {
            index = submit_traced(queue, [encoder]);
        }
        index
    }
}

// Copies, upload callbacks and query commands do not start render/compute
// passes. Forward them to the current chunk without changing their ordering;
// recording one ends the shared compute pass.
impl Deref for CommandEncoder {
    type Target = wgpu::CommandEncoder;
    fn deref(&self) -> &Self::Target {
        &self.current
    }
}
impl DerefMut for CommandEncoder {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.compute = None;
        &mut self.current
    }
}

fn submit_traced(queue: &wgpu::Queue, encoders: impl IntoIterator<Item = wgpu::CommandEncoder>) -> wgpu::SubmissionIndex {
    let mut trace = crate::performance_trace::Span::new(c"capy.command_finish");
    let commands = encoders.into_iter().map(wgpu::CommandEncoder::finish).collect::<Vec<_>>();
    trace.next(c"capy.queue_submit");
    queue.submit(commands)
}

pub(crate) trait ColorPass {
    fn color_pass(&mut self, label: &str, view: &wgpu::TextureView, load: wgpu::LoadOp<wgpu::Color>) -> wgpu::RenderPass<'_>;
}
impl ColorPass for wgpu::CommandEncoder {
    fn color_pass(&mut self, label: &str, view: &wgpu::TextureView, load: wgpu::LoadOp<wgpu::Color>) -> wgpu::RenderPass<'_> {
        self.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some(label),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view,
                resolve_target: None,
                depth_slice: None,
                ops: wgpu::Operations { load, store: wgpu::StoreOp::Store },
            })],
            ..Default::default()
        })
    }
}
impl ColorPass for CommandEncoder {
    fn color_pass(&mut self, label: &str, view: &wgpu::TextureView, load: wgpu::LoadOp<wgpu::Color>) -> wgpu::RenderPass<'_> {
        self.begin_pass();
        self.current.color_pass(label, view, load)
    }
}
