fn material_contact(dab:Dab, world:vec2<f32>, field:vec2<f32>, prior:f32)->MaterialContact {
    if bristles_enabled() {
        let paint=bristle_paint(dab,world,field.y);
        let selected=brush_selection_at(world);
        if paint.held {return MaterialContact(paint.color,paint.coverage*selected,false,true);}
        let coverage=paint.opacity*select(paint.coverage*selected,max(paint.coverage*selected,prior),paint.reached);
        return MaterialContact(paint.color,coverage,paint.reached || (paint.rim && prior>=1.0),false);
    }
    return MaterialContact(dab.color.rgb,contact_coverage_field(dab,world,field),false,false);
}
