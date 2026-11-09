use crate::app::App;
use layer_workspace::WorkspaceInput;
use serde_json::{Value, json};

fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}
impl App {
    pub fn workspace(&mut self, request: Value) -> Result<Value, String> {
        let time = now();
        if request["type"] == "start" && self.workspaces.is_none() {
                let path = request["directory"]
                    .as_str()
                    .ok_or("Workspace storage directory is missing")?;
                let wake = self.workspace_wake.clone().ok_or("Workspace storage has no owner to wake")?;
                self.workspaces = Some(crate::launch::start_workspaces(&mut self.host, std::path::Path::new(path), time, wake)?);
        }
        let Some(c) = &mut self.workspaces else {
            return Ok(Value::Null);
        };
        let changes = self.host.take_service_changes();
        c.observe_regions(&mut self.host.session, changes, time);
        if !matches!(request["type"].as_str(), Some("start" | "tick" | "capture")) {
            let input: WorkspaceInput =
                serde_json::from_value(request.clone()).map_err(|e| e.to_string())?;
            let before = self.host.session.state().revision;
            match c.input(&mut self.host.session, input, time) {
                Ok(change) => self.host.apply_change(before, change),
                Err(error) => c.set_error(error),
            }
        }
        if request["type"] == "capture" {
            return serde_json::to_value(self.host.session.capture_workspace()?)
                .map_err(|e| e.to_string());
        }
        let before = self.host.session.state().revision;
        let change = c.tick(&mut self.host.session, time);
        self.host.apply_change(before, change);
        Ok(json!({"view": c.view, "wake": change.needs_present(), "refresh": change.regions != 0}))
    }
}
