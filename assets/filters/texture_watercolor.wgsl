fn capy_watercolor(p: vec2f, b: u32) -> f32 {
    let felt = fx_parameter(b, 0u).x;
    let pressing = fx_parameter(b, 1u).x / 100.0;
    let seed = u32(fx_parameter(b, 2u).x);
    let q = tx_warp(p, 0.6 * felt, vec2f(3.0 * felt), seed);
    let cells = tx_cells(q, vec2f(felt), 0.9, seed + 1u);
    let mounds = 1.0 - cells.x * cells.x;
    let tooth = tx_fbm(p, vec2f(0.3 * felt), 3u, seed + 2u);
    let pressed = tx_smin(mounds, mix(1.2, 0.4, pressing), mix(0.25, 0.6, pressing)) + mix(0.6, 0.35, pressing) * tooth;
    let cotton = tx_smin(tx_fibers(p, 2.0, 0.08, 2.0, 0.0, 0.0, 0.5, seed + 3u), mix(1.0, 0.2, pressing), 0.15);
    let wool = tx_fibers(q, 3.0, 0.1, 1.0, 0.0, 0.0, 1.0, seed + 4u);
    return mix(120.0, 10.0, pressing) * pressed + mix(40.0, 44.0, pressing) * cotton - mix(30.0, 16.0, pressing) * wool;
}
