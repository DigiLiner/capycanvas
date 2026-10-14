struct PaperBake {
    inverse: vec4<f32>,
    origin: vec4<f32>,
    texels: vec4<f32>,
    paper: vec4<f32>,
    mode: vec4<f32>,
}
@group(0) @binding(0) var<uniform> bake_parameters: PaperBake;
@group(0) @binding(1) var<storage, read> effect_data: array<vec4<f32>>;

fn fx_parameter(b: u32, i: u32) -> vec4<f32> {
    return effect_data[b + 1u + i];
}
fn tx_period() -> f32 {
    return bake_parameters.paper.x;
}
fn tx_texel() -> f32 {
    return bake_parameters.paper.z;
}

@vertex
fn vertex_main(@builtin(vertex_index) i: u32) -> @builtin(position) vec4<f32> {
    let uv = vec2<f32>(f32((i << 1u) & 2u), f32(i & 2u));
    return vec4<f32>(uv * 2.0 - 1.0, 0.0, 1.0);
}

fn paper_hash(c: vec2<i32>, salt: u32) -> vec4<f32> {
    var v = vec4<u32>(bitcast<vec2<u32>>(c), salt, 0x85ebca6bu) * 1664525u + 1013904223u;
    v.x += v.y * v.w; v.y += v.z * v.x; v.z += v.x * v.y; v.w += v.y * v.z;
    v ^= v >> vec4<u32>(16u);
    v.x += v.y * v.w; v.y += v.z * v.x; v.z += v.x * v.y; v.w += v.y * v.z;
    return vec4<f32>(v >> vec4<u32>(8u)) / 16777216.0;
}

fn paper_position(pixel: vec2<f32>) -> vec2<f32> {
    let q = pixel - bake_parameters.origin.xy;
    let m = bake_parameters.inverse;
    let mm = vec2<f32>(m.x * q.x + m.z * q.y, m.y * q.x + m.w * q.y);
    let period = bake_parameters.paper.xy;
    return mm - period * floor(mm / period);
}

fn paper_jitter(texel: vec2<i32>) -> vec2<f32> {
    let r = paper_hash(texel, 0x6a09e667u);
    let radius = sqrt(-2.0 * log(max(r.x, 1e-7)));
    return radius * vec2<f32>(cos(6.2831853 * r.y), sin(6.2831853 * r.y));
}

@fragment
fn paper_probe(@builtin(position) position: vec4<f32>) -> @location(0) vec4<f32> {
    let r = paper_hash(vec2<i32>(floor(position.xy)), 0x51ed270bu);
    return vec4<f32>(paper_height(r.xy * bake_parameters.paper.xy), 0.0, 0.0, 1.0);
}

@fragment
fn paper_bake(@builtin(position) position: vec4<f32>) -> @location(0) vec4<f32> {
    let texel = floor(position.xy);
    let pixels = bake_parameters.origin.zw / bake_parameters.texels.xy;
    let pixel = (texel + 0.5 + bake_parameters.paper.w * paper_jitter(vec2<i32>(texel))) * pixels;
    let h = paper_height(paper_position(pixel));
    let range = bake_parameters.texels.zw;
    return vec4<f32>(clamp((h - range.x) / (range.y - range.x), 0.0, 1.0), 0.0, 0.0, 1.0);
}
