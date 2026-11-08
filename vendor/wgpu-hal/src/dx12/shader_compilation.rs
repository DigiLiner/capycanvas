use alloc::{string::String, vec::Vec};
use core::ffi::CStr;
use std::{hash::{Hash, Hasher}, path::PathBuf};

use crate::auxil::dxgi::result::HResult;
use thiserror::Error;
use windows::{
    core::{Interface, PCSTR, PCWSTR},
    Win32::{Graphics::Direct3D::{Dxc, Fxc, ID3DBlob, D3D_SHADER_MACRO}, System::Com::CoTaskMemFree},
};

pub(super) enum CompilerContainer {
    Fxc(CompilerFxc),
    DynamicDxc(CompilerDynamicDxc),
    #[cfg_attr(not(static_dxc), allow(unused))]
    StaticDxc(CompilerStaticDxc),
}

pub(super) struct CompilerFxc {
    fxc: FxcLib,
}

pub(super) struct CompilerDynamicDxc {
    max_shader_model: wgt::DxcShaderModel,
    identity: CompilerIdentity,
    compiler: Dxc::IDxcCompiler3,
    // Has to be held onto for the lifetime of the device otherwise shaders will fail to compile.
    // Only needed when using dynamic linking.
    _dxc: DxcLib,
}

pub(super) struct CompilerStaticDxc {
    max_shader_model: wgt::DxcShaderModel,
    identity: CompilerIdentity,
    compiler: Dxc::IDxcCompiler3,
}

