#![no_main]

libfuzzer_sys::fuzz_target!(|data: &[u8]| {
    let _ = layer_ui::ColorLibrary::import_file(data, "Palette");
});
