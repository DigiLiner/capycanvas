fn leaf_outline(t: f32, shape: f32, lobes: f32) -> f32 {
    let base = pow(max(sin(3.14159265 * t), 0.0), mix(0.6, 1.1, shape)) * (1.2 - 0.4 * t);
    let lobed = 0.7 + 0.3 * abs(cos(3.14159265 * lobes * t));
    return base * mix(1.0, lobed, step(0.6, shape));
}

fn capy_leaf(p: vec2f, b: u32) -> f32 {
    let size = fx_parameter(b, 0u).x;
    let veins = fx_parameter(b, 1u).x;
    let seed = u32(fx_parameter(b, 2u).x);
    let g = tx_grid(vec2f(0.55 * size));
    let c = tx_cell(g, p);
    var layer = -1.0;
    var depth = 0.0;
    var leaf = vec4f(0.0);
    for (var y = -1; y <= 1; y += 1) {
        for (var x = -1; x <= 1; x += 1) {
            let n = c + vec2i(x, y);
            for (var k = 0u; k < 3u; k += 1u) {
                let r = tx_random(g, n, seed + 31u * k);
                let s = tx_random(g, n, seed + 31u * k + 1u);
                let blade_length = size * (0.6 + 0.6 * s.x);
                let a = TX_TAU * r.z;
                let rel = p - (vec2f(n) + r.xy) * g.cell;
                let u = dot(rel, vec2f(cos(a), sin(a)));
                let v = dot(rel, vec2f(-sin(a), cos(a)));
                let t = u / blade_length + 0.5;
                let half_width = blade_length * (0.12 + 0.16 * s.y) * leaf_outline(t, s.z, 3.0 + floor(3.0 * s.w));
                let teeth = 0.04 * half_width * abs(fract(t * blade_length / 1.5) - 0.5) * step(0.3, s.w);
                let edge = half_width - teeth - abs(v);
                if t <= 0.0 || t >= 1.0 || edge <= 0.0 {
                    continue;
                }
                depth += 1.0;
                if r.w <= layer {
                    continue;
                }
                layer = r.w;
                let spacing = veins * (0.8 + 0.4 * s.z) * blade_length / size;
                let chevron = abs(fract((u - 1.2 * abs(v)) / spacing) - 0.5) * spacing;
                leaf = vec4f(edge, abs(v), chevron, (1.0 - abs(v) / max(half_width, 0.1)));
            }
        }
    }
    let present = step(0.0, layer);
    let rim = smoothstep(0.0, 0.35, leaf.x);
    let midrib = exp(-leaf.y * leaf.y / 0.05) * present;
    let secondary = (1.0 - smoothstep(0.05, 0.2, leaf.z)) * present * smoothstep(0.1, 0.6, leaf.x);
    let areoles = tx_cells(p, vec2f(0.8), 0.9, seed + 7u);
    let veinlets = (1.0 - smoothstep(0.0, 0.1, 0.4 * (areoles.y - areoles.x))) * present;
    let dome = (1.0 - areoles.x * areoles.x) * present;
    let skin = tx_fbm(p, vec2f(0.15), 2u, seed + 8u);
    return 3.0 * rim * min(depth, 3.0) + 4.0 * leaf.w * present + 24.0 * midrib + 18.0 * secondary + 10.0 * veinlets + 10.0 * dome + 24.0 * skin;
}
