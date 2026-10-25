#![no_main]

libfuzzer_sys::fuzz_target!(|data: &[u8]| {
    let _ = layer_core::Lut3d::parse_cube(data);
});
