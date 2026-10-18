use super::{PixelRect,GpuRasterError};
#[derive(Clone, Copy)]
struct Bounds { low: f64, high: f64 }
impl Bounds {
    fn exact(n: f64) -> Self { Self { low: n, high: n } }
    fn rounded(low: f64, high: f64) -> Self {
        let magnitude = low.abs().max(high.abs());
        if !magnitude.is_finite() || magnitude > f64::from(f32::MAX) * 0.25 {
            return Self { low: f64::NEG_INFINITY, high: f64::INFINITY };
        }
        let error = (magnitude * f64::from(f32::EPSILON) + f64::from(f32::MIN_POSITIVE)) * 32.;
        Self { low: low - error, high: high + error }
    }
    fn add(self, b: Self) -> Self { Self::rounded(self.low + b.low, self.high + b.high) }
    fn sub(self, b: Self) -> Self { Self::rounded(self.low - b.high, self.high - b.low) }
    fn mul(self, b: Self) -> Self {
        let products = [self.low*b.low, self.low*b.high, self.high*b.low, self.high*b.high];
        Self::rounded(products.into_iter().fold(f64::INFINITY, f64::min),
            products.into_iter().fold(f64::NEG_INFINITY, f64::max))
    }
    fn div(self, b: Self) -> Self {
        if b.high < 0. { return Self { low:-self.high, high:-self.low }.div(Self { low:-b.high, high:-b.low }); }
        if b.low <= 0. || !b.high.is_finite() { return Self { low: f64::NEG_INFINITY, high: f64::INFINITY }; }
        self.mul(Self::rounded(1./b.high, 1./b.low))
    }
    fn abs(self) -> Self {
        Self { low: if self.low <= 0. && self.high >= 0. { 0. } else { self.low.abs().min(self.high.abs()) },
            high: self.low.abs().max(self.high.abs()) }
    }
    fn square(self) -> Self {
        let n = self.abs(); Self::rounded(n.low*n.low, n.high*n.high)
    }
    fn sqrt(self) -> Self {
        if self.low < 0. { return Self { low: f64::NEG_INFINITY, high: f64::INFINITY }; }
        Self::rounded(self.low.sqrt(), self.high.sqrt())
    }
}
fn dot(row: [Bounds;3], point: [Bounds;2]) -> Bounds {
    row[0].mul(point[0]).add(row[1].mul(point[1])).add(row[2])
}
fn norm(x: Bounds, y: Bounds) -> Bounds {
    let square = x.square().add(y.square());
    Bounds { low: square.low.max(0.), high: square.high }.sqrt()
}
fn lod(rows: [[Bounds;3];3], point: [Bounds;2]) -> Bounds {
    let [x,y,w] = rows;
    let q = [dot(x,point),dot(y,point),dot(w,point)];
    let denominator = q[2].square();
    if q[2].low <= 0. || denominator.low < f64::from(1e-20f32) {
        return Bounds { low: f64::NEG_INFINITY, high: f64::INFINITY };
    }
    let [a,b,c,d] = [x[0].mul(q[2]).sub(q[0].mul(w[0])).div(denominator),
        y[0].mul(q[2]).sub(q[1].mul(w[0])).div(denominator),
        x[1].mul(q[2]).sub(q[0].mul(w[1])).div(denominator),
        y[1].mul(q[2]).sub(q[1].mul(w[1])).div(denominator)];
    footprint_lod([a,b,c,d])
}
fn footprint_lod([a,b,c,d]:[Bounds;4]) -> Bounds {
    let stretch = norm(a.add(d),b.sub(c)).add(norm(a.sub(d),b.add(c))).mul(Bounds::exact(0.5));
    if stretch.low < 2. || !stretch.high.is_finite() {
        return Bounds { low: f64::NEG_INFINITY, high: f64::INFINITY };
    }
    Bounds::rounded(stretch.low.log2(),stretch.high.log2())
}
fn footprint_inside(plane: [Bounds;3], point: [Bounds;2]) -> bool {
    let margin = plane[0].abs().add(plane[1].abs()).mul(Bounds::exact(0.25));
    dot(plane,point).low >= margin.high
}
fn single_sample(moved: [[Bounds;3];3], clip: [[Bounds;3];2], extent: [Bounds;2],
    source: [Bounds;2], point: [Bounds;2]) -> bool {
    for (row,limit) in clip.into_iter().zip(extent) {
        let center=dot(row,point);
        let margin=row[0].abs().add(row[1].abs()).mul(Bounds::exact(0.25));
        if center.sub(margin).low < 0. || center.add(margin).high >= limit.low { return false; }
    }
    let [x,y,w]=moved;
    let right=std::array::from_fn(|i| w[i].mul(source[0]).sub(x[i]));
    let bottom=std::array::from_fn(|i| w[i].mul(source[1]).sub(y[i]));
    [w,x,y,right,bottom].into_iter().all(|plane| footprint_inside(plane,point))
}
pub(super) fn coarse_only(values: &[u8;crate::scene::resample::UNIFORM_BYTES as usize],
    texels: [u32;4], keeps: bool, mesh: bool) -> bool {
    if mesh || texels[2..].contains(&0) { return false; }
    let read=|offset| f64::from(f32::from_le_bytes(values[offset..offset+4].try_into().unwrap()));
    let scalar=|offset| Bounds::exact(read(offset));
    let row=|offset| [scalar(offset),scalar(offset+4),scalar(offset+8)];
    let moved=[row(0),row(16),row(32)];
    let kept=[row(48),row(64),row(80)];
    let clip=[row(96),row(112)];
    let extent=[scalar(128),scalar(132)];
    let source=[scalar(192),scalar(196)];
    let options=read(184);
    let Some(right)=texels[0].checked_add(texels[2]) else { return false; };
    let Some(bottom)=texels[1].checked_add(texels[3]) else { return false; };
    if right > 1<<22 || bottom > 1<<22 || f64::from(right)>read(208)
        || f64::from(bottom)>read(212) || read(180)<1. || !options.is_finite()
        || [read(180),read(208),read(212)].into_iter().any(|n|!n.is_finite())
        || extent.into_iter().chain(source).any(|n|n.low<=0.)
        || moved.into_iter().chain(kept).chain(clip).flatten().chain(extent).chain(source)
            .any(|n| !n.low.is_finite()) { return false; }
    let point=[Bounds { low:f64::from(texels[0])+0.5, high:f64::from(right)-0.5 },
        Bounds { low:f64::from(texels[1])+0.5, high:f64::from(bottom)-0.5 }];
    let moved_lod=lod(moved,point);
    if !keeps && options==0. && single_sample(moved,clip,extent,source,point) {
        return moved_lod.low >= 1.;
    }
    let tap_lod=Bounds::rounded(1.,1.);
    if moved_lod.sub(tap_lod).low < 1. { return false; }
    (!keeps && options==0.) || lod(kept,point).sub(tap_lod).low >= 1.
}


