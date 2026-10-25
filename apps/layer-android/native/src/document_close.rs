use layer_host::{NativeHost, Renderer, window::{Activation, DocumentWindow, PreparedClose}};
use layer_ui::UiSession;

pub(crate) struct CloseJob(pub Option<PreparedClose>);
impl CloseJob {
    pub fn commit(&mut self, window: &mut DocumentWindow<Box<UiSession<Renderer>>>, host: &mut NativeHost) -> Result<Activation, String> {
        window.validate_close(host, self.0.as_ref().ok_or("Drawing close task was already consumed")?)?;
        window.commit_close(host, self.0.take().unwrap(), |_| {}).map(|(activation, _)| activation)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use layer_ui::{CommandId, Platform, UiAction};

    fn prepared() -> (NativeHost, DocumentWindow<Box<UiSession<Renderer>>>, Box<CloseJob>) {
        let mut host = NativeHost::new(Platform::Android).unwrap();
        host.session.suspend_renderer().unwrap();
        let mut window = DocumentWindow::default();
        host.dispatch(UiAction::Invoke { command: CommandId::CloseDocument }).unwrap();
        let job = Box::new(CloseJob(Some(window.prepare_close(&mut host, Default::default()).unwrap())));
        (host, window, job)
    }

    #[test]
    fn rejected_commit_preserves_the_job_for_cancellation() {
        let (mut host, mut window, mut job) = prepared();
        host.session.replace_renderer(Renderer::default()).unwrap();
        assert!(job.commit(&mut window, &mut host).is_err());
        window.cancel_close(&mut host, job.0.take().unwrap()).unwrap();
        assert!(!host.document_close_prepared());
    }

    #[test]
    fn committed_payload_leaves_its_allocation_owned_until_release() {
        let (mut host, mut window, mut job) = prepared();
        let address = &*job as *const CloseJob;
        let activation = job.commit(&mut window, &mut host).unwrap();
        assert!(job.0.is_none());
        assert!(job.commit(&mut window,&mut host).err().unwrap().contains("already consumed"));
        assert_eq!(&*job as *const CloseJob, address);
        drop(job);
        drop(activation);
    }
}
