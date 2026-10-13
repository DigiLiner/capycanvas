@group(0) @binding(2) var material_coverage_output: texture_storage_2d<r32float, write>;
override MATERIAL_IN_PLACE: bool = false;
fn dry_block_result(id: vec2<u32>) -> MaterialOutput {
    let block = style.operation.z;
    return material_result(vec4<f32>(vec2<f32>(id * block) + 0.5 * f32(block), 0.0, 1.0));
}

fn cropped_color() -> bool {
    return MATERIAL_OPERATION==OP_DEPOSIT && contact_feature(64u,style.contact_a.z>0.5) && !contact_uniform();
}

@compute @workgroup_size(32, 2)
fn copy_color(@builtin(global_invocation_id) id:vec3<u32>) {
    if any(id.xy>=textureDimensions(material_color_output)) {return;}
    let block=style.operation.z;
    let bounds=material_sources.pages[0];
    if all(id.xy>=bounds.xy/block) && all(id.xy*block<bounds.zw) {return;}
    store_display_color(id.xy,dry_original(vec2<i32>(id.xy*block)));
}

@compute @workgroup_size(32, 2)
fn compute_display_color(@builtin(global_invocation_id) invocation: vec3<u32>) {
    let block=style.operation.z;
    let cropped=cropped_color();
    let bounds=material_sources.pages[0];
    let id=vec3<u32>(invocation.xy+select(vec2<u32>(0u),bounds.xy/block,cropped),invocation.z);
    if any(id.xy >= textureDimensions(material_color_output)) || (cropped && any(id.xy*block>=bounds.zw)) { return; }
    let result = dry_block_result(id.xy);
    store_display_color(id.xy,result.color);
}

fn saturated_dry_coverage(p: vec2<i32>) -> f32 {
    if !grouped_opacity() && MATERIAL_IN_PLACE && style.canvas_opacity.w < 0.5
        && contact_uniform() && brush_normal() && !bristles_enabled() {
        return dry_coverage(p);
    }
    return 0.0;
}

@compute @workgroup_size(32, 2)
fn compute_color(@builtin(global_invocation_id) invocation: vec3<u32>) {
    let cropped=cropped_color();
    let bounds=material_sources.pages[0];
    let id=invocation.xy+select(vec2<u32>(0u),bounds.xy,cropped);
    if any(id>=vec2<u32>(256u)) || (cropped && any(id>=bounds.zw)) {return;}
    let p=vec2<i32>(id);
    if saturated_dry_coverage(p)>=1.0 {return;}
    let result=material_result(vec4<f32>(vec2<f32>(id)+0.5,0.0,1.0));
    if !MATERIAL_IN_PLACE || any(result.color!=dry_original(p)) {
        textureStore(material_color_output,p,result.color);
        mark_changed_cell(id);
    }
}

@compute @workgroup_size(32, 2)
fn compute_coverage(@builtin(global_invocation_id) invocation: vec3<u32>) {
    let id=invocation.xy;
    if any(id>=vec2<u32>(256u)) {return;}
    let p=vec2<i32>(id);
    let coverage=saturated_dry_coverage(p);
    if coverage>=1.0 {
        textureStore(material_coverage_output,p,vec4<f32>(coverage,0.0,0.0,1.0));
        return;
    }
    let result=material_result(vec4<f32>(vec2<f32>(id)+0.5,0.0,1.0));
    if !MATERIAL_IN_PLACE || any(result.color!=dry_original(p)) {
        textureStore(material_color_output,p,result.color);
        mark_changed_cell(id);
    }
    textureStore(material_coverage_output,p,result.coverage);
}
