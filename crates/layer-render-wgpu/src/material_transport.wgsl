const MIX_OKLAB:u32=1u;
const MIX_CLASSIC:u32=2u;

struct SmudgeTrace {
    coordinate: vec2<f32>,
    influence: f32,
}

fn load_neighborhood(index: i32, coordinate: vec2<i32>) -> vec4<f32> {
    switch index {
        case 0: { return textureLoad(source_00, coordinate, 0); }
        case 1: { return textureLoad(source_10, coordinate, 0); }
        case 2: { return textureLoad(source_20, coordinate, 0); }
        case 3: { return textureLoad(source_01, coordinate, 0); }
        case 4: { return textureLoad(source_11, coordinate, 0); }
        case 5: { return textureLoad(source_21, coordinate, 0); }
        case 6: { return textureLoad(source_02, coordinate, 0); }
        case 7: { return textureLoad(source_12, coordinate, 0); }
        default: { return textureLoad(source_22, coordinate, 0); }
    }
}

fn local_canvas_load(document_position: vec2<f32>) -> vec4<f32> {
    if any(document_position < vec2<f32>(0.0))
        || any(document_position >= render_target.document_extent.xy) {
        return vec4<f32>(0.0);
    }
    if material_sources.header.x == 1u {
        let page = vec2<u32>(floor(document_position / 256.0));
        let local = vec2<i32>(floor(document_position)) - vec2<i32>(page * 256u);
        for (var index = 0u; index < material_sources.header.y; index += 1u) {
            if all(page == material_sources.pages[index].xy) {
                return load_neighborhood(i32(index), local);
            }
        }
        return vec4<f32>(0.0);
    }
    let relative = document_position - render_target.origin_extent.xy;
    let page_offset = vec2<i32>(floor(relative / 256.0));
    if any(page_offset < vec2<i32>(-1)) || any(page_offset > vec2<i32>(1)) {
        return vec4<f32>(0.0);
    }
    let local = vec2<i32>(floor(relative - vec2<f32>(page_offset) * 256.0));
    let coordinate = clamp(local, vec2<i32>(0), vec2<i32>(255));
    let index = (page_offset.y + 1) * 3 + page_offset.x + 1;
    return load_neighborhood(index, coordinate);
}

fn canvas_load(brush_position: vec2<f32>) -> vec4<f32> {
    return local_canvas_load(brush_position);
}

fn canvas_sample(brush_position: vec2<f32>) -> vec4<f32> {
    let document_position = brush_position;
    let base = floor(document_position - vec2<f32>(0.5)) + vec2<f32>(0.5);
    let fraction = clamp(document_position - base, vec2<f32>(0.0), vec2<f32>(1.0));
    let top = mix(
        local_canvas_load(base),
        local_canvas_load(base + vec2<f32>(1.0, 0.0)),
        fraction.x,
    );
    let bottom = mix(
        local_canvas_load(base + vec2<f32>(0.0, 1.0)),
        local_canvas_load(base + vec2<f32>(1.0, 1.0)),
        fraction.x,
    );
    return mix(top, bottom, fraction.y);
}

fn blurred_canvas_load(position: vec2<f32>, amount: f32) -> vec4<f32> {
    let radius = clamp(amount, 0.0, 1.0) * 4.0;
    if radius < 0.01 {
        return canvas_load(position);
    }
    return (canvas_load(position) * 4.0
        + canvas_load(position + vec2<f32>( radius, 0.0))
        + canvas_load(position + vec2<f32>(-radius, 0.0))
        + canvas_load(position + vec2<f32>(0.0,  radius))
        + canvas_load(position + vec2<f32>(0.0, -radius))) / 8.0;
}

fn blurred_canvas_sample(position: vec2<f32>, amount: f32) -> vec4<f32> {
    let radius = clamp(amount, 0.0, 1.0) * 4.0;
    if radius < 0.01 {
        return canvas_sample(position);
    }
    return (canvas_sample(position) * 4.0
        + canvas_load(position + vec2<f32>( radius, 0.0))
        + canvas_load(position + vec2<f32>(-radius, 0.0))
        + canvas_load(position + vec2<f32>(0.0,  radius))
        + canvas_load(position + vec2<f32>(0.0, -radius))) / 8.0;
}

fn watercolor_canvas_sample(position: vec2<f32>, amount: f32) -> vec4<f32> {
    let radius = clamp(amount, 0.0, 1.0) * 4.0;
    if radius < 0.01 {
        return canvas_sample(position);
    }
    return (canvas_sample(position) * 4.0
        + canvas_load(position + vec2<f32>( radius, 0.0))
        + canvas_load(position + vec2<f32>(-radius, 0.0))
        + canvas_load(position + vec2<f32>(0.0,  radius))
        + canvas_load(position + vec2<f32>(0.0, -radius))) / 8.0;
}

