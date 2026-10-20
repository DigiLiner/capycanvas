struct Page { origin: vec2<u32>, extent: vec2<u32>, output: vec2<u32>, side: u32, clear: u32 }
@group(0) @binding(0) var<uniform> page: Page;
@group(0) @binding(1) var pixels: texture_2d<f32>;
@group(0) @binding(2) var reduced: texture_storage_2d<rgba32float, write>;
@group(0) @binding(3) var painted: texture_storage_2d<rgba16float, write>;
var<workgroup> sums: array<vec4<f32>, 64>;
var<workgroup> lows: array<vec2<u32>, 64>;
var<workgroup> highs: array<vec2<u32>, 64>;
@compute @workgroup_size(8, 8)
fn reduce(@builtin(workgroup_id) texel: vec3<u32>, @builtin(local_invocation_id) thread: vec3<u32>, @builtin(local_invocation_index) i: u32) {
    let start = texel.xy * page.side;
    var sum = vec4(0.);
    var low = vec2(page.side);
    var high = vec2(0u);
    for (var y = thread.y; y < page.side; y += 8u) {
        for (var x = thread.x; x < page.side; x += 8u) {
            let local = start + vec2(x, y);
            if page.clear == 0u && all(local < vec2(256u)) && all(page.origin + local < page.extent) {
                let value = textureLoad(pixels, vec2<i32>(local), 0);
                sum += value;
                if value.a > 0. { low = min(low, vec2(x, y)); high = max(high, vec2(x, y) + 1u); }
            }
        }
    }
    sums[i] = sum;
    lows[i] = low;
    highs[i] = high;
    workgroupBarrier();
    for (var stride = 32u; stride > 0u; stride /= 2u) {
        if i < stride {
            sums[i] += sums[i + stride];
            lows[i] = min(lows[i], lows[i + stride]);
            highs[i] = max(highs[i], highs[i + stride]);
        }
        workgroupBarrier();
    }
    if i == 0u {
        let filled = all(highs[0] > lows[0]);
        let size = select(vec2(1u), highs[0] - lows[0], filled);
        textureStore(reduced, vec2<i32>(page.output + texel.xy), select(vec4(0.), sums[0] / f32(size.x * size.y), filled));
        textureStore(painted, vec2<i32>(page.output + texel.xy), select(vec4(0.), vec4<f32>(vec4(lows[0], highs[0])), filled));
    }
}
