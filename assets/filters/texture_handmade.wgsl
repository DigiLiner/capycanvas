fn capy_handmade(p: vec2f, b: u32) -> f32 {
    let amount = fx_parameter(b, 0u).x / 100.0;
    let size = fx_parameter(b, 1u).x;
    let seed = u32(fx_parameter(b, 2u).x);
    let q = tx_warp(p, 1.2, vec2f(2.0), seed);
    let fine = tx_fbm(q, vec2f(0.7), 4u, seed + 7u);
    let felt = (tx_noise(q, vec2f(1.6), seed + 2u) + 0.9375 * fine) / 1.9375;
    let g = tx_grid(vec2f(3.0 * size));
    let c = tx_cell(g, p);
    var lump = 0.0;
    for (var y = -1; y <= 1; y += 1) {
        for (var x = -1; x <= 1; x += 1) {
            let n = c + vec2i(x, y);
            let r = tx_random(g, n, seed + 3u);
            if r.w > amount {
                continue;
            }
            let s = tx_random(g, n, seed + 4u);
            let shape = tx_ellipse(p - (vec2f(n) + r.xy) * g.cell, size * vec2f(0.5 + 0.6 * s.x, 0.25 + 0.2 * s.y), 6.283185 * r.z);
            lump = max(lump, (0.4 + 0.6 * s.z) * smoothstep(0.5 * size, -0.2 * size, shape));
        }
    }
    let fibers = tx_fibers(p, 2.5, 0.08, 3.0, 0.0, 0.0, 0.6, seed + 5u);
    let flocs = abs(fine - 0.2);
    return 40.0 * felt + 20.0 * lump + 20.0 * fibers + 92.0 * flocs;
}
