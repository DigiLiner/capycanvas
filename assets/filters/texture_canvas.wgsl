fn capy_canvas(p: vec2f, b: u32) -> f32 {
    let threads = fx_parameter(b, 0u).x;
    let irregularity = fx_parameter(b, 1u).x / 100.0;
    let priming = fx_parameter(b, 2u).x / 100.0;
    let seed = u32(fx_parameter(b, 3u).x);
    let pitch = vec2f(10.0 / threads, 10.0 / (0.9 * threads));
    let weave = tx_weave(p, pitch, irregularity, seed);
    var level = 0.0;
    for (var i = 0u; i < 12u; i += 1u) {
        let t = tx_disc(i, 12u);
        level += t.z * tx_weave(p + 0.7 * pitch.x * t.xy, pitch, irregularity, seed);
    }
    let ground = mix(weave, max(weave, level + 0.2), priming);
    let skin = tx_fbm(p, vec2f(0.4), 3u, seed + 1u);
    return mix(500.0, 120.0, priming) * 13.0 / threads * ground + 8.0 * priming * skin;
}
