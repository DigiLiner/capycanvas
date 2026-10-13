fn material_contact(dab:Dab, world:vec2<f32>, field:vec2<f32>, prior:f32)->MaterialContact {
    return MaterialContact(dab.color.rgb,contact_coverage_field(dab,world,field),false,false);
}
