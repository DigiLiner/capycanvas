fn paper_shade_factor(slope: vec2<f32>, light: vec4<f32>) -> f32 {
    return max(0., 1. - light.z * dot(slope, light.xy));
}

fn paper_lit(color: vec4<f32>, shade: f32, weight: f32) -> vec4<f32> {
    var s = 1. - (1. - shade) * weight;
    let brightest = max(color.r, max(color.g, color.b));
    if s > 1. && brightest > 0. {
        s = min(s, max(1., color.a / brightest));
    }
    return vec4<f32>(color.rgb * s, color.a);
}
