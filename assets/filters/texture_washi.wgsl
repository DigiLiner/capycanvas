fn capy_washi(p: vec2f, b: u32) -> f32 {
    let fiber = fx_parameter(b, 0u).x;
    let density = fx_parameter(b, 1u).x;
    let strands = fx_parameter(b, 2u).x / 100.0;
    let seed = u32(fx_parameter(b, 3u).x);
    let pulp = tx_fbm(p, vec2f(0.4), 3u, seed + 2u);
    let fibers = tx_fibers(p, 0.6 * fiber, 0.1, 1.5 * density, 0.0, 0.5, 0.4, seed + 3u);
    let swirl = tx_fibers(tx_warp(p, 6.0, vec2f(30.0), seed + 4u), 4.0 * fiber, 0.3, 0.01 * strands, 0.0, 0.0, 1.0, seed + 5u);
    return 22.0 * pulp + 18.0 * fibers + 16.0 * swirl;
}
