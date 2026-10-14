fn capy_handmade(p: vec2f, b: u32) -> f32 {
    let amount = fx_parameter(b, 0u).x / 100.0;
    let size = fx_parameter(b, 1u).x;
    let seed = u32(fx_parameter(b, 2u).x);
    let felt = tx_fbm(tx_warp(p, 0.8, vec2f(3.0), seed), vec2f(1.6), 5u, seed + 2u);
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
    let fibers = tx_fibers(p, 2.5, 0.03, 2.0, 0.0, 0.0, 0.4, seed + 5u);
    return 34.0 * felt + 14.0 * lump + 10.0 * fibers;
}
