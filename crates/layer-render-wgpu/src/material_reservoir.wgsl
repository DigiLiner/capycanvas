fn reservoir_exchange_amount(dab: Dab, coverage: f32) -> f32 {
    let pull = clamp(dab.material.y, 0.0, 1.0);
    return clamp(
        ((1.0 - style.material_a.x) * style.material_b.x + pull * 0.25)
            * coverage * dab.flow,
        0.0,
        1.0,
    );
}

fn replenished_reservoir_alpha(carried: f32, pickup: f32, exchange: f32) -> f32 {
    return max(carried, mix(carried, pickup, exchange));
}

@compute @workgroup_size(8, 8)
fn reservoir_exchange(@builtin(global_invocation_id) id: vec3<u32>) {
    let local = (vec2<f32>(id.xy) + 0.5) / vec2<f32>(textureDimensions(reservoir_texture)) * 2.0 - vec2<f32>(1.0);
    var carried = textureLoad(reservoir_texture, vec2<i32>(id.xy), 0);
    if style.canvas_opacity.w > 0.5 {
        carried = bitcast<vec4<f32>>(material_sources.pages[0]);
    }
    for (var offset = 0u; offset < style.operation.y; offset += 1u) {
        let dab = dabs[style.operation.x + offset];
        let scaled = local * dab.radii;
        let world = dab.center + rotate(scaled, dab.rotation.x, dab.rotation.y);
        let coverage = contact_coverage(dab, world);
        if coverage <= 0.0 { continue; }
        let pull = clamp(dab.material.y, 0.0, 1.0);
        let pickup = blurred_canvas_load(world - dab.motion * pull, style.material_b.z);
        if !working_has_color(pickup.a) { continue; }
        let exchange = reservoir_exchange_amount(dab, coverage);
        let alpha_exchange = exchange * (1.0 - style.material_a.w * 0.5);
        carried = vec4<f32>(
            mix_color(carried.rgb, pickup.rgb / pickup.a, exchange),
            replenished_reservoir_alpha(carried.a, pickup.a, alpha_exchange),
        );
    }
    store_dry_color(id.xy, carried);
}
