fn material_contact(dab:Dab, world:vec2<f32>, field:vec2<f32>, prior:f32, selected:f32)->MaterialContact {
    return MaterialContact(dab.color.rgb,brush_footprint(dab,world,field)*selected,false,false);
}