fn mesh_vertex_screen(vertex:[f32;5],clip:[[Bounds;3];2],size:[Bounds;2])->[Bounds;2] {
    let [x,y]=clip;
    let p=[Bounds::exact(f64::from(vertex[0])).sub(x[2]),Bounds::exact(f64::from(vertex[1])).sub(y[2])];
    let determinant=x[0].mul(y[1]).sub(x[1].mul(y[0]));
    let output=[y[1].mul(p[0]).sub(x[1].mul(p[1])).div(determinant),
        x[0].mul(p[1]).sub(y[0].mul(p[0])).div(determinant)];
    std::array::from_fn(|axis| {
        let normalized=output[axis].div(size[axis]);
        let clip=if axis==0 { normalized.mul(Bounds::exact(2.)).sub(Bounds::exact(1.)) }
            else { Bounds::exact(1.).sub(normalized.mul(Bounds::exact(2.))) };
        let screen=if axis==0 { clip.add(Bounds::exact(1.)) } else { Bounds::exact(1.).sub(clip) }
            .mul(size[axis]).mul(Bounds::exact(0.5));
        Bounds { low:screen.low-1./16., high:screen.high+1./16. }
    })
}
fn mesh_source_row(vertices:[[Bounds;2];3],source:[f32;3])->[Bounds;3] {
    let [p,a,b]=vertices;
    let x=[a[0].sub(p[0]),b[0].sub(p[0])];
    let y=[a[1].sub(p[1]),b[1].sub(p[1])];
    let s=source.map(|n|Bounds::exact(f64::from(n)));
    let delta=[s[1].sub(s[0]),s[2].sub(s[0])];
    let determinant=x[0].mul(y[1]).sub(x[1].mul(y[0]));
    let dx=delta[0].mul(y[1]).sub(delta[1].mul(y[0])).div(determinant);
    let dy=x[0].mul(delta[1]).sub(x[1].mul(delta[0])).div(determinant);
    [dx,dy,s[0].sub(dx.mul(p[0])).sub(dy.mul(p[1]))]
}
pub(super) fn mesh_coarse_only(values:&[u8;crate::scene::resample::UNIFORM_BYTES as usize],
    texels:[u32;4],keeps:bool,triangle:[[f32;5];3])->bool {
    let read=|o| f64::from(f32::from_le_bytes(values[o..o+4].try_into().unwrap()));
    let scalar=|o|Bounds::exact(read(o));
    let row=|o|[scalar(o),scalar(o+4),scalar(o+8)];
    if keeps || read(184)!=0. || read(180)<1. || !read(180).is_finite()
        || triangle.into_iter().any(|v|v[4]!=1. || v.into_iter().any(|n|!n.is_finite()))
        || [0,4,8,16,20,24,32,36,40].into_iter().map(read).ne([1.,0.,0.,0.,1.,0.,0.,0.,1.])
        || texels[2..].contains(&0) {return false;}
    let Some(right)=texels[0].checked_add(texels[2]) else{return false;};
    let Some(bottom)=texels[1].checked_add(texels[3]) else{return false;};
    let clip=[row(96),row(112)];let extent=[scalar(128),scalar(132)];
    let source=[scalar(192),scalar(196)];let size=[scalar(208),scalar(212)];
    if right>1<<22 || bottom>1<<22 || f64::from(right)>size[0].low || f64::from(bottom)>size[1].low
        || clip.into_iter().flatten().any(|n|!n.low.is_finite())
        || extent.into_iter().chain(source).chain(size).any(|n|!n.low.is_finite() || n.low<=0.) {return false;}
    let size=size.map(|n|Bounds::exact(n.low.ceil()));
    let vertices=triangle.map(|v|mesh_vertex_screen(v,clip,size));
    if vertices.into_iter().any(|v|v.into_iter().zip(size).any(|(n,size)|n.low<0. || n.high>size.low)) {return false;}
    let point=[Bounds {low:f64::from(texels[0])+0.5,high:f64::from(right)-0.5},
        Bounds {low:f64::from(texels[1])+0.5,high:f64::from(bottom)-0.5}];
    for (r,limit) in clip.into_iter().zip(extent) {
        let center=dot(r,point);let margin=r[0].abs().add(r[1].abs()).mul(Bounds::exact(0.25));
        if center.sub(margin).low<0. || center.add(margin).high>=limit.low {return false;}
    }
    let rows=[mesh_source_row(vertices,triangle.map(|v|v[2])),mesh_source_row(vertices,triangle.map(|v|v[3]))];
    let helper=point.map(|n|Bounds {low:n.low-1.,high:n.high+1.});
    let mut derivatives=[[Bounds::exact(0.);2];2];
    for (axis,(r,limit)) in rows.into_iter().zip(source).enumerate() {
        let magnitude=r[0].abs().high*helper[0].abs().high+r[1].abs().high*helper[1].abs().high+r[2].abs().high;
        let error=(magnitude*f64::from(f32::EPSILON)+f64::from(f32::MIN_POSITIVE))*32.;
        if !error.is_finite() {return false;}
        let derivative=[r[0],r[1]].map(|n|Bounds {low:n.low-2.*error,high:n.high+2.*error});
        let q=dot(r,point);let q=Bounds {low:q.low-error,high:q.high+error};
        let reach=derivative[0].abs().add(derivative[1].abs()).mul(Bounds::exact(0.25));
        if q.sub(reach).low<0. || q.add(reach).high>=limit.low {return false;}
        derivatives[axis]=derivative;
    }
    footprint_lod([derivatives[0][0],derivatives[1][0],derivatives[0][1],derivatives[1][1]]).low>=1.
}

