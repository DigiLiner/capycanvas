fn papyrus_edge(k: i32, cell: f32, count: i32, seed: u32) -> f32 {
    return (f32(k) + 0.8 * (tx_hash(vec2i((k % count + count) % count, 0), seed).x - 0.5)) * cell;
}

fn papyrus_layer(p: vec2f, width: f32, seed: u32) -> f32 {
    let g = tx_grid(vec2f(tx_period(), width));
    let count = g.count.y;
    let row = i32(floor(p.y / g.cell.y));
    var strip = row;
    var low = papyrus_edge(row, g.cell.y, count, seed);
    var high = papyrus_edge(row + 1, g.cell.y, count, seed);
    if p.y < low {
        strip = row - 1;
        high = low;
        low = papyrus_edge(row - 1, g.cell.y, count, seed);
    } else if p.y >= high {
        strip = row + 1;
        low = high;
        high = papyrus_edge(row + 2, g.cell.y, count, seed);
    }
    let r = tx_hash(vec2i((strip % count + count) % count, 1), seed);
    let gap = select(0.0, 1.0, r.w < 0.2);
    let edge = min(p.y - low, high - p.y);
    let wander = 1.2 * r.z + 0.5 * tx_noise(vec2f(p.x, 17.0 * r.x), vec2f(40.0, 1.0), seed + 1u);
    let q = vec2f(p.x, p.y + wander);
    let strength = 0.55 + 0.75 * r.y;
    let bundles = 1.0 - smoothstep(0.0, 0.3, tx_lines(q.yx, 1.2, 0.95, 0.3, 0.3, seed + 2u).y);
    let veins = 1.0 - smoothstep(0.0, 0.14, tx_lines(q.yx, 0.55, 0.95, 0.14, 0.15, seed + 3u).y);
    let fine = tx_lines(q.yx, 0.22, 0.9, 0.08, 0.08, seed + 4u).x;
    let grain = tx_fbm(q, vec2f(5.0, 0.1), 2u, seed + 5u);
    let fibers = strength * (0.36 * bundles + 0.22 * veins + 0.12 * fine) + 0.16 * grain;
    let seam = (1.0 - gap) * (1.0 - smoothstep(0.6, 2.2, edge)) - gap * (1.0 - smoothstep(0.15, 0.6, edge));
    return fibers + 0.22 * (r.x - 0.5) + 0.22 * seam;
}

fn capy_papyrus(p: vec2f, b: u32) -> f32 {
    let width = fx_parameter(b, 0u).x;
    let across = fx_parameter(b, 1u).x / 100.0;
    let down = fx_parameter(b, 2u).x / 100.0;
    let seed = u32(fx_parameter(b, 3u).x);
    var height = 0.0;
    if across > 0.0 {
        height += 70.0 * across * papyrus_layer(p, width, seed);
    }
    if down > 0.0 {
        height += 70.0 * down * papyrus_layer(p.yx, 1.2 * width, seed + 10u);
    }
    let specks = tx_fibers(p, 1.2, 0.15, 0.04, 0.0, 0.0, 0.5, seed + 20u);
    return height + 16.0 * specks;
}
