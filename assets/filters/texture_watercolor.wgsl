fn capy_watercolor(p: vec2f, b: u32) -> f32 {
    let felt = fx_parameter(b, 0u).x;
    let pressing = fx_parameter(b, 1u).x / 100.0;
    let seed = u32(fx_parameter(b, 2u).x);
    let q = tx_warp(p, 0.6 * felt, vec2f(3.0 * felt), seed);
    let cells = tx_cells(q, vec2f(felt), 0.9, seed + 1u);
    let mounds = 1.0 - cells.x * cells.x;
    let tooth = tx_fbm(p, vec2f(0.3 * felt), 3u, seed + 2u);
    let pressed = tx_smin(mounds, mix(1.2, 0.4, pressing), mix(0.25, 0.6, pressing)) + 0.35 * tooth;
    let grain = tx_fbm(p, vec2f(0.25), 3u, seed + 3u);
    let fibers = tx_fibers(p, 1.0, 0.02, 2.0, 0.0, 0.0, 0.4, seed + 4u);
    return mix(120.0, 10.0, pressing) * pressed + mix(16.0, 4.0, pressing) * grain + 10.0 * fibers;
}
