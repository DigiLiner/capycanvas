use layer_render::{FrameRate, FrameRateSource, PresentationRate};
use std::time::Instant;

pub struct CanvasFrameRate {
    epoch: Instant,
    submitted: PresentationRate,
    presented: PresentationRate,
}
impl Default for CanvasFrameRate {
    fn default() -> Self {
        Self { epoch: Instant::now(), submitted: PresentationRate::default(), presented: PresentationRate::default() }
    }
}
impl CanvasFrameRate {
    fn now(&self) -> u64 {
        self.epoch.elapsed().as_nanos().min(u128::from(u64::MAX - 1)) as u64 + 1
    }
    pub fn submitted(&self) { self.submitted.observe(self.now()); }
    pub fn presented(&self, presented_ns: u64, observed_ns: u64) {
        if presented_ns == 0 || observed_ns < presented_ns { return; }
        if let Some(time) = self.now().checked_sub(observed_ns - presented_ns) {
            self.presented.observe(time);
        }
    }
    pub fn sample(&self) -> Option<FrameRate> { self.sample_at(self.now()) }
    fn sample_at(&self, now: u64) -> Option<FrameRate> {
        self.presented.fps(now).map(|fps| FrameRate { fps, source: FrameRateSource::Presented })
            .or_else(|| self.submitted.fps(now).map(|fps| FrameRate { fps, source: FrameRateSource::Submitted }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn actual_feedback_precedes_submissions_even_after_it_expires() {
        let rate = CanvasFrameRate::default();
        assert_eq!(rate.sample_at(1_000_000_000), None);
        rate.submitted.observe(2_500_000_000);
        assert_eq!(rate.sample_at(3_000_000_000), Some(FrameRate { fps: 1., source: FrameRateSource::Submitted }));
        rate.presented.observe(1_000_000_000);
        assert_eq!(rate.sample_at(3_000_000_000), Some(FrameRate { fps: 0., source: FrameRateSource::Presented }));
    }

    #[test]
    fn invalid_feedback_and_retired_observers_do_not_leak_into_new_surfaces() {
        let previous = std::sync::Arc::new(CanvasFrameRate::default());
        let current = CanvasFrameRate::default();
        current.presented(0, 1);
        current.presented(2, 1);
        previous.submitted();
        previous.presented(1, 1);
        assert!(previous.sample().is_some());
        assert_eq!(current.sample(), None);
    }
}
