//! Stroke recordings leave the owner as one raw copy; compression and file
//! writes run on a worker.
use super::*;
use std::io::Write;
use std::mem::ManuallyDrop;
use std::os::fd::FromRawFd;

#[unsafe(no_mangle)]
pub extern "C" fn capy_apple_stroke_recording_data(raw:u64)->u64 {
    let Some(mut app)=handles::app(raw) else {return 0;};
    app.perform(|app|app.host.session.stroke_recording().snapshot().map_err(|error|error.to_string()))
        .map_or(0,handles::store)
}
#[unsafe(no_mangle)]
pub extern "C" fn capy_bytes_length(raw:u64)->usize {handles::shared::<Vec<u8>>(raw).map_or(0,|bytes|bytes.len())}
/// # Safety
/// Nonempty output is writable and exclusive for length bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn capy_bytes_copy(raw:u64,offset:usize,output:*mut u8,length:usize)->bool {
    handles::shared::<Vec<u8>>(raw).is_some_and(|bytes|unsafe {layer_host::ffi::copy_bytes(&bytes,offset,output,length)})
}
#[unsafe(no_mangle)]
pub extern "C" fn capy_bytes_free(raw:u64) {handles::free::<Vec<u8>>(raw);}

/// # Safety
/// Worker only. Borrows count readable bytes and a descriptor open for writing.
/// Returns null on success or owned error JSON.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn capy_stroke_recording_write(bytes: *const u8, count: usize, output: i32) -> u64 {
    let result = (|| -> Result<(), String> {
        if count>layer_engine::recording::MAX_BYTES+4096 {return Err("Recording exceeds supported limits".into());}
        let raw = if count == 0 { &[][..] } else {
            if bytes.is_null() { return Err("Recording bytes are unavailable".into()); }
            unsafe { std::slice::from_raw_parts(bytes, count) }
        };
        let compressed = layer_engine::recording::compress(raw).map_err(|e| e.to_string())?;
        let mut file = ManuallyDrop::new(unsafe { std::fs::File::from_raw_fd(output) });
        file.write_all(&compressed).map_err(|e| e.to_string())
    })();
    match result {
        Ok(()) => 0,
        Err(error) => CString::new(serde_json::json!({"error": error}).to_string()).map_or(0,handles::text),
    }
}
