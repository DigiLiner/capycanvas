const TX_TAU: f32 = 6.283185307;

struct TxGrid {
    count: vec2i,
    cell: vec2f,
}

fn tx_hash(c: vec2i, salt: u32) -> vec4f {
    var v = vec4u(bitcast<vec2u>(c), salt, 0x9e3779b9u) * 1664525u + 1013904223u;
    v.x += v.y * v.w; v.y += v.z * v.x; v.z += v.x * v.y; v.w += v.y * v.z;
    v ^= v >> vec4u(16u);
    v.x += v.y * v.w; v.y += v.z * v.x; v.z += v.x * v.y; v.w += v.y * v.z;
    return vec4f(v >> vec4u(8u)) / 16777216.0;
}

fn tx_grid(size: vec2f) -> TxGrid {
    let count = max(vec2i(round(tx_period() / max(size, vec2f(1e-4)))), vec2i(1));
    return TxGrid(count, tx_period() / vec2f(count));
}

fn tx_cell(g: TxGrid, p: vec2f) -> vec2i {
    return vec2i(floor(p / g.cell));
}

fn tx_random(g: TxGrid, c: vec2i, salt: u32) -> vec4f {
    return tx_hash(((c % g.count) + g.count) % g.count, salt);
}

fn tx_gradient(g: TxGrid, c: vec2i, f: vec2f, seed: u32) -> f32 {
    let a = TX_TAU * tx_random(g, c, seed).x;
    return dot(vec2f(cos(a), sin(a)), f);
}

fn tx_noise(p: vec2f, size: vec2f, seed: u32) -> f32 {
    let g = tx_grid(size);
    let q = p / g.cell;
    let c = vec2i(floor(q));
    let f = q - floor(q);
    let u = f * f * f * (f * (f * 6.0 - 15.0) + 10.0);
    let a = tx_gradient(g, c, f, seed);
    let b = tx_gradient(g, c + vec2i(1, 0), f - vec2f(1.0, 0.0), seed);
    let d = tx_gradient(g, c + vec2i(0, 1), f - vec2f(0.0, 1.0), seed);
    let e = tx_gradient(g, c + vec2i(1, 1), f - vec2f(1.0, 1.0), seed);
    return 1.4 * mix(mix(a, b, u.x), mix(d, e, u.x), u.y);
}

fn tx_fbm(p: vec2f, size: vec2f, octaves: u32, seed: u32) -> f32 {
    var sum = 0.0;
    var amplitude = 1.0;
    var total = 0.0;
    var s = size;
    for (var i = 0u; i < min(octaves, 8u); i += 1u) {
        sum += amplitude * tx_noise(p, s, seed + 7919u * i);
        total += amplitude;
        amplitude *= 0.5;
        s *= 0.5;
    }
    return sum / total;
}

fn tx_warp(p: vec2f, amount: f32, size: vec2f, seed: u32) -> vec2f {
    return p + amount * vec2f(tx_fbm(p, size, 3u, seed), tx_fbm(p, size, 3u, seed + 101u));
}

fn tx_cells(p: vec2f, size: vec2f, jitter: f32, seed: u32) -> vec4f {
    let g = tx_grid(size);
    let c = tx_cell(g, p);
    let unit = min(g.cell.x, g.cell.y);
    var f1 = 1e9;
    var f2 = 1e9;
    var id = vec2f(0.0);
    for (var y = -1; y <= 1; y += 1) {
        for (var x = -1; x <= 1; x += 1) {
            let n = c + vec2i(x, y);
            let r = tx_random(g, n, seed);
            let site = (vec2f(n) + 0.5 + jitter * (r.xy - 0.5)) * g.cell;
            let d = length(p - site) / unit;
            if d < f1 {
                f2 = f1;
                f1 = d;
                id = r.zw;
            } else if d < f2 {
                f2 = d;
            }
        }
    }
    return vec4f(f1, f2, id);
}

fn tx_lines(p: vec2f, spacing: f32, spread: f32, width: f32, wobble: f32, seed: u32) -> vec2f {
    let g = tx_grid(vec2f(spacing, tx_period()));
    let i = tx_cell(g, p).x;
    let drawn = max(width, tx_texel());
    var nearest = 1e9;
    for (var k = -1; k <= 1; k += 1) {
        let c = vec2i(i + k, 0);
        let r = tx_random(g, c, seed);
        let lane = (f32(c.x) + 0.5) * g.cell.x;
        let bend = wobble * tx_noise(vec2f(lane, p.y), vec2f(g.cell.x, 8.0 * g.cell.x), seed + 1u);
        let x = lane + spread * g.cell.x * (r.x - 0.5) + bend;
        nearest = min(nearest, abs(p.x - x));
    }
    let ridge = (width / drawn) * smoothstep(0.5 * drawn, 0.0, nearest);
    return vec2f(ridge, nearest);
}

fn tx_thread(across: f32, radius: f32) -> f32 {
    let x = clamp(across / max(radius, 1e-4), -1.0, 1.0);
    return sqrt(1.0 - x * x);
}

