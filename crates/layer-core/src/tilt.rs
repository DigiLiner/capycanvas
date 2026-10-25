use crate::PressureResponse;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(try_from = "Vec<[f32; 2]>", into = "Vec<[f32; 2]>")]
pub struct TiltResponse {
    curve: PressureResponse,
    extent: [f32; 2],
}

impl Default for TiltResponse {
    fn default() -> Self { Self { curve: PressureResponse::linear(), extent: [1., 1.] } }
}
impl TryFrom<Vec<[f32; 2]>> for TiltResponse {
    type Error = &'static str;
    fn try_from(points: Vec<[f32; 2]>) -> Result<Self, Self::Error> {
        let extent = points.last().copied().ok_or("Invalid tilt control points")?;
        if points.first() != Some(&[0., 0.]) || extent.iter().any(|v| !v.is_finite() || !(0.025..=1.).contains(v))
            || extent[0].max(extent[1]) != 1.
        { return Err("Invalid tilt control points"); }
        let curve = PressureResponse::try_from(points.into_iter().map(|p| [p[0]/extent[0], p[1]/extent[1]]).collect::<Vec<_>>())?;
        Ok(Self { curve, extent })
    }
}
impl From<TiltResponse> for Vec<[f32; 2]> {
    fn from(response: TiltResponse) -> Self { response.points() }
}
impl TiltResponse {
    pub fn normalized(&self) -> &PressureResponse { &self.curve }
    pub fn extent(&self) -> [f32; 2] { self.extent }
    pub fn points(&self) -> Vec<[f32; 2]> {
        self.curve.points().iter().map(|p| [p[0]*self.extent[0], p[1]*self.extent[1]]).collect()
    }
    pub fn map(&self, input: f32) -> f32 { self.curve.map(input/self.extent[0])*self.extent[1] }
    pub fn set_point(&mut self, index: usize, point: [f32; 2]) -> bool {
        if index == 0 || index >= self.curve.points().len() || point.iter().any(|v| !v.is_finite()) { return false; }
        if index+1 == self.curve.points().len() {
            let extent = if point[0] > point[1] { [1., point[1].clamp(0.025, 1.)] }
                else { [point[0].clamp(0.025, 1.), 1.] };
            if self.extent == extent { return false; }
            self.extent = extent; true
        } else {
            self.curve.set_point(index, [point[0]/self.extent[0], point[1]/self.extent[1]])
        }
    }
    pub fn insert(&mut self, point: [f32; 2]) -> Option<usize> {
        self.curve.insert([point[0]/self.extent[0], point[1]/self.extent[1]])
    }
    pub fn remove(&mut self, index: usize) -> bool { self.curve.remove(index) }
    pub fn sensitivity_enabled(&self, more: bool) -> bool {
        if more { self.extent[0] > 0.025 } else { self.extent[1] > 0.025 }
    }
    pub fn sensitivity(&mut self, more: bool) -> bool {
        if !self.sensitivity_enabled(more) { return false; }
        let [x,y] = self.extent;
        self.extent = if more {
            if y < 1. { [1., (y+0.025).min(1.)] } else { [(x-0.025).max(0.025), 1.] }
        } else if x < 1. { [(x+0.025).min(1.), 1.] } else { [1., (y-0.025).max(0.025)] };
        true
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct TiltAxes {
    pub reverse_horizontal: bool,
    pub reverse_vertical: bool,
    pub swap: bool,
}
impl TiltAxes {
    fn matrix(self) -> [[i8; 2]; 2] {
        let x = if self.reverse_horizontal { -1 } else { 1 };
        let y = if self.reverse_vertical { -1 } else { 1 };
        if self.swap { [[0,x],[y,0]] } else { [[x,0],[0,y]] }
    }
    fn from_matrix(m: [[i8; 2]; 2]) -> Self {
        let swap = m[0][0] == 0;
        Self { swap, reverse_horizontal: m[0][usize::from(swap)] < 0,
            reverse_vertical: m[1][usize::from(!swap)] < 0 }
    }
    pub fn apply(self, mut value: [f32; 2]) -> [f32; 2] {
        if self.swap { value.swap(0,1); }
        if self.reverse_horizontal { value[0] = -value[0]; }
        if self.reverse_vertical { value[1] = -value[1]; }
        value
    }
    pub fn inverse(self) -> Self {
        let m=self.matrix(); Self::from_matrix([[m[0][0],m[1][0]],[m[0][1],m[1][1]]])
    }
    pub fn compose(self, other: Self) -> Self {
        let a=self.matrix();let b=other.matrix();
        Self::from_matrix(std::array::from_fn(|i| std::array::from_fn(|j| a[i][0]*b[0][j]+a[i][1]*b[1][j])))
    }
    pub fn in_frame(self, native_to_surface: Self) -> Self {
        native_to_surface.compose(self).compose(native_to_surface.inverse())
    }
    pub fn quarter_turns(turns: u8) -> Self {
        Self::from_matrix(match turns%4 { 0=>[[1,0],[0,1]],1=>[[0,-1],[1,0]],2=>[[-1,0],[0,-1]],_=>[[0,1],[-1,0]] })
    }
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct TiltCalibration {
    pub response: TiltResponse,
    pub axes: TiltAxes,
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn tilt_response_starts_neutral_and_changes_sensitivity_in_both_directions() {
        let mut response=TiltResponse::default();
        for i in 0..=90 { let x=i as f32/90.;assert!((response.map(x)-x).abs()<1e-6); }
        assert!(response.sensitivity(false));assert_eq!(response.extent(),[1.,0.975]);
        assert!(response.map(0.5)<0.5);assert_eq!(response.map(0.),0.);
        assert!(response.sensitivity(true));assert_eq!(response.extent(),[1.,1.]);
        assert!(response.sensitivity(true));assert_eq!(response.extent(),[0.975,1.]);
        assert!(response.map(0.5)>0.5);assert_eq!(response.map(1.),1.);
    }
    #[test]
    fn tilt_curve_editing_preserves_its_shape_and_validates_authored_data() {
        let mut response=TiltResponse::default();let index=response.insert([0.3,0.7]).unwrap();
        let shape=response.normalized().clone();assert!(response.set_point(2,[0.5,1.]));
        assert_eq!(response.normalized(),&shape);assert!(response.map(0.5)>0.999);
        assert!(!response.set_point(0,[0.,0.2]));assert!(!response.remove(0));
        assert!(!response.remove(2));assert!(response.remove(index));
        let saved=serde_json::to_string(&response).unwrap();assert_eq!(serde_json::from_str::<TiltResponse>(&saved).unwrap(),response);
        for invalid in ["[[0,0.1],[1,1]]","[[0,0],[0.5,0.5]]","[[0,0],[0,1]]","[[0,0],[0.5,0.9],[1,0.5]]"] {
            assert!(serde_json::from_str::<TiltResponse>(invalid).is_err(),"{invalid}");
        }
    }
    #[test]
    fn all_axis_mappings_preserve_magnitude_and_follow_the_input_frame() {
        let mut mappings=std::collections::BTreeSet::new();
        for reverse_horizontal in [false,true] {for reverse_vertical in [false,true] {for swap in [false,true] {
            let axes=TiltAxes{reverse_horizontal,reverse_vertical,swap};mappings.insert(axes.matrix());
            for turn in 0..4 {
                let frame=TiltAxes::quarter_turns(turn);let vector=[0.3,-0.7];
                assert_eq!(axes.in_frame(frame).apply(frame.apply(vector)),frame.apply(axes.apply(vector)));
                assert_eq!(axes.inverse().apply(axes.apply(vector)),vector);
                let mapped=axes.apply(vector);assert_eq!(mapped[0].hypot(mapped[1]),vector[0].hypot(vector[1]));
            }
        }}}
        assert_eq!(mappings.len(),8);
        let x=TiltAxes{reverse_horizontal:true,..Default::default()};
        assert_eq!(x.in_frame(TiltAxes::quarter_turns(1)),TiltAxes{reverse_vertical:true,..Default::default()});
        let swap=TiltAxes{swap:true,..Default::default()};
        assert_eq!(swap.in_frame(TiltAxes::quarter_turns(1)),TiltAxes{swap:true,reverse_horizontal:true,reverse_vertical:true});
    }
}
