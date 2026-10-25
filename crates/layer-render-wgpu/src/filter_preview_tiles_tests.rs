use super::{TILES, EXTENT as TILES_EXTENT};
use std::f64::consts::{SQRT_2, TAU};

type Rgb = [f64; 3];
const MISSING: [u32; 2] = [10, 0];
const CORNER_RADIUS: f64 = 20.;

fn clamp01(v: f64) -> f64 { v.clamp(0., 1.) }
fn smooth(a: f64, b: f64, x: f64) -> f64 { let t = clamp01((x - a) / (b - a)); t * t * (3. - 2. * t) }
fn mix(a: f64, b: f64, t: f64) -> f64 { a + (b - a) * t }
fn mixc(a: Rgb, b: Rgb, t: f64) -> Rgb { std::array::from_fn(|i| mix(a[i], b[i], t)) }
fn mulc(a: Rgb, k: f64) -> Rgb { a.map(|v| v * k) }
fn gauss(v: f64, width: f64) -> f64 { (-(v / width).powi(2)).exp() }
fn int(v: f64) -> i32 { v as i32 }
fn hex(c: u32) -> Rgb { [(c >> 16) as f64 / 255., ((c >> 8) & 255) as f64 / 255., (c & 255) as f64 / 255.] }

fn hash(x: i32, y: i32, s: i32) -> f64 {
    let mut h = x.wrapping_mul(374761393).wrapping_add(y.wrapping_mul(668265263)).wrapping_add(s.wrapping_mul(1442695041)) as u32;
    h = (h ^ (h >> 13)).wrapping_mul(1274126177);
    h ^= h >> 16;
    f64::from(h) / 4294967296.
}
fn vnoise(x: f64, y: f64, s: i32) -> f64 {
    let (xi, yi) = (x.floor(), y.floor());
    let (xf, yf) = (x - xi, y - yi);
    let (u, v) = (xf * xf * (3. - 2. * xf), yf * yf * (3. - 2. * yf));
    let (xi, yi) = (xi as i32, yi as i32);
    let (a, b, c, d) = (hash(xi, yi, s), hash(xi + 1, yi, s), hash(xi, yi + 1, s), hash(xi + 1, yi + 1, s));
    a + (b - a) * u + (c - a) * v + (a - b - c + d) * u * v
}
fn fbm(x: f64, y: f64, s: i32, octaves: i32) -> f64 {
    let (mut total, mut amplitude, mut frequency, mut norm) = (0., 0.5, 1., 0.);
    for i in 0..octaves {
        total += amplitude * vnoise(x * frequency, y * frequency, s + i * 17);
        norm += amplitude;
        frequency *= 2.;
        amplitude *= 0.5;
    }
    total / norm
}
fn voronoi(u: f64, v: f64, seed: i32) -> (f64, f64, f64) {
    let (iu, iv) = (u.floor() as i32, v.floor() as i32);
    let (mut d1, mut d2, mut id) = (9., 9., 0.);
    for j in -1..=1 {
        for i in -1..=1 {
            let (cu, cv) = (iu + i, iv + j);
            let d = (u - f64::from(cu) - hash(cu, cv, seed)).hypot(v - f64::from(cv) - hash(cu, cv, seed + 1));
            if d < d1 { d2 = d1; d1 = d; id = hash(cu, cv, seed + 2); } else if d < d2 { d2 = d; }
        }
    }
    (d1, d2, id)
}

#[derive(Clone, Copy)]
struct At { m: Motif, x: f64, y: f64, u: f64, v: f64, n: i32 }
#[derive(Clone, Copy)]
enum Motif { Arcs, Dot, Tri, Stripes, Quarter, Checker, Squares, Semi }
use Motif::*;
const MOTIFS: [Motif; 16] = [Arcs, Dot, Tri, Stripes, Quarter, Checker, Squares, Semi, Semi, Checker, Arcs, Dot, Quarter, Tri, Stripes, Squares];

