@fragment
fn gather_fragment(@builtin(position) position: vec4<f32>) -> @location(0) vec4<f32> {
    let world = render_target.origin_extent.xy + position.xy;
    return gathered_sample(world) + textureLoad(reservoir_texture, vec2<i32>(floor(position.xy)), 0);
}
