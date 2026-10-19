@group(0) @binding(3) var material_wetness_output: texture_storage_2d<r32float, write>;

fn transport_dirty(id: vec2<u32>) -> bool {
    let bounds = material_sources.pages[0];
    return all(id >= bounds.xy) && all(id < bounds.zw);
}

fn transport_result(id: vec2<u32>) -> MaterialOutput {
    return material_result(vec4<f32>(vec2<f32>(id) + 0.5, 0.0, 1.0));
}

@compute @workgroup_size(TRANSPORT_WORKGROUP_X, TRANSPORT_WORKGROUP_Y)
fn transport_color(@builtin(global_invocation_id) id: vec3<u32>) {
    if transport_dirty(id.xy) {
        store_dry_color(id.xy, transport_result(id.xy).color);
    } else {
        store_dry_color(id.xy, textureLoad(source_11, vec2<i32>(id.xy), 0));
    }
}

fn store_transport_coverage(id: vec2<u32>, dirty: bool) -> MaterialOutput {
    let p = vec2<i32>(id);
    if dirty {
        let result = transport_result(id);
        store_dry_color(id, result.color);
        textureStore(material_coverage_output, p, result.coverage);
        return result;
    }
    store_dry_color(id, textureLoad(source_11, p, 0));
    textureStore(material_coverage_output, p, vec4<f32>(dry_coverage(p), 0.0, 0.0, 1.0));
    return MaterialOutput(vec4<f32>(0.0), vec4<f32>(0.0), vec4<f32>(0.0));
}

@compute @workgroup_size(TRANSPORT_WORKGROUP_X, TRANSPORT_WORKGROUP_Y)
fn transport_coverage(@builtin(global_invocation_id) id: vec3<u32>) {
    store_transport_coverage(id.xy, transport_dirty(id.xy));
}

@compute @workgroup_size(TRANSPORT_WORKGROUP_X, TRANSPORT_WORKGROUP_Y)
fn transport_watercolor(@builtin(global_invocation_id) id: vec3<u32>) {
    let dirty = transport_dirty(id.xy);
    let wetness = store_transport_coverage(id.xy, dirty).wetness;
    if dirty {
        textureStore(material_wetness_output, vec2<i32>(id.xy), wetness);
    } else if material_sources.header.w != 0u {
        textureStore(material_wetness_output, vec2<i32>(id.xy), textureLoad(reservoir_texture, vec2<i32>(id.xy), 0));
    }
}
