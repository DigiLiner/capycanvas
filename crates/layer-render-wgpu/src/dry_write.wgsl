@group(0) @binding(1) var material_color_output: texture_storage_2d<rgba32float, write>;

fn load_dry_destination(pixel: vec2<u32>) {}
fn dry_start_stored() -> bool {
    return false;
}
fn dry_original(p: vec2<i32>) -> vec4<f32> {
    return sampled_original(p);
}
fn store_dry_color(pixel: vec2<u32>, color: vec4<f32>) {
    textureStore(material_color_output, vec2<i32>(pixel), color);
}