fn band(q: f64, n: f64) -> (i64, f64) { let f = q * n; let k = f.floor(); (k as i64, (f - k).min(1. - f + k) / n) }
fn motif(m: Motif, x: f64, y: f64) -> (bool, f64) {
    match m {
        Arcs => { let (k, e) = band(x.hypot(1. - y), 5.); (k % 2 == 1, e) }
        Dot => { let d = (x - 0.5).hypot(y - 0.5); (d < 0.3, (d - 0.3).abs()) }
        Tri => (x + y < 1., (x + y - 1.).abs() / SQRT_2),
        Stripes => { let (k, e) = band(y, 5.); (k % 2 == 1, e) }
        Quarter => { let (a, b) = (x.hypot(y), (1. - x).hypot(1. - y)); (a < 0.5 || b < 0.5, (a - 0.5).abs().min((b - 0.5).abs())) }
        Checker => { let ((kx, ex), (ky, ey)) = (band(x, 3.), band(y, 3.)); ((kx + ky) % 2 == 1, ex.min(ey)) }
        Squares => { let (k, e) = band((x - 0.5).abs().max((y - 0.5).abs()) * 2., 4.); (k % 2 == 1, e / 2.) }
        Semi => { let d = (x - 0.5).hypot(y - 1.); (d < 0.42, (d - 0.42).abs()) }
    }
}
fn sd(m: Motif, x: f64, y: f64) -> f64 { let (inside, e) = motif(m, x, y); if inside { -e } else { e } }
fn across(m: Motif, x: f64, y: f64) -> f64 {
    match m {
        Arcs => x.hypot(1. - y) * 60.,
        Dot => (x - 0.5).hypot(y - 0.5) * 60.,
        Semi => (x - 0.5).hypot(y - 1.) * 60.,
        Quarter => x.hypot(y).min((1. - x).hypot(1. - y)) * 60.,
        Stripes => y * 60.,
        Checker => (x + y) * 42.,
        Squares => (x - 0.5).abs().max((y - 0.5).abs()) * 60.,
        Tri => (x - y) * 42.,
    }
}
fn lit(h: &impl Fn(f64, f64) -> f64, x: f64, y: f64, k: f64) -> (f64, f64) {
    let e = 0.005;
    let (dx, dy) = ((h(x + e, y) - h(x - e, y)) / (2. * e), (h(x, y + e) - h(x, y - e)) / (2. * e));
    let (nx, ny) = (-dx * k, -dy * k);
    (((nx * -0.5 + ny * -0.6 + 0.62) / (nx * nx + ny * ny + 1.).sqrt() / 0.62).clamp(0.35, 1.45), dx.hypot(dy))
}
fn stripe(t: f64, duty: f64) -> f64 { smooth(duty + 0.06, duty - 0.06, (t - t.floor() - 0.5).abs() * 2.) }

