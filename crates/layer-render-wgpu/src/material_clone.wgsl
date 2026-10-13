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
