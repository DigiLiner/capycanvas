use layer_host::{ffi,handles::Handle};
use std::ffi::CString;

pub(crate) fn text(value:CString)->u64 {ffi::text(value).map_or(0,Handle::raw)}

#[unsafe(no_mangle)]
pub extern "C" fn capy_string_length(value:u64)->usize {
    Handle::from_raw(value).and_then(ffi::text_length).unwrap_or(0)
}

/// # Safety
/// Nonempty output is writable and exclusive for length bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn capy_string_copy(value:u64,offset:usize,output:*mut u8,length:usize)->bool {
    Handle::from_raw(value).and_then(|handle|unsafe {ffi::copy_text(handle,offset,output,length)}).unwrap_or(false)
}

#[unsafe(no_mangle)]
pub extern "C" fn capy_string_free(value:u64) {let _=Handle::from_raw(value).and_then(ffi::free_text);}

#[cfg(test)]
pub(crate) fn take_text(value:u64)->CString {
    let mut bytes=vec![0;capy_string_length(value)];
    for offset in (0..bytes.len()).step_by(layer_core::package::MAX_RANGE_BYTES) {
        let length=(bytes.len()-offset).min(layer_core::package::MAX_RANGE_BYTES);
        assert!(unsafe {capy_string_copy(value,offset,bytes.as_mut_ptr().add(offset),length)});
    }
    assert!(unsafe {capy_string_copy(value,bytes.len(),std::ptr::null_mut(),0)});
    capy_string_free(value);CString::new(bytes).unwrap()
}
