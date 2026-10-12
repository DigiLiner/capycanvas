override MATERIAL_OPERATION: u32;
override MATERIAL_PREVIEW_CONTRIBUTION: bool = false;

const OP_DEPOSIT: u32 = 0u;
const OP_COVERAGE: u32 = 1u;
const OP_LIQUIFY: u32 = 2u;
const OP_SMUDGE: u32 = 3u;
const OP_WET: u32 = 4u;
const OP_WATERCOLOR: u32 = 5u;
const OP_CLONE: u32 = 6u;

struct Target {
    origin_extent: vec4<f32>,
    document_extent: vec4<f32>,
}

@group(0) @binding(0) var<uniform> style: Style;
@group(1) @binding(0) var<uniform> render_target: Target;

@group(2) @binding(0) var source_00: texture_2d<f32>;
@group(2) @binding(1) var source_10: texture_2d<f32>;
@group(2) @binding(2) var source_20: texture_2d<f32>;
@group(2) @binding(3) var source_01: texture_2d<f32>;
@group(2) @binding(4) var source_11: texture_2d<f32>;
@group(2) @binding(5) var source_21: texture_2d<f32>;
@group(2) @binding(6) var source_02: texture_2d<f32>;
@group(2) @binding(7) var source_12: texture_2d<f32>;
@group(2) @binding(8) var source_22: texture_2d<f32>;
@group(2) @binding(9) var<storage, read> dabs: array<Dab>;
@group(2) @binding(10) var stroke_coverage_texture: texture_2d<f32>;
@group(2) @binding(11) var reservoir_texture: texture_2d<f32>;
struct MaterialSources {
    // 0: adjacent pages; 1: disjoint gather pages; 2: gathered sample field.
    // Adjacent pages also provide the ordered dry-contact range in zw.
    header: vec4<u32>,
    // For dry paint, pages[0] is the local dirty rectangle (min xy, max xy).
    // A clone's pages[1..5] map it to its source, whose 2x2 blocks of target
    // and reference pages it binds around source_11.
    pages: array<vec4<u32>, 9>,
}
@group(2) @binding(12) var<uniform> material_sources: MaterialSources;

fn retouch_target_load(page: i32, texel: vec2<i32>) -> vec4<f32> {
    switch page {
        case 0: { return textureLoad(source_00, texel, 0); }
        case 1: { return textureLoad(source_10, texel, 0); }
        case 2: { return textureLoad(source_20, texel, 0); }
        default: { return textureLoad(source_01, texel, 0); }
    }
}

fn retouch_reference_load(page: i32, texel: vec2<i32>) -> vec4<f32> {
    switch page {
        case 0: { return textureLoad(source_21, texel, 0); }
        case 1: { return textureLoad(source_02, texel, 0); }
        case 2: { return textureLoad(source_12, texel, 0); }
        default: { return textureLoad(source_22, texel, 0); }
    }
}

fn clone_source_at(fragment_position: vec2<f32>) -> vec4<f32> {
    let mapping = retouch_mapping(array<vec4<u32>, 4>(
        material_sources.pages[1],
        material_sources.pages[2],
        material_sources.pages[3],
        material_sources.pages[4],
    ));
    return retouch_source(mapping, render_target.origin_extent.xy + fragment_position);
}

