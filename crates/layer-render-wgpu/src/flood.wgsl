// Four-connected, fixed-seed color region: union-find within bounded windows
// and over their border pixels, then packed coverage/bounds reduction.
struct Params {
    extent_seed: vec4<u32>, options: vec4<f32>, input: vec4<u32>,
    window: vec4<u32>, grid: vec4<u32>, offsets: vec4<u32>,
}
// Bounds/count occupy eight extra words after the coverage, sharing one
// allocation/binding and keeping the portable four-storage-buffer limit.
struct Coverage { rect: vec4<u32>, info: vec4<u32>, values: array<atomic<u32>> }
@group(0) @binding(1) var<uniform> params: Params;
@group(0) @binding(2) var<storage, read_write> parents: array<atomic<u32>>;
@group(0) @binding(3) var<storage, read_write> coverage: Coverage;

const NONE = 0xffffffffu;
fn summary_index(field: u32) -> u32 {
    return ((params.extent_seed.x+7u)/8u)*params.extent_seed.y + field;
}
fn origin() -> vec2<u32> { return params.window.xy; }
fn size() -> vec2<u32> { return params.window.zw; }
fn window_index() -> u32 { return params.grid.x; }
fn side() -> u32 { return params.grid.w; }
fn slots() -> u32 { return params.offsets.x; }
fn flags() -> u32 { return params.offsets.y; }
fn marks() -> u32 { return params.offsets.z; }
fn seed() -> u32 { return params.offsets.w; }
fn slot_base(window: u32) -> u32 { return window * 4u * side(); }
fn perimeter(size: vec2<u32>) -> u32 {
    if size.y == 1u { return size.x; }
    if size.x == 1u { return size.y; }
    return 2u*size.x + 2u*(size.y - 2u);
}
fn local_index(p: vec2<u32>, size: vec2<u32>) -> u32 {
    if p.y == 0u { return p.x; }
    if p.y + 1u == size.y { return size.x + p.x; }
    if p.x == 0u { return 2u*size.x + p.y - 1u; }
    if p.x + 1u == size.x { return 2u*size.x + size.y - 2u + p.y - 1u; }
    return perimeter(size) + (p.y - 1u)*(size.x - 2u) + p.x - 1u;
}
fn border_pixel(s: u32, size: vec2<u32>) -> vec2<u32> {
    if size.y == 1u || s < size.x { return vec2<u32>(s, 0u); }
    if s < 2u*size.x { return vec2<u32>(s - size.x, size.y - 1u); }
    let k = s - 2u*size.x;
    if k < size.y - 2u { return vec2<u32>(0u, k + 1u); }
    return vec2<u32>(size.x - 1u, k - (size.y - 2u) + 1u);
}
fn window_at(q: vec2<u32>) -> vec3<u32> {
    let cell = q / side();
    return vec3<u32>(cell * side(), cell.y * params.grid.y + cell.x);
}
fn window_extent(at: vec2<u32>) -> vec2<u32> { return min(vec2<u32>(side()), params.extent_seed.xy - at); }
fn border_slot(q: vec2<u32>) -> u32 {
    let w = window_at(q);
    return slots() + slot_base(w.z) + local_index(q - w.xy, window_extent(w.xy));
}
fn bit_set(base: u32, index: u32) -> bool {
    return (atomicLoad(&parents[base + index/32u]) & (1u << (index%32u))) != 0u;
}
fn set_bit(base: u32, index: u32) { atomicOr(&parents[base + index/32u], 1u << (index%32u)); }
var<workgroup> local_parent: array<atomic<u32>, 256>;
var<workgroup> local_minimum: array<atomic<u32>, 256>;

