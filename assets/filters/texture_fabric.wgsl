fn fabric_threads(p: vec2f, pitch: f32, irregularity: f32, width: f32, seed: u32) -> vec4f {
    let pairs = tx_grid(vec2f(2.0 * pitch, tx_period()));
    let step = 0.5 * pairs.cell.x;
    let count = 2 * pairs.count.x;
    let i0 = i32(floor(p.x / step));
    let twist = tx_grid(vec2f(0.3 * step)).cell.y;
    var best = vec4f(0.0);
    for (var k = -1; k <= 1; k += 1) {
        let i = i0 + k;
        let r = tx_hash(vec2i((i % count + count) % count, 0), seed);
        let x = (f32(i) + 0.5 + 0.35 * irregularity * (r.y - 0.5)) * step;
        let slub = 1.0 + irregularity * (0.5 * max(tx_noise(vec2f(64.0 * r.z, p.y), vec2f(step, 12.0), seed + 2u), 0.0) + 0.2 * tx_noise(vec2f(64.0 * r.x, p.y), vec2f(step, 2.0 * step), seed + 3u));
        let radius = 0.5 * step * width * (0.85 + 0.3 * irregularity * (r.w - 0.5)) * slub;
        let across = p.x - x;
        let h = tx_thread(across, radius);
        if h > best.x {
            let ply = abs(fract((p.y + 0.45 * across) / twist + r.x) - 0.5);
            best = vec4f(h, f32(i), across / step, ply);
        }
    }
    return best;
}

fn capy_fabric(p: vec2f, b: u32) -> f32 {
    let threads = fx_parameter(b, 0u).x;
    let irregularity = fx_parameter(b, 1u).x / 100.0;
    let openness = fx_parameter(b, 2u).x / 100.0;
    let priming = fx_parameter(b, 3u).x / 100.0;
    let seed = u32(fx_parameter(b, 4u).x);
    let pitch = 10.0 / threads;
    let q = tx_warp(p, 0.35 * irregularity * pitch, vec2f(18.0), seed + 7u);
    let width = 1.0 - 0.6 * openness;
    let warp = fabric_threads(q, pitch, irregularity, 0.85 * width, seed);
    let weft = fabric_threads(q.yx, 1.12 * pitch, irregularity, 1.1 * width, seed + 5u);
    let warp_over = 0.5 + 0.5 * cos(3.14159265 * (warp.y + weft.y + weft.z));
    let weft_over = 0.5 - 0.5 * cos(3.14159265 * (warp.y + weft.y + warp.z));
    let up = warp.x * (0.6 + 0.4 * warp_over);
    let across = weft.x * (0.45 + 0.55 * weft_over);
    let top = select(weft, warp, up > across);
    let twist = 1.0 - smoothstep(0.08, 0.3, top.w);
    let along = select(tx_fbm(p, vec2f(1.6, 0.09), 2u, seed + 10u), tx_fbm(p, vec2f(0.09, 1.6), 2u, seed + 11u), up > across);
    let cloth = tx_smax(max(up, across), 0.6 * priming, 0.15);
    let hairs = tx_fibers(p, 3.0, 0.08, 0.8, 0.0, 0.0, 0.8, seed + 12u);
    let fiber = (1.0 - priming) * (18.0 * twist + 14.0 * along);
    return (40.0 + 25.0 * (1.0 - priming)) * cloth + fiber * smoothstep(0.0, 0.3, cloth) + 14.0 * (1.0 - priming) * hairs;
}
