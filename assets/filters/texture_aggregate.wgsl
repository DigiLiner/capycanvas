fn aggregate_stones(p: vec2f, size: f32, angular: f32, seed: u32) -> f32 {
    let c = tx_cells(p, vec2f(size), 1.0, seed);
    let rounded = tx_thread(c.x, 0.34 + 0.2 * c.z);
    let chipped = smoothstep(0.04 + 0.45 * c.w, 0.5 + 0.45 * c.w, c.y - c.x) * (1.0 - 0.4 * c.x * c.x);
    return mix(sqrt(rounded), chipped, angular) * (0.85 + 0.15 * c.z);
}

fn capy_aggregate(p: vec2f, b: u32) -> f32 {
    let grain = fx_parameter(b, 0u).x;
    let embed = fx_parameter(b, 1u).x / 100.0;
    let seed = u32(fx_parameter(b, 2u).x);
    let angular = smoothstep(1.0, 4.0, grain);
    let medium = aggregate_stones(p, mix(0.62, 0.45, angular) * grain, angular, seed + 1u);
    let small = aggregate_stones(p, 0.2 * grain, angular, seed + 2u);
    let stones = max(max(aggregate_stones(p, grain, angular, seed), 0.85 * medium), 0.6 * angular * small);
    let skin = tx_fbm(p, vec2f(0.35), 3u, seed + 3u);
    let fines = tx_cells(p, vec2f(0.1), 1.0, seed + 4u);
    let surface = tx_smax(stones, mix(0.05, 0.9, embed) + 0.1 * (0.6 * tx_thread(fines.x, 0.55) + 0.4 * skin), 0.1);
    let chips = tx_fbm(p, vec2f(0.6), 3u, seed + 5u);
    return min(110.0 * pow(grain, 0.35), 70.0) * surface + mix(10.0, 14.0, angular) * skin + 26.0 * angular * chips + 8.0 * tx_thread(fines.x, 0.5);
}