#[cfg(test)]
mod tests {
    use super::*;
    const N:usize=crate::scene::resample::UNIFORM_BYTES as usize;
    fn put(values:&mut [u8;N],offset:usize,n:f32) { values[offset..offset+4].copy_from_slice(&n.to_le_bytes()); }
    fn values(scale:f32)->[u8;N] {
        let mut v=[0;N];
        for (offset,n) in [(0,scale),(20,scale),(40,1.),(48,scale),(68,scale),(88,1.),
            (96,1.),(116,1.),(128,4096.),(132,4096.),(180,8.),(192,16384.),(196,16384.),(208,4096.),(212,4096.)] { put(&mut v,offset,n); }
        v
    }
    #[test]
    fn covered_full_selection_uses_the_unsubdivided_footprint() {
        let v=values(2.1);
        assert!(coarse_only(&v,[20,20,8,8],false,false));
        assert!(!coarse_only(&v,[20,20,8,8],true,false));
        let mut keep_source=v; put(&mut keep_source,184,1.);
        assert!(!coarse_only(&keep_source,[20,20,8,8],false,false));
    }
    #[test]
    fn selection_and_clip_boundaries_require_fallback_footprints() {
        let mut v=values(2.1);put(&mut v,8,-1.);
        assert!(!coarse_only(&v,[0,0,8,8],false,false));
        let mut minified=values(4.1);put(&mut minified,8,-2.);
        assert!(coarse_only(&minified,[0,0,8,8],false,false));
        let mut clipped=v; put(&mut clipped,104,-20.5);
        assert!(!coarse_only(&clipped,[20,20,8,8],false,false));
    }
    #[test]
    fn retained_pixels_must_independently_exclude_fine_sampling() {
        let mut v=values(4.1);
        assert!(coarse_only(&v,[20,20,8,8],true,false));
        put(&mut v,48,1.); put(&mut v,68,1.);
        assert!(!coarse_only(&v,[20,20,8,8],true,false));
        put(&mut v,184,1.);
        assert!(!coarse_only(&v,[20,20,8,8],false,false));
    }
    #[test]
    fn uncertain_derivatives_and_partial_edges_keep_fine_pages() {
        let mut v=values(2.1);
        assert!(!coarse_only(&v,[20,20,8,8],false,true));
        put(&mut v,208,27.5);
        assert!(!coarse_only(&v,[20,20,8,8],false,false));
        put(&mut v,208,4096.);put(&mut v,40,0.);
        assert!(!coarse_only(&v,[20,20,8,8],false,false));
        put(&mut v,40,1.);put(&mut v,0,f32::NAN);
        assert!(!coarse_only(&v,[20,20,8,8],false,false));
    }
    #[test]
    fn threshold_rounding_never_omits_a_fine_page() {
        assert!(!coarse_only(&values(2.),[20,20,8,8],false,false));
        assert!(!coarse_only(&values(f32::from_bits(2f32.to_bits()-1)),[20,20,8,8],false,false));
        assert!(!coarse_only(&values(4.),[20,20,8,8],true,false));
    }
    fn reference_lod(v:&[u8;N],base:usize,p:[f32;2])->f32 {
        let read=|o| f32::from_le_bytes(v[o..o+4].try_into().unwrap());
        let rows:[[_;3];3]=std::array::from_fn(|r|std::array::from_fn(|c|read(base+r*16+c*4)));
        let q=rows.map(|r|r[0]*p[0]+r[1]*p[1]+r[2]);
        let d=(q[2]*q[2]).max(1e-20);
        let [a,b,c,e]=[(rows[0][0]*q[2]-q[0]*rows[2][0])/d,
            (rows[1][0]*q[2]-q[1]*rows[2][0])/d,
            (rows[0][1]*q[2]-q[0]*rows[2][1])/d,
            (rows[1][1]*q[2]-q[1]*rows[2][1])/d];
        (((a+e).hypot(b-c)+(a-e).hypot(b+c))*0.5).max(1.).log2()
    }
    #[test]
    fn admitted_projective_regions_have_no_fine_taps_at_any_pixel() {
        let mut admitted=0;
        for seed in 0..1200 {
            let mut v=values(1.8+(seed%17) as f32*0.19);
            put(&mut v,4,(seed%7) as f32*0.07-0.2);
            put(&mut v,16,(seed%11) as f32*0.04-0.2);
            put(&mut v,32,(seed%9) as f32*0.0001-0.0004);
            put(&mut v,36,(seed%13) as f32*0.00005-0.0003);
            let texels=[20+(seed%19) as u32,20+(seed%23) as u32,8,8];
            if !coarse_only(&v,texels,false,false) { continue; }
            admitted+=1;
            for y in texels[1]..texels[1]+8 { for x in texels[0]..texels[0]+8 {
                assert!(reference_lod(&v,0,[x as f32+0.5,y as f32+0.5])>=1.,"seed={seed}, pixel={x},{y}");
            }}
        }
        assert!(admitted>300,"no useful admitted regions: {admitted}");
    }