@vertex
fn vertex_main(@builtin(vertex_index) vertex_index: u32) -> @builtin(position) vec4<f32> {
    var positions = array<vec2<f32>, 3>(
        vec2<f32>(-1.0, -1.0),
        vec2<f32>( 3.0, -1.0),
        vec2<f32>(-1.0,  3.0),
    );
    return vec4<f32>(positions[vertex_index], 0.0, 1.0);
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

fn contact_coverage(dab: Dab, world: vec2<f32>) -> f32 {
    return contact_coverage_field(dab, world, contact_field(world));
}
fn contact_coverage_field(dab: Dab, world: vec2<f32>, field: vec2<f32>) -> f32 {
    return brush_footprint(dab, world, field) * brush_selection_at(world);
}

fn contact_segment_progress(dab: Dab, world: vec2<f32>) -> f32 {
    let motion_squared = dot(dab.motion, dab.motion);
    if motion_squared <= 0.000001 {
        return 1.0;
    }
    let previous_center = dab.center - dab.motion;
    return clamp(dot(world - previous_center, dab.motion) / motion_squared, 0.0, 1.0);
}

// ColorMixSpace discriminants, carried in render_mode.z.
const MIX_OKLAB: u32 = 1u;
const MIX_CLASSIC: u32 = 2u;

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

fn brush_blend() -> u32 {
    return u32(style.render_mode.x);
}

fn brush_normal() -> bool {
    return blend_mode(brush_blend()) == 0u;
}

fn deposit_encoded() -> bool {
    return (brush_blend() & BLEND_PERCEPTUAL) != 0u && style.operation.w == 0u;
}

fn deposit_color(color: vec3<f32>) -> vec3<f32> {
    if deposit_encoded() {
        return sdr_encode(color, WORKING_SPACE);
    }
    return color;
}

fn deposit_paint(paint: vec4<f32>) -> vec4<f32> {
    if deposit_encoded() {
        return working_encode(paint);
    }
    return paint;
}

fn stored_paint(paint: vec4<f32>) -> vec4<f32> {
    if deposit_encoded() {
        return working_decode(paint);
    }
    return paint;
}

fn source_over(destination: vec4<f32>, source_color: vec3<f32>, source_alpha: f32) -> vec4<f32> {
    let da = destination.a;
    // Normal blending has no backdrop-color term. Besides avoiding an
    // unassociation and two redundant products, keep this direct form: the
    // expanded general expression produces dark contact edges in specialized
    // shaders on the Wacom's Adreno Vulkan driver (including native saved paint).
    if brush_normal() {
        if style.color.a > 0.5 {
            return vec4<f32>(mix(destination.rgb, source_color * da, source_alpha), da);
        }
        return vec4<f32>(
            destination.rgb * (1.0 - source_alpha) + source_color * source_alpha,
            source_alpha + da * (1.0 - source_alpha),
        );
    }
    let backdrop = working_unassociate(destination);
    let blended = blend(source_color, backdrop, brush_blend());
    if style.color.a > 0.5 {
        return vec4<f32>(mix(destination.rgb, blended * da, source_alpha), da);
    }
    let rgb = destination.rgb * (1.0 - source_alpha)
        + source_color * source_alpha * (1.0 - da)
        + blended * source_alpha * da;
    return vec4<f32>(rgb, source_alpha + da * (1.0 - source_alpha));
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

fn stroke_increment(previous: f32, next: f32) -> f32 {
    let opacity = style.canvas_opacity.z;
    return clamp(working_ratio(opacity * (next - previous), 1.0 - opacity * previous), 0.0, 1.0);
}

fn grouped_opacity() -> bool {
    return style.canvas_opacity.z < 1.0
        && (MATERIAL_OPERATION == OP_DEPOSIT || MATERIAL_OPERATION == OP_COVERAGE || MATERIAL_OPERATION == OP_CLONE);
}

fn deposit_unchanged(paint: vec4<f32>, background: vec4<f32>, pigment: vec3<f32>) -> bool {
    if grouped_opacity() {
        let source = select(vec4<f32>(pigment,1.0),vec4<f32>(0.0),style.operation.w!=0u);
        let limit_color = background*(1.0-style.canvas_opacity.z)+source*style.canvas_opacity.z;
        if paint.a!=limit_color.a { return false; }
        return all(paint==stored_paint(limit_color));
    }
    return (style.operation.w!=0u && all(paint==vec4<f32>(0.0)))
        || (style.operation.w==0u && paint.a==1.0 && all(paint.rgb==style.material_b.xyz));
}

fn grouped_deposit(result: vec4<f32>, background: vec4<f32>, pigment: vec3<f32>, alpha: f32) -> vec4<f32> {
    let opacity = style.canvas_opacity.z;
    if opacity<=0.0 { return result; }
    if style.operation.w != 0u {
        return result * (1.0-alpha) + background * ((1.0-opacity)*alpha);
    }
    if brush_normal() {
        let source = select(vec4<f32>(pigment,1.0),vec4<f32>(pigment*background.a,background.a),style.color.a>0.5);
        return mix(result,background*(1.0-opacity)+source*opacity,alpha);
    }
    let full = (result-background*(1.0-opacity))/opacity;
    return mix(background,source_over(full,pigment,alpha),opacity);
}

struct SmudgeTrace {
    coordinate: vec2<f32>,
    influence: f32,
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
    let original = canvas_load(world);
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

struct MaterialOutput {
    @location(0) color: vec4<f32>,
    @location(1) coverage: vec4<f32>,
    @location(2) wetness: vec4<f32>,
}

fn wet_fragment(
    fragment_position: vec2<f32>,
    world: vec2<f32>,
    first: u32,
    count: u32,
) -> MaterialOutput {
    let original = canvas_load(world);
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

    if (MATERIAL_OPERATION == OP_DEPOSIT || MATERIAL_OPERATION == OP_COVERAGE) && contact_feature(64u,style.contact_a.z>0.5)
        && contact_feature(8192u,style.material_a.w>0.5) {
        let range = material_sources.header.zw;
        var paint = dry_original(vec2<i32>(floor(fragment_position.xy)));
        let grouped = grouped_opacity();
        var background = vec4<f32>(0.0);
        if grouped { background = deposit_paint(dry_stroke_start(vec2<i32>(floor(fragment_position.xy)))); }
        if MATERIAL_OPERATION == OP_DEPOSIT && contact_feature(4096u,style.material_a.x>0.0)
            && !grouped && deposit_unchanged(paint,background,dabs[range.x].color.rgb) {
            return MaterialOutput(paint,vec4<f32>(0.0),vec4<f32>(0.0));
        }
        let field = contact_field(world);
        var dose = 0.0;
        for (var offset = 0u; offset < range.y; offset += 1u) {
            let dab = dabs[range.x+offset];
            dose += brush_footprint(dab,world,field)*dab.flow*dab.color.a;
        }
        let remaining = exp(-6.0*dose);
        if grouped && dose>0.0 {
            paint = stored_paint(grouped_deposit(deposit_paint(paint),background,dabs[range.x].color.rgb,1.0-remaining));
        } else if style.operation.w != 0u { paint *= remaining; }
        else if dose>0.0 {
            let alpha=1.0-remaining;
            let pigment=style.material_b.xyz;
            if paint.a==0.0 || all(paint.rgb==pigment*paint.a) {
                let coverage=alpha+paint.a*(1.0-alpha);
                paint=vec4<f32>(pigment*coverage,coverage);
            } else {
                let destination=deposit_paint(paint);
                paint=stored_paint(vec4<f32>(destination.rgb*(1.0-alpha)+dabs[range.x].color.rgb*alpha,
                    alpha+destination.a*(1.0-alpha)));
            }
        }
        var coverage = 0.0;
        if MATERIAL_OPERATION == OP_COVERAGE {
            let prior = select(dry_coverage(vec2<i32>(floor(fragment_position.xy))),0.0,style.canvas_opacity.w>0.5);
            coverage = 1.0-(1.0-prior)*remaining;
        }
        return MaterialOutput(paint,vec4<f32>(coverage,0.0,0.0,1.0),vec4<f32>(0.0));
    }

    var result = dry_original(vec2<i32>(floor(fragment_position.xy)));
    let state_coordinate = clamp(
        vec2<i32>(floor(fragment_position.xy)),
        vec2<i32>(0),
        vec2<i32>(255),
    );
    var stroke_coverage = dry_coverage(state_coordinate);
    if style.canvas_opacity.w > 0.5 {
        stroke_coverage = 0.0;
    }
    let range = material_sources.header.zw;
    let grouped = grouped_opacity();
    var background = vec4<f32>(0.0);
    if grouped { background = deposit_paint(dry_stroke_start(state_coordinate)); }
    if !grouped && contact_uniform() && brush_normal() && !bristles_enabled() && stroke_coverage >= 1.0 {
        return MaterialOutput(result, vec4<f32>(stroke_coverage, 0.0, 0.0, 1.0), vec4<f32>(0.0));
    }
    if MATERIAL_OPERATION == OP_DEPOSIT && grouped && style.material_a.w>0.5
        && deposit_unchanged(result,background,dabs[range.x].color.rgb) {
        return MaterialOutput(result,vec4<f32>(0.0),vec4<f32>(0.0));
    }
    let telescoped = !grouped && MATERIAL_OPERATION == OP_CLONE && contact_uniform() && brush_normal();
    var clone_source = vec4<f32>(0.0);
    if MATERIAL_OPERATION == OP_CLONE && !telescoped {
        let source = clone_source_at(fragment_position.xy);
        clone_source = vec4<f32>(deposit_color(working_unassociate(source)), source.a);
    }
    var deposited = false;
    let first_coverage = stroke_coverage;
    let tooth = contact_paper(world);
    let field = contact_field_with_paper(world, tooth);
    for (var offset = 0u; offset < range.y; offset += 1u) {
        let dab = dabs[range.x + offset];
        var pigment = dab.color.rgb;
        var coverage = 0.0;
        var repaint = false;
        if bristles_enabled() {
            let paint = bristle_paint(dab, world, field.y);
            let selected = brush_selection_at(world);
            if paint.held {
                stroke_coverage = max(stroke_coverage, paint.coverage * selected);
                continue;
            }
            pigment = paint.color;
            coverage = paint.opacity * select(paint.coverage * selected,
                max(paint.coverage * selected, stroke_coverage), paint.reached);
            repaint = paint.reached || (paint.rim && stroke_coverage >= 1.0);
        } else {
            coverage = contact_coverage_field(dab, world, field);
        }
        if coverage <= 0.0 { continue; }
        let selected = brush_selection_at(world);
        let requested_alpha = brush_alpha(dab,coverage/max(selected,0.000001))*selected;
        if telescoped {
            stroke_coverage = max(stroke_coverage,requested_alpha);
            continue;
        }
        var source_alpha = requested_alpha;
        if grouped {
            if MATERIAL_OPERATION == OP_CLONE && contact_uniform() {
                let next = max(stroke_coverage,requested_alpha);
                source_alpha = clamp(working_ratio(clone_source.a*(next-stroke_coverage),
                    1.0-clone_source.a*stroke_coverage),0.0,1.0);
                stroke_coverage = next;
            } else if bristles_enabled() && contact_uniform() {
                if repaint { stroke_coverage = 0.0; }
                let next = max(stroke_coverage,source_alpha);
                source_alpha = clamp(working_ratio(next-stroke_coverage,1.0-stroke_coverage),0.0,1.0);
                stroke_coverage = next;
            } else {
                if MATERIAL_OPERATION == OP_CLONE { source_alpha *= clone_source.a; }
                stroke_coverage += (1.0-stroke_coverage)*source_alpha;
            }
        } else if contact_uniform() {
            if repaint && style.canvas_opacity.z == 1.0 { stroke_coverage = 0.0; }
            let next_coverage = select(stroke_coverage + (1.0 - stroke_coverage) * requested_alpha,
                max(stroke_coverage, requested_alpha), MATERIAL_OPERATION == OP_CLONE
                    || (bristles_enabled() && style.canvas_opacity.z == 1.0));
            if MATERIAL_OPERATION == OP_CLONE {
                source_alpha = clamp(
                    stroke_increment(clone_source.a * stroke_coverage, clone_source.a * next_coverage),
                    0.0,
                    1.0,
                );
            } else {
                source_alpha = clamp(
                    stroke_increment(stroke_coverage, next_coverage),
                    0.0,
                    1.0,
                );
            }
            stroke_coverage = next_coverage;
        } else {
            if MATERIAL_OPERATION == OP_CLONE { source_alpha *= clone_source.a; }
            if MATERIAL_OPERATION == OP_COVERAGE || style.canvas_opacity.z < 1.0 {
                let next_coverage = stroke_coverage + (1.0 - stroke_coverage) * source_alpha;
                source_alpha = stroke_increment(stroke_coverage, next_coverage);
                stroke_coverage = next_coverage;
            }
        }
        if grouped && source_alpha>0.0 {
            if !deposited { deposited = true; result = deposit_paint(result); }
            result = grouped_deposit(result,background,select(pigment,clone_source.rgb,MATERIAL_OPERATION==OP_CLONE),source_alpha);
        } else if style.operation.w != 0u { result *= 1.0 - source_alpha; }
        else if source_alpha > 0.0 {
            if !deposited {
                deposited = true;
                result = deposit_paint(result);
            }
            if MATERIAL_OPERATION == OP_CLONE { result = source_over(result, clone_source.rgb, source_alpha); }
            else { result = source_over(result, pigment, source_alpha); }
        }
    }
    if deposited {
        result = stored_paint(result);
    }
    if telescoped && stroke_coverage > first_coverage {
        let source = clone_source_at(fragment_position.xy);
        let alpha = clamp(
            stroke_increment(source.a * first_coverage, source.a * stroke_coverage),
            0.0,
            1.0,
        );
        if style.operation.w != 0u { result *= 1.0 - alpha; }
        else { result = stored_paint(source_over(deposit_paint(result), deposit_color(working_unassociate(source)), alpha)); }
    }
    return MaterialOutput(
        result,
        vec4<f32>(stroke_coverage, 0.0, 0.0, 1.0),
        vec4<f32>(0.0),
    );
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

fn material_result(fragment_position: vec4<f32>) -> MaterialOutput {
    if MATERIAL_OPERATION == OP_DEPOSIT || MATERIAL_OPERATION == OP_COVERAGE || MATERIAL_OPERATION == OP_CLONE {
        let p = vec2<u32>(fragment_position.xy);
        let bounds = material_sources.pages[0];
        if any(p < bounds.xy) || any(p >= bounds.zw) {
            return MaterialOutput(
                dry_original(vec2<i32>(p)),
                vec4<f32>(dry_coverage(vec2<i32>(p)), 0.0, 0.0, 1.0),
                vec4<f32>(0.0),
            );
        }
    }
    var result = paint_fragment(fragment_position);
    if style.color.a > 0.5 && !contact_feature(8192u,false) {
        let world = render_target.origin_extent.xy + fragment_position.xy;
        var original: vec4<f32>;
        if MATERIAL_OPERATION == OP_DEPOSIT || MATERIAL_OPERATION == OP_COVERAGE || MATERIAL_OPERATION == OP_CLONE {
            original = dry_original(vec2<i32>(floor(fragment_position.xy)));
        } else { original = canvas_load(world); }
        if style.operation.w != 0u { result.color = original; }
        else {
            result.color = vec4<f32>(working_unassociate(result.color) * original.a, original.a);
        }
        result.wetness *= select(0.0, 1.0, original.a > 0.0);
    }
    result.color = layer_color(result.color, WORKING_LUMA, style.color_mode.x, style.color_mode.y);
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
// dry_material::shader supplies material_color_output and dry_original.
@group(0) @binding(2) var material_coverage_output: texture_storage_2d<r32float, write>;
override MATERIAL_IN_PLACE: bool = false;
fn dry_block_result(id: vec2<u32>) -> MaterialOutput {
    let block = style.operation.z;
    return material_result(vec4<f32>(vec2<f32>(id * block) + 0.5 * f32(block), 0.0, 1.0));
}

fn cropped_color() -> bool {
    return MATERIAL_OPERATION==OP_DEPOSIT && contact_feature(64u,style.contact_a.z>0.5) && !contact_uniform();
}

@compute @workgroup_size(32, 2)
fn copy_color(@builtin(global_invocation_id) id:vec3<u32>) {
    if any(id.xy>=textureDimensions(material_color_output)) {return;}
    let block=style.operation.z;
    let bounds=material_sources.pages[0];
    if all(id.xy>=bounds.xy/block) && all(id.xy*block<bounds.zw) {return;}
    store_display_color(id.xy,dry_original(vec2<i32>(id.xy*block)));
}

@compute @workgroup_size(32, 2)
fn compute_display_color(@builtin(global_invocation_id) invocation: vec3<u32>) {
    let block=style.operation.z;
    let cropped=cropped_color();
    let bounds=material_sources.pages[0];
    let id=vec3<u32>(invocation.xy+select(vec2<u32>(0u),bounds.xy/block,cropped),invocation.z);
    if any(id.xy >= textureDimensions(material_color_output)) || (cropped && any(id.xy*block>=bounds.zw)) { return; }
    let result = dry_block_result(id.xy);
    store_display_color(id.xy,result.color);
}

fn saturated_dry_coverage(p: vec2<i32>) -> f32 {
    if !grouped_opacity() && MATERIAL_IN_PLACE && style.canvas_opacity.w < 0.5
        && contact_uniform() && brush_normal() && !bristles_enabled() {
        return dry_coverage(p);
    }
    return 0.0;
}

@compute @workgroup_size(32, 2)
fn compute_color(@builtin(global_invocation_id) invocation: vec3<u32>) {
    let cropped=cropped_color();
    let bounds=material_sources.pages[0];
    let id=invocation.xy+select(vec2<u32>(0u),bounds.xy,cropped);
    if any(id>=vec2<u32>(256u)) || (cropped && any(id>=bounds.zw)) {return;}
    let p=vec2<i32>(id);
    if saturated_dry_coverage(p)>=1.0 {return;}
    let result=material_result(vec4<f32>(vec2<f32>(id)+0.5,0.0,1.0));
    if !MATERIAL_IN_PLACE || any(result.color!=dry_original(p)) {
        textureStore(material_color_output,p,result.color);
        mark_changed_cell(id);
    }
}

@compute @workgroup_size(32, 2)
fn compute_coverage(@builtin(global_invocation_id) invocation: vec3<u32>) {
    let id=invocation.xy;
    if any(id>=vec2<u32>(256u)) {return;}
    let p=vec2<i32>(id);
    let coverage=saturated_dry_coverage(p);
    if coverage>=1.0 {
        textureStore(material_coverage_output,p,vec4<f32>(coverage,0.0,0.0,1.0));
        return;
    }
    let result=material_result(vec4<f32>(vec2<f32>(id)+0.5,0.0,1.0));
    if !MATERIAL_IN_PLACE || any(result.color!=dry_original(p)) {
        textureStore(material_color_output,p,result.color);
        mark_changed_cell(id);
    }
    textureStore(material_coverage_output,p,result.coverage);
}
