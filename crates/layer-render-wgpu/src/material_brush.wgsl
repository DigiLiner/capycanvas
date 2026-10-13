@vertex
fn vertex_main(@builtin(vertex_index) vertex_index: u32) -> @builtin(position) vec4<f32> {
    var positions = array<vec2<f32>, 3>(
        vec2<f32>(-1.0, -1.0),
        vec2<f32>( 3.0, -1.0),
        vec2<f32>(-1.0,  3.0),
    );
    return vec4<f32>(positions[vertex_index], 0.0, 1.0);
}

@fragment
fn gather_fragment(@builtin(position) position: vec4<f32>) -> @location(0) vec4<f32> {
    let world = render_target.origin_extent.xy + position.xy;
    var value = vec4<f32>(0.0);
    if MATERIAL_OPERATION == OP_SMUDGE {
        let trace = trace_smudge(world, style.operation.x, style.operation.y);
        value = blurred_canvas_sample(trace.coordinate, style.material_b.z);
    } else {
        value = canvas_sample(deform_coordinate(world, style.operation.x, style.operation.y));
    }
    return value + textureLoad(reservoir_texture, vec2<i32>(floor(position.xy)), 0);
}

fn material_result(fragment_position:vec4<f32>)->MaterialOutput {
    if MATERIAL_OPERATION==OP_DEPOSIT || MATERIAL_OPERATION==OP_COVERAGE || MATERIAL_OPERATION==OP_CLONE {
        return pointwise_material_result(fragment_position);
    }
    var result=paint_fragment(fragment_position);
    if style.color.a>0.5 && !pointwise_aggregate() {
        let world=render_target.origin_extent.xy+fragment_position.xy;
        result=material_alpha_locked(result,canvas_load(world));
    }
    result.color=layer_color(result.color,WORKING_LUMA,style.color_mode.x,style.color_mode.y);
    return result;
}

@fragment
fn reservoir_fragment(@builtin(position) fragment_position: vec4<f32>) -> @location(0) vec4<f32> {
    let first = style.operation.x;
    let count = style.operation.y;
    if count == 0u {
        return vec4<f32>(0.0);
    }
    let reservoir_size = vec2<f32>(textureDimensions(reservoir_texture));
    let local = fragment_position.xy / reservoir_size * 2.0 - vec2<f32>(1.0);
    var carried = textureLoad(
        reservoir_texture,
        clamp(vec2<i32>(floor(fragment_position.xy)), vec2<i32>(0), vec2<i32>(reservoir_size) - 1),
        0,
    );
    for (var offset = 0u; offset < count; offset += 1u) {
        let dab = dabs[first + offset];
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
    return carried;
}

@fragment
fn fragment_main(@builtin(position) fragment_position: vec4<f32>) -> MaterialOutput {
    return material_result(fragment_position);
}
