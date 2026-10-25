use crate::workspace_service::WorkspaceService;
use layer_host::handles::{Handle,LocalHandles};

type Service=WorkspaceService<layer_workspace::StoreWorker>;
thread_local! {static SERVICES:LocalHandles=Default::default();}
pub(crate) struct WorkspaceOwner(Handle);
impl WorkspaceOwner {
    pub(crate) fn new(service:Service)->Result<Self,String> {
        SERVICES.with(|services|services.insert(service)).map(Self).map_err(|error|error.to_string())
    }
    pub(crate) fn with<T>(&self,work:impl FnOnce(&mut Service)->Result<T,String>)->Result<T,String> {
        SERVICES.with(|services|services.with(self.0,work)).map_err(|error|error.to_string())?
    }
    pub(crate) fn stop(&mut self)->Result<(),String> {
        let mut service=SERVICES.with(|services|services.take::<Service>(self.0)).map_err(|error|error.to_string())?;
        service.stop();Ok(())
    }
}
impl Drop for WorkspaceOwner {
    fn drop(&mut self) {
        let _=SERVICES.try_with(|services| {
            if let Ok(mut service)=services.take::<Service>(self.0) {service.stop();}
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn workspace_state_rejects_handoff_and_stops_on_its_creator() {
        let directory=crate::test_support::TempDir::new();
        let store=layer_workspace::StoreWorker::shared(&directory.path).unwrap();
        let service=Service::new(store,directory.path.clone(),layer_ui::Localizer::shared(layer_ui::UiLanguage::English),||{});
        let owner=WorkspaceOwner::new(service).unwrap();
        assert!(owner.with(|_|Ok(())).is_ok());
        let mut owner=std::thread::spawn(move|| {
            assert!(owner.with(|_|Ok(())).err().unwrap().contains("another thread"));owner
        }).join().unwrap();
        assert!(owner.with(|_|Ok(())).is_ok());
        owner.stop().unwrap();assert!(owner.with(|_|Ok(())).is_err());
    }
}
