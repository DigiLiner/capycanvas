@compute @workgroup_size(8, 8)
fn gather_samples(@builtin(global_invocation_id) id: vec3<u32>) {
    let p = vec2<i32>(id.xy);
    var samples = gathered_sample(render_target.origin_extent.xy + (vec2<f32>(id.xy) + 0.5));
    if material_sources.header.z != 0u {
        samples += textureLoad(reservoir_texture, p, 0);
    }
    store_dry_color(id.xy, samples);
}
