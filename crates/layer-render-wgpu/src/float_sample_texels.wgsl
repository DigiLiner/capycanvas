fn bilinear_texels(image:texture_2d<f32>,uv:vec2<f32>,level:i32)->vec4<f32> {
    let last=vec2<i32>(textureDimensions(image,level))-1;
    let p=uv*vec2<f32>(last+1)-.5;
    let t=fract(p);
    let a=clamp(vec2<i32>(floor(p)),vec2(0),last);
    let b=clamp(vec2<i32>(floor(p))+1,vec2(0),last);
    return mix(mix(textureLoad(image,a,level),textureLoad(image,vec2(b.x,a.y),level),t.x),
        mix(textureLoad(image,vec2(a.x,b.y),level),textureLoad(image,b,level),t.x),t.y);
}
fn sample_float(image:texture_2d<f32>,sampling:sampler,uv:vec2<f32>,level:f32)->vec4<f32> {
    let lod=clamp(level,0.,f32(textureNumLevels(image)-1u));
    let low=floor(lod);
    let near=bilinear_texels(image,uv,i32(low));
    if lod==low {return near;}
    return mix(near,bilinear_texels(image,uv,i32(low)+1),lod-low);
}
