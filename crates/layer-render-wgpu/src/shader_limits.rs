use crate::GpuRasterError;

pub(crate) struct ValidatedModule {
    module: naga::Module,
    info: naga::valid::ModuleInfo,
}

impl ValidatedModule {
    pub fn parse(source: &str) -> Result<Self, GpuRasterError> {
        let module = naga::front::wgsl::parse_str(source)
            .map_err(|e| GpuRasterError::Effect(e.emit_to_string(source)))?;
        let info = naga::valid::Validator::new(naga::valid::ValidationFlags::all(), naga::valid::Capabilities::empty())
            .validate(&module)
            .map_err(|e| GpuRasterError::Effect(e.emit_to_string(source)))?;
        Ok(Self { module, info })
    }

    pub fn module(&self) -> &naga::Module { &self.module }

    pub fn into_parts(self) -> (naga::Module, naga::valid::ModuleInfo) { (self.module, self.info) }

    pub fn workgroup_storage(&self, entry: &str) -> Result<u64, GpuRasterError> {
        let invalid = || GpuRasterError::Effect("Workgroup storage layout is unresolved".into());
        let index = self.module.entry_points.iter().position(|e| e.name == entry && e.stage == naga::ShaderStage::Compute)
            .ok_or_else(invalid)?;
        if self.module.types.iter().any(|(_, ty)| matches!(ty.inner, naga::TypeInner::Array { size: naga::ArraySize::Pending(_), .. })) {
            return Err(invalid());
        }
        let mut sizes = naga::proc::Layouter::default();
        sizes.update(self.module.to_ctx()).map_err(|e| GpuRasterError::Effect(e.to_string()))?;
        self.module.global_variables.iter()
            .filter(|(handle, v)| v.space == naga::AddressSpace::WorkGroup && !self.info.get_entry_point(index)[*handle].is_empty())
            .try_fold(0u64, |total, (_, v)| {
                let bytes = u64::from(sizes[v.ty].size).div_ceil(16) * 16;
                total.checked_add(bytes).ok_or_else(invalid)
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn storage_counts_each_reachable_variable_once_with_webgpu_padding() {
        let module = ValidatedModule::parse("
            var<workgroup> first:u32;
            var<workgroup> second:u32;
            var<workgroup> unused:array<u32,4096>;
            fn inner(){first=1u;}
            fn outer(){inner();first=2u;}
            fn dead(){unused[0]=1u;}
            @compute @workgroup_size(1) fn both(){outer();second=1u;}
            @compute @workgroup_size(1) fn one(){inner();}
            @compute @workgroup_size(1) fn none(){}
        ").unwrap();
        assert_eq!(module.workgroup_storage("both").unwrap(),32);
        assert_eq!(module.workgroup_storage("one").unwrap(),16);
        assert_eq!(module.workgroup_storage("none").unwrap(),0);
        assert!(module.workgroup_storage("missing").is_err());
    }

    #[test]
    fn storage_includes_type_layout_and_static_references() {
        let module = ValidatedModule::parse("
            struct Aligned {@align(32) @size(48) value:u32}
            var<workgroup> aligned:Aligned;
            var<workgroup> phony:u32;
            @compute @workgroup_size(1) fn main(){aligned.value=1u;_=phony;}
        ").unwrap();
        assert_eq!(module.workgroup_storage("main").unwrap(),80);
    }

    #[test]
    fn levels_entries_fit_the_downlevel_storage_budget() {
        let module = ValidatedModule::parse(crate::snapshot::levels::SHADER).unwrap();
        assert_eq!(module.workgroup_storage("measure").unwrap(),2560);
        assert_eq!(module.workgroup_storage("count").unwrap(),u64::from(wgpu::Limits::downlevel_defaults().max_compute_workgroup_storage_size));
        assert_eq!(module.workgroup_storage("fold").unwrap(),0);
    }

    #[test]
    fn unpadded_storage_sum_accepts_a_shader_above_the_device_boundary() {
        let limit=u64::from(wgpu::Limits::downlevel_defaults().max_compute_workgroup_storage_size);
        let head=limit-16;
        let source=format!("var<workgroup> head:array<u32,{}>;var<workgroup> one:u32;var<workgroup> two:u32;@compute @workgroup_size(1) fn main(){{head[0]=1u;one=1u;two=1u;}}",head/4);
        let module=ValidatedModule::parse(&source).unwrap();
        let unpadded=head+8;
        let padded=module.workgroup_storage("main").unwrap();
        assert!(unpadded<=limit);
        assert_eq!(padded,limit+16);
        assert!(padded>limit);
    }

    #[test]
    fn override_sized_workgroup_array_refuses_unresolved_storage_layout() {
        let module=ValidatedModule::parse("
            override count:u32=4u;
            var<workgroup> scratch:array<u32,count>;
            @compute @workgroup_size(1) fn main(){scratch[0]=1u;}
        ").unwrap();
        let error=module.workgroup_storage("main").unwrap_err();
        assert!(error.to_string().contains("Workgroup storage layout is unresolved"),"{error}");
    }

    #[test]
    fn renderer_limits_retain_the_minimum_and_request_full_histograms_when_supported() {
        for storage in [16351,16352,16368,16383,16384,32768] {
            let supported=wgpu::Limits {max_compute_workgroup_storage_size:storage,..wgpu::Limits::defaults()};
            let actual=crate::rendering_limits(supported.clone());
            let expected=if storage>=16384 {16384}else{16352};
            assert_eq!(actual.max_compute_workgroup_storage_size,expected);
            assert_eq!(actual,wgpu::Limits {max_compute_workgroup_storage_size:expected,
                ..wgpu::Limits::downlevel_defaults().using_resolution(supported.clone())});
            assert_eq!(actual.check_limits(&supported),storage>=16352);
        }
    }
}
