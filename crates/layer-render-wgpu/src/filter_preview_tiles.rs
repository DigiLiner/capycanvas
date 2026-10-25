use super::*;
use layer_core::color::{RgbSpace, SampleDepth, rgb, source::{SourceBuilder, SourceChannels, SourceImage, SourceInterpretation}};

pub(super) const EXTENT: [u32; 2] = [768, 128];
pub(super) const TILES: &[u8] = include_bytes!("../../../assets/filters/filter_preview_tiles.rgba");

pub(super) fn texture(r: &WgpuRasterizer) -> Image {
    let matrix = RgbSpace::Srgb.linear_transform(r.device.working_space());
    let decode: Vec<f64> = (0..=255).map(|v| RgbSpace::Srgb.decode(f64::from(v) / 255.)).collect();
    let mut bytes = Vec::with_capacity(TILES.len() * 4);
    for pixel in TILES.as_chunks::<4>().0 {
        let alpha = f64::from(pixel[3]) / 255.;
        let [red, green, blue] = rgb::apply(matrix, [pixel[0], pixel[1], pixel[2]].map(|v| decode[usize::from(v)]));
        for value in [red * alpha, green * alpha, blue * alpha, alpha] { bytes.extend((value as f32).to_ne_bytes()); }
    }
    let image = create_color_target(&r.device, EXTENT, "filter preview tiles");
    r.queue.write_texture(
        image.0.as_image_copy(),
        &bytes,
        wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(EXTENT[0] * 16), rows_per_image: Some(EXTENT[1]) },
        image.0.size(),
    );
    image
}

pub(super) fn source() -> Result<Arc<SourceImage>, GpuRasterError> {
    static SOURCE: std::sync::OnceLock<Arc<SourceImage>> = std::sync::OnceLock::new();
    if let Some(source) = SOURCE.get() { return Ok(source.clone()); }
    let interpretation = SourceInterpretation { channels: SourceChannels::Rgba, depth: SampleDepth::U8, profile: Default::default(), profile_assumed: false };
    let mut builder = SourceBuilder::new(EXTENT, interpretation, layer_core::ProjectLimits::default().asset_bytes as usize).map_err(GpuRasterError::Effect)?;
    for line in TILES.as_chunks::<{ EXTENT[0] as usize * 4 }>().0 {
        builder.push_row(line).map_err(GpuRasterError::Effect)?;
    }
    let source = Arc::new(builder.finish().map_err(GpuRasterError::Effect)?);
    Ok(SOURCE.get_or_init(|| source).clone())
}

#[cfg(test)]
#[path = "filter_preview_tiles_tests.rs"]
mod tests;
