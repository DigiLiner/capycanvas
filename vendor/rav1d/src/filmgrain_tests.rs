use super::*;
use crate::include::common::bitdepth::{BitDepth8, BitDepth16};
use crate::src::align::{Align64, ArrayDefault};

fn chroma_edge<BD: BitDepth>(width: usize, bd: BD) {
    let shift = bd.bitdepth() - 8;
    let mut luma = Align64(std::array::from_fn::<_, 64, _>(|i| {
        ((((i * 17 + 23) & 255) as i32) << shift).as_::<BD::Pixel>()
    }));
    let mut source = Align64(std::array::from_fn::<_, 64, _>(|i| {
        ((((i * 7 + 61) & 255) as i32) << shift).as_::<BD::Pixel>()
    }));
    let original_luma = luma.0;
    let original_source = source.0;
    let mut padded = luma;
    padded[width] = padded[width - 1];
    let mut scaling = <BD::Scaling as ArrayDefault>::default();
    for (i, value) in scaling.as_mut().iter_mut().enumerate() {
        *value = ((i >> shift) / 2 + 1) as u8;
    }
    let grain = [[2i32.as_::<BD::Entry>(); GRAIN_WIDTH]; GRAIN_HEIGHT + 1];

    for from_luma in [false, true] {
        for subsampled_y in [false, true] {
            let mut output = Align64([BD::Pixel::from(0); 64]);
            let mut reference = output;
            let data = Rav1dFilmGrainData {
                seed: 12345,
                scaling_shift: 5,
                chroma_scaling_from_luma: from_luma,
                uv_luma_mult: [48, 32],
                uv_mult: [16, 32],
                uv_offset: [7, -9],
                overlap_flag: true,
                ..Default::default()
            };
            {
                // SAFETY: Each aligned buffer is live and accessed only through
                // its component until this scope ends; all buffers are disjoint.
                let [luma, source, padded, output, reference] = unsafe {
                    [&mut luma.0, &mut source.0, &mut padded.0, &mut output.0, &mut reference.0]
                        .map(|pixels| Rav1dPictureDataComponent::wrap_buf::<BD>(pixels, 64))
                };
                for (destination, input, luma_width) in [
                    (&output, &luma, width),
                    (&reference, &padded, width + 1),
                ] {
                    fguv_32x32xn_rust::<BD>(destination.with_offset::<BD>(), source.with_offset::<BD>(),
                        &data, (width + 1) >> 1, &scaling, &grain, 1, 0,
                        input.with_offset::<BD>(), luma_width, false, false, true, subsampled_y, bd);
                }
            }
            assert!(output.0 == reference.0, "odd luma edge differs from explicit padding");
            assert!(luma.0 == original_luma, "luma source allocation changed");
            assert!(source.0 == original_source, "chroma source allocation changed");
        }
    }
}

#[test]
fn odd_luma_edges_match_explicit_padding_without_source_writes() {
    for width in [1, 33] {
        chroma_edge::<BitDepth8>(width, BitDepth8::from_c(255));
        chroma_edge::<BitDepth16>(width, BitDepth16::from_c(1023));
        chroma_edge::<BitDepth16>(width, BitDepth16::from_c(4095));
    }
}