fn local_root(start: u32) -> u32 {
    var node = start;
    loop {
        let parent = atomicLoad(&local_parent[node]);
        if parent == node { return node; }
        let grand = atomicLoad(&local_parent[parent]);
        atomicMin(&local_parent[node], grand);
        node = grand;
    }
    return node;
}
fn join_local(first: u32, second: u32) {
    var a = first;
    var b = second;
    loop {
        a = local_root(a);
        b = local_root(b);
        if a == b { return; }
        let low = min(a,b);
        let high = max(a,b);
        let previous = atomicMin(&local_parent[high], low);
        if previous == high { return; }
        a = low;
        b = previous;
    }
}

@compute @workgroup_size(16,16)
fn initialize(@builtin(global_invocation_id) id: vec3<u32>,
              @builtin(local_invocation_index) lane: u32) {
    let inside = all(id.xy < size());
    var eligible = false;
    if inside { eligible = mask_bit(vec2<i32>(origin() + id.xy)); }
    atomicStore(&local_parent[lane], select(NONE, lane, eligible));
    atomicStore(&local_minimum[lane], NONE);
    workgroupBarrier();
    if eligible {
        if lane % 16u != 0u && atomicLoad(&local_parent[lane-1u]) != NONE { join_local(lane,lane-1u); }
        if lane >= 16u && atomicLoad(&local_parent[lane-16u]) != NONE { join_local(lane,lane-16u); }
    }
    workgroupBarrier();
    var index = NONE;
    if eligible {
        index = local_index(id.xy, size());
        atomicMin(&local_minimum[local_root(lane)], index);
    }
    workgroupBarrier();
    if inside {
        var label = NONE;
        if eligible { label = atomicLoad(&local_minimum[local_root(lane)]); }
        atomicStore(&parents[local_index(id.xy, size())], label);
    }
}

fn root(start: u32) -> u32 {
    var node = start;
    loop {
        let parent = atomicLoad(&parents[node]);
        if parent == node || parent == NONE { return parent; }
        let grand = atomicLoad(&parents[parent]);
        atomicMin(&parents[node], grand);
        node = grand;
    }
    return node;
}
fn join(first: u32, second: u32) {
    var a = first;
    var b = second;
    loop {
        a = root(a);
        b = root(b);
        if a == b || a == NONE || b == NONE { return; }
        let low = min(a,b);
        let high = max(a,b);
        let previous = atomicMin(&parents[high], low);
        if previous == high { return; }
        a = low;
        b = previous;
    }
}

@compute @workgroup_size(64)
fn merge(@builtin(global_invocation_id) id: vec3<u32>, @builtin(num_workgroups) groups: vec3<u32>) {
    let s = size();
    let horizontal = s.x * ((s.y-1u)/16u);
    let vertical = s.y * ((s.x-1u)/16u);
    let i = id.x + id.y * groups.x * 64u;
    if i < horizontal {
        let p = vec2<u32>(i%s.x, (i/s.x+1u)*16u);
        join(local_index(p, s), local_index(p - vec2<u32>(0u, 1u), s));
    } else if i < horizontal+vertical {
        let j = i-horizontal;
        let p = vec2<u32>((j/s.y+1u)*16u, j%s.y);
        join(local_index(p, s), local_index(p - vec2<u32>(1u, 0u), s));
    }
}

@compute @workgroup_size(64)
fn export_border(@builtin(global_invocation_id) id: vec3<u32>, @builtin(num_workgroups) groups: vec3<u32>) {
    let s = id.x + id.y * groups.x * 64u;
    let window = window_index();
    let base = slots() + slot_base(window);
    let border = perimeter(size());
    if s == 0u && all(params.extent_seed.zw >= origin()) && all(params.extent_seed.zw < origin() + size()) {
        let start = root(local_index(params.extent_seed.zw - origin(), size()));
        atomicStore(&parents[seed()], window);
        atomicStore(&parents[seed() + 1u], start);
        atomicStore(&parents[seed() + 3u], u32(start < border));
    }
    if s >= border { return; }
    let label = root(s);
    if label == NONE { atomicStore(&parents[base + s], NONE); return; }
    atomicStore(&parents[base + s], base + label);
    if params.input.z != 0u && bit_set(marks(), label) { set_bit(flags(), slot_base(window) + label); }
}

