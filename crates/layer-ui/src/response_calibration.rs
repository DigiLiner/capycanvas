use crate::{Bounds, ContactPhase, CurveEditorAction, CurveDomain, CurveAxisView, CurveControls,
    CurveEditorView, Localizer, MessageId, PressureCalibrationView};
use layer_core::{PressureResponse, TiltResponse};
use layer_engine::{PressureCurve, TiltCurve};

pub(crate) trait EditableResponse: Clone + PartialEq + Default {
    type Compiled: Clone + Default;
    fn points(&self) -> Vec<[f32;2]>;
    fn set_point(&mut self, index:usize, point:[f32;2]) -> bool;
    fn insert(&mut self, point:[f32;2]) -> Option<usize>;
    fn remove(&mut self, index:usize) -> bool;
    fn compile(&self) -> Self::Compiled;
    fn map(compiled:&Self::Compiled,input:f32)->f32;
    fn sensitivity_enabled(&self,more:bool)->bool;
    fn sensitivity(&mut self,more:bool)->usize;
}
impl EditableResponse for PressureResponse {
    type Compiled=PressureCurve;
    fn points(&self)->Vec<[f32;2]> { self.points().to_vec() }
    fn set_point(&mut self,index:usize,point:[f32;2])->bool { self.set_point(index,point) }
    fn insert(&mut self,point:[f32;2])->Option<usize> { self.insert(point) }
    fn remove(&mut self,index:usize)->bool { self.remove(index) }
    fn compile(&self)->PressureCurve { self.clone().into() }
    fn map(compiled:&PressureCurve,input:f32)->f32 { compiled.map(input) }
    fn sensitivity_enabled(&self,more:bool)->bool { if more { self.points()[0][1]<self.points()[1][1] } else { self.points()[0][1]>0. } }
    fn sensitivity(&mut self,more:bool)->usize {
        let mut first=self.points()[0];first[1]+=if more {0.025}else{-0.025};self.set_point(0,first);0
    }
}
impl EditableResponse for TiltResponse {
    type Compiled=TiltCurve;
    fn points(&self)->Vec<[f32;2]> { self.points() }
    fn set_point(&mut self,index:usize,point:[f32;2])->bool { self.set_point(index,point) }
    fn insert(&mut self,point:[f32;2])->Option<usize> { self.insert(point) }
    fn remove(&mut self,index:usize)->bool { self.remove(index) }
    fn compile(&self)->TiltCurve { TiltCurve::new(&layer_core::TiltCalibration{response:self.clone(),axes:Default::default()},Default::default()) }
    fn map(compiled:&TiltCurve,input:f32)->f32 { compiled.response(input) }
    fn sensitivity_enabled(&self,more:bool)->bool { self.sensitivity_enabled(more) }
    fn sensitivity(&mut self,more:bool)->usize { self.sensitivity(more);self.points().len()-1 }
}

pub(super) struct PointDrag<R> { baseline: R, index: Option<usize>, press: [f32; 2], extent: [f32; 2] }

pub(crate) struct Calibration<R: EditableResponse> {
    pub(super) draft: Option<R>,
    pub(super) compiled: R::Compiled,
    pub(super) selected: Option<usize>,
    pub(super) epoch: u64,
    pub(super) point_drag: Option<PointDrag<R>>,
    pub(super) placement: Option<Bounds>,
    pub(super) frame_drag: Option<(Bounds, [f32; 2])>,
}

impl<R: EditableResponse> Default for Calibration<R> {
    fn default() -> Self { Self { draft:None, compiled:Default::default(), selected:None, epoch:0, point_drag:None, placement:None, frame_drag:None } }
}