fn clay(u: f64, v: f64, n: i32, body: Rgb) -> Rgb {
    let c = mulc(body, 0.9 + 0.14 * fbm(u / 6., v / 6., n + 11, 3));
    let g = hash(int(u * 1.6), int(v * 1.6), n + 13);
    if g > 0.93 { mulc(c, 1.12) } else if g < 0.035 { mulc(c, 0.55) } else { c }
}
fn speck(c: Rgb, u: f64, v: f64, n: i32) -> Rgb {
    let (d1, _, id) = voronoi(u / 1.7, v / 1.7, n + 21);
    if id <= 0.86 { return c; }
    let k = smooth(0.2, 0.08, d1);
    mulc(mixc(c, mixc(c, [0.62, 0.36, 0.2], 0.6), smooth(0.45, 0.2, d1) * 0.5), 1. - k * 0.6)
}
fn tube(p: At, line: Rgb, inside: Rgb, outside: Rgb) -> Rgb {
    let At { m, x, y, u, v, n } = p;
    let d = sd(m, x, y);
    let h = |px: f64, py: f64| smooth(0.016, 0.005, sd(m, px, py).abs());
    let fill = if d < 0. { inside } else { outside };
    let height = h(x, y);
    if height > 0.02 { let (s, _) = lit(&h, x, y, 0.02); return mixc(mulc(fill, 0.8), mulc(line, 0.3 + 0.7 * s), height); }
    mulc(fill, (0.9 + 0.12 * fbm(u / 6., v / 6., n, 3)) * (1. - 0.25 * gauss(d.abs() - 0.016, 0.012)))
}
fn raku(p: At, white: Rgb, copper: [Rgb; 2], black: Rgb) -> Rgb {
    let At { m, x, y, u, v, n } = p;
    let d = sd(m, x, y);
    if d < 0. {
        let f = fbm(u / 5., v / 5., n, 4);
        return mixc(mixc(copper[0], copper[1], smooth(0.35, 0.65, f)), black, smooth(0.58, 0.72, fbm(u / 4., v / 4., n + 3, 3)) * 0.7);
    }
    let ((d1, d2, _), (e1, e2, _)) = (voronoi(u / 3., v / 3., n), voronoi(u / 1.2, v / 1.2, n + 4));
    let crack = smooth(0.08, 0.015, d2 - d1).max(smooth(0.07, 0.02, e2 - e1) * 0.5);
    mulc(mixc(mulc(white, 0.94 + 0.06 * fbm(u / 6., v / 6., n, 2)), black, crack * 0.9), 1. - 0.3 * gauss(d, 0.06))
}
fn terrazzo(p: At, outside: Rgb, inside: Rgb, chips: &[Rgb]) -> Rgb {
    let At { m, x, y, u, v, n } = p;
    let mut c = mulc(if sd(m, x, y) < 0. { inside } else { outside }, 0.95 + 0.08 * vnoise(u * 2., v * 2., n));
    let (d1, _, id) = voronoi(u / 2.3, v / 2.3, n + 31);
    if id > 0.38 {
        c = mixc(c, mulc(chips[(id * 97.).floor() as usize % chips.len()], 0.9 + 0.15 * vnoise(u * 3., v * 3., n + 1)), smooth(0.24 + 0.2 * id, 0.2 + 0.2 * id, d1));
    }
    if hash(int(u * 2.5), int(v * 2.5), n + 5) < 0.05 {
        c = mulc(chips[(hash(int(u * 2.5), int(v * 2.5), n + 6) * chips.len() as f64) as usize], 0.95);
    }
    c
}
fn combed(p: At, ground: Rgb, slip: Rgb) -> Rgb {
    let At { m, x, y, u, v, n } = p;
    let t = if sd(m, x, y) < 0. { x * 13. + 0.3 * (y * TAU * 2.4 + f64::from(n)).sin() } else { y * 11. + 0.38 * (x * TAU * 2.6 + f64::from(n)).sin() };
    mixc(clay(u, v, n, ground), mulc(slip, 0.92 + 0.1 * vnoise(u / 2., v / 2., n)), stripe(t, 0.48))
}
fn sgraffito(p: At, body: Rgb, slip: Rgb) -> Rgb {
    let At { m, x, y, u, v, n } = p;
    let d = sd(m, x, y);
    let hatch = if d < 0. { stripe((u + v) / 1.9, 0.3) } else { 0. };
    let reveal = smooth(0.009, 0.004, d.abs()).max(hatch * (0.8 + 0.2 * vnoise(u / 3., v / 3., n)));
    mixc(mulc(slip, 0.88 + 0.14 * fbm(u / 5., v * 0.6, n + 2, 3)), clay(u, v, n, body), reveal)
}
fn mosaic(m: Motif, lx: f64, ly: f64, n: i32, outside: Rgb, inside: Rgb, grout: Rgb) -> Rgb {
    let s = 4.3;
    let (gx, gy) = ((lx / s).floor(), (ly / s).floor());
    let (fx, fy) = (lx / s - gx, ly / s - gy);
    if fx.min(fy).min(1. - fx).min(1. - fy) < 0.09 { return grout; }
    mulc(if sd(m, (gx + 0.5) * s / 60.6, (gy + 0.5) * s / 60.6) < 0. { inside } else { outside }, 0.88 + 0.2 * hash(gx as i32, gy as i32, n))
}
fn relief(p: At, light: Rgb, dark: Rgb) -> Rgb {
    let At { m, x, y, u, v, n } = p;
    let h = |px: f64, py: f64| 1. - smooth(0.024, -0.024, sd(m, px, py)) + 0.06 * vnoise(px * 38., py * 38., n);
    let (s, slope) = lit(&h, x, y, 0.028);
    let d = sd(m, x, y);
    let foot = if d < 0. { gauss(d, 0.04) } else { 0. };
    let c = mixc(light, dark, clamp01((1. - h(x, y)) * 0.55 + 0.2 * fbm(u / 8., v / 8., n, 3) + 0.3 * foot));
    let c = mixc(c, light.map(|q| (q * 1.08).min(1.)), clamp01(slope * 0.012) * 0.5);
    mulc(c, (0.3 + 0.7 * s) * (0.97 + 0.05 * vnoise(u * 1.5, v * 1.5, n + 4)))
}
fn ash(p: At, flash: [Rgb; 2], run: Rgb, shino: Rgb) -> Rgb {
    let At { m, x, y, u, v, n } = p;
    let mut c = mixc(flash[0], flash[1], smooth(0.3, 0.85, x * 0.6 + y * 0.4 + 0.3 * (fbm(u / 10., v / 10., n, 3) - 0.5)));
    let d = sd(m, x, y);
    if d < 0. { c = mixc(shino, flash[0], smooth(0.04, 0., -d) * 0.6 + 0.15 * fbm(u / 5., v / 5., n + 4, 3)); }
    let column = (x * 16.).floor();
    let width = (x * 16. - column - 0.5).abs() * 2.;
    let column = column as i32;
    if hash(column, n, 1) > 0.55 && y < 0.15 + hash(column, n, 2) * 0.7 && width < 0.55 + 0.3 * vnoise(y * 20., f64::from(column), n) { c = mixc(c, run, 0.7); }
    speck(c, u, v, n)
}
fn crawl(p: At, body: Rgb, outside: Rgb, inside: Rgb) -> Rgb {
    let At { m, x, y, u, v, n } = p;
    let (wu, wv) = (u + 0.9 * (vnoise(u / 4., v / 4., n) - 0.5), v + 0.9 * (vnoise(u / 4. + 3., v / 4., n) - 0.5));
    let (d1, d2, _) = voronoi(wu / 3.6, wv / 3.6, n);
    let w = 0.07 + 0.1 * vnoise(u / 6., v / 6., n + 2);
    let island = mulc(if sd(m, x, y) < 0. { inside } else { outside }, 0.74 + 0.28 * smooth(w, w + 0.4, d2 - d1));
    mixc(mulc(body, 0.8 + 0.25 * vnoise(u, v, n)), island, smooth(w, w + 0.05, d2 - d1))
}
fn cement(p: At, base: Rgb, inlay: Rgb) -> Rgb {
    let At { m, x, y, u, v, n } = p;
    let (jx, jy) = (x + 0.024 * (vnoise(u / 2.2, v / 2.2, n) - 0.5), y + 0.024 * (vnoise(u / 2.2 + 9., v / 2.2, n) - 0.5));
    let c = mixc(base, inlay, smooth(0.004, -0.004, sd(m, jx, jy)));
    let c = mulc(c, (0.92 + 0.12 * fbm(u / 12., v / 12., n + 3, 3)) * (0.96 + 0.08 * (hash(int(u * 3.), int(v * 3.), n) - 0.5)));
    let wear = smooth(0.6, 0.78, fbm(u / 7., v / 7., n + 50, 3)) * 0.24;
    let c = c.map(|q| mix(q, q * 0.55 + 0.42, wear));
    let (d1, _, id) = voronoi(u / 1.3, v / 1.3, n + 9);
    if id > 0.8 { mulc(c, 1. - smooth(0.16, 0.07, d1) * 0.45) } else { c }
}
fn brushed(p: At, ground: Rgb, paint: Rgb) -> Rgb {
    let At { m, x, y, u, v, n } = p;
    let d = sd(m, x, y);
    let streak = if d < 0. { vnoise(across(m, x, y) * 1.3, (x + y) * 3., n) } else { vnoise(u / 9., v * 1.4, n + 1) };
    mulc(if d < 0. { paint } else { ground }, (0.84 + 0.2 * streak) * (1. - 0.22 * gauss(d, 0.01)))
}
fn stamped(p: At, light: Rgb, dark: Rgb) -> Rgb {
    let At { m, x, y, u, v, n } = p;
    let r = if sd(m, x, y) < 0. { 0.045 } else { 0.022 };
    let h = |px: f64, py: f64| smooth(r * 9. - 0.12, r * 9. + 0.12, (px * 9. - (px * 9.).floor() - 0.5).hypot(py * 9. - (py * 9.).floor() - 0.5));
    let (s, _) = lit(&h, x, y, 0.012);
    mulc(mixc(light, dark, clamp01((1. - h(x, y)) * 0.75 + 0.15 * fbm(u / 7., v / 7., n, 3))), 0.3 + 0.7 * s)
}
fn majolica(p: At, pigment: Rgb, wash: Rgb) -> Rgb {
    let At { m, x, y, u, v, n } = p;
    let base = mulc([0.95, 0.925, 0.865], 0.96 + 0.05 * fbm(u / 10., v / 10., n + 2, 3));
    let density = 0.24 * (0.7 + 0.5 * vnoise(u / 7., v * 0.9, n + 6));
    let base: Rgb = std::array::from_fn(|i| base[i] * (1. - density * (1. - wash[i])));
    let (d1, d2, _) = voronoi(u / 5., v / 5., n + 30);
    let base = mulc(base, 1. - smooth(0.04, 0., d2 - d1) * 0.07);
    let (jx, jy) = (x + 0.018 * (vnoise(u / 3., v / 3., n) - 0.5), y + 0.018 * (vnoise(u / 3. + 5., v / 3., n) - 0.5));
    let d = sd(m, jx, jy);
    let a = smooth(0.005, -0.005, d);
    let streak = 0.6 * vnoise(across(m, jx, jy) * 1.1, (jx + jy) * 6., n + 3) + 0.4 * fbm(u / 4., v / 4., n + 9, 3);
    let paint = clamp01(a * (0.68 + 0.32 * streak) + a * 0.22 * gauss(d, 0.012));
    let c: Rgb = std::array::from_fn(|i| base[i] * (1. - paint * (1. - pigment[i])));
    mixc(c, [0.29, 0.17, 0.14], gauss(d, 0.0055) * (0.55 + 0.4 * vnoise(u / 2., v / 2., n + 8)))
}
fn grog(p: At, body: Rgb) -> Rgb {
    let At { m, x, y, u, v, n } = p;
    let h = |px: f64, py: f64| smooth(-0.02, 0.02, sd(m, px, py));
    let (s, _) = lit(&h, x, y, 0.03);
    let c = mulc(clay(u, v, n, body), 0.88 + 0.2 * fbm(u / 4., v / 4., n + 1, 3));
    let g = hash(int(u * 2.2), int(v * 2.2), n + 3);
    let c = if g > 0.86 { mulc(c, 1.2) } else if g < 0.08 { mulc(c, 0.7) } else { c };
    mulc(c, (0.3 + 0.7 * s) * mix(0.78, 1., h(x, y)))
}
fn dipped(p: At, glaze: Rgb, second: Rgb, body: Rgb) -> Rgb {
    let At { m, x, y, u, v, n } = p;
    let d = sd(m, x + 0.016 * (vnoise(u / 3., v / 3., n) - 0.5), y + 0.016 * (vnoise(u / 3. + 7., v / 3., n) - 0.5));
    let line = 0.56 + 0.05 * (x * 9. + f64::from(n)).sin();
    let drip = (0..4).any(|k| {
        let (xk, length, width) = (0.08 + 0.84 * hash(n, k, 3), 0.08 + 0.3 * hash(n, k, 4), 0.02 + 0.022 * hash(n, k, 5));
        let t = (y - line) / length;
        t > 0. && t < 1. && (x - xk).abs() < width * (1. - t * 0.6) + 0.004
    });
    if y >= line && !drip { return mixc(speck(clay(u, v, n, body), u, v, n), [0.73, 0.49, 0.29], gauss(y - line, 0.02) * 0.5); }
    let c = speck(mulc(if d < 0. && y < line { second } else { glaze }, 0.9 + 0.14 * fbm(u / 9., v / 9., n + 7, 3)), u, v, n);
    mulc(c, 1. - 0.22 * smooth(line - 0.05, line, y) * if y < line { 0.7 } else { 1. })
}
fn smoke(p: At, black: Rgb, flash: Rgb, buff: Rgb) -> Rgb {
    let At { m, x, y, u, v, n } = p;
    let (px, py) = (u / 20., v / 20.);
    let (qx, qy) = (fbm(px, py, n, 4), fbm(px + 5.2, py + 1.3, n + 1, 4));
    let f = fbm(px + 3.2 * qx, py + 3.2 * qy, n + 2, 5);
    let wisp = fbm(px * 3.5 + 2. * qx, py * 3.5 + 2. * qy, n + 3, 4);
    let d = sd(m, x, y);
    let mask = smooth(0.006, -0.006, d);
    let smoke = smooth(0.3, 0.62, f + 0.25 * (wisp - 0.5));
    let smoke = smoke * (1. - 0.72 * mask) + if d > 0. { 0.3 * gauss(d, 0.01) * smoke } else { 0. };
    let c = mixc(buff, [0.85, 0.6, 0.47], smooth(0.25, 0.6, f) * (1. - 0.6 * mask));
    let c = mixc(c, flash, smooth(0.5, 0.72, fbm(px * 1.6 + 9., py * 1.6, n + 5, 4)) * (1. - smoke) * (1. - mask) * 0.6);
    let c = mixc(c, mixc([0.45, 0.41, 0.38], black, smooth(0.45, 1., smoke)), smooth(0., 0.9, smoke) * 0.95);
    mulc(c, (0.97 + 0.04 * vnoise(u * 0.25 + v * 1.4, v * 0.1, n + 6)) * (0.98 + 0.03 * (hash(int(u * 3.), int(v * 3.), n) - 0.5)))
}

