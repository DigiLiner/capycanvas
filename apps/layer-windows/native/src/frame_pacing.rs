use std::{collections::VecDeque, time::Duration};

#[derive(Default)]
pub(crate) struct FramePacing {
    in_flight: VecDeque<wgpu::SubmissionIndex>,
}

impl FramePacing {
    const IN_FLIGHT: usize = 2;
    const WAIT: Duration = Duration::from_millis(100);

    pub(crate) fn ready(&mut self, device: &wgpu::Device) -> Result<bool, wgpu::PollError> {
        while self.saturated() {
            let oldest = self.in_flight[0].clone();
            match device.poll(wgpu::PollType::Wait { submission_index: Some(oldest), timeout: Some(Self::WAIT) }) {
                Ok(_) => { self.in_flight.pop_front(); }
                Err(wgpu::PollError::Timeout) => return Ok(false),
                Err(error) => return Err(error),
            }
        }
        Ok(true)
    }

    pub(crate) fn submitted(&mut self, queue: &wgpu::Queue) {
        self.in_flight.push_back(queue.submit([]));
    }

    fn saturated(&self) -> bool {
        self.in_flight.len() >= Self::IN_FLIGHT
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_third_frame_waits_until_the_gpu_finishes_an_earlier_one() {
        let gpu = layer_render_wgpu::WgpuRasterizer::new_native_headless(Default::default()).unwrap();
        let device = gpu.device();
        let size = 32 << 20;
        let buffer = |usage| device.create_buffer(&wgpu::BufferDescriptor { label: None, size, usage, mapped_at_creation: false });
        let (source, target) = (buffer(wgpu::BufferUsages::COPY_SRC), buffer(wgpu::BufferUsages::COPY_DST));
        let mut pacing = FramePacing::default();
        for _ in 0..2 {
            assert!(pacing.ready(device).unwrap());
            let mut encoder = device.create_command_encoder(&Default::default());
            for _ in 0..2 { encoder.copy_buffer_to_buffer(&source, 0, &target, 0, size); }
            gpu.queue().submit([encoder.finish()]);
            pacing.submitted(gpu.queue());
        }
        assert!(pacing.saturated(), "two unfinished frames hold back the next one until completion is observed");
        let start = std::time::Instant::now();
        let ready = pacing.ready(device).unwrap();
        assert!(ready || start.elapsed() >= FramePacing::WAIT / 2, "a busy GPU delays the next frame instead of skipping it");
        assert_eq!(pacing.saturated(), !ready);
        device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
        assert!(pacing.ready(device).unwrap(), "completed frames release their slots");
        assert_eq!(pacing.in_flight.len(), 1);
        drop(gpu);
        layer_render_wgpu::finish_shader_compiler_shutdown();
    }
}
