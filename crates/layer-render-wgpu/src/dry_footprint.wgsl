fn brush_footprint(dab:Dab, world:vec2<f32>, field:vec2<f32>)->f32 {
    return relief_contact(dab,world,field.y);
}
fn footprint_and_start(dab:Dab, world:vec2<f32>, field:vec2<f32>)->vec2<f32> {
    return vec2<f32>(relief_contact(dab,world,field.y),0.0);
}
