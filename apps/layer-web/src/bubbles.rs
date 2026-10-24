use super::*;
use layer_ui::BubbleGrab;
#[wasm_bindgen]
impl WebApp {
    pub fn bubble_count_badge(&self, width: f32, height: f32) -> Result<JsValue, JsValue> {
        serialize(&layer_ui::count_badge([width, height]))
    }

    pub fn bubble_chooser_tag(&self, x: f32, y: f32, width: f32) -> Result<JsValue, JsValue> {
        serialize(&layer_ui::chooser_tag([x, y], width))
    }

    pub fn bubbles(&mut self) -> Result<JsValue, JsValue> {
        let packets = self.session.bubble_presentations();
        self.bubble_views = packets.iter().map(|packet| packet.view.clone()).collect();
        serialize(&packets)
    }

    pub fn bubble_hit(&self, id: u32, x: f32, y: f32) -> Result<JsValue, JsValue> {
        let hit = self.bubble_views.iter().find(|view| view.id == id).and_then(|view| view.hit([x, y]));
        js_sys::JSON::parse(&serde_json::to_string(&hit).map_err(js)?)
    }

    pub fn bubble_grab(&self, request: JsValue) -> Result<JsValue, JsValue> {
        #[derive(Deserialize)]
        #[serde(tag = "type", rename_all = "snake_case")]
        enum Request {
            Grab { grab: BubbleGrab, held: bool },
            Choice { action: layer_ui::UiAction },
        }
        let request = serde_wasm_bindgen::from_value(request).map_err(js)?;
        let grab = match request {
            Request::Choice { action } => BubbleGrab::for_choice(&action),
            Request::Grab { grab, held } => self.session.bubble_grab(grab, held),
        };
        serialize(&grab)
    }

    pub fn can_take_bubble(&mut self, grab: JsValue, x: f32, y: f32, width: f32, height: f32) -> Result<bool, JsValue> {
        let grab = serde_wasm_bindgen::from_value(grab).map_err(js)?;
        Ok(self.session.can_take_bubble(grab, [x, y], [width, height]))
    }

    pub fn bubble_motion(&mut self, now_ms: f64, reduced: bool) -> Result<JsValue, JsValue> {
        self.session.reduce_bubble_motion(reduced);
        let moving = self.session.step_bubbles((now_ms.max(0.) * 1_000_000.) as u64);
        serialize(&layer_ui::BubbleMotion {moving, placements: self.session.bubble_placements(), fade: self.session.bubble_fade()})
    }

    pub fn bubble_color_drag_out(&self, size: f32, x: f32, y: f32) -> bool {
        layer_ui::color_drag_out(size, [x, y])
    }

    pub fn bubble_pinched_size(&self, size: f32, scale: f32) -> f32 {
        layer_ui::pinched_size(size, scale)
    }

    pub fn bubble_stamp(&mut self, known_request: Option<u64>) -> Result<JsValue, JsValue> {
        serialize(&self.session.toolbar_stamp(self.session.state().toolbar_context(), known_request).map_err(js)?)
    }

    pub fn bubble_wheel_color(&self) -> Result<JsValue, JsValue> {
        serialize(&self.session.state().colors.definition())
    }
}