    fn mesh_triangle(scale:f32)->[[f32;5];3] {
        [[16.,16.,16.*scale,16.*scale,1.],[256.,16.,256.*scale,16.*scale,1.],[16.,256.,16.*scale,256.*scale,1.]]
    }
    #[test]
    fn affine_mesh_uses_actual_triangle_derivatives() {
        let v=values(1.);let triangle=mesh_triangle(4.);
        assert!(mesh_coarse_only(&v,[30,30,8,8],false,triangle));
        assert!(!mesh_coarse_only(&v,[30,30,8,8],true,triangle));
        assert!(!mesh_coarse_only(&v,[30,30,8,8],false,mesh_triangle(2.)));
        let mut flipped=triangle;flipped.swap(1,2);
        assert!(mesh_coarse_only(&v,[30,30,8,8],false,flipped));
    }
    #[test]
    fn mesh_edges_and_uncertain_geometry_keep_fine_pages() {
        let mut v=values(1.);let triangle=mesh_triangle(4.);
        put(&mut v,192,123.);
        assert!(!mesh_coarse_only(&v,[30,30,1,1],false,triangle));
        let v=values(1.);let mut rational=triangle;rational[0][4]=0.5;
        assert!(!mesh_coarse_only(&v,[30,30,8,8],false,rational));
        let mut degenerate=triangle;degenerate[2]=degenerate[1];
        assert!(!mesh_coarse_only(&v,[30,30,8,8],false,degenerate));
        let mut keep_source=v;put(&mut keep_source,184,1.);
        assert!(!mesh_coarse_only(&keep_source,[30,30,8,8],false,triangle));
    }
    #[test]
    fn affine_mesh_admission_keeps_threshold_and_source_margin() {
        let v=values(1.);let mut accepted=0;
        for i in 0..160 {
            let scale=1.5+i as f32/32.;let triangle=mesh_triangle(scale);
            let region=[30+(i%19),30+(i%23),8,8];
            if !mesh_coarse_only(&v,region,false,triangle) {continue;}
            accepted+=1;
            assert!(scale.log2()>1.);
            for y in region[1]..region[1]+8 {for x in region[0]..region[0]+8 {
                for n in [x,y] {let q=(n as f32+0.5)*scale;let reach=scale*0.25;assert!(q-reach>=0. && q+reach<16384.);}
            }}
        }
        assert!(accepted>80,"accepted={accepted}");
    }
}


