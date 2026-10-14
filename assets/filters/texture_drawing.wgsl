fn capy_drawing(p: vec2f, b: u32) -> f32 {
    let tooth = fx_parameter(b, 0u).x;
    let finish = fx_parameter(b, 1u).x / 100.0;
    let seed = u32(fx_parameter(b, 2u).x);
    let grain = tx_fbm(p, vec2f(1.3 * tooth, tooth), 4u, seed);
    let fibers = tx_fibers(p, 2.0, 0.03, 3.0, 0.0, 0.5, 0.2, seed + 1u);
    let calendered = tx_smin(grain + 0.6 * fibers, mix(0.8, -0.1, finish), 0.3);
    return mix(83.0, 20.0, finish) * calendered;
}
