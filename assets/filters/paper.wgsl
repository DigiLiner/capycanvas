fn capy_paper(c:vec4<f32>,p:vec2<f32>,b:u32)->vec4<f32>{
    return fx_rgba(fx_parameter(b,0u).rgb,1.);
}
