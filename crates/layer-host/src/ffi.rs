use crate::handles::{self,Error,Handle};
use std::{ffi::CString,sync::Arc};

pub struct Text(CString);

pub fn text(value:CString)->Result<Handle,Error> {handles::insert_shared(Arc::new(Text(value)))}
pub fn text_length(handle:Handle)->Result<usize,Error> {Ok(handles::shared::<Text>(handle)?.0.as_bytes().len())}
pub fn free_text(handle:Handle)->Result<(),Error> {handles::release::<Text>(handle)}

/// # Safety
/// Nonempty output is writable and exclusive for length bytes and does not overlap bytes.
pub unsafe fn copy_bytes(bytes:&[u8],offset:usize,output:*mut u8,length:usize)->bool {
    if length>layer_core::package::MAX_RANGE_BYTES || offset.checked_add(length).is_none_or(|end|end>bytes.len()) {return false;}
    if length==0 {return true;}
    if output.is_null() {return false;}
    unsafe {std::ptr::copy_nonoverlapping(bytes.as_ptr().add(offset),output,length);}
    true
}

/// # Safety
/// Nonempty output is writable and exclusive for length bytes.
pub unsafe fn copy_text(handle:Handle,offset:usize,output:*mut u8,length:usize)->Result<bool,Error> {
    let text=handles::shared::<Text>(handle)?;
    Ok(unsafe {copy_bytes(text.0.as_bytes(),offset,output,length)})
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn copied_text_survives_release_and_rejects_stale_wrong_kind_and_bad_ranges() {
        let handle=text(CString::new("日本語🎨").unwrap()).unwrap();
        let length=text_length(handle).unwrap();let mut output=vec![93;length];
        for (offset,count) in [(0,length+1),(usize::MAX,1),(length,1)] {
            assert!(!unsafe {copy_text(handle,offset,output.as_mut_ptr(),count)}.unwrap());
            assert!(output.iter().all(|byte|*byte==93));
        }
        assert!(!unsafe {copy_text(handle,0,std::ptr::null_mut(),length)}.unwrap());
        assert!(unsafe {copy_text(handle,length,std::ptr::null_mut(),0)}.unwrap());
        assert!(unsafe {copy_text(handle,0,output.as_mut_ptr(),length)}.unwrap());
        let wrong=handles::insert(7usize).unwrap();
        assert_eq!(text_length(wrong),Err(Error::WrongType));assert_eq!(free_text(wrong),Err(Error::WrongType));
        assert_eq!(handles::take::<usize>(wrong),Ok(7));
        free_text(handle).unwrap();assert_eq!(text_length(handle),Err(Error::Invalid));
        assert_eq!(unsafe {copy_text(handle,0,output.as_mut_ptr(),0)},Err(Error::Invalid));
        assert_eq!(free_text(handle),Err(Error::Invalid));
        assert_eq!(std::str::from_utf8(&output).unwrap(),"日本語🎨");
    }

    #[test]
    fn range_copy_admits_only_bounded_initialized_bytes() {
        let bytes=vec![11;layer_core::package::MAX_RANGE_BYTES+1];let mut output=[93;4];
        assert!(!unsafe {copy_bytes(&bytes,0,output.as_mut_ptr(),bytes.len())});
        assert_eq!(output,[93;4]);
        assert!(unsafe {copy_bytes(&bytes,bytes.len()-4,output.as_mut_ptr(),4)});
        assert_eq!(output,[11;4]);
    }
}
