fn gesso_coat(p: vec2f, spacing: f32, seed: u32) -> f32 {
    let q = tx_warp(p, 1.2, vec2f(24.0), seed);
    let bristles = 1.0 - smoothstep(0.0, 0.55 * spacing, tx_lines(q, spacing, 0.8, 0.45 * spacing, 0.12, seed + 1u).y);
    let clumps = 1.0 - smoothstep(0.0, 1.2 * spacing, tx_lines(q, 2.3 * spacing, 0.9, 0.7 * spacing, 0.2, seed + 2u).y);
    let load = 0.5 + 0.5 * tx_noise(q, vec2f(25.0, 60.0), seed + 3u);
    return (0.6 * bristles + 0.4 * clumps) * (0.5 + 0.5 * load);
}

fn capy_gesso(p: vec2f, b: u32) -> f32 {
    let brush = fx_parameter(b, 0u).x;
    let sanding = fx_parameter(b, 1u).x / 100.0;
    let down = fx_parameter(b, 2u).x / 100.0;
    let seed = u32(fx_parameter(b, 3u).x);
    let vertical = gesso_coat(p, brush, seed);
    let horizontal = gesso_coat(p.yx, brush, seed + 10u);
    let top = smoothstep(-0.08, 0.08, 0.9 * tx_fbm(p, vec2f(64.0), 2u, seed + 20u) + 2.0 * down - 1.0);
    let ridges = tx_smin(mix(horizontal, vertical, top), mix(1.2, 0.45, sanding), mix(0.1, 0.25, sanding));
    let chalk = tx_fbm(p, vec2f(0.25), 2u, seed + 21u);
    let grit = tx_cells(p, vec2f(0.12), 1.0, seed + 22u);
    return mix(70.0, 20.0, sanding) * ridges + 18.0 * chalk + 12.0 * tx_thread(grit.x, 0.6);
}
