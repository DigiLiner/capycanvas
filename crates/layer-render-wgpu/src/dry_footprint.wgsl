fn brush_footprint(dab:Dab, world:vec2<f32>, field:vec2<f32>)->f32 {
    return relief_contact(dab,world,field.y);
}
