use libjpeg_turbo_rs::{Decoder, Encoder, JpegError, PixelFormat, Subsampling};
use std::{alloc::{GlobalAlloc, Layout, System}, cell::Cell, ptr};

struct Allocator;
thread_local! { static REFUSE: Cell<(usize, usize)> = const { Cell::new((0, 0)) }; }
fn refused(size: usize) -> bool {
    REFUSE.try_with(|target| {
        let (wanted, remaining) = target.get();
        if size == 0 || wanted != size { return false; }
        if remaining > 1 { target.set((wanted, remaining - 1)); false } else { target.set((0, 0)); true }
    }).unwrap_or(false)
}
unsafe impl GlobalAlloc for Allocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        if refused(layout.size()) { ptr::null_mut() } else { unsafe { System.alloc(layout) } }
    }
    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        if refused(layout.size()) { ptr::null_mut() } else { unsafe { System.alloc_zeroed(layout) } }
    }
    unsafe fn realloc(&self, pointer: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        if refused(size) { ptr::null_mut() } else { unsafe { System.realloc(pointer, layout, size) } }
    }
    unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) { unsafe { System.dealloc(pointer, layout) } }
}
#[global_allocator]
static ALLOCATOR: Allocator = Allocator;

fn jpeg(progressive: bool, profile: Option<&[u8]>) -> Vec<u8> {
    let pixels = (0..64*64*3).map(|i| (i % 251) as u8).collect::<Vec<_>>();
    let encoder = Encoder::new(&pixels, 64, 64, PixelFormat::Rgb).subsampling(Subsampling::S444).progressive(progressive);
    match profile { Some(profile) => encoder.icc_profile(profile).encode(), None => encoder.encode() }.unwrap()
}
fn refusal(size: usize, run: impl FnOnce() -> Result<(), JpegError>) { refusal_nth(size, 1, run); }
fn refusal_nth(size: usize, nth: usize, run: impl FnOnce() -> Result<(), JpegError>) {
    struct Reset;
    impl Drop for Reset { fn drop(&mut self) { REFUSE.with(|target| target.set((0, 0))); } }
    REFUSE.with(|target| target.set((size, nth)));
    let reset = Reset;
    let result = run();
    let remaining = REFUSE.with(Cell::get);
    drop(reset);
    assert_eq!(remaining, (0, 0), "the requested allocation must be reached");
    assert!(matches!(result, Err(JpegError::AllocationFailed { bytes, .. }) if bytes == size as u64), "{result:?}");
}
#[test]
fn output_allocation_refusal_preserves_the_decoder() {
    let bytes = jpeg(false, None);
    let decoder = Decoder::new(&bytes).unwrap();
    refusal(64*64*3, || decoder.decode_image().map(|_| ()));
    assert_eq!(decoder.decode_image().unwrap().data.len(), 64*64*3);
}

#[test]
fn estimated_decode_memory_matches_the_enforced_limit() {
    for progressive in [false, true] {
        let bytes = jpeg(progressive, None);
        let mut decoder = Decoder::new(&bytes).unwrap();
        for format in [PixelFormat::Grayscale, PixelFormat::Rgb, PixelFormat::Rgba] {
            decoder.set_output_format(format);
            let estimate = decoder.estimated_decode_memory();
            decoder.set_max_memory(estimate as usize);
            decoder.output_buffer_size().unwrap();
            decoder.set_max_memory(estimate as usize - 1);
            assert!(matches!(decoder.output_buffer_size(), Err(JpegError::LimitExceeded {
                what: "estimated decode memory", actual, limit,
            }) if actual == estimate && limit == estimate - 1));
        }
    }
}
#[test]
fn component_allocation_refusal_preserves_the_decoder() {
    let bytes = jpeg(false, None);
    let decoder = Decoder::new(&bytes).unwrap();
    refusal(64*64, || decoder.decode_image().map(|_| ()));
    assert_eq!(decoder.decode_image().unwrap().data.len(), 64*64*3);
}
#[test]
fn progressive_coefficient_allocation_refusal_preserves_the_decoder() {
    let bytes = jpeg(true, None);
    let decoder = Decoder::new(&bytes).unwrap();
    refusal(8*8*128, || decoder.decode_image().map(|_| ()));
    assert_eq!(decoder.decode_image().unwrap().data.len(), 64*64*3);
}
#[test]
fn metadata_allocation_refusal_is_an_error() {
    let bytes = jpeg(false, Some(&[17; 257]));
    refusal(257, || Decoder::new(&bytes).map(|_| ()));
    assert_eq!(Decoder::new(&bytes).unwrap().decode_image().unwrap().icc_profile.as_deref(), Some(&[17;257][..]));
}

