fn evolving_contact_prepared(
    world: vec2<f32>, center: vec2<f32>, radii: vec2<f32>, rotation: vec2<f32>,
    motion: vec2<f32>, previous: vec4<f32>, sensors: vec4<f32>,
    previous_sensors: vec4<f32>, hardness: f32, field: vec2<f32>,
    metric: vec4<f32>, invariants: vec2<f32>,
) -> f32 {
    let distance2 = dot(motion, motion);
    let start = center - motion;
    let nib_motion = vec2<f32>(dot(motion, metric.xy), dot(motion, metric.zw));
    let nib_offset = vec2<f32>(dot(world - start, metric.xy), dot(world - start, metric.zw));
    let projected = dot(nib_offset, nib_motion) / max(dot(nib_motion, nib_motion), 0.000001);
    var progress = select(1.0, clamp(projected, 0.0, 1.0), distance2 > 0.000001);
    if distance2 > 0.000001 && any(abs(previous.xy - radii) > vec2<f32>(0.000001)) {
        let middle_axes = max((previous.xy + radii) * 0.5, vec2<f32>(0.005));
        let relative_start = previous.xy / middle_axes;
        let relative_end = radii / middle_axes;
        let r0 = max((relative_start.x + relative_start.y) * 0.5, 0.000001);
        let r1 = max((relative_end.x + relative_end.y) * 0.5, 0.000001);
        let slope = r1 - r0;
        let a = dot(nib_offset, nib_motion);
        let b = dot(nib_motion, nib_motion);
        let c = dot(nib_offset, nib_offset);
        let denominator = b * r0 + slope * a;
        if denominator > 0.000001 {
            progress = clamp((a * r0 + slope * c) / denominator, 0.0, 1.0);
        } else {
            progress = select(0.0, 1.0, (c - 2.0 * a + b) / (r1 * r1) < c / (r0 * r0));
        }
    }
    var axes = max(radii, vec2<f32>(0.005));
    var orientation = rotation;
    if any(previous != vec4<f32>(radii, rotation)) {
        axes = max(mix(previous.xy, radii, progress), vec2<f32>(0.005));
        let angle = mix(previous.zw, rotation, progress);
        orientation = select(rotation, angle / max(length(angle), 0.00001), dot(angle, angle) > 0.00001);
    }
    let local = rotate(world - mix(start, center, progress), orientation.x, -orientation.y) / axes;
    let radius = length(local);
    if radius > 1.45 { return 0.0; }
    let input = mix(previous_sensors, sensors, progress);
    let pressure = clamp(input.x, 0.0, 1.0);
    let aa = min(1.0 / max(min(axes.x, axes.y), 0.005), 1.0);

    var boundary = 1.0;
    if contact_feature(4u, style.contact_a.w > 0.0) {
        boundary += (field.x - 0.5) * style.contact_a.w * (1.5 - pressure * 0.5);
    }
    let pool = select(0.0, style.contact_b.w, contact_feature(16u, style.contact_b.w > 0.0));
    boundary += pool * 0.12 * (0.3 + 0.7 * pressure);
    let feather = max(1.0 - hardness, aa);
    var coverage = 1.0 - smoothstep(boundary - feather, boundary + aa * 0.5, radius);
    if contact_feature(256u, style.contact_c.w > 0.5) {
        coverage = clamp((boundary - radius) / feather, 0.0, 1.0);
    }
    if !contact_uniform() {
        var integral_motion = nib_motion;
        var integral_offset = nib_offset;
        if any(previous != vec4<f32>(radii, rotation)) {
            integral_motion = rotate(motion, orientation.x, -orientation.y) / axes;
            integral_offset = rotate(world - start, orientation.x, -orientation.y) / axes;
        }
        let integral_projected = dot(integral_offset, integral_motion)
            / max(dot(integral_motion, integral_motion), 0.000001);
        let travel = length(integral_motion);
        let q = max(boundary * boundary - dot(integral_offset - integral_motion * integral_projected,
            integral_offset - integral_motion * integral_projected), 0.0);
        let reach = sqrt(q);
        let a = clamp(-integral_projected * travel, -reach, reach);
        let b = clamp((1.0 - integral_projected) * travel, -reach, reach);
        let parabolic = max(q * (b - a) - (b*b*b - a*a*a) / 3.0, 0.0);
        let firmness = invariants.y;
        let hard_weight = firmness * 0.7;
        var integral = mix(parabolic, b - a, hard_weight);
        let normalization = invariants.x / max(travel * 2.0 * sqrt(axes.x * axes.y), 0.000001);
        let section = 2.0 * reach * mix(q * (2.0 / 3.0), 1.0, hard_weight);
        let feed = mix(1.0, 1.0 / max(section, 0.001), firmness);
        let directional_feed = mix(normalization, 1.0, firmness);
        coverage = integral * directional_feed * feed * smoothstep(0.0, aa * 2.0, q);
        if distance2 <= 0.000001 {
            coverage = 0.08 * max(1.0 - radius * radius, 0.0);
        }
    }
    if coverage <= 0.0 { return 0.0; }

    if contact_feature(2u, style.contact_a.y > 0.0) {
        let tooth = field.y;
        let penetration = clamp(pressure * style.contact_c.x, 0.0, 1.0);
        var contact = smoothstep(0.5 - penetration * 0.5, 0.85 - penetration * 0.35, tooth)
            * (0.3 + penetration * 0.7);
        if contact_uniform() {
            let threshold = 0.62 - penetration * 0.3;
            contact = smoothstep(threshold - 0.08, threshold + 0.08, tooth);
        }
        coverage *= mix(1.0, contact, style.contact_a.y);
    }

    var load = 1.0;
    if contact_feature(32u, style.contact_c.y > 0.0) { load = exp(-style.contact_c.y * input.z); }
    if contact_feature(8u, style.contact_b.z > 0.0) && sensors.z > 0.0 {
        var strand_y = local.y;
        if distance2 > 0.000001 {
            let turn = previous.z * rotation.y - previous.w * rotation.x;
            let along_nib = local.x * axes.x;
            strand_y -= 0.5 * turn * along_nib * along_nib
                / max(invariants.x * axes.y, 0.000001);
        }
        let across = (strand_y * 0.5 + 0.5) * style.contact_b.y;
        let along = previous_sensors.z + dot(world - start, motion)
            * (sensors.z - previous_sensors.z) / max(distance2, 0.000001);
        let variation = contact_noise(vec2<f32>(across,
            along * style.contact_b.y * 0.08 + sensors.w));
        let fibers = smoothstep(0.1, 0.72 - pressure * 0.12, variation);
        let separation = style.contact_b.z * (1.0 - pressure * 0.45)
            * smoothstep(0.12, 0.78, abs(local.y));
        coverage *= mix(1.0, fibers, separation);
        let supply = smoothstep(0.08, 0.3, load - variation * 0.65);
        coverage *= mix(1.0, supply, style.contact_b.z);
    }
    coverage *= mix(1.0, load, 0.85 * (1.0 - style.contact_b.z));
    return clamp(coverage, 0.0, 1.0);
}
