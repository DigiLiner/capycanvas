fn capy_honeycomb(p: vec2f, b: u32) -> f32 {
    let pitch = fx_parameter(b, 0u).x;
    let seed = u32(fx_parameter(b, 1u).x);
    let g = tx_grid(vec2f(pitch, 1.7 * pitch));
    let q = tx_warp(p, 0.1 * pitch, vec2f(5.0 * pitch), seed);
    let c = tx_cell(g, q);
    var dimple = 0.0;
    for (var y = -1; y <= 1; y += 1) {
        for (var x = -1; x <= 1; x += 1) {
            let n = c + vec2i(x, y);
            let r = tx_random(g, n, seed + 1u);
            for (var k = 0; k < 2; k += 1) {
                let site = vec2f(0.25 + 0.5 * f32(k)) + 0.08 * (select(r.xy, r.zw, k == 1) - 0.5);
                let d = tx_ellipse(q - (vec2f(n) + site) * g.cell, pitch * vec2f(0.56, 0.44), 0.15 * (r.z - 0.5));
                dimple = max(dimple, smoothstep(0.0, -0.3 * pitch, d));
            }
        }
    }
    let felt = tx_fibers(p, pitch, 0.05, 6.0, 0.0, 0.0, 0.6, seed + 2u);
    return 40.0 * (1.0 - dimple) + 25.0 * felt;
}