fn marker(bytes: &mut Vec<u8>, code: u8, payload: &[u8]) {
    let length = u16::try_from(payload.len() + 2).unwrap();
    bytes.extend_from_slice(&[0xff, code]);
    bytes.extend_from_slice(&length.to_be_bytes());
    bytes.extend_from_slice(payload);
}
fn extended_xmp() -> Vec<u8> {
    let original = jpeg(false, None);
    let mut bytes = original[..2].to_vec();
    marker(&mut bytes, 0xe1, b"http://ns.adobe.com/xap/1.0/\0<x/>");
    let mut extension = b"http://ns.adobe.com/xmp/extension/\0".to_vec();
    extension.extend_from_slice(&[b'A'; 32]);
    extension.extend_from_slice(&64u32.to_be_bytes());
    extension.extend_from_slice(&0u32.to_be_bytes());
    extension.extend_from_slice(&[b'x'; 64]);
    marker(&mut bytes, 0xe1, &extension);
    bytes.extend_from_slice(&original[2..]);
    bytes
}
#[test]
fn extended_xmp_reassembly_refusal_is_an_error() {
    let bytes = extended_xmp();
    refusal_nth(64, 2, || Decoder::new(&bytes).map(|_| ()));
    assert_eq!(Decoder::new(&bytes).unwrap().decode_image().unwrap().xmp_data.unwrap().len(), 68);
}
#[test]
fn extended_xmp_append_refusal_is_an_error() {
    let bytes = extended_xmp();
    refusal(68, || Decoder::new(&bytes).map(|_| ()));
}
#[test]
fn metadata_limit_applies_before_the_initial_parse_allocates_payload() {
    let bytes = extended_xmp();
    let result = Decoder::new_with_limits(&bytes, libjpeg_turbo_rs::DecodeLimits { max_memory: Some(48), ..Default::default() });
    assert!(matches!(result, Err(JpegError::LimitExceeded { what: "JPEG metadata allocation", .. })));
}
#[test]
fn lossy_comment_allocation_refusal_is_an_error() {
    let original = jpeg(false, None);
    let mut bytes = original[..2].to_vec();
    let text = [0xff, b'a', 0xe2, 0x82, 0xac];
    marker(&mut bytes, 0xfe, &text);
    bytes.extend_from_slice(&original[2..]);
    refusal(7, || Decoder::new(&bytes).map(|_| ()));
    assert_eq!(Decoder::new(&bytes).unwrap().decode_image().unwrap().comment.as_deref(), Some(String::from_utf8_lossy(&text).as_ref()));
}

#[test]
fn metadata_limit_counts_utf8_replacement_bytes() {
    let original = jpeg(false, None);
    let mut bytes = original[..2].to_vec();
    marker(&mut bytes, 0xfe, &[0xff; 3]);
    bytes.extend_from_slice(&original[2..]);
    let result = Decoder::new_with_limits(&bytes, libjpeg_turbo_rs::DecodeLimits { max_memory: Some(3), ..Default::default() });
    assert!(matches!(result, Err(JpegError::LimitExceeded { what: "JPEG metadata allocation", actual: 9, limit: 3 })));
}

#[test]
fn raw_component_allocation_refusal_is_an_error() {
    let bytes = jpeg(false, None);
    let decoder = Decoder::new(&bytes).unwrap();
    refusal(64*64, || decoder.decode_raw().map(|_| ()));
}
#[test]
fn raw_progressive_coefficient_allocation_refusal_is_an_error() {
    let bytes = jpeg(true, None);
    let decoder = Decoder::new(&bytes).unwrap();
    refusal(8*8*128, || decoder.decode_raw().map(|_| ()));
}

#[test]
fn raw_stop_on_warning_rejects_recovered_entropy() {
    let original = jpeg(false, None);
    let sos = original.windows(2).position(|bytes| bytes == [0xff, 0xda]).unwrap();
    let entropy = sos + 2 + usize::from(u16::from_be_bytes([original[sos + 2], original[sos + 3]]));
    let truncated = original[..entropy + 1].to_vec();
    let mut decoder = Decoder::new(&truncated).unwrap();
    decoder.set_lenient(true);
    assert!(decoder.decode_raw().is_ok());
    let mut decoder = Decoder::new(&truncated).unwrap();
    decoder.set_lenient(true);
    decoder.set_stop_on_warning(true);
    assert!(matches!(decoder.decode_raw(), Err(JpegError::CorruptData(_))));
}
