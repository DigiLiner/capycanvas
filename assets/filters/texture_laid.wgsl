fn capy_laid(p: vec2f, b: u32) -> f32 {
    let laid = fx_parameter(b, 0u).x;
    let chain = fx_parameter(b, 1u).x;
    let seed = u32(fx_parameter(b, 2u).x);
    let wires = tx_lines(p, laid, 0.04, laid, 0.04, seed);
    let chains = tx_lines(p.yx, chain, 0.06, 0.6, 0.15, seed + 1u);
    let shadow = exp(-pow(chains.y / 3.0, 2.0));
    let felt = tx_fbm(tx_warp(p, 0.3, vec2f(1.5), seed + 2u), vec2f(0.7), 3u, seed + 3u);
    let grain = tx_fbm(p, vec2f(0.15, 0.6), 2u, seed + 4u);
    return 22.0 * wires.x * (1.0 - chains.x) - 8.0 * chains.x + 5.0 * shadow + 14.0 * felt + 26.0 * grain;
}
