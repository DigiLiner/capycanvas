@group(0) @binding(1) var material_color_output: texture_storage_2d<rgba32float, read_write>;
@group(0) @binding(3) var<storage, read_write> changed_cells: ChangedCells;
var<private> destination_texel: vec4<f32>;

fn load_dry_destination(pixel: vec2<u32>) {
    destination_texel = textureLoad(material_color_output, vec2<i32>(pixel));
}
fn dry_start_stored() -> bool {
    return style.flags.w < 1.5;
}
fn dry_original(p: vec2<i32>) -> vec4<f32> {
    return select(vec4<f32>(0.), destination_texel, dry_start_stored());
}
fn store_dry_color(pixel: vec2<u32>, color: vec4<f32>) {
    if any(bitcast<vec4<u32>>(destination_texel) != bitcast<vec4<u32>>(color)) {
        textureStore(material_color_output, vec2<i32>(pixel), color);
        mark_changed_cell(pixel * style.operation.z);
    }
}
