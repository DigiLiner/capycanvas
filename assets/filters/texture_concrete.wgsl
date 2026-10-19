fn concrete_pores(p: vec2f, size: f32, share: f32, seed: u32) -> f32 {
    let g = tx_grid(vec2f(size));
    let c = tx_cell(g, p);
    var pore = 0.0;
    for (var y = -1; y <= 1; y += 1) {
        for (var x = -1; x <= 1; x += 1) {
            let n = c + vec2i(x, y);
            let r = tx_random(g, n, seed);
            if r.w > share {
                continue;
            }
            let radius = 0.5 * size * (0.25 + 0.75 * r.z * r.z);
            let d = length(p - (vec2f(n) + r.xy) * g.cell) / radius;
            pore = max(pore, (1.0 - smoothstep(0.82, 1.0, d)) * (1.0 - 0.25 * d * d));
        }
    }
    return pore;
}

fn capy_concrete(p: vec2f, b: u32) -> f32 {
    let grain = fx_parameter(b, 0u).x;
    let pores = fx_parameter(b, 1u).x / 100.0;
    let size = fx_parameter(b, 2u).x;
    let scratches = fx_parameter(b, 3u).x / 100.0;
    let seed = u32(fx_parameter(b, 4u).x);
    let paste = tx_fbm(p, vec2f(2.5 * grain), 3u, seed);
    let sand = tx_cells(p, vec2f(grain), 1.0, seed + 1u);
    let surface = 0.55 * paste + 0.45 * tx_thread(sand.x, 0.55 + 0.2 * sand.z);
    let pitted = max(concrete_pores(p, 3.0 * size, pores, seed + 2u), 0.8 * concrete_pores(p, size, 0.15 * pores, seed + 3u));
    let scratched = tx_fibers(p, 30.0, 0.08, 0.004 * scratches, 0.0, 0.0, 0.05, seed + 4u);
    return min(24.0 + 200.0 * grain, 56.0) * surface - 30.0 * pitted - 26.0 * scratched;
}