fn projected(rows:[[Bounds;3];3],point:[Bounds;2])->Option<Option<[Bounds;2]>> {
    let q=rows.map(|r|dot(r,point));
    if q[2].high<=0. {return Some(None);}
    if q[2].low<=0. {return None;}
    let xy=[q[0].div(q[2]),q[1].div(q[2])];
    xy.into_iter().all(|n|n.low.is_finite() && n.high.is_finite()).then_some(Some(xy))
}
fn matrix(values:&[u8;crate::scene::resample::UNIFORM_BYTES as usize],base:usize)->[[Bounds;3];3] {
    std::array::from_fn(|row|std::array::from_fn(|column| {
        let offset=base+row*16+column*4;
        Bounds::exact(f64::from(f32::from_le_bytes(values[offset..offset+4].try_into().unwrap())))
    }))
}
fn fine_page_rectangle(q:[Bounds;2],extent:[f64;2],phase:[u32;2])->Option<Option<[[u32;2];2]>> {
    let mut range=[[0;2];2];
    for axis in 0..2 {
        if !extent[axis].is_finite() || extent[axis]<=0. || extent[axis]>(1u32<<24) as f64
            || !q[axis].low.is_finite() || !q[axis].high.is_finite() {return None;}
        if q[axis].high<0. || q[axis].low>=extent[axis] {return Some(None);}
        let last=extent[axis].ceil()-1.;
        let low=(q[axis].low.clamp(0.5,last+0.5)-0.5).floor().max(0.) as u32;
        let high=((q[axis].high.clamp(0.5,last+0.5)-0.5).floor()+1.).min(last) as u32;
        if high<phase[axis] {return Some(None);}
        range[0][axis]=low.saturating_sub(phase[axis])/super::PAGE_SIZE;
        range[1][axis]=(high-phase[axis])/super::PAGE_SIZE;
    }
    Some(Some(range))
}
fn insert_fine_pages(pages:&mut std::collections::BTreeSet<[u32;2]>,q:[Bounds;2],extent:[f64;2],phase:[u32;2],capacity:usize)->Option<()> {
    let Some([low,high])=fine_page_rectangle(q,extent,phase)? else{return Some(());};
    let count=u64::from(high[0]-low[0]+1)*u64::from(high[1]-low[1]+1);
    if count>capacity as u64 {return None;}
    for y in low[1]..=high[1] {for x in low[0]..=high[0] {
        pages.insert([x,y]);if pages.len()>capacity {return None;}
    }}
    Some(())
}
fn mesh_row(vertices:[[Bounds;2];3],source:[Bounds;3])->[Bounds;3] {
    let [p,a,b]=vertices;
    let x=[a[0].sub(p[0]),b[0].sub(p[0])];
    let y=[a[1].sub(p[1]),b[1].sub(p[1])];
    let delta=[source[1].sub(source[0]),source[2].sub(source[0])];
    let determinant=x[0].mul(y[1]).sub(x[1].mul(y[0]));
    let dx=delta[0].mul(y[1]).sub(delta[1].mul(y[0])).div(determinant);
    let dy=x[0].mul(delta[1]).sub(x[1].mul(delta[0])).div(determinant);
    [dx,dy,source[0].sub(dx.mul(p[0])).sub(dy.mul(p[1]))]
}
fn mesh_rows(values:&[u8;crate::scene::resample::UNIFORM_BYTES as usize],triangle:[[f32;5];3])->Option<([[Bounds;3];3],[Bounds;3])> {
    if triangle.into_iter().any(|v|v.into_iter().any(|n|!n.is_finite()) || v[4]<=0.) {return None;}
    let read=|o|Bounds::exact(f64::from(f32::from_le_bytes(values[o..o+4].try_into().unwrap())));
    let clip=[matrix(values,96)[0],matrix(values,96)[1]];
    let size=[read(208),read(212)].map(|n|Bounds::exact(n.low.ceil()));
    if size.into_iter().any(|n|!n.low.is_finite() || n.low<=0.) {return None;}
    let vertices=triangle.map(|v|mesh_vertex_screen(v,clip,size));
    let reciprocal=triangle.map(|v|Bounds::exact(1.).div(Bounds::exact(f64::from(v[4]))));
    let owner=[mesh_row(vertices,std::array::from_fn(|i|Bounds::exact(f64::from(triangle[i][2])).mul(reciprocal[i]))),
        mesh_row(vertices,std::array::from_fn(|i|Bounds::exact(f64::from(triangle[i][3])).mul(reciprocal[i]))),
        mesh_row(vertices,reciprocal)];
    let moved=matrix(values,0);
    Some((std::array::from_fn(|row|std::array::from_fn(|column|
        moved[row][0].mul(owner[0][column]).add(moved[row][1].mul(owner[1][column])).add(moved[row][2].mul(owner[2][column])))),owner[2]))
}
pub(super) struct Demand {
    values:[u8;crate::scene::resample::UNIFORM_BYTES as usize],
    keeps:bool,
    phase:[u32;2],
    triangles:Option<Vec<[[f32;5];3]>>,
}
impl Demand {
    pub fn new(values:[u8;crate::scene::resample::UNIFORM_BYTES as usize],keeps:bool,phase:[u32;2],triangles:Option<Vec<[[f32;5];3]>>)->Self {
        Self {values,keeps,phase,triangles}
    }
    pub fn pages(&self,output:PixelRect,capacity:usize)->Result<std::collections::BTreeSet<[u32;2]>,GpuRasterError> {
        region_pages(&self.values,[output.min_x(),output.min_y(),output.width(),output.height()],self.keeps,self.phase,self.triangles.as_deref(),capacity)
    }
}
pub(super) fn region_pages(values:&[u8;crate::scene::resample::UNIFORM_BYTES as usize],
    texels:[u32;4],keeps:bool,phase:[u32;2],triangles:Option<&[[[f32;5];3]]>,capacity:usize,
)->Result<std::collections::BTreeSet<[u32;2]>,super::GpuRasterError> {
    let failed=||super::GpuRasterError::SourceWorkingSetExceeded;
    let mut pages=std::collections::BTreeSet::new();
    if texels[2..].contains(&0) {return Ok(pages);}
    let read=|offset|f64::from(f32::from_le_bytes(values[offset..offset+4].try_into().unwrap()));
    let right=texels[0].checked_add(texels[2]).ok_or_else(failed)?;
    let bottom=texels[1].checked_add(texels[3]).ok_or_else(failed)?;
    let target=[read(208),read(212)];
    if [right,bottom].into_iter().zip(target).any(|(n,size)|!size.is_finite() || f64::from(n)>size.ceil() || n>1<<22)
        || matrix(values,0).into_iter().chain(matrix(values,48)).flatten().any(|n|!n.low.is_finite()) {return Err(failed());}
    let point=[Bounds::rounded(f64::from(texels[0]),f64::from(right).min(target[0])),
        Bounds::rounded(f64::from(texels[1]),f64::from(bottom).min(target[1]))];
    let extent=[read(192),read(196)];
    let full=f64::from(right)<=target[0] && f64::from(bottom)<=target[1];
    if triangles.is_none() && coarse_only(values,texels,keeps,false) {return Ok(pages);}
    let mut add_projected=|rows,point|->Option<()> {
        if let Some(q)=projected(rows,point)? {insert_fine_pages(&mut pages,q,extent,phase,capacity)?;}
        Some(())
    };
    if triangles.is_none() {add_projected(matrix(values,0),point).ok_or_else(failed)?;}
    if keeps || read(184)!=0. {
        let rows=matrix(values,48);
        if !full || lod(rows,point).low<2. {add_projected(rows,point).ok_or_else(failed)?;}
    }
    if let Some(triangles)=triangles {
        for &triangle in triangles {
            let clip=[matrix(values,96)[0],matrix(values,96)[1]];
            let size=target.map(|n|Bounds::exact(n.ceil()));
            let vertices=triangle.map(|v|mesh_vertex_screen(v,clip,size));
            if (0..2).any(|axis|vertices.into_iter().all(|p|p[axis].high<point[axis].low)
                || vertices.into_iter().all(|p|p[axis].low>point[axis].high)) {continue;}
            if mesh_coarse_only(values,texels,false,triangle) {continue;}
            let (rows,owner_w)=mesh_rows(values,triangle).ok_or_else(failed)?;
            let helper=point.map(|n|Bounds {low:n.low-1.,high:n.high+1.});
            if dot(owner_w,helper).low<=0. {return Err(failed());}
            let Some(center)=projected(rows,point).ok_or_else(failed)? else{continue;};
            let Some(neighbours)=projected(rows,helper).ok_or_else(failed)? else{continue;};
            let samples=std::array::from_fn(|axis| {
                let reach=1.5*(neighbours[axis].high-neighbours[axis].low);
                Bounds::rounded(center[axis].low-reach,center[axis].high+reach)
            });
            insert_fine_pages(&mut pages,samples,extent,phase,capacity).ok_or_else(failed)?;
        }
    }
    Ok(pages)
}

