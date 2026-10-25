//! Immutable creative color tables; parsing, hashing and verification run on file workers.
use crate::color::{RgbSpace, rgb};
use crate::authored::Resource;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::sync::Arc;

const MEMORY_ERROR:&str="Color lookup data exceeds the shared memory budget";

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all="snake_case")]
pub enum Look { Warm, Cool, Monochrome }
impl Look {
    pub const ALL:[Self;3]=[Self::Warm,Self::Cool,Self::Monochrome];
    fn title(self)->&'static str {match self {Self::Warm=>"Warm",Self::Cool=>"Cool",Self::Monochrome=>"Monochrome"}}
    pub fn resource(self)->Arc<Lut3d> {
        static RESOURCES:[std::sync::OnceLock<Arc<Lut3d>>;3]=[const {std::sync::OnceLock::new()};3];
        RESOURCES[self as usize].get_or_init(|| {
            let samples=(0..17u32.pow(3)).map(|index| {
                let rgb=[index%17,(index/17)%17,index/289].map(|v|f64::from(v)/16.);
                match self {
                    Self::Monochrome=>{
                        let linear=rgb.map(|v|RgbSpace::Srgb.decode(v));let weights=RgbSpace::Srgb.to_xyz()[1];
                        [RgbSpace::Srgb.encode(linear.iter().zip(weights).map(|(v,w)|v*w).sum()) as f32;3]
                    },
                    _=>{
                        let direction=if self==Self::Warm {1.} else {-1.};
                        std::array::from_fn(|i|(rgb[i]+[0.16,0.02,-0.16][i]*direction*rgb[i]*(1.-rgb[i])) as f32)
                    }
                }
            }).collect::<Vec<_>>();
            Arc::new(Lut3d::from_builtin_samples(self.title().into(),&samples).expect("built-in color look"))
        }).clone()
    }
    pub fn for_resource(resource:&Lut3d)->Option<Self> {
        Self::ALL.into_iter().find(|look|resource.title()==look.title() && resource==look.resource().as_ref())
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Descriptor {
    size: u32,
    domain: [[f32; 3]; 2],
    title: Arc<str>,
    digest: [u8; 32],
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Lut3d {
    #[serde(flatten)]
    descriptor: Descriptor,
    #[serde(skip)]
    payload: Option<Resource<[u8]>>,
    #[serde(skip)]
    spaces: u8,
}
impl PartialEq for Lut3d {
    fn eq(&self, other: &Self) -> bool { self.descriptor == other.descriptor }
}
impl std::fmt::Debug for Lut3d {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Lut3d").field("descriptor", &self.descriptor)
            .field("ready", &self.payload.is_some()).field("bytes", &self.bytes()).finish()
    }
}
impl Lut3d {
    pub const MAX_TEXT_BYTES: usize = 16 * 1024 * 1024;
    pub const MAX_SIZE: u32 = 65;

    pub fn size(&self) -> u32 { self.descriptor.size }
    pub fn domain(&self) -> [[f32; 3]; 2] { self.descriptor.domain }
    pub fn title(&self) -> &str { &self.descriptor.title }
    pub fn with_title(&self, title: Arc<str>) -> Result<Self, &'static str> {
        let mut result = self.clone();
        result.descriptor.title = title;
        result.validate_descriptor()?;
        Ok(result)
    }
    pub fn digest(&self) -> [u8; 32] { self.descriptor.digest }
    pub fn payload(&self) -> Option<&[u8]> { self.payload.as_deref() }
    pub fn storage(&self) -> Option<&Arc<[u8]>> { self.payload.as_ref().map(Resource::storage) }
    pub fn resource(&self) -> Option<&Resource<[u8]>> { self.payload.as_ref() }
    pub(crate) fn alias_resource(&self, id: crate::authored::PortableId) -> Result<Self, &'static str> {
        let payload = self.payload.as_ref().ok_or("Unresolved color lookup resource")?;
        crate::memory::images().track(payload.storage()).map_err(|_|MEMORY_ERROR)?;
        let payload = payload.alias(id);
        Ok(Self { descriptor: self.descriptor.clone(), payload: Some(payload), spaces: self.spaces })
    }
    pub fn expected_bytes(&self) -> usize { (self.size() as usize).pow(3) * 12 }
    pub fn samples(&self) -> Option<impl Iterator<Item = [f32; 3]> + '_> {
        self.payload.as_deref().map(|bytes| bytes.as_chunks::<12>().0.iter().map(|record| std::array::from_fn(|i| f32::from_le_bytes(record[i*4..i*4+4].try_into().unwrap()))))
    }
    pub fn bytes(&self) -> usize { self.payload.as_ref().map_or(0, |payload| payload.len()) }
    pub fn accepts(&self, space: RgbSpace) -> bool {
        self.payload.is_some() && self.spaces & (1 << space.shader_code()) != 0 && self.validate_gpu_domain().is_ok()
    }
    pub fn validate_descriptor(&self) -> Result<(), &'static str> {
        let d = &self.descriptor;
        if !(2..=Self::MAX_SIZE).contains(&d.size) || d.title.chars().count() > 256
            || d.title.chars().any(char::is_control)
            || d.domain.iter().flatten().any(|v| !v.is_finite())
            || (0..3).any(|i| d.domain[0][i] >= d.domain[1][i]) {
            return Err("Invalid color lookup descriptor");
        }
        Ok(())
    }
    pub fn validate_gpu_domain(&self) -> Result<(), &'static str> {
        let d = &self.descriptor;
        if d.domain.iter().flatten().any(|v| v.abs() > 1e37) {
            return Err("Color lookup domain exceeds GPU range");
        }
        let tiny = f64::from(f32::MIN_POSITIVE);
        for axis in 0..3 {
            let lo = f64::from(d.domain[0][axis]); let hi = f64::from(d.domain[1][axis]);
            if lo < tiny && hi > -tiny {
                let loss = lo.max(-tiny).abs().max(hi.min(tiny).abs()) / (hi-lo) * f64::from(d.size-1);
                if loss > f64::from(f32::EPSILON) { return Err("Color lookup domain exceeds GPU precision"); }
            }
        }
        Ok(())
    }
    pub fn from_samples(size: u32, domain: [[f32; 3]; 2], title: Arc<str>, samples: Arc<[[f32; 3]]>) -> Result<Self, &'static str> {
        let budget=crate::memory::images();
        let _samples_memory=budget.reserve(std::mem::size_of_val(samples.as_ref())).map_err(|_|MEMORY_ERROR)?;
        Self::from_samples_in(budget,size,domain,title,&samples)
    }
    fn sample_descriptor(size:u32,domain:[[f32;3];2],title:Arc<str>,samples:&[[f32;3]])->Result<Self,&'static str> {
        let mut value = Self { descriptor: Descriptor { size, domain, title, digest: [0; 32] }, payload: None, spaces: 0 };
        value.validate_descriptor()?;
        value.validate_gpu_domain()?;
        if samples.len() != (size as usize).pow(3) {
            return Err("Invalid color lookup samples");
        }
        value.spaces = Self::space_bits(samples.iter().copied())?;
        Ok(value)
    }
    fn encode_samples(samples:&[[f32;3]],payload:&mut Vec<u8>) {
        for sample in samples.iter() { for component in sample.iter().copied() { payload.extend(component.to_le_bytes()); } }
    }
    fn from_samples_in(budget:&crate::memory::Budget,size:u32,domain:[[f32;3];2],title:Arc<str>,samples:&[[f32;3]])->Result<Self,&'static str> {
        let mut value=Self::sample_descriptor(size,domain,title,samples)?;
        let mut scratch=budget.reserve(value.expected_bytes()).map_err(|_|MEMORY_ERROR)?;
        let mut payload=Vec::new();
        payload.try_reserve_exact(value.expected_bytes()).map_err(|_|MEMORY_ERROR)?;
        scratch.resize(payload.capacity()).map_err(|_|MEMORY_ERROR)?;
        Self::encode_samples(samples,&mut payload);
        value.descriptor.digest=Sha256::digest(&payload).into();
        let memory=budget.reserve_payload(payload.len()).map_err(|_|MEMORY_ERROR)?;
        value.payload=Some(memory.retain(Arc::<[u8]>::from(payload)).map_err(|_|MEMORY_ERROR)?.into());
        Ok(value)
    }
    fn from_builtin_samples(title:Arc<str>,samples:&[[f32;3]])->Result<Self,&'static str> {
        let mut value=Self::sample_descriptor(17,[[0.;3],[1.;3]],title,samples)?;
        let mut payload=Vec::with_capacity(value.expected_bytes());
        Self::encode_samples(samples,&mut payload);
        value.descriptor.digest = Sha256::digest(&payload).into();
        value.payload = Some(payload.into());
        Ok(value)
    }
    fn space_bits(samples: impl Iterator<Item=[f32;3]>) -> Result<u8, &'static str> {
        let mut bounds = [[f32::INFINITY;3],[f32::NEG_INFINITY;3]];
        for sample in samples {
            for i in 0..3 {
                if !sample[i].is_finite() { return Err("Invalid color lookup samples"); }
                bounds[0][i] = bounds[0][i].min(sample[i]);
                bounds[1][i] = bounds[1][i].max(sample[i]);
            }
        }
        let limit = f32::MAX * (1. - 2048. * f32::EPSILON);
        let admitted = |v:f32| v.is_finite() && v.abs() <= limit;
        let mut spaces = 0;
        for space in RgbSpace::ALL {
            let transforms = RgbSpace::ALL.map(|destination| space.linear_transform(destination));
            let valid = (0..8).all(|corner| {
                let sample: [f32;3] = std::array::from_fn(|i| bounds[(corner >> i) & 1][i]);
                let linear = sample.map(|v| space.decode(f64::from(v)));
                linear.iter().all(|v| admitted(*v as f32)) && transforms.iter().all(|matrix| {
                    rgb::apply(*matrix, linear).iter().all(|v| admitted(*v as f32))
                        && matrix.iter().all(|row| {
                            let products: [f32; 3] = std::array::from_fn(|i| row[i] as f32 * linear[i] as f32);
                            products.iter().all(|v| admitted(*v))
                                && (0..3).all(|i| (0..3).all(|j| i == j || admitted(products[i] + products[j])))
                                && admitted((products[0] + products[1]) + products[2])
                                && admitted((products[0] + products[2]) + products[1])
                                && admitted((products[1] + products[2]) + products[0])
                        })
                })
            });
            if valid { spaces |= 1 << space.shader_code(); }
        }
        Ok(spaces)
    }
    pub fn parse_cube_named(bytes: &[u8], filename: &str) -> Result<Self, &'static str> {
        let mut resource = Self::parse_cube(bytes)?;
        if resource.title().is_empty() {
            resource.descriptor.title = filename.chars().filter(|c| !c.is_control()).take(256).collect::<String>().into();
        }
        Ok(resource)
    }
    pub fn parse_cube(bytes: &[u8]) -> Result<Self, &'static str> {
        Self::parse_cube_in(crate::memory::images(),bytes)
    }
    fn parse_cube_in(budget:&crate::memory::Budget,bytes:&[u8])->Result<Self,&'static str> {
        if bytes.len() > Self::MAX_TEXT_BYTES { return Err("Color lookup file is too large"); }
        let text = std::str::from_utf8(bytes).map_err(|_| "Invalid color lookup text")?;
        let mut size = None;
        let mut title = None;
        let mut domain = [None, None];
        let mut samples = Vec::new();
        let mut samples_memory=budget.reserve(0).map_err(|_|MEMORY_ERROR)?;
        let mut data = false;
        let number = |word: &str| -> Result<f32, &'static str> {
            let value = word.parse::<f64>().map_err(|_| "Invalid color lookup number")?;
            if !value.is_finite() || value.abs() > 1e37 { return Err("Color lookup number is out of range"); }
            Ok(value as f32)
        };
        for source in text.split('\n') {
            let source = source.strip_suffix('\r').unwrap_or(source);
            if source.len() > 4096 { return Err("Color lookup line is too long"); }
            let line = source.trim_matches([' ', '\t']);
            if line.is_empty() || line.starts_with('#') { continue; }
            let mut words = line.split_ascii_whitespace();
            let first = words.next().ok_or("Invalid color lookup line")?;
            if first == "TITLE" {
                if data || title.is_some() { return Err("Misplaced color lookup title"); }
                let value = line[first.len()..].trim_matches([' ', '\t']);
                let value = value.strip_prefix('"').and_then(|v| v.strip_suffix('"')).ok_or("Invalid color lookup title")?;
                if value.contains('"') || value.chars().count() > 256 || value.chars().any(char::is_control) { return Err("Invalid color lookup title"); }
                title = Some(Arc::from(value));
                continue;
            }
            if !line.is_ascii() || line.bytes().any(|b| b.is_ascii_control() && b != b'\t') { return Err("Invalid color lookup syntax"); }
            match first {
                "LUT_1D_SIZE" | "LUT_1D_INPUT_RANGE" => return Err("Use a 3D .cube LUT without a 1D shaper"),
                "LUT_3D_SIZE" => {
                    if data || size.is_some() { return Err("Duplicate or misplaced color lookup size"); }
                    let n: u32 = words.next().ok_or("Missing color lookup size")?.parse().map_err(|_| "Invalid color lookup size")?;
                    if !(2..=Self::MAX_SIZE).contains(&n) || words.next().is_some() { return Err("Invalid color lookup size"); }
                    samples_memory.resize((n as usize).pow(3)*std::mem::size_of::<[f32;3]>()).map_err(|_|MEMORY_ERROR)?;
                    samples.try_reserve_exact((n as usize).pow(3)).map_err(|_|MEMORY_ERROR)?;
                    samples_memory.resize(samples.capacity()*std::mem::size_of::<[f32;3]>()).map_err(|_|MEMORY_ERROR)?;
                    size = Some(n);
                }
                "LUT_3D_INPUT_RANGE" => {
                    if data || domain.iter().any(Option::is_some) {return Err("Duplicate or mixed color lookup domain");}
                    let minimum=number(words.next().ok_or("Incomplete color lookup domain")?)?;
                    let maximum=number(words.next().ok_or("Incomplete color lookup domain")?)?;
                    if words.next().is_some() {return Err("Invalid color lookup domain");}
                    domain=[Some([minimum;3]),Some([maximum;3])];
                }
                "DOMAIN_MIN" | "DOMAIN_MAX" => {
                    let index = usize::from(first == "DOMAIN_MAX");
                    if data || domain[index].is_some() { return Err("Duplicate or misplaced color lookup domain"); }
                    let mut point = [0.; 3];
                    for value in &mut point { *value = number(words.next().ok_or("Incomplete color lookup domain")?)?; }
                    if words.next().is_some() { return Err("Invalid color lookup domain"); }
                    domain[index] = Some(point);
                }
                _ => {
                    let n = size.ok_or("Color lookup samples precede its size")?;
                    if samples.len() >= (n as usize).pow(3) { return Err("Too many color lookup samples"); }
                    let sample = [number(first)?, number(words.next().ok_or("Incomplete color lookup sample")?)?, number(words.next().ok_or("Incomplete color lookup sample")?)?];
                    if words.next().is_some() { return Err("Invalid color lookup sample"); }
                    samples.push(sample);data = true;
                }
            }
        }
        Self::from_samples_in(budget,size.ok_or("Missing color lookup size")?, [domain[0].unwrap_or([0.; 3]), domain[1].unwrap_or([1.; 3])], title.unwrap_or_else(|| Arc::from("")), &samples)
    }
    pub fn with_payload(&self, bytes: &[u8]) -> Result<Self, &'static str> {
        self.with_payload_in(crate::memory::images(),bytes)
    }
    fn with_payload_in(&self,budget:&crate::memory::Budget,bytes:&[u8])->Result<Self,&'static str> {
        self.validate_descriptor()?;
        if bytes.len()!=self.expected_bytes() {return Err("Invalid color lookup payload length");}
        let memory=budget.reserve_payload(bytes.len()).map_err(|_|MEMORY_ERROR)?;
        let payload=memory.retain(Arc::<[u8]>::from(bytes)).map_err(|_|MEMORY_ERROR)?;
        self.with_resource_in(budget,payload.into())
    }
    pub fn with_shared_payload(&self, donor: &Self) -> Result<Self, &'static str> {
        self.validate_descriptor()?;
        if self.size() != donor.size() || self.digest() != donor.digest()
            || self.domain().iter().flatten().zip(donor.domain().iter().flatten()).any(|(a,b)| a.to_bits() != b.to_bits()) {
            return Err("Color lookup descriptor does not match verified payload");
        }
        let payload=donor.payload.clone().ok_or("Color lookup donor is not verified")?;
        crate::memory::images().track(payload.storage()).map_err(|_|MEMORY_ERROR)?;
        Ok(Self { descriptor: self.descriptor.clone(), payload: Some(payload),
            spaces: donor.spaces })
    }
    pub fn admitted_spaces(&self) -> u8 { self.spaces }
    #[cfg(target_arch = "wasm32")]
    pub fn from_verified_worker(&self, payload: Arc<[u8]>, spaces: u8) -> Result<Self, &'static str> {
        self.from_verified_worker_resource(payload.into(), spaces)
    }
    pub fn from_verified_worker_resource(&self, payload: Resource<[u8]>, spaces: u8) -> Result<Self, &'static str> {
        self.verified_worker_resource_in(crate::memory::images(),payload,spaces)
    }
    fn verified_worker_resource_in(&self,budget:&crate::memory::Budget,payload:Resource<[u8]>,spaces:u8)->Result<Self,&'static str> {
        self.validate_descriptor()?;
        if payload.len() != self.expected_bytes() || spaces == 0 || spaces & !15 != 0 {
            return Err("Invalid verified color lookup payload");
        }
        budget.track(payload.storage()).map_err(|_|MEMORY_ERROR)?;
        Ok(Self {descriptor: self.descriptor.clone(), payload: Some(payload), spaces})
    }
    pub fn from_verified_resource(size:u32,domain:[[f32;3];2],title:Arc<str>,digest:[u8;32],payload:Resource<[u8]>,spaces:u8)->Result<Self,&'static str> {
        let value=Self {descriptor:Descriptor {size,domain,title,digest},payload:None,spaces:0};
        value.from_verified_worker_resource(payload,spaces)
    }
    pub fn with_owned_payload(&self, payload: Arc<[u8]>) -> Result<Self, &'static str> {
        self.with_resource(payload.into())
    }
    pub fn from_resource(size: u32, domain: [[f32; 3]; 2], title: Arc<str>, payload: Resource<[u8]>) -> Result<Self, &'static str> {
        Self::from_resource_in(crate::memory::images(),size,domain,title,payload)
    }
    fn from_resource_in(budget:&crate::memory::Budget,size:u32,domain:[[f32;3];2],title:Arc<str>,payload:Resource<[u8]>)->Result<Self,&'static str> {
        let mut value = Self { descriptor: Descriptor { size, domain, title, digest: [0; 32] }, payload: None, spaces: 0 };
        budget.track(payload.storage()).map_err(|_|MEMORY_ERROR)?;
        value.spaces = value.payload_spaces(&payload)?;
        value.descriptor.digest = Sha256::digest(&payload).into();
        value.payload = Some(payload);
        Ok(value)
    }
    pub fn with_resource(&self, payload: Resource<[u8]>) -> Result<Self, &'static str> {
        self.with_resource_in(crate::memory::images(),payload)
    }
    fn with_resource_in(&self,budget:&crate::memory::Budget,payload:Resource<[u8]>)->Result<Self,&'static str> {
        budget.track(payload.storage()).map_err(|_|MEMORY_ERROR)?;
        let spaces = self.payload_spaces(&payload)?;
        if self.digest() != <[u8;32]>::from(Sha256::digest(&payload)) { return Err("Color lookup integrity check failed"); }
        Ok(Self { descriptor: self.descriptor.clone(), spaces, payload: Some(payload) })
    }
    fn payload_spaces(&self, bytes: &[u8]) -> Result<u8, &'static str> {
        self.validate_descriptor()?;
        if bytes.len() != self.expected_bytes() { return Err("Invalid color lookup payload length"); }
        let samples = bytes.as_chunks::<12>().0.iter().map(|sample| std::array::from_fn(|i| f32::from_le_bytes(sample[i*4..i*4+4].try_into().unwrap())));
        Self::space_bits(samples)
    }
}

