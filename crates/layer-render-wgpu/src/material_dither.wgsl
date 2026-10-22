struct DitherSample {
    lightness: f32,
    weight: f32,
    index: u32,
}

const DITHER_OFFSETS = array<vec2<f32>, 7>(
    vec2<f32>(0.267261242, 0.000000000),
    vec2<f32>(-0.341335464, 0.312691246),
    vec2<f32>(0.052246864, -0.595326064),
    vec2<f32>(0.430231244, 0.561160473),
    vec2<f32>(-0.789527247, -0.139656254),
    vec2<f32>(0.747909132, -0.475758568),
    vec2<f32>(-0.250160969, 0.930586330)
);

fn dither_hash(value: u32) -> u32 {
    var v = value * 747796405u + 2891336453u;
    v = ((v >> ((v >> 28u) + 4u)) ^ v) * 277803737u;
    return (v >> 22u) ^ v;
}

fn dither_rank(world: vec2<f32>, seed: u32) -> f32 {
    var p = bitcast<vec2<u32>>(vec2<i32>(floor(world))) + vec2<u32>(seed & 255u, (seed >> 8u) & 255u);
    var rank = 0u;
    for (var bit = 0u; bit < 8u; bit += 1u) {
        let bx = p.x & 1u;
        let by = p.y & 1u;
        rank = (rank << 2u) | ((bx ^ by) << 1u) | by;
        p >>= vec2<u32>(1u);
    }
    return (f32(rank) + 0.5) / 65536.0;
}

fn dither_point(index: u32, world: vec2<f32>) -> vec2<f32> {
    let seed = dither_hash(u32(style.transport_b.z));
    let point = DITHER_OFFSETS[(index & 7u) - 1u];
    let swapped = select(point, point.yx, (seed & 65536u) != 0u);
    let offset = swapped * vec2<f32>(select(1.0, -1.0, (seed & 131072u) != 0u),
        select(1.0, -1.0, (seed & 262144u) != 0u));
    return world + offset * clamp(style.deformation.x * style.transport_b.y, 0.0, 112.0);
}

fn dither_paint(index: u32, world: vec2<f32>, original: vec4<f32>) -> vec4<f32> {
    if (index & 8u) == 0u { return original; }
    var paint = canvas_load(dither_point(index, world));
    if style.color.a > 0.5 && paint.a != original.a {
        paint = vec4<f32>(working_unassociate(paint) * original.a, original.a);
    }
    return paint;
}

fn dither_sample(index: u32, world: vec2<f32>, original: vec4<f32>, weight: f32) -> DitherSample {
    let point = dither_point(index, world);
    if any(point < vec2<f32>(0.0)) || any(point >= render_target.document_extent.xy) {
        return DitherSample(dot(original.rgb, WORKING_LUMA), 0.0, index);
    }
    var paint = canvas_load(point);
    if style.color.a > 0.5 {
        if !working_has_color(paint.a) || !working_has_color(original.a) { paint = original; }
        else if paint.a != original.a { paint = vec4<f32>(working_unassociate(paint) * original.a, original.a); }
    }
    return DitherSample(dot(paint.rgb, WORKING_LUMA), weight * brush_selection_at(point),
        index | select(0u, 8u, any(paint != original)));
}

fn dither_before(a: DitherSample, b: DitherSample, world: vec2<f32>, original: vec4<f32>) -> bool {
    if a.lightness != b.lightness { return a.lightness < b.lightness; }
    if ((a.index | b.index) & 8u) == 0u { return false; }
    let ca = dither_paint(a.index, world, original);
    let cb = dither_paint(b.index, world, original);
    if ca.r != cb.r { return ca.r < cb.r; }
    if ca.g != cb.g { return ca.g < cb.g; }
    if ca.b != cb.b { return ca.b < cb.b; }
    return ca.a < cb.a;
}

fn dither_order(a: ptr<function, DitherSample>, b: ptr<function, DitherSample>, world: vec2<f32>, original: vec4<f32>) {
    if dither_before(*b, *a, world, original) {
        let saved = *a;
        *a = *b;
        *b = saved;
    }
}

