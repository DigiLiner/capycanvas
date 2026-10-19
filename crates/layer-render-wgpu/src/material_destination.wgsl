fn transport_dirty(id: vec2<u32>) -> bool {
    let bounds = material_sources.pages[0];
    return all(id >= bounds.xy) && all(id < bounds.zw);
}

fn transport_result(id: vec2<u32>) -> MaterialOutput {
    return material_result(vec4<f32>(vec2<f32>(id) + 0.5, 0.0, 1.0));
}

@compute @workgroup_size(8, 8)
fn transport_color(@builtin(global_invocation_id) id: vec3<u32>) {
    if transport_dirty(id.xy) {
        store_dry_color(id.xy, transport_result(id.xy).color);
    } else {
        store_dry_color(id.xy, textureLoad(source_11, vec2<i32>(id.xy), 0));
    }
}

@compute @workgroup_size(8, 8)
fn transport_coverage(@builtin(global_invocation_id) id: vec3<u32>) {
    let p = vec2<i32>(id.xy);
    if transport_dirty(id.xy) {
        let result = transport_result(id.xy);
        store_dry_color(id.xy, result.color);
        textureStore(material_coverage_output, p, result.coverage);
    } else {
        store_dry_color(id.xy, textureLoad(source_11, p, 0));
        textureStore(material_coverage_output, p, vec4<f32>(dry_coverage(p), 0.0, 0.0, 1.0));
    }
}
