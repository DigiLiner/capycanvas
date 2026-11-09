use super::*;

#[cfg(not(target_arch = "wasm32"))]
impl SnapshotGpu {
    async fn run_snapshot(&self, request: layer_render::SnapshotRequest, control: CaptureControl)
        -> Result<layer_render::SnapshotResult, String> {
        match request {
            layer_render::SnapshotRequest::LevelsStatistics(query)=>self.levels_statistics(query,control).await.map(layer_render::SnapshotResult::LevelsStatistics),
            layer_render::SnapshotRequest::ArtworkStatistics(request) => self.artwork_statistics(request, control).await.map(layer_render::SnapshotResult::ArtworkStatistics),
            layer_render::SnapshotRequest::ArtworkSample(request) => self.artwork_sample(request, control).await.map(layer_render::SnapshotResult::ArtworkSample),
            layer_render::SnapshotRequest::Bounds(request) => self.content_bounds(request, control).await.map(layer_render::SnapshotResult::Bounds),
            layer_render::SnapshotRequest::TransformPixels(plan) => self.transform_pixels(plan, control).await
                .map(layer_render::SnapshotResult::TransformPixels),
            layer_render::SnapshotRequest::Image(capture) => self.image_capture(capture, control).await
                .map(layer_render::SnapshotResult::Image),
            layer_render::SnapshotRequest::Remap(plan) => self.remap(plan, control).await
                .map(layer_render::SnapshotResult::Remap),
        }
    }
}

pub struct Job<T> {
    control: CaptureControl,
    receiver: mpsc::Receiver<Result<T, String>>,
    #[cfg(not(target_arch = "wasm32"))]
    worker: Option<std::thread::JoinHandle<()>>,
}
pub type SnapshotJob = Job<layer_render::SnapshotResult>;
impl<T> Drop for Job<T> {
    fn drop(&mut self) {
        self.control.cancel();
        #[cfg(not(target_arch = "wasm32"))]
        crate::compile::retire(self.worker.take());
    }
}
#[cfg(target_arch = "wasm32")]
pub type BrowserSnapshot = std::rc::Rc<dyn Fn(layer_render::SnapshotRequest, CaptureControl)
    -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<layer_render::SnapshotResult, String>>>>>;

impl<T> Job<T> {
    #[cfg(not(target_arch = "wasm32"))]
    pub(crate) fn spawn(name: &str, work: impl FnOnce(CaptureControl) -> Result<T, String> + Send + 'static) -> Result<Self, String>
    where T: Send + 'static {
        let control = CaptureControl::default();
        let capture = control.clone();
        let (sender, receiver) = mpsc::channel();
        let worker = std::thread::Builder::new().name(name.into()).stack_size(8 * 1024 * 1024).spawn(move || {
            let _ = sender.send(work(capture));
        }).map_err(|e| e.to_string())?;
        Ok(Self { control, receiver, worker: Some(worker) })
    }
    #[cfg(target_arch = "wasm32")]
    pub(crate) fn spawn_local<F: std::future::Future<Output = Result<T, String>> + 'static>(work: impl FnOnce(CaptureControl) -> F + 'static) -> Self
    where T: 'static {
        let control = CaptureControl::default();
        let capture = control.clone();
        let (sender, receiver) = mpsc::channel();
        wasm_bindgen_futures::spawn_local(async move {
            let _ = sender.send(work(capture).await);
        });
        Self { control, receiver }
    }
    pub fn take(&mut self) -> Option<Result<T, String>> {
        match self.receiver.try_recv() {
            Ok(result) => Some(result),
            Err(mpsc::TryRecvError::Empty) => None,
            Err(mpsc::TryRecvError::Disconnected) => Some(Err("The operation stopped".into())),
        }
    }
}
impl SnapshotJob {
    #[cfg(not(target_arch = "wasm32"))]
    pub fn start(gpu: SnapshotGpu, request: layer_render::SnapshotRequest) -> Result<Self, String> {
        Self::spawn("capy-snapshot", move |control| pollster::block_on(gpu.run_snapshot(request, control)))
    }
    #[cfg(target_arch = "wasm32")]
    pub fn start(worker: BrowserSnapshot, request: layer_render::SnapshotRequest) -> Result<Self, String> {
        Ok(Self::spawn_local(move |control| worker(request, control)))
    }
}