fn contact_segment_progress(dab: Dab, world: vec2<f32>) -> f32 {
    let motion_squared = dot(dab.motion, dab.motion);
    if motion_squared <= 0.000001 {
        return 1.0;
    }
    let previous_center = dab.center - dab.motion;
    return clamp(dot(world - previous_center, dab.motion) / motion_squared, 0.0, 1.0);
}

fn mix_color(a: vec3<f32>, b: vec3<f32>, amount: f32) -> vec3<f32> {
    if amount==0. || all(a==b) {return a;}
    if amount==1. {return b;}
    let space = u32(style.render_mode.z);
    if space == MIX_OKLAB {
        return working_from_oklab(mix(working_to_oklab(a), working_to_oklab(b), amount));
    }
    if space == MIX_CLASSIC {
        return sdr_decode(mix(sdr_encode(a, WORKING_SPACE), sdr_encode(b, WORKING_SPACE), amount), WORKING_SPACE);
    }
    return working_mix(a, b, amount);
}

fn random_unit(value: vec2<f32>) -> f32 {
    return fract(sin(dot(value, vec2<f32>(12.9898, 78.233))) * 43758.5453);
}

fn transport_conductance(world: vec2<f32>) -> f32 {
    if style.transport_a.x <= 0.0 {
        return 1.0;
    }
    let coordinate = rotate(
        world / 256.0,
        style.transport_a.y,
        style.transport_a.z,
    ) * style.transport_a.x;
    let sampled = textureSampleLevel(transport_texture, brush_sampler, coordinate, 0.0).r;
    let shaped = pow(clamp(sampled, 0.0, 1.0), 0.72);
    return mix(1.0, shaped, style.transport_a.w);
}

fn reservoir_load(dab: Dab, world: vec2<f32>) -> vec4<f32> {
    let delta = world - dab.center;
    let local = rotate(delta, dab.rotation.x, -dab.rotation.y)
        / max(dab.radii, vec2<f32>(0.005));
    let size = vec2<i32>(textureDimensions(reservoir_texture));
    let coordinate = clamp(
        vec2<i32>(floor((local * 0.5 + vec2<f32>(0.5)) * vec2<f32>(size))),
        vec2<i32>(0),
        size - vec2<i32>(1),
    );
    return textureLoad(reservoir_texture, coordinate, 0);
}

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

fn trace_smudge(initial: vec2<f32>, first: u32, count: u32) -> SmudgeTrace {
    var coordinate = initial;
    var optical_depth = 0.0;
    var remaining = count;
    loop {
        if remaining == 0u { break; }
        remaining -= 1u;
        let dab = dabs[first + remaining];
        let coverage = contact_coverage(dab, coordinate);
        if coverage <= 0.0 { continue; }
        let contact = clamp(
            coverage * max(style.material_b.x, dab.material.y)
                * dab.flow,
            0.0,
            0.9999,
        );
        let diameter = max(dab.radii.x * 2.0, 0.01);
        let distance_steps = length(dab.motion) / (diameter * 0.04);
        optical_depth += -log(1.0 - contact) * distance_steps;
        let pull = clamp(dab.material.y, 0.0, 1.0);
        coordinate -= dab.motion * pull * coverage * dab.flow;
    }
    return SmudgeTrace(coordinate, 1.0 - exp(-optical_depth));
}

fn mix_smudged_material(original: vec4<f32>, dragged: vec4<f32>, influence: f32) -> vec4<f32> {
    if influence <= 0.0 { return original; }
    let original_color = select(
        working_unassociate(dragged),
        working_unassociate(original),
        working_has_color(original.a),
    );
    let dragged_color = select(
        original_color,
        working_unassociate(dragged),
        working_has_color(dragged.a),
    );
    let alpha = max(original.a, dragged.a * influence);
    let color = mix_color(original_color, dragged_color, influence * dragged.a);
    return vec4<f32>(color * alpha, alpha);
}

