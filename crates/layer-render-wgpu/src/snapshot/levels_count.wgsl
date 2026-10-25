var<workgroup> bins:array<atomic<u32>,4096>;
@compute @workgroup_size(64)
fn count(@builtin(local_invocation_index) lane:u32,@builtin(workgroup_id) group:vec3<u32>) {
    for(var i=lane;i<4096u;i+=64u) {atomicStore(&bins[i],0u);}workgroupBarrier();
    let channel=group.y;let range=area.ranges[channel];
    for(var i=group.x*64u+lane;i<area.size.x*area.size.y;i+=1024u) {
        let color=textureLoad(pixels,vec2<i32>(i32(i%area.size.x),i32(i/area.size.x)),0);
        if (bitcast<u32>(color.a)&0x7f800000u)==0u {continue;}
        let value=encoded(color[channel],color.a)/range.z;
        var index=0u;
        if range.y>range.x {index=u32(clamp(floor((value-range.x)/(range.y-range.x)*4096.),0.,4095.));}
        atomicAdd(&bins[index],1u);
    }
    workgroupBarrier();
    for(var i=lane;i<4096u;i+=64u) {let n=atomicLoad(&bins[i]);if n!=0u {atomicAdd(&shards[(group.x*3u+channel)*4096u+i],n);}}
}
