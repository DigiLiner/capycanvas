#![no_main]
use layer_color::photo::DecodeLimits;
use layer_ui::{ImportIntent, PhotoOpenPolicy};
use std::sync::atomic::AtomicBool;

libfuzzer_sys::fuzz_target!(|data: &[u8]| {
    let limits = DecodeLimits { source_bytes: 64 << 20, codec_bytes: 256 << 20, dimension: 4096 };
    let names = layer_core::DocumentNames { paint: "Photo".into(), paper: "Paper".into() };
    let _ = layer_ui::read_import(std::io::Cursor::new(data), ImportIntent::Open, PhotoOpenPolicy::default(),
        names, Default::default(), limits, &AtomicBool::new(false));
});