fn watercolor_fragment(
    fragment_position: vec2<f32>,
    world: vec2<f32>,
    first: u32,
    count: u32,
) -> MaterialOutput {
    let original = working_clamp(canvas_load(world));
    let state_coordinate = clamp(
        vec2<i32>(floor(fragment_position)),
        vec2<i32>(0),
        vec2<i32>(255),
    );
    var stroke_coverage = dry_coverage(state_coordinate);
    if style.canvas_opacity.w > 0.5 {
        stroke_coverage = 0.0;
    }

    let prior_coverage = stroke_coverage;
    var pigment_color = dabs[first].color.rgb;
    var strongest_request = 0.0;
    let prior_wetness = textureLoad(reservoir_texture, state_coordinate, 0).r;
    var trace_coordinate = world;
    var remaining = count;
    loop {
        if remaining == 0u { break; }
        remaining -= 1u;
        let dab = dabs[first + remaining];
        let coverage = contact_coverage(dab, world);
        if coverage > 0.0 {
            let requested = clamp(
                coverage * dab.flow * dab.color.a * style.material_a.y
                    * style.material_b.x * dab.material.z,
                0.0,
                1.0,
            );
            if requested > strongest_request {
                strongest_request = requested;
                pigment_color = dab.color.rgb;
            }
            stroke_coverage += (1.0 - stroke_coverage) * requested;
        }

        let trace_delta = trace_coordinate - dab.center;
        let trace_local = rotate(trace_delta, dab.rotation.x, -dab.rotation.y)
            / max(dab.radii, vec2<f32>(0.005));
        let trace_coverage = analytic_coverage(
            trace_local,
            dab.hardness,
            min(dab.radii.x, dab.radii.y),
        ) * brush_selection_at(trace_coordinate);
        if trace_coverage > 0.0 {
            trace_coordinate -= dab.motion
                * clamp(dab.material.y, 0.0, 1.0)
                * trace_coverage;
        }
    }
    let water_load = select(1.0, style.transport_b.w, style.transport_a.x > 0.0);
    let water_charge = clamp(
        stroke_coverage * water_load
            * (0.38 + 0.62 * transport_conductance(world)),
        0.0,
        1.0,
    );
    let deposited_wetness = prior_wetness
        + (1.0 - prior_wetness) * water_charge;
    let coverage_increment = stroke_increment(prior_coverage, stroke_coverage);
    var result = original;
    if working_has_color(coverage_increment) && style.operation.w == 0u {
        let dragged = watercolor_canvas_sample(trace_coordinate, style.material_b.z);
        let mixing = clamp(style.material_a.w * coverage_increment, 0.0, 1.0);
        if working_has_color(original.a) && working_has_color(dragged.a) && working_has_color(mixing) {
            let original_color = original.rgb / original.a;
            let dragged_color = dragged.rgb / dragged.a;
            let mixed_color = mix_color(
                original_color,
                dragged_color,
                mixing * dragged.a,
            );
            result = vec4<f32>(mixed_color * original.a, original.a);
        }
        let paint_load = mix(0.42, 1.0, style.material_a.x);
        let prior_pigment = prior_coverage * paint_load;
        let next_pigment = stroke_coverage * paint_load;
        let pigment = clamp(
            stroke_increment(prior_pigment, next_pigment),
            0.0,
            1.0,
        );
        result = stored_paint(source_over(deposit_paint(result), deposit_color(pigment_color), pigment));
    }
    if style.operation.w != 0u {
        result *= 1.0 - coverage_increment;
    }
    return MaterialOutput(
        result,
        vec4<f32>(stroke_coverage, 0.0, 0.0, 1.0),
        vec4<f32>(deposited_wetness, 0.0, 0.0, 1.0),
    );
}

fn deform_coordinate(initial: vec2<f32>, first: u32, count: u32) -> vec2<f32> {
    var coordinate = initial;
    var remaining = count;
    loop {
        if remaining == 0u { break; }
        remaining -= 1u;
        let dab = dabs[first + remaining];
        let coverage = contact_coverage(dab, coordinate);
        if coverage <= 0.0 { continue; }
        let strength = clamp(dab.material.w * coverage, 0.0, 1.0);
        let motion = dab.motion * (1.0 + style.deformation.w);
        let mode = style.deformation.x;
        if mode < 0.5 {
            coordinate -= motion * strength;
        } else if mode < 2.5 {
            let direction = select(-1.0, 1.0, mode < 1.5);
            let angle = direction * strength * 0.75;
            coordinate = dab.center + rotate(coordinate - dab.center, cos(angle), sin(angle));
        } else if mode < 4.5 {
            let direction = select(-1.0, 1.0, mode < 3.5);
            coordinate = dab.center + (coordinate - dab.center) * (1.0 + direction * strength * 0.35);
        } else if mode < 5.5 {
            let noise = random_unit(floor(coordinate * 0.25) + dab.center);
            coordinate += (vec2<f32>(noise, fract(noise * 7.13)) - 0.5)
                * strength * style.render_mode.w * 24.0;
        } else {
            let perpendicular = vec2<f32>(-motion.y, motion.x);
            coordinate -= normalize(perpendicular + vec2<f32>(0.0001)) * strength * length(motion);
        }
    }
    return coordinate;
}

