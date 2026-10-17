fn relief_contact(dab:Dab,world:vec2<f32>,paper:f32)->f32 {
    return relief_evaluate(dab,world,paper,false).x;
}