impl<R: EditableResponse> Calibration<R> {
    pub fn sensitivity(&mut self,more:bool) {
        if let Some(draft)=&mut self.draft { self.selected=Some(draft.sensitivity(more));self.compile(); }
    }
    pub fn drag(&mut self,phase:ContactPhase,position:[f32;2],viewport:[f32;2]) {
        if position.iter().chain(&viewport).any(|v|!v.is_finite()) { return; }
        match phase {
            ContactPhase::Down=>self.frame_drag=self.placement.map(|b|(b,position)),
            ContactPhase::Move|ContactPhase::Up=>if let Some((mut b,press))=self.frame_drag {
                b.x+=position[0]-press[0];b.y+=position[1]-press[1];self.placement=Some(b);
                if phase==ContactPhase::Up { self.fit([b.width,b.height],viewport);self.frame_drag=None; }
            },
            ContactPhase::Cancel=>if let Some((b,_))=self.frame_drag.take() {self.placement=Some(b);},
        }
    }
    pub fn view_as(&self,l:&Localizer,title:MessageId,less:MessageId,more:MessageId,domain:CurveDomain)->Option<PressureCalibrationView> {
        let draft=self.source()?;let points=draft.points();
        let axis=|id|CurveAxisView {label:l.text(id).to_string(),minimum:domain.axis_text(0.),maximum:domain.axis_text(1.),white:None};
        Some(PressureCalibrationView {bounds:self.placement.unwrap_or(Bounds{x:500.,y:64.,width:336.,height:360.}),
            title:l.text(title).to_string(),close:l.text(MessageId::COMMON_CLOSE).to_string(),firmer:l.text(less).to_string(),lighter:l.text(more).to_string(),
            reset:l.text(MessageId::COMMON_RESET).to_string(),cancel:l.text(MessageId::COMMON_CANCEL).to_string(),apply:l.text(MessageId::COMMON_APPLY).to_string(),
            firmer_enabled:draft.sensitivity_enabled(false),lighter_enabled:draft.sensitivity_enabled(true),
            editor:CurveEditorView {controls:CurveControls {control_polygon:true,coordinate_readouts:false,inset:8.,epoch:self.epoch,numeric:domain.numeric(),selected:self.selected,
                input:None,output:None,axes:[axis(MessageId::RESOURCES_SECTION_LEVELS_INPUT),axis(MessageId::RESOURCES_SECTION_LEVELS_OUTPUT)],domain,
                help:l.text(MessageId::PRESSURE_CURVE_HELP).to_string(),reset_label:l.text(MessageId::COMMON_RESET).to_string()},
                points,plot:(0..=256).map(|i|{let x=i as f32/256.;[x,R::map(&self.compiled,x)]}).collect(),marker:None,modified:draft!=&R::default()}})
    }
    pub fn input_busy(&self) -> bool { self.point_drag.is_some() || self.frame_drag.is_some() }
    pub fn source(&self) -> Option<&R> { self.draft.as_ref() }
    pub fn curve(&self) -> R::Compiled { self.compiled.clone() }
    pub fn open(&mut self, current: &R) {
        if self.draft.is_some() { return; }
        self.draft = Some(current.clone()); self.compiled = current.compile(); self.selected = Some(0);
        self.epoch = self.epoch.wrapping_add(1);
    }
    pub fn close(&mut self) -> Option<R> {
        self.point_drag = None;
        if let Some((bounds,_))=self.frame_drag.take() { self.placement=Some(bounds); }
        self.draft.take()
    }
    pub fn fit(&mut self, extent: [f32; 2], viewport: [f32; 2]) {
        if extent.iter().chain(&viewport).any(|v| !v.is_finite() || *v <= 0.) { return; }
        let mut b = self.placement.unwrap_or(Bounds { x: viewport[0] - extent[0] - 24., y: 64., width: extent[0], height: extent[1] });
        b.width = extent[0].min(viewport[0]); b.height = extent[1].min(viewport[1]);
        b.x = b.x.clamp(0., viewport[0] - b.width); b.y = b.y.clamp(0., viewport[1] - b.height);
        self.placement = Some(b);
    }
    pub fn action(&mut self, action: CurveEditorAction) {
        let Some(draft) = &mut self.draft else { return; };
        match action {
            CurveEditorAction::Contact { epoch, phase, point, extent } => {
                if epoch != self.epoch || point.iter().any(|v| !v.is_finite()) { return; }
                match phase {
                    ContactPhase::Down => {
                        if extent.iter().any(|v| !v.is_finite() || *v <= 0.) { return; }
                        let baseline = draft.clone();
                        let index = super::effects::curves::hit(&draft.points(), point, extent)
                            .or_else(|| draft.insert([point[0] / extent[0], 1. - point[1] / extent[1]]));
                        self.selected = index;
                        if let Some(index) = index { self.point_drag = Some(PointDrag { baseline, index: Some(index), press: point, extent }); }
                    }
                    ContactPhase::Move | ContactPhase::Up => {
                        if let Some(drag) = &mut self.point_drag && let Some(index) = drag.index {
                            let origin = drag.baseline.points().get(index).copied()
                                .filter(|_| drag.baseline.points().len() == draft.points().len())
                                .unwrap_or([drag.press[0] / drag.extent[0], 1. - drag.press[1] / drag.extent[1]]);
                            draft.set_point(index, [origin[0] + (point[0] - drag.press[0]) / drag.extent[0],
                                origin[1] - (point[1] - drag.press[1]) / drag.extent[1]]);
                            if crate::curve_editor::dragged_outside(point, drag.extent) && draft.remove(index) {
                                self.selected = None; drag.index = None;
                            }
                        }
                        if phase == ContactPhase::Up { self.point_drag = None; }
                    }
                    ContactPhase::Cancel => if let Some(drag) = self.point_drag.take() { *draft = drag.baseline; self.selected = None; },
                }
            }
            CurveEditorAction::RemoveAt { epoch, point, extent, point_count } => {
                if epoch != self.epoch { return; }
                if point_count.is_some_and(|count| count != draft.points().len()) {
                    if let Some(drag) = self.point_drag.take() { *draft = drag.baseline; }
                    self.selected = None;
                } else if let Some(index) = super::effects::curves::hit(&draft.points(), point, extent) && draft.remove(index) {
                    self.selected = None; self.point_drag = None;
                }
            }
            CurveEditorAction::Key { epoch, key_event, pressed, modifiers, .. } => {
                if epoch != self.epoch || !pressed { return; }
                if key_event == "Escape" {
                    if let Some(drag) = self.point_drag.take() { *draft = drag.baseline; self.selected = None; }
                    else if let Some((bounds,_))=self.frame_drag.take() { self.placement=Some(bounds); }
                } else if let Some(index) = self.selected.filter(|i| *i < draft.points().len()) {
                    if key_event == "Delete" || key_event == "Backspace" { if draft.remove(index) { self.selected = None; } }
                    else {
                        let step = if modifiers.shift { 0.05 } else { 0.005 };
                        let mut point = draft.points()[index];
                        match key_event.as_str() { "ArrowLeft" => point[0] -= step, "ArrowRight" => point[0] += step,
                            "ArrowUp" => point[1] += step, "ArrowDown" => point[1] -= step, _ => return }
                        draft.set_point(index, point);
                    }
                }
            }
            CurveEditorAction::Reset => { *draft = R::default(); self.selected = Some(0); self.point_drag = None; },
            CurveEditorAction::Number { .. } | CurveEditorAction::Gesture { .. } => {},
        }
    }
    pub fn compile(&mut self) { if let Some(draft) = &self.draft { self.compiled = draft.compile(); } }
}