fn wet_fragment(
    fragment_position: vec2<f32>,
    world: vec2<f32>,
    first: u32,
    count: u32,
) -> MaterialOutput {
    let original = working_clamp(canvas_load(world));
    let state_coordinate = clamp(
        vec2<i32>(floor(fragment_position)),
        vec2<i32>(0),
        vec2<i32>(255),
    );
    var stroke_coverage = dry_coverage(state_coordinate);
    if style.canvas_opacity.w > 0.5 {
        stroke_coverage = 0.0;
    }
    var batch_color = vec3<f32>(0.0);
    var batch_alpha = 0.0;
    for (var offset = 0u; offset < count; offset += 1u) {
        let dab = dabs[first + offset];
        var contact_dab = dab;
        var contact_progress = 1.0;
        if contact_uniform() {
            contact_progress = contact_segment_progress(dab, world);
            contact_dab.center = dab.center - dab.motion * (1.0 - contact_progress);
        }
        let coverage = contact_coverage(contact_dab, world);
        if coverage <= 0.0 { continue; }
        let wet_jitter = mix(1.0, random_unit(world + dab.center), style.material_b.w);
        var carried = reservoir_load(contact_dab, world);
        if style.canvas_opacity.w > 0.5 {
            carried = vec4<f32>(dab.color.rgb, style.material_a.x * dab.material.z);
        }
        let pull = clamp(dab.material.y, 0.0, 1.0);
        let pickup = blurred_canvas_load(
            world - dab.motion * pull,
            style.material_b.z,
        );
        let empty_color = select(dab.color.rgb, carried.rgb, working_has_color(carried.a));
        let pickup_color = select(
            empty_color,
            working_unassociate(pickup),
            working_has_color(pickup.a),
        );
        let carried_weight = carried.a * (1.0 - style.material_a.w);
        let paint_color = mix_color(pickup_color, carried.rgb, carried_weight);
        let available_material = max(
            pickup.a,
            carried.a * (1.0 - style.material_a.w * 0.5),
        );
        var source_alpha = clamp(
            coverage * dab.flow * dab.color.a * style.material_a.y
                * style.material_b.x * dab.material.z * wet_jitter * available_material,
            0.0,
            1.0,
        );
        if contact_uniform() || style.canvas_opacity.z < 1.0 {
            let next_coverage = stroke_coverage + (1.0 - stroke_coverage) * source_alpha;
            source_alpha = stroke_increment(stroke_coverage, next_coverage);
            stroke_coverage = next_coverage;
        }
        batch_color = batch_color * (1.0 - source_alpha) + deposit_color(paint_color) * source_alpha;
        batch_alpha = source_alpha + batch_alpha * (1.0 - source_alpha);
    }
    var result = original;
    if style.operation.w != 0u {
        result *= 1.0 - batch_alpha;
    } else if working_has_color(batch_alpha) {
        result = stored_paint(source_over(deposit_paint(original), batch_color / batch_alpha, batch_alpha));
    }
    return MaterialOutput(
        result,
        vec4<f32>(stroke_coverage, 0.0, 0.0, 1.0),
        vec4<f32>(0.0),
    );
}

fn paint_fragment(fragment_position: vec4<f32>) -> MaterialOutput {
    let world = render_target.origin_extent.xy + fragment_position.xy;
    let first = style.operation.x;
    let count = style.operation.y;
    if MATERIAL_OPERATION == OP_WATERCOLOR {
        return watercolor_fragment(fragment_position.xy, world, first, count);
    }
    if MATERIAL_OPERATION == OP_WET {
        return wet_fragment(fragment_position.xy, world, first, count);
    }
    if MATERIAL_OPERATION == OP_SMUDGE {
        let original = canvas_load(world);
        let trace = trace_smudge(world, first, count);
        var dragged = vec4<f32>(0.0);
        if material_sources.header.x == 2u {
            dragged = textureLoad(reservoir_texture, vec2<i32>(floor(fragment_position.xy)), 0);
        } else {
            dragged = blurred_canvas_sample(trace.coordinate, style.material_b.z);
        }
        var influence = trace.influence;
        var coverage = 0.0;
        if style.canvas_opacity.z < 1.0 {
            var previous = dry_coverage(vec2<i32>(floor(fragment_position.xy)));
            if style.canvas_opacity.w > 0.5 { previous = 0.0; }
            coverage = previous + (1.0 - previous) * influence;
            influence = stroke_increment(previous, coverage);
        }
        return MaterialOutput(
            mix_smudged_material(original, dragged, influence),
            vec4<f32>(coverage, 0.0, 0.0, 1.0),
            vec4<f32>(0.0),
        );
    }
    if MATERIAL_OPERATION == OP_LIQUIFY {
        if material_sources.header.x == 2u {
            return MaterialOutput(
                textureLoad(reservoir_texture, vec2<i32>(floor(fragment_position.xy)), 0),
                vec4<f32>(0.0), vec4<f32>(0.0),
            );
        }
        return MaterialOutput(
            canvas_sample(deform_coordinate(world, first, count)),
            vec4<f32>(0.0),
            vec4<f32>(0.0),
        );
    }

    return pointwise_paint_fragment(fragment_position);
}