#[derive(Debug, Error)]
pub(super) enum GetContainerError {
    #[error(transparent)]
    Device(#[from] crate::DeviceError),
    #[error("Failed to load {0}: {1}")]
    FailedToLoad(&'static str, libloading::Error),
}

impl CompilerContainer {
    pub(super) fn new_fxc() -> Result<Self, GetContainerError> {
        FxcLib::new_dynamic().map(|fxc| Self::Fxc(CompilerFxc { fxc }))
    }

    pub(super) fn new_dynamic_dxc(dxc_path: PathBuf) -> Result<Self, GetContainerError> {
        let dxc = DxcLib::new_dynamic(dxc_path)
            .map_err(|e| GetContainerError::FailedToLoad("dxcompiler.dll", e))?;

        let compiler = dxc.create_instance::<Dxc::IDxcCompiler3>()?;

        let identity = compiler_identity(&compiler);

        Ok(Self::DynamicDxc(CompilerDynamicDxc {
            max_shader_model: wgt::DxcShaderModel::from_dxc_version(identity.major, identity.minor),
            identity,
            compiler,
            _dxc: dxc,
        }))
    }

    /// Creates a [`CompilerContainer`] that delegates to the statically-linked version of DXC.
    pub(super) fn new_static_dxc() -> Result<CompilerContainer, crate::DeviceError> {
        #[cfg(static_dxc)]
        {
            unsafe {
                let compiler = dxc_create_instance::<Dxc::IDxcCompiler3>(|clsid, iid, ppv| {
                    windows_core::HRESULT(mach_dxcompiler_rs::DxcCreateInstance(
                        clsid.cast(),
                        iid.cast(),
                        ppv,
                    ))
                })?;

                Ok(CompilerContainer::StaticDxc(CompilerStaticDxc {
                    max_shader_model: wgt::DxcShaderModel::V6_7,
                    identity: compiler_identity(&compiler),
                    compiler,
                }))
            }
        }
        #[cfg(not(static_dxc))]
        {
            panic!("Attempted to create a static DXC shader compiler, but the static-dxc feature was not enabled")
        }
    }

    pub(super) fn max_shader_model(&self) -> Option<wgt::DxcShaderModel> {
        match self {
            CompilerContainer::Fxc(..) => None,
            CompilerContainer::DynamicDxc(CompilerDynamicDxc {
                max_shader_model, ..
            })
            | CompilerContainer::StaticDxc(CompilerStaticDxc {
                max_shader_model, ..
            }) => Some(max_shader_model.clone()),
        }
    }

    fn arguments(&self, device: &super::Device) -> CompilerArguments {
        compile_arguments(!matches!(self, Self::Fxc(_)), device.shared.private_caps.instance_flags,
            device.shared.private_caps.workarounds.avoid_shader_debug_info, device.features)
    }

    pub(super) fn pipeline_cache_validation_key(&self, device: &super::Device) -> [u8; 16] {
        let (kind, identity) = match self {
            Self::Fxc(_) => ("fxc", None),
            Self::DynamicDxc(compiler) => ("dynamic dxc", Some(&compiler.identity)),
            Self::StaticDxc(compiler) => ("static dxc", Some(&compiler.identity)),
        };
        cache_validation_key(kind, identity, &self.arguments(device), device.shared.private_caps.shader_model)
    }

    pub(super) fn compile(
        &self,
        device: &super::Device,
        source: &str,
        source_name: Option<&CStr>,
        raw_ep: &str,
        stage_bit: wgt::ShaderStages,
        full_stage: &str,
    ) -> Result<super::CompiledShader, crate::PipelineError> {
        match (self, self.arguments(device)) {
            (CompilerContainer::Fxc(CompilerFxc { fxc }), CompilerArguments::Fxc(flags)) => compile_fxc(
                source,
                source_name,
                raw_ep,
                stage_bit,
                full_stage,
                fxc,
                flags,
            ),
            (CompilerContainer::DynamicDxc(CompilerDynamicDxc { compiler, .. })
            | CompilerContainer::StaticDxc(CompilerStaticDxc { compiler, .. }), CompilerArguments::Dxc(arguments)) => compile_dxc(
                source,
                source_name,
                raw_ep,
                stage_bit,
                full_stage,
                compiler,
                &arguments,
            ),
            _ => unreachable!(),
        }
    }
}

#[derive(Clone, Hash)]
struct CompilerIdentity {
    major: u32,
    minor: u32,
    commit: Option<(u32, Vec<u8>)>,
}

fn compiler_identity(compiler: &Dxc::IDxcCompiler3) -> CompilerIdentity {
    let version = compiler.cast::<Dxc::IDxcVersionInfo>().ok().and_then(|version| {
        let (mut major, mut minor) = (0, 0);
        unsafe { version.GetVersion(&mut major, &mut minor) }.ok().map(|()| (major, minor))
    });
    let (major, minor) = version.unwrap_or((1, 0));
    let commit = if let Ok(version) = compiler.cast::<Dxc::IDxcVersionInfo2>() {
        let mut count = 0;
        let mut hash = core::ptr::null_mut();
        let result = unsafe { version.GetCommitInfo(&mut count, &mut hash) };
        let commit = if result.is_ok() && !hash.is_null() {
            Some((count, unsafe { CStr::from_ptr(hash) }.to_bytes().to_vec()))
        } else { None };
        if !hash.is_null() { unsafe { CoTaskMemFree(Some(hash.cast())) }; }
        commit
    } else { None };
    CompilerIdentity { major, minor, commit }
}

#[derive(Clone)]
enum CompilerArguments {
    Fxc(u32),
    Dxc(arrayvec::ArrayVec<PCWSTR, 7>),
}
impl Hash for CompilerArguments {
    fn hash<H: Hasher>(&self, state: &mut H) {
        core::mem::discriminant(self).hash(state);
        match self {
            Self::Fxc(flags) => flags.hash(state),
            Self::Dxc(arguments) => {
                arguments.len().hash(state);
                for argument in arguments { unsafe { argument.as_wide() }.hash(state); }
            }
        }
    }
}

fn compile_arguments(dxc: bool, flags: wgt::InstanceFlags, avoid_debug: bool, features: wgt::Features) -> CompilerArguments {
    if !dxc {
        let mut arguments = Fxc::D3DCOMPILE_ENABLE_STRICTNESS;
        if flags.contains(wgt::InstanceFlags::DEBUG) {
            arguments |= Fxc::D3DCOMPILE_DEBUG | Fxc::D3DCOMPILE_SKIP_OPTIMIZATION;
        }
        return CompilerArguments::Fxc(arguments);
    }
    let mut arguments = arrayvec::ArrayVec::new();
    arguments.extend([windows::core::w!("-HV"), windows::core::w!("2018"), windows::core::w!("-no-warnings"), Dxc::DXC_ARG_ENABLE_STRICTNESS]);
    if flags.contains(wgt::InstanceFlags::DEBUG) && !avoid_debug {
        arguments.extend([Dxc::DXC_ARG_DEBUG, Dxc::DXC_ARG_SKIP_OPTIMIZATIONS]);
    }
    if features.intersects(wgt::Features::SHADER_F16 | wgt::Features::SHADER_I16) {
        arguments.push(windows::core::w!("-enable-16bit-types"));
    }
    CompilerArguments::Dxc(arguments)
}

fn cache_validation_key(kind: &str, identity: Option<&CompilerIdentity>, arguments: &CompilerArguments, model: naga::back::hlsl::ShaderModel) -> [u8; 16] {
    super::device::pipeline_cache_key(&("wgpu-hal dx12 dxil v2", kind, identity, arguments, model))
}

type D3DCompileFn = unsafe extern "system" fn(
    psrcdata: *const core::ffi::c_void,
    srcdatasize: usize,
    psourcename: PCSTR,
    pdefines: *const D3D_SHADER_MACRO,
    pinclude: *mut core::ffi::c_void,
    pentrypoint: PCSTR,
    ptarget: PCSTR,
    flags1: u32,
    flags2: u32,
    ppcode: *mut *mut core::ffi::c_void,
    pperrormsgs: *mut *mut core::ffi::c_void,
) -> windows_core::HRESULT;

#[derive(Debug)]
struct FxcLib {
    // `d3dcompile_fn` points into `_lib`, so `_lib` must be held for as long
    // as we want to keep compiling shaders with FXC.
    _lib: crate::dx12::DynLib,
    d3dcompile_fn: D3DCompileFn,
}

impl FxcLib {
    const PATH: &str = "d3dcompiler_47.dll";

    fn new_dynamic() -> Result<Self, GetContainerError> {
        unsafe {
            let lib = crate::dx12::DynLib::new(Self::PATH)
                .map_err(|e| GetContainerError::FailedToLoad(FxcLib::PATH, e))?;
            let d3dcompile_fn: D3DCompileFn = *lib.get::<D3DCompileFn>(c"D3DCompile".to_bytes())?;

            Ok(Self {
                _lib: lib,
                d3dcompile_fn,
            })
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn compile(
        &self,
        source: &str,
        source_name: Option<&CStr>,
        raw_ep: &str,
        full_stage: &str,
        compile_flags: u32,
        shader_data: &mut Option<ID3DBlob>,
        error: &mut Option<ID3DBlob>,
    ) -> Result<windows_core::Result<()>, crate::DeviceError> {
        unsafe {
            let raw_ep = alloc::ffi::CString::new(raw_ep).unwrap();
            let full_stage = alloc::ffi::CString::new(full_stage).unwrap();

            // If no name has been set, D3DCompile wants the null pointer.
            let source_name = source_name
                .map(|cstr| cstr.as_ptr().cast())
                .unwrap_or(core::ptr::null());

            let shader_data: *mut Option<ID3DBlob> = shader_data;
            let error: *mut Option<ID3DBlob> = error;

            {
                profiling::scope!("Fxc::D3DCompile");
                Ok((self.d3dcompile_fn)(
                    source.as_ptr().cast(),
                    source.len(),
                    PCSTR(source_name),
                    core::ptr::null(),
                    core::ptr::null_mut(),
                    PCSTR(raw_ep.as_ptr().cast()),
                    PCSTR(full_stage.as_ptr().cast()),
                    compile_flags,
                    0,
                    shader_data.cast(),
                    error.cast(),
                )
                .ok())
            }
        }
    }
}

fn compile_fxc(
    source: &str,
    source_name: Option<&CStr>,
    raw_ep: &str,
    stage_bit: wgt::ShaderStages,
    full_stage: &str,
    fxc: &FxcLib,
    compile_flags: u32,
) -> Result<super::CompiledShader, crate::PipelineError> {
    profiling::scope!("compile_fxc");
    let mut shader_data = None;
    let mut error = None;
    let hr = fxc.compile(
        source,
        source_name,
        raw_ep,
        full_stage,
        compile_flags,
        &mut shader_data,
        &mut error,
    )?;

    match hr {
        Ok(()) => {
            let shader_data = shader_data.unwrap();
            Ok(super::CompiledShader::Fxc(shader_data))
        }
        Err(e) => {
            let mut full_msg = format!("FXC D3DCompile error ({e})");
            if let Some(error) = error {
                use core::fmt::Write as _;
                let message = unsafe {
                    core::slice::from_raw_parts(
                        error.GetBufferPointer().cast(),
                        error.GetBufferSize(),
                    )
                };
                let _ = write!(full_msg, ": {}", String::from_utf8_lossy(message));
            }
            Err(crate::PipelineError::Linkage(stage_bit, full_msg))
        }
    }
}

trait DxcObj: Interface {
    const CLSID: windows::core::GUID;
}
impl DxcObj for Dxc::IDxcCompiler3 {
    const CLSID: windows::core::GUID = Dxc::CLSID_DxcCompiler;
}
impl DxcObj for Dxc::IDxcUtils {
    const CLSID: windows::core::GUID = Dxc::CLSID_DxcUtils;
}
impl DxcObj for Dxc::IDxcValidator {
    const CLSID: windows::core::GUID = Dxc::CLSID_DxcValidator;
}

#[derive(Debug)]
struct DxcLib {
    lib: crate::dx12::DynLib,
}

impl DxcLib {
    fn new_dynamic(lib_path: PathBuf) -> Result<Self, libloading::Error> {
        unsafe { crate::dx12::DynLib::new(lib_path).map(|lib| Self { lib }) }
    }

    pub fn create_instance<T: DxcObj>(&self) -> Result<T, crate::DeviceError> {
        unsafe {
            type DxcCreateInstanceFn = unsafe extern "system" fn(
                rclsid: *const windows_core::GUID,
                riid: *const windows_core::GUID,
                ppv: *mut *mut core::ffi::c_void,
            )
                -> windows_core::HRESULT;

            let func: libloading::Symbol<DxcCreateInstanceFn> =
                self.lib.get(c"DxcCreateInstance".to_bytes())?;
            dxc_create_instance::<T>(|clsid, iid, ppv| func(clsid, iid, ppv))
        }
    }
}

/// Invokes the provided library function to create a DXC object.
unsafe fn dxc_create_instance<T: DxcObj>(
    f: impl Fn(
        *const windows_core::GUID,
        *const windows_core::GUID,
        *mut *mut core::ffi::c_void,
    ) -> windows_core::HRESULT,
) -> Result<T, crate::DeviceError> {
    let mut result__ = None;
    f(&T::CLSID, &T::IID, <*mut _>::cast(&mut result__))
        .ok()
        .into_device_result("DxcCreateInstance")?;
    result__.ok_or(crate::DeviceError::Unexpected)
}

/// Owned PCWSTR
#[allow(clippy::upper_case_acronyms)]
struct OPCWSTR {
    inner: Vec<u16>,
}

impl OPCWSTR {
    fn new(s: &str) -> Self {
        let mut inner: Vec<_> = s.encode_utf16().collect();
        inner.push(0);
        Self { inner }
    }

    fn ptr(&self) -> PCWSTR {
        PCWSTR(self.inner.as_ptr())
    }
}

fn get_output<T: Interface>(
    res: &Dxc::IDxcResult,
    kind: Dxc::DXC_OUT_KIND,
) -> Result<T, crate::DeviceError> {
    let mut result__: Option<T> = None;
    unsafe { res.GetOutput::<T>(kind, &mut None, <*mut _>::cast(&mut result__)) }
        .into_device_result("GetOutput")?;
    result__.ok_or(crate::DeviceError::Unexpected)
}

fn as_err_str(blob: &Dxc::IDxcBlobUtf8) -> Result<&str, crate::DeviceError> {
    let ptr = unsafe { blob.GetStringPointer() };
    let len = unsafe { blob.GetStringLength() };
    core::str::from_utf8(unsafe { core::slice::from_raw_parts(ptr.0, len) })
        .map_err(|_| crate::DeviceError::Unexpected)
}

fn compile_dxc(
    source: &str,
    source_name: Option<&CStr>,
    raw_ep: &str,
    stage_bit: wgt::ShaderStages,
    full_stage: &str,
    compiler: &Dxc::IDxcCompiler3,
    arguments: &[PCWSTR],
) -> Result<crate::dx12::CompiledShader, crate::PipelineError> {
    profiling::scope!("compile_dxc");

    let source_name = source_name.and_then(|cstr| cstr.to_str().ok());

    let source_name = source_name.map(OPCWSTR::new);
    let raw_ep = OPCWSTR::new(raw_ep);
    let full_stage = OPCWSTR::new(full_stage);

    let mut compile_args = arrayvec::ArrayVec::<PCWSTR, 13>::new_const();

    if let Some(source_name) = source_name.as_ref() {
        compile_args.push(source_name.ptr())
    }

    compile_args.extend([
        windows::core::w!("-E"),
        raw_ep.ptr(),
        windows::core::w!("-T"),
        full_stage.ptr(),
    ]);
    compile_args.extend(arguments.iter().copied());

    let buffer = Dxc::DxcBuffer {
        Ptr: source.as_ptr().cast(),
        Size: source.len(),
        Encoding: Dxc::DXC_CP_UTF8.0,
    };

    let compile_res: Dxc::IDxcResult =
        unsafe { compiler.Compile(&buffer, Some(&compile_args), None) }
            .into_device_result("Compile")?;

    drop(compile_args);
    drop(source_name);
    drop(raw_ep);
    drop(full_stage);

    let err_blob = get_output::<Dxc::IDxcBlobUtf8>(&compile_res, Dxc::DXC_OUT_ERRORS)?;

    let len = unsafe { err_blob.GetStringLength() };
    if len != 0 {
        let err = as_err_str(&err_blob)?;
        return Err(crate::PipelineError::Linkage(
            stage_bit,
            format!("DXC compile error: {err}"),
        ));
    }

    let blob = get_output::<Dxc::IDxcBlob>(&compile_res, Dxc::DXC_OUT_OBJECT)?;

    Ok(crate::dx12::CompiledShader::Dxc(blob))
}

#[cfg(test)]
mod cache_validation_tests {
    use super::*;

    #[test]
    fn dxc_validation_covers_compiler_identity_and_every_constant_argument() {
        let identity = CompilerIdentity { major: 1, minor: 8, commit: Some((42, b"abcdef12".to_vec())) };
        let arguments = compile_arguments(true, wgt::InstanceFlags::DEBUG, false, wgt::Features::SHADER_F16);
        for kind in ["dynamic dxc", "static dxc"] {
            let key = cache_validation_key(kind, Some(&identity), &arguments, naga::back::hlsl::ShaderModel::V6_7);
            assert_eq!(key, cache_validation_key(kind, Some(&identity.clone()), &arguments.clone(), naga::back::hlsl::ShaderModel::V6_7));
            for changed in [
                CompilerIdentity { major: 2, ..identity.clone() },
                CompilerIdentity { minor: 9, ..identity.clone() },
                CompilerIdentity { commit: Some((43, b"abcdef12".to_vec())), ..identity.clone() },
                CompilerIdentity { commit: Some((42, b"abcdef13".to_vec())), ..identity.clone() },
            ] {
                assert_ne!(key, cache_validation_key(kind, Some(&changed), &arguments, naga::back::hlsl::ShaderModel::V6_7));
            }
            assert_ne!(key, cache_validation_key(kind, Some(&CompilerIdentity { commit: None, ..identity.clone() }), &arguments, naga::back::hlsl::ShaderModel::V6_7));
            assert_ne!(key, cache_validation_key(kind, Some(&identity), &arguments, naga::back::hlsl::ShaderModel::V6_6));
            let CompilerArguments::Dxc(values) = &arguments else { panic!("DXC arguments required") };
            for index in 0..values.len() {
                let mut changed = values.clone();
                changed[index] = windows::core::w!("changed");
                assert_ne!(key, cache_validation_key(kind, Some(&identity), &CompilerArguments::Dxc(changed), naga::back::hlsl::ShaderModel::V6_7));
            }
            let owned: Vec<Vec<u16>> = values.iter().map(|value| {
                let mut text = unsafe { value.as_wide() }.to_vec(); text.push(0); text
            }).collect();
            let copied = CompilerArguments::Dxc(owned.iter().map(|value| PCWSTR(value.as_ptr())).collect());
            assert_eq!(key, cache_validation_key(kind, Some(&identity), &copied, naga::back::hlsl::ShaderModel::V6_7));
        }
        assert_ne!(cache_validation_key("dynamic dxc", Some(&identity), &arguments, naga::back::hlsl::ShaderModel::V6_7),
            cache_validation_key("static dxc", Some(&identity), &arguments, naga::back::hlsl::ShaderModel::V6_7));
    }

    #[test]
    fn compiler_arguments_preserve_debug_workaround_and_sixteen_bit_flags() {
        for debug in [false, true] { for avoid_debug in [false, true] { for features in [wgt::Features::empty(), wgt::Features::SHADER_F16, wgt::Features::SHADER_I16] {
            let flags = if debug { wgt::InstanceFlags::DEBUG } else { wgt::InstanceFlags::empty() };
            let CompilerArguments::Dxc(arguments) = compile_arguments(true, flags, avoid_debug, features) else { panic!("DXC arguments required") };
            let mut expected = vec!["-HV", "2018", "-no-warnings", "-Ges"];
            if debug && !avoid_debug { expected.extend(["-Zi", "-Od"]); }
            if !features.is_empty() { expected.push("-enable-16bit-types"); }
            let actual: Vec<_> = arguments.iter().map(|value| String::from_utf16(unsafe { value.as_wide() }).unwrap()).collect();
            assert_eq!(actual, expected);
            let CompilerArguments::Fxc(fxc) = compile_arguments(false, flags, avoid_debug, features) else { panic!("FXC flags required") };
            assert_eq!(fxc, Fxc::D3DCOMPILE_ENABLE_STRICTNESS | if debug { Fxc::D3DCOMPILE_DEBUG | Fxc::D3DCOMPILE_SKIP_OPTIMIZATION } else { 0 });
        }}}
    }

    #[test]
    fn fxc_validation_covers_its_compile_flags() {
        let arguments = compile_arguments(false, wgt::InstanceFlags::empty(), false, wgt::Features::empty());
        let key = cache_validation_key("fxc", None, &arguments, naga::back::hlsl::ShaderModel::V5_1);
        assert_eq!(key, cache_validation_key("fxc", None, &arguments, naga::back::hlsl::ShaderModel::V5_1));
        let debug = compile_arguments(false, wgt::InstanceFlags::DEBUG, false, wgt::Features::empty());
        assert_ne!(key, cache_validation_key("fxc", None, &debug, naga::back::hlsl::ShaderModel::V5_1));
        let CompilerArguments::Fxc(flags) = arguments else { panic!("FXC flags required") };
        for flag in [Fxc::D3DCOMPILE_ENABLE_STRICTNESS, Fxc::D3DCOMPILE_DEBUG, Fxc::D3DCOMPILE_SKIP_OPTIMIZATION] {
            assert_ne!(key, cache_validation_key("fxc", None, &CompilerArguments::Fxc(flags ^ flag), naga::back::hlsl::ShaderModel::V5_1));
        }
    }
}
