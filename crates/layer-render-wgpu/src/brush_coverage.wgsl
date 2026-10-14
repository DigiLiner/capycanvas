// Shared contact-shape code for direct textured deposition and destination-
// aware materials. Both shader bodies provide `brush_sampler` and the brush
// textures consumed here.

fn tip_coverage(
    analytic: bool,
    texture: texture_2d<f32>,
    local: vec2<f32>,
    sign: vec2<f32>,
    hardness: f32,
    min_radius: f32,
) -> f32 {
    if dot(local, local) >= 1.0 {
        return 0.0;
    }
    if analytic {
        return analytic_coverage(local, hardness, min_radius);
    }
    let uv = local * sign * 0.5 + vec2<f32>(0.5);
    // Brush assets have one mip level. Explicit LOD is valid after per-pixel
    // silhouette rejection and in ordered destination-contact loops on WebGPU.
    return textureSampleLevel(texture, brush_sampler, uv, 0.0).r;
}