#[cfg(test)]
mod memory_tests {
    use super::*;
    use crate::memory::Budget;

    fn cube()->Vec<u8> {
        let mut text=String::from("LUT_3D_SIZE 2\n");
        for b in 0..2 {for g in 0..2 {for r in 0..2 {text.push_str(&format!("{r} {g} {b}\n"));}}}
        text.into_bytes()
    }
    #[test]
    fn parsed_samples_and_payload_conversion_require_scratch_headroom() {
        assert_eq!(Lut3d::parse_cube_in(&Budget::new(95),b"LUT_3D_SIZE 2").unwrap_err(),MEMORY_ERROR);
        assert_eq!(Lut3d::from_samples_in(&Budget::new(400),2,[[0.;3],[1.;3]],"".into(),&[[0.5;3];8]).unwrap_err(),MEMORY_ERROR);
        assert_eq!(Lut3d::parse_cube_in(&Budget::new(512),&cube()).unwrap_err(),MEMORY_ERROR);
        let lut=Lut3d::parse_cube_in(&Budget::new(1024),&cube()).unwrap();
        assert_eq!(lut.bytes(),96);assert_eq!(lut.samples().unwrap().count(),8);
    }
    #[test]
    fn parsed_payload_aliases_retain_their_charge_and_failed_imports_release_scratch() {
        let budget=Budget::new(1024);let bytes=cube();
        let first=Lut3d::parse_cube_in(&budget,&bytes).unwrap();
        let second=Lut3d::parse_cube_in(&budget,&bytes).unwrap();
        let alias=first.storage().unwrap().clone();drop(first);budget.collect().unwrap();
        assert_eq!(Lut3d::parse_cube_in(&budget,&bytes).unwrap_err(),MEMORY_ERROR);
        drop(alias);budget.collect().unwrap();
        let third=Lut3d::parse_cube_in(&budget,&bytes).unwrap();
        assert_eq!(second.digest(),third.digest());drop((second,third));budget.collect().unwrap();
        assert!(budget.reserve(budget.maximum()).is_ok());
    }
    #[test]
    fn incoming_resources_share_allocation_charges_and_copies_need_new_admission() {
        let budget=Budget::new(512);
        let bytes:Arc<[u8]>=[0.25f32,0.5,0.75].iter().flat_map(|v|v.to_le_bytes()).collect::<Vec<_>>().repeat(8).into();
        let first=Lut3d::from_resource_in(&budget,2,[[0.;3],[1.;3]],"".into(),bytes.clone().into()).unwrap();
        let alias=first.with_resource_in(&budget,bytes.clone().into()).unwrap();
        let verified=first.verified_worker_resource_in(&budget,bytes.clone().into(),first.admitted_spaces()).unwrap();
        assert!(Arc::ptr_eq(first.storage().unwrap(),alias.storage().unwrap()));
        assert!(Arc::ptr_eq(first.storage().unwrap(),verified.storage().unwrap()));
        assert_eq!(first.with_payload_in(&budget,&bytes).unwrap_err(),MEMORY_ERROR);
        assert_eq!(Lut3d::from_resource_in(&budget,2,first.domain(),"".into(),bytes.to_vec().into()).unwrap_err(),MEMORY_ERROR);
        assert_eq!(first.verified_worker_resource_in(&budget,bytes.to_vec().into(),first.admitted_spaces()).unwrap_err(),MEMORY_ERROR);
        drop((first,alias,verified,bytes));budget.collect().unwrap();
        assert!(budget.reserve(budget.maximum()).is_ok());
    }
}
