fn sample_float(image:texture_2d<f32>,sampling:sampler,uv:vec2<f32>,level:f32)->vec4<f32> {
    return textureSampleLevel(image,sampling,uv,level);
}