fn technique(t: usize, p: At, local: [f64; 2]) -> Rgb {
    let (cream, blue, ochre, terracotta, white, black) = (hex(0xefe6d2), hex(0x2f5597), hex(0xd29b3b), hex(0xb9653f), hex(0xeee8dc), hex(0x1c1a19));
    match t {
        0 => tube(p, cream, blue, ochre),
        1 => raku(p, white, [hex(0x3f6e5b), hex(0xa0603c)], black),
        2 => terrazzo(p, hex(0xe8dfcb), terracotta, &[black, ochre, hex(0x9a968e), hex(0xa8442c), hex(0x8e9b79), white, blue]),
        3 => combed(p, hex(0x6b4128), cream),
        4 => sgraffito(p, terracotta, hex(0x2b2420)),
        5 => mosaic(p.m, local[0], local[1], p.n, white, hex(0x3b3a3c), hex(0xc9c1b0)),
        6 => relief(p, hex(0xa5cfc8), hex(0x2a625e)),
        7 => ash(p, [hex(0xd9874a), hex(0x9a968e)], hex(0x8d9a62), hex(0xead9c2)),
        8 => crawl(p, hex(0x5a3d2e), white, hex(0xd9a597)),
        9 => cement(p, hex(0x8e9b79), hex(0xe8dfcb)),
        10 => brushed(p, blue, cream),
        11 => stamped(p, hex(0xead08f), hex(0x8f6223)),
        12 => majolica(p, hex(0x2f8f99), hex(0xe6bd45)),
        13 => grog(p, hex(0xd9b48a)),
        14 => dipped(p, hex(0x5f81a3), hex(0xf0ebe0), hex(0xc7a079)),
        _ => smoke(p, hex(0x231f1d), hex(0xc45f2c), hex(0xdcbb92)),
    }
}
struct Tile { technique: usize, motif: Motif, seed: i32, shift: f64 }
fn tile(column: u32, row: u32) -> Option<Tile> {
    if [column, row] == MISSING { return None; }
    if (2..10).contains(&column) {
        let index = (row * 8 + column - 2) as usize;
        return Some(Tile { technique: index, motif: MOTIFS[index], seed: index as i32, shift: -128. });
    }
    let side = if column < 2 { column + 2 } else { column - 6 };
    Some(Tile {
        technique: ((1 - row) * 8 + (side + 3) % 8) as usize,
        motif: MOTIFS[((row * 8 + side + 5) % 16) as usize],
        seed: 16 + (row * 8 + side) as i32,
        shift: if column < 2 { 1152. } else { 640. },
    })
}
fn sample(u: f64, v: f64) -> [f64; 4] {
    let [r, g, b, a] = tiled(u, v);
    let [width, height] = TILES_EXTENT.map(f64::from);
    let (qx, qy) = ((u - width / 2.).abs() - (width / 2. - CORNER_RADIUS), (v - height / 2.).abs() - (height / 2. - CORNER_RADIUS));
    let outside = qx.max(0.).hypot(qy.max(0.)) + qx.max(qy).min(0.) - CORNER_RADIUS;
    [r, g, b, a * (0.5 - outside).clamp(0., 1.)]
}
fn tiled(u: f64, v: f64) -> [f64; 4] {
    let (inset, size, radius) = (1.7, 60.6, 3.);
    let (column, row) = (((u / 64.).floor() as u32).min(11), ((v / 64.).floor() as u32).min(1));
    let (lx, ly) = (u - f64::from(column) * 64. - inset, v - f64::from(row) * 64. - inset);
    let tile = tile(column, row);
    let (qx, qy) = ((lx - size / 2.).abs() - (size / 2. - radius), (ly - size / 2.).abs() - (size / 2. - radius));
    let edge = qx.max(0.).hypot(qy.max(0.)) + qx.max(qy).min(0.) - radius + 0.9 * (vnoise((u + tile.as_ref().map_or(0., |t| t.shift)) / 3., v / 3., 7) - 0.5);
    if edge > 0. {
        let g = mulc(hex(0xb6a991), (0.86 + 0.2 * vnoise(u * 1.6, v * 1.6, 9)) * (0.6 + 0.4 * smooth(0., 1.6, edge)));
        return [g[0], g[1], g[2], 1.];
    }
    let Some(tile) = tile else { return [0.; 4] };
    let (x, y) = (lx / size, ly / size);
    let c = technique(tile.technique, At { m: tile.motif, x, y, u: u + tile.shift, v, n: tile.seed }, [lx, ly]);
    let k = (0.82 + 0.18 * smooth(0., 2., -edge)) * (1. - 0.08 * ((x - 0.5).powi(2) + (y - 0.5).powi(2)) * 4.);
    [c[0] * k, c[1] * k, c[2] * k, 1.]
}
fn byte(v: f64) -> u8 { v.round_ties_even().clamp(0., 255.) as u8 }

