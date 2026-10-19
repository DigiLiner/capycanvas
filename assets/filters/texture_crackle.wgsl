fn crackle_edges(p: vec2f, size: f32, seed: u32) -> vec3f {
    let g = tx_grid(vec2f(size));
    let c = tx_cell(g, p);
    var d1 = 1e9;
    var d2 = 1e9;
    var s1 = vec2f(0.0);
    var s2 = vec2f(0.0);
    var r1 = 0.0;
    var r2 = 0.0;
    for (var y = -1; y <= 1; y += 1) {
        for (var x = -1; x <= 1; x += 1) {
            let n = c + vec2i(x, y);
            let r = tx_random(g, n, seed);
            let site = (vec2f(n) + 0.5 + 0.9 * (r.xy - 0.5)) * g.cell;
            let d = length(p - site);
            if d < d1 {
                d2 = d1;
                s2 = s1;
                r2 = r1;
                d1 = d;
                s1 = site;
                r1 = r.z;
            } else if d < d2 {
                d2 = d;
                s2 = site;
                r2 = r.z;
            }
        }
    }
    let edge = abs(dot(p - 0.5 * (s1 + s2), normalize(s2 - s1)));
    return vec3f(edge, fract(97.31 * (r1 + r2)), r1);
}

fn crackle_line(e: vec3f, width: f32, folds: f32) -> f32 {
    return select(-1.0, 1.0, e.y < folds) * (1.0 - smoothstep(0.0, width, e.x));
}

fn capy_crackle(p: vec2f, b: u32) -> f32 {
    let size = fx_parameter(b, 0u).x;
    let gap = fx_parameter(b, 1u).x;
    let folds = fx_parameter(b, 2u).x / 100.0;
    let seed = u32(fx_parameter(b, 3u).x);
    let q = tx_warp(p, 0.08 * size, vec2f(0.6 * size), seed);
    let width = max(gap, 0.35);
    let primary = crackle_edges(q, size, seed + 1u);
    let secondary = crackle_edges(q, 0.38 * size, seed + 2u);
    let lines = 36.0 * crackle_line(primary, width, folds) + 20.0 * crackle_line(secondary, 0.6 * width, folds) * step(0.3, secondary.y);
    let facets = 10.0 * (primary.z - 0.5) + 6.0 * (secondary.z - 0.5);
    let rim = 14.0 * (1.0 - folds) * exp(-primary.x / (0.08 * size));
    let silt = tx_fbm(p, vec2f(0.35), 3u, seed + 4u);
    let grit = tx_cells(p, vec2f(0.1), 1.0, seed + 5u);
    return lines + facets + rim + 24.0 * silt + 12.0 * tx_thread(grit.x, 0.5);
}
