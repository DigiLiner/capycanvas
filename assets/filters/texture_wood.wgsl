fn capy_wood(p: vec2f, b: u32) -> f32 {
    let rings = fx_parameter(b, 0u).x;
    let weathering = fx_parameter(b, 1u).x / 100.0;
    let knots = fx_parameter(b, 2u).x / 100.0;
    let seed = u32(fx_parameter(b, 3u).x);
    let g = tx_grid(vec2f(60.0));
    let c = tx_cell(g, p);
    var push = 0.0;
    var nearest = 1e9;
    for (var y = -1; y <= 1; y += 1) {
        for (var x = -1; x <= 1; x += 1) {
            let n = c + vec2i(x, y);
            let r = tx_random(g, n, seed + 1u);
            if r.w > knots {
                continue;
            }
            let rel = p - (vec2f(n) + 0.2 + 0.6 * r.xy) * g.cell;
            let radii = vec2f(4.0 + 5.0 * r.z, 2.0 + 2.0 * r.z);
            let e = length(rel / radii);
            push += sign(rel.y) * 1.8 * radii.y * exp(-0.35 * e * e);
            nearest = min(nearest, e);
        }
    }
    let q = tx_warp(vec2f(p.x, p.y - push), 3.0, vec2f(70.0, 18.0), seed + 2u);
    let ring = tx_lines(q.yx, rings, 0.35, 0.5, 1.5, seed + 3u);
    let late = pow(1.0 - smoothstep(0.0, 0.5 * rings, ring.y), 1.5);
    let fibers = tx_fibers(q, 3.0, 0.1, 1.5, 0.0, 0.95, 0.05, seed + 4u);
    let streaks = tx_fbm(q, vec2f(1.2, 0.12), 2u, seed + 5u);
    let check = tx_lines(q.yx, 9.0, 1.0, 0.12, 0.6, seed + 6u).x * smoothstep(0.3, 0.6, tx_noise(q, vec2f(40.0, 8.0), seed + 7u));
    let rim = (nearest - 1.0) / 0.06;
    let knot = smoothstep(1.0, 0.75, nearest) * (0.6 + 0.4 * cos(TX_TAU * 2.5 * nearest)) - exp(-rim * rim);
    return (10.0 + 36.0 * weathering) * late + 26.0 * fibers + 22.0 * streaks - 35.0 * weathering * check + 25.0 * knot;
}
