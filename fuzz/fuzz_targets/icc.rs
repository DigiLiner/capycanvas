#![no_main]
use layer_core::color::{ColorProfile, ProfileChannels, SampleDepth, source::{SourceChannels, SourceInterpretation}};

libfuzzer_sys::fuzz_target!(|data: &[u8]| {
    if data.len() > layer_color::MAX_ICC_BYTES { return; }
    let profile = ColorProfile::Icc(data.to_vec().into());
    let Ok(channels) = layer_color::profile_channels(&profile) else { return };
    let _ = layer_color::profile_description_optional(&profile);
    let channels = match channels {
        ProfileChannels::Rgb => SourceChannels::Rgb,
        ProfileChannels::Gray => SourceChannels::Gray,
        ProfileChannels::Cmyk => SourceChannels::Cmyk,
    };
    for depth in [SampleDepth::U8, SampleDepth::U16] {
        let source = SourceInterpretation { channels, depth, profile: profile.clone(), profile_assumed: false };
        let _ = layer_color::WorkingDecoder::new(&source, layer_core::color::RgbSpace::Srgb, Default::default());
    }
});