@compute @workgroup_size(64)
fn seam_merge(@builtin(global_invocation_id) id: vec3<u32>, @builtin(num_workgroups) groups: vec3<u32>) {
    let i = id.x + id.y * groups.x * 64u;
    let extent = params.extent_seed.xy;
    let columns = params.grid.y;
    let vertical = (columns - 1u) * extent.y;
    let horizontal = (params.grid.z - 1u) * extent.x;
    var q = vec2<u32>(0u);
    var step = vec2<u32>(0u);
    if i < vertical { q = vec2<u32>((i / extent.y + 1u) * side(), i % extent.y); step = vec2<u32>(1u, 0u); }
    else if i < vertical + horizontal {
        let j = i - vertical;
        q = vec2<u32>(j % extent.x, (j / extent.x + 1u) * side());
        step = vec2<u32>(0u, 1u);
    } else { return; }
    join(border_slot(q), border_slot(q - step));
}

@compute @workgroup_size(64)
fn propagate_flags(@builtin(global_invocation_id) id: vec3<u32>, @builtin(num_workgroups) groups: vec3<u32>) {
    let slot = id.x + id.y * groups.x * 64u;
    if slot >= params.grid.y * params.grid.z * 4u * side() || !bit_set(flags(), slot) { return; }
    let label = root(slots() + slot);
    if label != NONE { set_bit(flags(), label - slots()); }
}

@compute @workgroup_size(1)
fn resolve_seed() {
    if atomicLoad(&parents[seed() + 3u]) == 0u { return; }
    let start = slots() + slot_base(atomicLoad(&parents[seed()])) + atomicLoad(&parents[seed() + 1u]);
    atomicStore(&parents[seed() + 2u], root(start));
}

var<workgroup> lows: array<vec2<u32>,64>;
var<workgroup> highs: array<vec2<u32>,64>;
var<workgroup> counts: array<u32,64>;

@compute @workgroup_size(64)
fn pack(@builtin(global_invocation_id) id: vec3<u32>, @builtin(local_invocation_index) lane: u32,
        @builtin(num_workgroups) groups: vec3<u32>) {
    let extent = params.extent_seed.xy;
    let stride = (extent.x+7u)/8u;
    let word = id.x + id.y * groups.x * 64u;
    let y = word/stride;
    let x = (word%stride)*8u;
    var packed = 0u;
    var low = extent;
    var high = vec2<u32>(0);
    var count = 0u;
    if y < extent.y {
        for (var i = 0u; i < 8u && x+i < extent.x; i++) {
            let value = refined_coverage(vec2<i32>(i32(x+i),i32(y)));
            let samples = u32(round(value * brush_selection_at(vec2<f32>(f32(x+i)+.5, f32(y)+.5))*4.));
            if samples != 0u {
                packed |= samples << (i*4u);
                low = min(low, vec2<u32>(x+i,y));
                high = max(high, vec2<u32>(x+i+1u,y+1u));
                count++;
            }
        }
        atomicStore(&coverage.values[word], packed);
    }
    lows[lane] = low;
    highs[lane] = high;
    counts[lane] = count;
    workgroupBarrier();
    for (var step = 32u; step > 0u; step /= 2u) {
        if lane < step {
            lows[lane] = min(lows[lane], lows[lane+step]);
            highs[lane] = max(highs[lane], highs[lane+step]);
            counts[lane] += counts[lane+step];
        }
        workgroupBarrier();
    }
    if lane == 0u && counts[0] != 0u {
        atomicMin(&coverage.values[summary_index(0u)], lows[0].x);
        atomicMin(&coverage.values[summary_index(1u)], lows[0].y);
        atomicMax(&coverage.values[summary_index(2u)], highs[0].x);
        atomicMax(&coverage.values[summary_index(3u)], highs[0].y);
        atomicAdd(&coverage.values[summary_index(4u)], counts[0]);
    }
}
