// `region` is in the coarser level; it covers the finer level's texels that
// reduce into it.
struct Region { origin: vec2<u32>, size: vec2<u32> }
@group(0) @binding(0) var<uniform> region: Region;
@group(1) @binding(0) var finer: texture_2d<f32>;
@group(1) @binding(1) var coarser: texture_2d<f32>;
@group(1) @binding(2) var finer_half: texture_storage_2d<rgba16float, write>;
@group(1) @binding(3) var coarser_half: texture_storage_2d<rgba16float, write>;

fn half_bits(value: f32) -> u32 {
    let bits = pack2x16float(vec2(clamp(value, -65504., 65504.), 0.)) & 0xffffu;
    if (bits & 0x7fffu) == 0u && value != 0. { return select(0x8001u, 1u, value > 0.); }
    return bits;
}
fn half_value(bits: u32) -> f32 { return unpack2x16float(bits).x; }

fn half_keeping_clipping(color: vec4<f32>) -> vec4<f32> {
    let alpha = half_bits(color.a);
    var rgb: vec3<f32>;
    for (var i = 0; i < 3; i++) {
        var bits = half_bits(color[i]);
        if color[i] < color.a && alpha != 0u && (alpha & 0x8000u) == 0u && (bits & 0x8000u) == 0u && bits >= alpha { bits = alpha - 1u; }
        rgb[i] = half_value(bits);
    }
    return vec4(rgb, half_value(alpha));
}

@compute @workgroup_size(8, 8)
fn convert(@builtin(global_invocation_id) id: vec3<u32>) {
    let fine = region.size * 2u;
    if id.x >= fine.x { return; }
    if id.y < fine.y {
        let p = region.origin * 2u + id.xy;
        if all(p < textureDimensions(finer)) { textureStore(finer_half, p, half_keeping_clipping(textureLoad(finer, p, 0))); }
        return;
    }
    let paired_rows = vec2(id.x % region.size.x, (id.y - fine.y) * 2u + id.x / region.size.x);
    let q = region.origin + paired_rows;
    if paired_rows.y < region.size.y && all(q < textureDimensions(coarser)) { textureStore(coarser_half, q, half_keeping_clipping(textureLoad(coarser, q, 0))); }
}
