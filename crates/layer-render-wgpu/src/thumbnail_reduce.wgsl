struct Page { origin: vec2<u32>, extent: vec2<u32>, output: vec2<u32>, side: u32, clear: u32 }
@group(0) @binding(0) var<uniform> page: Page;
@group(0) @binding(1) var pixels: texture_2d<f32>;
@group(0) @binding(2) var reduced: texture_storage_2d<rgba32float, write>;
var<workgroup> sums: array<vec4<f32>, 64>;
var<workgroup> counts: array<u32, 64>;
@compute @workgroup_size(8, 8)
fn reduce(@builtin(workgroup_id) texel: vec3<u32>, @builtin(local_invocation_id) thread: vec3<u32>, @builtin(local_invocation_index) i: u32) {
    let start = texel.xy * page.side;
    var sum = vec4(0.);
    var count = 0u;
    for (var y = thread.y; y < page.side; y += 8u) {
        for (var x = thread.x; x < page.side; x += 8u) {
            let local = start + vec2(x, y);
            if all(local < vec2(256u)) && all(page.origin + local < page.extent) {
                if page.clear == 0u { sum += textureLoad(pixels, vec2<i32>(local), 0); }
                count += 1u;
            }
        }
    }
    sums[i] = sum;
    counts[i] = count;
    workgroupBarrier();
    for (var stride = 32u; stride > 0u; stride /= 2u) {
        if i < stride { sums[i] += sums[i + stride]; counts[i] += counts[i + stride]; }
        workgroupBarrier();
    }
    if i == 0u && counts[0] > 0u { textureStore(reduced, vec2<i32>(page.output + texel.xy), sums[0] / f32(counts[0])); }
}