#[cfg(test)]
mod region_contract_tests {
    use super::*;
    const N:usize=crate::scene::resample::UNIFORM_BYTES as usize;
    fn put(v:&mut[u8;N],o:usize,n:f32) {v[o..o+4].copy_from_slice(&n.to_le_bytes());}
    fn values()->[u8;N] {
        let mut v=[0;N];for (o,n) in [(0,1.),(20,1.),(40,1.),(48,1.),(68,1.),(88,1.),
            (96,1.),(116,1.),(128,16384.),(132,16384.),(180,8.),(192,16384.),(196,16384.),(208,8192.),(212,8192.)] {put(&mut v,o,n);}v
    }
    fn assert_samples(v:&[u8;N],texels:[u32;4],phase:[u32;2],pages:&std::collections::BTreeSet<[u32;2]>) {
        let read=|o|f32::from_le_bytes(v[o..o+4].try_into().unwrap());
        for y in texels[1]..texels[1]+texels[3] {for x in texels[0]..texels[0]+texels[2] {
            let footprint=[(read(208)-x as f32).min(1.),(read(212)-y as f32).min(1.)];
            for delta in [[0.,0.],[-0.25,-0.25],[-0.25,0.25],[0.25,-0.25],[0.25,0.25]] {
                let p=[x as f32+footprint[0]*(0.5+delta[0]),y as f32+footprint[1]*(0.5+delta[1])];
                let q:[f32;3]=std::array::from_fn(|r|read(r*16)*p[0]+read(r*16+4)*p[1]+read(r*16+8));
                if q[2]<=0. {continue;}let q=[q[0]/q[2],q[1]/q[2]];
                if q.into_iter().zip([read(192),read(196)]).any(|(n,extent)|n<0. || n>=extent) {continue;}
                let low:[u32;2]=std::array::from_fn(|i|(q[i].clamp(0.5,read(192+i*4).ceil()-0.5)-0.5).floor() as u32);
                for dy in 0..2 {for dx in 0..2 {
                    let p=[(low[0]+dx).min(read(192).ceil() as u32-1),(low[1]+dy).min(read(196).ceil() as u32-1)];
                    if p[0]<phase[0] || p[1]<phase[1] {continue;}
                    let page=std::array::from_fn(|i|(p[i]-phase[i])/super::super::PAGE_SIZE);
                    assert!(pages.contains(&page),"missing {page:?},pixel={x},{y},sample={q:?}");
                }}
            }
        }}
    }
    #[test]
    fn local_region_does_not_retain_the_whole_16384_source() {
        let v=values();let pages=region_pages(&v,[100,100,16,16],false,[0;2],None,4).unwrap();
        assert!(pages.len()<=4);assert_samples(&v,[100,100,16,16],[0;2],&pages);
    }
    #[test]
    fn bilinear_boundary_and_phase_require_both_adjacent_pages() {
        let mut v=values();let side=super::super::PAGE_SIZE;
        put(&mut v,8,side as f32-0.75);put(&mut v,24,side as f32-0.75);
        let pages=region_pages(&v,[0,0,1,1],false,[0;2],None,4).unwrap();
        assert_eq!(pages,std::collections::BTreeSet::from([[0,0],[0,1],[1,0],[1,1]]));
        let phase=[17,23];let phased=region_pages(&v,[0,0,1,1],false,phase,None,4).unwrap();
        assert_samples(&v,[0,0,1,1],phase,&phased);assert_eq!(phased.len(),1);
    }
    #[test]
    fn projective_fractional_lod_and_fractional_last_pixel_keep_every_sample() {
        for seed in 0..64 {
            let mut v=values();put(&mut v,0,1.2);put(&mut v,20,1.3);put(&mut v,8,510.3);
            put(&mut v,32,(seed as f32-32.)*0.00001);put(&mut v,208,27.25);put(&mut v,212,29.75);
            let region=[20,22,8,8];let pages=region_pages(&v,region,false,[7,9],None,64).unwrap();
            assert_samples(&v,region,[7,9],&pages);
        }
    }
    #[test]
    fn kept_and_keep_source_add_their_independent_original_pages() {
        let mut v=values();put(&mut v,8,2048.);let output=[40,40,8,8];
        let moved=region_pages(&v,output,false,[0;2],None,64).unwrap();
        let kept=region_pages(&v,output,true,[0;2],None,64).unwrap();
        assert!(moved.is_subset(&kept));assert!(kept.len()>moved.len());
        put(&mut v,184,1.);assert_eq!(region_pages(&v,output,false,[0;2],None,64).unwrap(),kept);
    }
    #[test]
    fn output_subdivision_never_changes_global_mip_or_source_map() {
        let mut v=values();put(&mut v,8,211.3);put(&mut v,24,397.8);
        let output=[100,100,20,20];let all=region_pages(&v,output,false,[0;2],None,64).unwrap();
        let mut split=region_pages(&v,[100,100,10,20],false,[0;2],None,64).unwrap();
        split.extend(region_pages(&v,[110,100,10,20],false,[0;2],None,64).unwrap());
        assert!(split.is_subset(&all));assert_samples(&v,output,[0;2],&split);
    }
    #[test]
    fn budget_and_projective_poles_request_subdivision_not_a_clipped_page_set() {
        let mut v=values();assert!(region_pages(&v,[0,0,4096,4096],false,[0;2],None,1).is_err());
        put(&mut v,32,1.);put(&mut v,40,-0.5);
        assert!(region_pages(&v,[0,0,1,1],false,[0;2],None,64).is_err());
    }
    #[test]
    fn affine_mesh_is_local_and_rational_mesh_is_bounded_without_whole_source_union() {
        let v=values();let tri=[[0.,0.,1000.,2000.,1.],[256.,0.,1256.,2000.,1.],[0.,256.,1000.,2256.,1.]];
        let a=region_pages(&v,[30,30,8,8],false,[0;2],Some(&[tri]),64).unwrap();
        assert!(a.len()<=16);assert!(!a.is_empty());
        let mut rational=tri;rational[1][4]=0.9;rational[2][4]=1.1;
        let b=region_pages(&v,[30,30,8,8],false,[0;2],Some(&[rational]),64).unwrap();
        assert!(b.len()<=64);assert!(!b.is_empty());
    }
    #[test]
    fn rational_mesh_pages_cover_independent_fragment_interpolation_and_helper_lane_taps() {
        let v=values();
        for weights in [[1.,1.,1.],[1.,0.9,1.1]] {
            let triangle=[[0.,0.,225.,225.,weights[0]],[256.,0.,481.,225.,weights[1]],[0.,256.,225.,481.,weights[2]]];
            let phase=[3,7];let pages=region_pages(&v,[30,30,8,8],false,phase,Some(&[triangle]),64).unwrap();
            let q=|p:[f32;2]| {
                let bary=[1.-p[0]/256.-p[1]/256.,p[0]/256.,p[1]/256.];
                let w:[f32;3]=std::array::from_fn(|i|bary[i]/weights[i]);
                let denominator=w.into_iter().sum::<f32>();
                std::array::from_fn::<_,2,_>(|axis|(0..3).map(|i|w[i]*triangle[i][axis+2]).sum::<f32>()/denominator)
            };
            for y in 30..38 {for x in 30..38 {
                let center=[x as f32+0.5,y as f32+0.5];let source=q(center);
                let h=q([(x/2*2) as f32+0.5,center[1]]);let h1=q([(x/2*2) as f32+1.5,center[1]]);
                let k=q([center[0],(y/2*2) as f32+0.5]);let k1=q([center[0],(y/2*2) as f32+1.5]);
                for sy in [-0.25,0.25] {for sx in [-0.25,0.25] {
                    let tap:[f32;2]=std::array::from_fn(|axis|source[axis]+sx*(h1[axis]-h[axis])+sy*(k1[axis]-k[axis]));
                    let low=tap.map(|n|(n-0.5).floor() as u32);
                    for dy in 0..2 {for dx in 0..2 {
                        let pixel=[low[0]+dx,low[1]+dy];let page=std::array::from_fn(|i|(pixel[i]-phase[i])/super::super::PAGE_SIZE);
                        assert!(pages.contains(&page),"weights={weights:?},fragment={x},{y},tap={tap:?},page={page:?}");
                    }}
                }}
            }}
        }
    }

}