fn tx_weave(p: vec2f, pitch: vec2f, irregularity: f32, seed: u32) -> f32 {
    let pairs = tx_grid(2.0 * pitch);
    let threads = 2 * pairs.count;
    let step = 0.5 * pairs.cell;
    let c = vec2i(floor(p / step));
    var warp = 0.0;
    var weft = 0.0;
    for (var k = -1; k <= 1; k += 1) {
        let i = ((c.x + k) % threads.x + threads.x) % threads.x;
        let ri = tx_hash(vec2i(i, 0), seed);
        let slub_i = 1.0 + 0.6 * irregularity * max(tx_noise(vec2f(ri.x * 64.0, p.y), vec2f(step.x, 6.0), seed + 2u), 0.0);
        let x = (f32(c.x + k) + 0.5 + 0.25 * irregularity * (ri.y - 0.5)) * step.x;
        let rx = 0.5 * step.x * (0.8 + 0.3 * irregularity * (ri.z - 0.5)) * slub_i;
        let lift_i = 0.5 + 0.5 * cos(3.14159265 * (p.y / step.y - 0.5 + f32(i)));
        warp = max(warp, tx_thread(p.x - x, rx) * (0.55 + 0.45 * lift_i));

        let j = ((c.y + k) % threads.y + threads.y) % threads.y;
        let rj = tx_hash(vec2i(0, j), seed + 1u);
        let slub_j = 1.0 + 0.6 * irregularity * max(tx_noise(vec2f(p.x, rj.x * 64.0), vec2f(6.0, step.y), seed + 3u), 0.0);
        let y = (f32(c.y + k) + 0.5 + 0.4 * irregularity * (rj.y - 0.5)) * step.y;
        let ry = 0.5 * step.y * (0.8 + 0.4 * irregularity * (rj.z - 0.5)) * slub_j;
        let lift_j = 0.5 - 0.5 * cos(3.14159265 * (p.x / step.x - 0.5 + f32(j)));
        weft = max(weft, tx_thread(p.y - y, ry) * (0.55 + 0.45 * lift_j));
    }
    return max(warp, weft);
}

fn tx_fibers(p: vec2f, len: f32, width: f32, density: f32, angle: f32, alignment: f32, curl: f32, seed: u32) -> f32 {
    let g = tx_grid(vec2f(0.75 * len));
    let c = tx_cell(g, p);
    let per_cell = density * g.cell.x * g.cell.y;
    let drawn = max(width, tx_texel());
    let amplitude = width / drawn;
    var height = 0.0;
    for (var y = -1; y <= 1; y += 1) {
        for (var x = -1; x <= 1; x += 1) {
            let n = c + vec2i(x, y);
            let count = min(u32(per_cell + tx_random(g, n, seed).x), 64u);
            for (var k = 0u; k < count; k += 1u) {
                let r = tx_random(g, n, seed + 977u * (k + 1u));
                let center = (vec2f(n) + r.xy) * g.cell;
                let l = len * (0.5 + r.w);
                let rel = p - center;
                let reach = 0.5 * l + drawn;
                if dot(rel, rel) > reach * reach {
                    continue;
                }
                let a = angle + 3.14159265 * (1.0 - alignment) * (r.z - 0.5);
                let along = dot(rel, vec2f(cos(a), sin(a)));
                let across = dot(rel, vec2f(-sin(a), cos(a)));
                let s = tx_random(g, n, seed + 977u * (k + 1u) + 1u);
                let t = clamp(along / l, -0.5, 0.5);
                let bow = curl * l * (s.x - 0.5) * (1.0 - 4.0 * t * t);
                let d = length(vec2f(along - t * l, across - bow));
                let dive = 0.55 + 0.45 * sin(TX_TAU * (s.y * 3.0 + 1.0) * t + TX_TAU * s.z);
                height = max(height, amplitude * dive * tx_thread(d, 0.5 * drawn));
            }
        }
    }
    return height;
}

fn tx_ellipse(q: vec2f, radii: vec2f, angle: f32) -> f32 {
    let r = vec2f(cos(angle) * q.x + sin(angle) * q.y, cos(angle) * q.y - sin(angle) * q.x);
    return (length(r / radii) - 1.0) * min(radii.x, radii.y);
}

fn tx_smin(a: f32, b: f32, k: f32) -> f32 {
    let h = clamp(0.5 + 0.5 * (b - a) / k, 0.0, 1.0);
    return mix(b, a, h) - k * h * (1.0 - h);
}

fn tx_smax(a: f32, b: f32, k: f32) -> f32 {
    return -tx_smin(-a, -b, k);
}

fn tx_disc(i: u32, n: u32) -> vec3f {
    let r = sqrt((f32(i) + 0.5) / f32(n));
    let a = 2.39996323 * f32(i);
    return vec3f(r * cos(a), r * sin(a), 1.0 / f32(n));
}
