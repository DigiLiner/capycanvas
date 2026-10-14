@group(0) @binding(2) var paper_image: texture_2d<f32>;
@group(0) @binding(3) var paper_sampler: sampler;

fn paper_height(p: vec2<f32>) -> f32 {
    let size = textureDimensions(paper_image);
    let uv = p / bake_parameters.paper.xy;
    if bake_parameters.mode.x > 0.5 {
        let n = vec2<i32>(size);
        let texel = vec2<i32>(floor(uv * vec2<f32>(size)));
        return textureLoad(paper_image, ((texel % n) + n) % n, 0).r;
    }
    return textureSampleLevel(paper_image, paper_sampler, uv, 0.0).r;
}