pub(super) fn bake() -> Vec<u8> {
    let [width, height] = TILES_EXTENT;
    let mut pixels = vec![0; (width * height * 4) as usize];
    for y in 0..height {
        for x in 0..width {
            let mut total = [0.; 4];
            for (sx, sy) in [(0.25, 0.25), (0.75, 0.25), (0.25, 0.75), (0.75, 0.75)] {
                let p = sample(f64::from(x) + sx, f64::from(y) + sy);
                for c in 0..3 { total[c] += p[c] * p[3]; }
                total[3] += p[3];
            }
            if total[3] == 0. { continue; }
            let u = f64::from(x) + tile(x / 64, y / 64).map_or(0., |t| t.shift);
            let grain = (hash(int(u * 7.3), int(f64::from(y) * 7.3), 13) - 0.5) * 2. + (fbm(u / 3., f64::from(y) / 3., 16, 2) - 0.5) * 1.6;
            let i = ((y * width + x) * 4) as usize;
            for c in 0..3 { pixels[i + c] = byte(f64::from(byte(total[c] / total[3] * 255.)) + grain); }
            pixels[i + 3] = byte(total[3] / 4. * 255.);
        }
    }
    pixels
}

#[test]
#[ignore = "writes assets/filters/filter_preview_tiles.rgba"]
fn write_filter_preview_tiles() {
    std::fs::write(concat!(env!("CARGO_MANIFEST_DIR"), "/../../assets/filters/filter_preview_tiles.rgba"), bake()).unwrap();
}

#[test]
fn filter_preview_tiles_match_their_bake() {
    let fresh = bake();
    assert_eq!(fresh.len(), TILES.len());
    let difference = fresh.iter().zip(TILES).map(|(a, b)| f64::from(a.abs_diff(*b))).sum::<f64>() / fresh.len() as f64;
    assert!(difference < 1., "The tiles are {difference:.2} codes from their bake; run write_filter_preview_tiles");
    let alpha = |column: u32, row: u32| TILES[(((row * 64 + 32) * TILES_EXTENT[0] + column * 64 + 32) * 4 + 3) as usize];
    for row in 0..2 { for column in 0..12 { assert_eq!(alpha(column, row), if [column, row] == MISSING { 0 } else { 255 }); } }
    let [width, height] = TILES_EXTENT;
    for (x, y) in [(1, 1), (width - 2, 1), (1, height - 2), (width - 2, height - 2)] {
        assert_eq!(TILES[((y * width + x) * 4 + 3) as usize], 0, "corner {x},{y} is rounded off before filters run");
    }
}
