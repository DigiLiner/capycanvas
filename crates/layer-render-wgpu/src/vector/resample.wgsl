struct Resample { linear: vec4<f32>, offset: vec4<f32>, origins: vec4<f32>, size: vec4<u32> }
@group(0) @binding(0) var<uniform> resample: Resample;
@group(0) @binding(1) var source: texture_2d<f32>;
@group(0) @binding(2) var output: texture_storage_2d<rgba32float, write>;

fn texel(q: vec2<i32>) -> vec4<f32> {
    if any(q < vec2<i32>(0)) || any(q >= vec2<i32>(textureDimensions(source))) { return vec4<f32>(0.); }
    return textureLoad(source, q, 0);
}
@compute @workgroup_size(8, 8)
fn main(@builtin(global_invocation_id) id: vec3<u32>) {
    if any(id.xy >= resample.size.xy) { return; }
    let p = resample.origins.xy + (vec2<f32>(id.xy) + .5) * resample.offset.z;
    let mapped = resample.linear.xy * p.x + resample.linear.zw * p.y + resample.offset.xy;
    let u = (mapped - resample.origins.zw) / resample.offset.w - .5;
    let base = vec2<i32>(floor(u));
    let f = u - floor(u);
    let top = mix(texel(base), texel(base + vec2<i32>(1, 0)), f.x);
    let bottom = mix(texel(base + vec2<i32>(0, 1)), texel(base + vec2<i32>(1, 1)), f.x);
    textureStore(output, vec2<i32>(id.xy + resample.size.zw), mix(top, bottom, f.y));
}
