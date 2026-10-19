fn quilt_band(d: f32, width: f32) -> f32 {
    return 1.0 - smoothstep(0.5 * width, 0.5 * width + 0.35, d);
}

fn quilt_rose(q: vec2f) -> vec2f {
    let r = length(q);
    let a = atan2(q.y, q.x);
    let pitch = 1.5;
    let turn = (r - 0.6 - pitch * a / TX_TAU) / pitch;
    let spiral = quilt_band(abs(turn - round(turn)) * pitch, 0.7) * step(r, 4.2);
    let petal = 5.6 + 1.0 * abs(sin(2.5 * a));
    let petals = quilt_band(abs(r - petal), 1.1) * step(0.12, abs(fract(2.5 * a / 3.14159265) - 0.5));
    let leaf_at = q - vec2f(-7.5, 6.0);
    let leaf = tx_ellipse(leaf_at, vec2f(3.6, 1.7), -0.7);
    let blade = smoothstep(0.3, -0.6, leaf) * (1.0 - 0.6 * quilt_band(abs(dot(leaf_at, vec2f(0.76, -0.64))), 0.3));
    let outline = quilt_band(abs(leaf), 0.5);
    let clear = 1.0 - smoothstep(9.0, 10.5, min(r, length(leaf_at) + 5.0));
    return vec2f(max(max(spiral, petals), max(0.85 * blade, outline)), clear);
}

fn capy_quilt(p: vec2f, b: u32) -> f32 {
    let cell = fx_parameter(b, 0u).x;
    let seed = u32(fx_parameter(b, 1u).x);
    let g = tx_grid(vec2f(cell));
    let c = tx_cell(g, p);
    var motif = 0.0;
    var clear = 0.0;
    for (var y = -1; y <= 1; y += 1) {
        for (var x = -1; x <= 1; x += 1) {
            let n = c + vec2i(x, y);
            let r = tx_random(g, n, seed);
            let a = TX_TAU * r.z;
            let rel = p - (vec2f(n) + 0.3 + 0.4 * r.xy) * g.cell;
            let rose = quilt_rose(vec2f(dot(rel, vec2f(cos(a), sin(a))), dot(rel, vec2f(-sin(a), cos(a)))));
            motif = max(motif, rose.x);
            clear = max(clear, rose.y);
        }
    }
    let pebbles = tx_cells(tx_warp(p, 0.3, vec2f(2.0), seed + 1u), vec2f(1.7), 1.0, seed + 2u);
    let pebble = smoothstep(0.02, 0.32, pebbles.y - pebbles.x) * (0.75 + 0.25 * tx_thread(pebbles.x, 1.0)) * (1.0 - clear);
    let fuzz = tx_fbm(p, vec2f(0.35), 3u, seed + 3u);
    let fibers = tx_fibers(p, 1.0, 0.08, 2.0, 0.0, 0.0, 0.6, seed + 4u);
    return 40.0 * max(motif, 0.6 * pebble + 0.3 * clear) + 14.0 * fuzz + 8.0 * fibers;
}
