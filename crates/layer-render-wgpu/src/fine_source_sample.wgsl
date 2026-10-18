struct FineDirectory { phase:vec2<i32>, page_side:u32, table_mask:u32, reserved:vec4<u32>, pages:array<vec4<i32>>, }
@group(0) @binding(5) var fine_pages:texture_2d<f32>;
@group(0) @binding(6) var<storage,read> fine_directory:FineDirectory;
fn fine_load(plane:u32,p:vec2<i32>,extent:vec2<f32>)->vec4<f32> {
    let coordinate=clamp(p,vec2(0),vec2<i32>(ceil(extent))-1)-fine_directory.phase;
    if any(coordinate<vec2(0)) {return vec4(0.);}
    let page=vec2<u32>(coordinate)/fine_directory.page_side;
    let key=vec2(page.x,page.y|(plane<<31u));
    var slot=(key.x*1664525u+key.y*1013904223u)&fine_directory.table_mask;
    for(var visited=0u;visited<=fine_directory.table_mask;visited++) {
        let entry=fine_directory.pages[slot];
        if entry.x<0 {return vec4(0.);}
        if all(bitcast<vec2<u32>>(entry.xy)==key) {
            let local=vec2<u32>(coordinate)%fine_directory.page_side;
            return textureLoad(fine_pages,entry.zw+vec2<i32>(local),0);
        }
        slot=(slot+1u)&fine_directory.table_mask;
    }
    return vec4(0.);
}
fn fine_sample(plane:u32,q:vec2<f32>,extent:vec2<f32>)->vec4<f32> {
    let coordinate=clamp(q,vec2(.5),ceil(extent)-.5)-.5;
    let low=vec2<i32>(floor(coordinate));let weight=fract(coordinate);
    return mix(mix(fine_load(plane,low,extent),fine_load(plane,low+vec2(1,0),extent),weight.x),
        mix(fine_load(plane,low+vec2(0,1),extent),fine_load(plane,low+vec2(1,1),extent),weight.x),weight.y);
}
fn fine_pyramid_sample(plane:u32,image:texture_2d<f32>,linear_sampler:sampler,extent:vec2<f32>,outside:f32,q:vec2<f32>,lod:f32,max_lod:f32)->vec4<f32> {
    if any(q<vec2(0.)) || any(q>=extent) {return vec4(outside);}
    let level=clamp(lod,0.,max_lod);
    if level>=1. {return pyramid_sample(image,linear_sampler,extent*.5,outside,q*.5,level-1.,max_lod-1.);}
    let low=fine_sample(plane,q,extent);
    let side=exp2(ceil(level));
    if !all(q<=(floor(extent/side)-.5)*side) && level<0.00001 {return low;}
    if level==0. {return low;}
    return mix(low,pyramid_sample(image,linear_sampler,extent*.5,outside,q*.5,0.,max_lod-1.),level);
}
fn mapped_source(plane:u32,q:vec2<f32>,lod:f32)->vec4<f32> {
    if plane==1u {return fine_pyramid_sample(plane,kept,linear_sampler,resample.source_extent.xy,resample.options.w,q,lod,resample.options.y);}
    return fine_pyramid_sample(plane,moved,linear_sampler,resample.source_extent.xy,resample.options.w,q,lod,resample.options.y);
}
