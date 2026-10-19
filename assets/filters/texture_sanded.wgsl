fn capy_sanded(p: vec2f, b: u32) -> f32 {
    let grit = fx_parameter(b, 0u).x;
    let seed = u32(fx_parameter(b, 1u).x);
    let grains = tx_cells(p, vec2f(1.5 * grit), 1.0, seed);
    let size = 0.6 + 0.8 * grains.z;
    let grain = size * tx_thread(grains.x, 0.6 * size);
    let binder = tx_fbm(p, vec2f(0.5), 3u, seed + 1u);
    return 40.0 * sqrt(max(grit / 0.035, 1.0)) * tx_smin(1.3 * grain, 0.9, 0.2) + 8.0 * binder;
}
