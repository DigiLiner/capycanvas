fn brush_alpha(dab: Dab, coverage: f32) -> f32 {
    let dose = coverage*dab.flow*dab.color.a;
    if contact_feature(1u,style.contact_a.x>0.5) && !contact_uniform() {
        return 1.0-exp(-6.0*dose);
    }
    return clamp(dose,0.0,1.0);
}
