fn capy_newsprint(p: vec2f, b: u32) -> f32 {
    let flocs = fx_parameter(b, 0u).x;
    let fuzz = fx_parameter(b, 1u).x / 100.0;
    let seed = u32(fx_parameter(b, 2u).x);
    let q = tx_warp(p, 0.4 * flocs, vec2f(2.0 * flocs), seed);
    let cloud = tx_fbm(q, vec2f(1.8 * flocs, flocs), 3u, seed + 1u);
    let grain = tx_fbm(p, vec2f(0.14), 2u, seed + 2u);
    let fines = tx_cells(p, vec2f(0.09), 1.0, seed + 3u);
    let hairs = tx_fibers(p, 0.7, 0.08, 5.0 * fuzz, 0.0, 0.4, 0.6, seed + 4u);
    let shives = tx_fibers(p, 2.0, 0.18, 0.04, 0.0, 0.8, 0.1, seed + 5u);
    return 24.0 * cloud + 16.0 * grain + 10.0 * tx_thread(fines.x, 0.5) + 14.0 * hairs + 20.0 * shives;
}