fn smudge_fragment(fragment_position: vec2<f32>, world: vec2<f32>, first: u32, count: u32) -> MaterialOutput {
    let pixel = vec2<i32>(floor(fragment_position));
    let original = canvas_load(world);
    var exposure = select(dry_coverage(pixel), 0.0, style.canvas_opacity.w > 0.5);
    let field = contact_field(world);
    for (var index = 0u; index < count; index += 1u) {
        let dab = dabs[first + index];
        let amount = contact_coverage_field(dab, world, field) * dab.flow
            * length(dab.motion) / max(min(dab.radii.x, dab.radii.y) * 2.0, 0.01)
            * max(style.deformation.y, 0.0);
        exposure = min(exposure + floor(min(amount, 16.0) * 1048576.0 + 0.5), 16777216.0);
    }
    let amount = select(1.0 - exp(-exposure / 1048576.0), 1.0, exposure >= 16777216.0)
        * clamp(style.canvas_opacity.z, 0.0, 1.0);
    if amount <= 0.0 { return MaterialOutput(original, vec4<f32>(exposure, 0.0, 0.0, 1.0), vec4<f32>(0.0)); }
    let weight = amount / 7.0;
    var s0 = DitherSample(dot(original.rgb, WORKING_LUMA), 0.0, 0u);
    var s1 = dither_sample(1u, world, original, weight);
    var s2 = dither_sample(2u, world, original, weight);
    var s3 = dither_sample(3u, world, original, weight);
    var s4 = dither_sample(4u, world, original, weight);
    var s5 = dither_sample(5u, world, original, weight);
    var s6 = dither_sample(6u, world, original, weight);
    var s7 = dither_sample(7u, world, original, weight);
    s0.weight = max(1.0 - (s1.weight + s2.weight + s3.weight + s4.weight + s5.weight + s6.weight + s7.weight), 0.0);
    if ((s1.index | s2.index | s3.index | s4.index | s5.index | s6.index | s7.index) & 8u) == 0u {
        return MaterialOutput(original, vec4<f32>(exposure, 0.0, 0.0, 1.0), vec4<f32>(0.0));
    }
    dither_order(&s0, &s1, world, original);
    dither_order(&s2, &s3, world, original);
    dither_order(&s4, &s5, world, original);
    dither_order(&s6, &s7, world, original);
    dither_order(&s0, &s2, world, original);
    dither_order(&s1, &s3, world, original);
    dither_order(&s4, &s6, world, original);
    dither_order(&s5, &s7, world, original);
    dither_order(&s1, &s2, world, original);
    dither_order(&s5, &s6, world, original);
    dither_order(&s0, &s4, world, original);
    dither_order(&s3, &s7, world, original);
    dither_order(&s1, &s5, world, original);
    dither_order(&s2, &s6, world, original);
    dither_order(&s1, &s4, world, original);
    dither_order(&s3, &s6, world, original);
    dither_order(&s2, &s4, world, original);
    dither_order(&s3, &s5, world, original);
    dither_order(&s3, &s4, world, original);
    let threshold = dither_rank(world, dither_hash(u32(style.transport_b.z)));
    var cumulative = 0.0;
    var chosen = 0u;
    var found = false;
    if s0.weight > 0.0 {
        if !found { chosen = s0.index; }
        cumulative += s0.weight;
        found = found || threshold < cumulative;
    }
    if s1.weight > 0.0 {
        if !found { chosen = s1.index; }
        cumulative += s1.weight;
        found = found || threshold < cumulative;
    }
    if s2.weight > 0.0 {
        if !found { chosen = s2.index; }
        cumulative += s2.weight;
        found = found || threshold < cumulative;
    }
    if s3.weight > 0.0 {
        if !found { chosen = s3.index; }
        cumulative += s3.weight;
        found = found || threshold < cumulative;
    }
    if s4.weight > 0.0 {
        if !found { chosen = s4.index; }
        cumulative += s4.weight;
        found = found || threshold < cumulative;
    }
    if s5.weight > 0.0 {
        if !found { chosen = s5.index; }
        cumulative += s5.weight;
        found = found || threshold < cumulative;
    }
    if s6.weight > 0.0 {
        if !found { chosen = s6.index; }
        cumulative += s6.weight;
        found = found || threshold < cumulative;
    }
    if s7.weight > 0.0 {
        if !found { chosen = s7.index; }
        cumulative += s7.weight;
        found = found || threshold < cumulative;
    }
    return MaterialOutput(dither_paint(chosen, world, original), vec4<f32>(exposure, 0.0, 0.0, 1.0), vec4<f32>(0.0));
}
