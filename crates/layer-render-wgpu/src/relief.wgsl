fn relief_transfer(x: f32) -> f32 {
    let t = clamp(x, 0.0, 1.0);
    return t*t*(3.0-2.0*t);
}

fn relief_inverse(height: f32, profile: vec3<f32>) -> f32 {
    let h = height-profile.x;
    if h <= 0.0 { return -1.0; }
    if contact_feature(2048u,true) && profile.z > 0.000001 {
        return 2.0*h/(profile.y+sqrt(profile.y*profile.y+4.0*profile.z*h));
    }
    return h/max(profile.y, 0.000001);
}

fn relief_ray(o: vec2<f32>, v: vec2<f32>) -> vec4<f32> {
    let speed2 = dot(v,v);
    if speed2 < 0.00000001 { return vec4<f32>(0.0,dot(o,o),0.0,0.0); }
    let middle = -dot(o,v)/speed2;
    let nearest = o+v*middle;
    return vec4<f32>(middle,dot(nearest,nearest),speed2,1.0/speed2);
}

fn relief_range(ray: vec4<f32>, limit: f32, bounds: vec2<f32>) -> vec2<f32> {
    if ray.z < 0.00000001 {
        return select(vec2<f32>(bounds.x), bounds, ray.y <= limit);
    }
    let reach2 = (limit-ray.y)*ray.w;
    if reach2 <= 0.0 { return vec2<f32>(bounds.x); }
    let reach = sqrt(reach2);
    let lo = clamp(ray.x-reach,bounds.x,bounds.y);
    return vec2<f32>(lo,max(lo,clamp(ray.x+reach,bounds.x,bounds.y)));
}

fn relief_polynomial_pair(t2: f32, z: vec4<f32>, z4: f32) -> f32 {
    let even = z.x+t2*(z.z+t2*z4);
    let odd = z.y+t2*z.w;
    return even*even*(3.0-2.0*even)+3.0*t2*odd*odd*(1.0-2.0*even);
}

fn relief_partial(o: vec2<f32>, v: vec2<f32>, interval: vec2<f32>, gap: f32,
    k: f32, profile: vec3<f32>) -> f32 {
    let half = (interval.y-interval.x)*0.5;
    if half <= 0.0 { return 0.0; }
    let center = o+v*((interval.x+interval.y)*0.5);
    let velocity = v*half;
    let q0 = dot(center,center);
    let q1 = 2.0*dot(center,velocity);
    let q2 = dot(velocity,velocity);
    let slope = profile.y+2.0*profile.z*q0;
    let scale = -k*style.material_a.y;
    let z0 = (gap-k*(profile.x+q0*(profile.y+profile.z*q0)))*style.material_a.y;
    let z1 = scale*slope*q1;
    let z2 = scale*(slope*q2+profile.z*q1*q1);
    var average = 0.0;
    if !contact_feature(2048u,true) || profile.z <= 0.000001 {
        average = z0*z0*(3.0-2.0*z0)+2.0*z0*z2*(1.0-z0)+z1*z1*(1.0-2.0*z0)
            +z2*z2*(0.6-1.2*z0)-1.2*z1*z1*z2-(2.0/7.0)*z2*z2*z2;
    } else {
        let z3 = scale*2.0*profile.z*q1*q2;
        let z4 = scale*profile.z*q2*q2;
        let z = vec4<f32>(z0,z1,z2,z3);
        average = 0.2089795918367347*z0*z0*(3.0-2.0*z0)
            +0.3818300505051189*relief_polynomial_pair(0.1647102868965424,z,z4)
            +0.2797053914892767*relief_polynomial_pair(0.5498684992164435,z,z4)
            +0.1294849661688697*relief_polynomial_pair(0.9008058292716293,z,z4);
    }
    return clamp(average,0.0,1.0)*half*2.0;
}

fn relief_piece(o: vec2<f32>, v: vec2<f32>, ray: vec4<f32>, bounds: vec2<f32>, gap: f32,
    k: f32, profile: vec3<f32>, whole: bool) -> vec2<f32> {
    let support = bounds;
    let full = support.y-support.x;
    if full <= 0.0 { return vec2<f32>(0.0); }
    let stroke = select(vec2<f32>(max(support.x,0.0),min(support.y,1.0)),support,whole);
    if stroke.y <= stroke.x { return vec2<f32>(0.0,full); }
    if ray.z < 0.00000001 || profile.y+profile.z <= 0.000001 {
        let q = dot(o,o);
        let c = relief_transfer((gap-k*(profile.x+q*(profile.y+profile.z*q)))*style.material_a.y);
        return vec2<f32>(c*(stroke.y-stroke.x),full);
    }
    let a = o+v*stroke.x;
    let b = o+v*stroke.y;
    let qmax = max(dot(a,a),dot(b,b));
    if gap-k*(profile.x+qmax*(profile.y+profile.z*qmax)) >= 1.0/style.material_a.y {
        return vec2<f32>(stroke.y-stroke.x,full);
    }
    let outer = relief_range(ray,relief_inverse(gap/k,profile),stroke);
    if outer.y <= outer.x { return vec2<f32>(0.0,full); }
    let inner = relief_range(ray,relief_inverse((gap-1.0/style.material_a.y)/k,profile),outer);
    let integral = inner.y-inner.x
        +relief_partial(o,v,vec2<f32>(outer.x,inner.x),gap,k,profile)
        +relief_partial(o,v,vec2<f32>(inner.y,outer.y),gap,k,profile);
    return vec2<f32>(integral,full);
}

fn relief_slab(o:f32,v:f32,inverse:f32,extent:f32)->vec2<f32> {
    if abs(v)<0.000001 {
        return select(vec2<f32>(1.0,-1.0),vec2<f32>(-1000000.0,1000000.0),abs(o)<=extent);
    }
    let a=(-extent-o)*inverse;
    let b=(extent-o)*inverse;
    return vec2<f32>(min(a,b),max(a,b));
}
fn relief_beveled_range(o:vec2<f32>,v:vec2<f32>,rim:vec2<f32>,inverse:vec4<f32>)->vec2<f32> {
    let face=vec2<f32>(1.0)-rim;
    let oo=o/rim;
    let vv=v/rim;
    let diagonal=1.0+face.x/rim.x+face.y/rim.y;
    let ax=relief_slab(o.x,v.x,inverse.x,1.0);
    let ay=relief_slab(o.y,v.y,inverse.y,1.0);
    let plus=relief_slab(oo.x+oo.y,vv.x+vv.y,inverse.z,diagonal);
    let minus=relief_slab(oo.x-oo.y,vv.x-vv.y,inverse.w,diagonal);
    let lo=max(max(ax.x,ay.x),max(plus.x,minus.x));
    let hi=min(min(ax.y,ay.y),min(plus.y,minus.y));
    return vec2<f32>(lo,max(lo,hi));
}

fn relief_origin(dab:Dab,origin:vec2<f32>)->vec2<f32> {
    if contact_feature(32768u,style.contact_c.z!=0.0) {return origin+dab.material.yz;}
    return origin;
}

fn relief_evaluate(dab: Dab, world: vec2<f32>, paper: f32, paired: bool) -> vec2<f32> {
    let relative = world-dab.center+dab.motion;
    let o = vec2<f32>(dot(relative,dab.metric.xy),dot(relative,dab.metric.zw));
    let corner = style.material_a.x;
    let rounded = contact_feature(4096u,corner>0.0);
    var v = vec2<f32>(-dab.invariants.x,0.0);
    var ray = vec4<f32>(o.x*dab.invariants.y,o.y*o.y,dab.invariants.z,dab.invariants.w);
    if rounded { v=dab.invariants.xy; ray=relief_ray(o,v); }
    else if contact_uniform() { ray=relief_ray(o,v); }
    else if ray.z<0.00000001 { ray=vec4<f32>(0.0,dot(o,o),0.0,0.0); }
    let progress = clamp(-dot(o,v)/max(dot(v,v),0.000001),0.0,1.0);
    let input = mix(dab.previous_contact,dab.contact,progress);
    let p = clamp(input.x,0.0,1.0);
    if p <= 0.0 { return vec2<f32>(0.0); }
    let d = 0.5-clamp(0.5+style.contact_a.y*(paper-0.5),0.0,1.0);
    let gap = 0.5*p-d;
    let k = 1.0+0.25*d;
    let profile = input.yzw;
    if gap <= k*profile.x { return vec2<f32>(0.0); }
    var engaged = true;
    var full_engaged = paired;
    if !rounded {
        let nearest = o+v*progress;
        let q = dot(nearest,nearest);
        engaged = q <= 1.0;
        if !contact_feature(32768u,style.contact_c.z!=0.0) {
            engaged = engaged && gap > k*(profile.x+q*(profile.y+profile.z*q));
        }
        if paired {
            let full_nearest = o+v*ray.x;
            let full_q = dot(full_nearest,full_nearest);
            full_engaged = full_q <= 1.0;
            if !contact_feature(32768u,style.contact_c.z!=0.0) {
                full_engaged = full_engaged && gap > k*(profile.x+full_q*(profile.y+profile.z*full_q));
            }
        }
        if !engaged && !full_engaged { return vec2<f32>(0.0); }
    }
    var sum = vec2<f32>(0.0);
    var full_integral = 0.0;
    if dot(dab.motion,dab.motion) < 0.000001 {
        var q = dot(o,o);
        if rounded {
            let axes = max(dab.radii,vec2<f32>(0.005));
            let rim = vec2<f32>(corner*min(axes.x,axes.y))/axes;
            let edge = max(abs(o)-(vec2<f32>(1.0)-rim),vec2<f32>(0.0))/rim;
            q = (edge.x+edge.y)*(edge.x+edge.y);
        }
        if q > 1.0 { return vec2<f32>(0.0); }
        if contact_feature(32768u,style.contact_c.z!=0.0) {
            let point=relief_origin(dab,o);q=dot(point,point);
        }
        sum = vec2<f32>(relief_transfer((gap-k*(profile.x+q*(profile.y+profile.z*q)))*style.material_a.y),1.0);
    } else if !rounded {
        let support=relief_range(ray,1.0,vec2<f32>(-1000000.0,1000000.0));
        let origin=relief_origin(dab,o);
        var field_ray=ray;
        if contact_feature(32768u,style.contact_c.z!=0.0) {
            field_ray=vec4<f32>(origin.x*dab.invariants.y,origin.y*origin.y,ray.z,ray.w);
            if ray.z<0.00000001 {field_ray=vec4<f32>(0.0,dot(origin,origin),0.0,0.0);}
        }
        sum.y = support.y-support.x;
        if engaged {sum.x = relief_piece(origin,v,field_ray,support,gap,k,profile,false).x;}
        if full_engaged {full_integral = relief_piece(origin,v,field_ray,support,gap,k,profile,true).x;}
    } else {
        var rim = dab.invariants.zw;
        if contact_uniform() {
            let axes = max((dab.radii+dab.previous.xy)*0.5,vec2<f32>(0.005));
            rim = vec2<f32>(corner*min(axes.x,axes.y))/axes;
        }
        let support = relief_beveled_range(o,v,rim,dab.material);
        if support.y <= support.x { return vec2<f32>(0.0); }
        let bounds = vec2<f32>(max(support.x,0.0),min(support.y,1.0));
        if bounds.y <= bounds.x { return vec2<f32>(0.0); }
        sum.y = support.y-support.x;
        let face = vec2<f32>(1.0)-rim;
        if all(max(abs(o+v*bounds.x),abs(o+v*bounds.y))<=face) {
            let contact=relief_transfer((gap-k*profile.x)*style.material_a.y);
            return vec2<f32>(contact*(bounds.y-bounds.x)/sum.y*(1.0-style.material_a.z+style.material_a.z*p),0.0);
        }
        let moving = abs(v)>vec2<f32>(0.000001);
        let safe = select(vec2<f32>(1.0),v,moving);
        let cuts = vec4<f32>(select(vec2<f32>(bounds.y),(-face-o)/safe,moving),
            select(vec2<f32>(bounds.y),(face-o)/safe,moving));
        var lo = bounds.x;
        for (var i = 0u; i < 5u; i += 1u) {
            let next = select(vec4<f32>(bounds.y),cuts,cuts>vec4<f32>(lo+0.000001));
            let hi = min(min(next.x,next.y),min(next.z,next.w));
            let mid = o+v*((lo+hi)*0.5);
            let engaged = abs(mid)>face;
            let edge_o = select(vec2<f32>(0.0),(sign(mid)*o-face)/rim,engaged);
            let po = vec2<f32>(edge_o.x+edge_o.y,0.0);
            let edge_v = select(vec2<f32>(0.0),sign(mid)*v/rim,engaged);
            let pv = vec2<f32>(edge_v.x+edge_v.y,0.0);
            sum.x += relief_piece(po,pv,relief_ray(po,pv),vec2<f32>(lo,hi),gap,k,profile,false).x;
            lo = hi;
            if hi >= bounds.y { break; }
        }
    }
    return clamp(vec2<f32>(sum.x,full_integral)/max(sum.y,0.000001),vec2<f32>(0.0),vec2<f32>(1.0))*(1.0-style.material_a.z+style.material_a.z*p);
}

fn relief_tap_pixel(dab:Dab,world:vec2<f32>,paper:f32,dose:f32)->f32 {
    let input=dab.previous_contact;
    let pressure=clamp(input.x,0.0,1.0);
    if pressure<=0.0 {return 0.0;}
    let profile=input.yzw;
    let depth=0.5-clamp(0.5+style.contact_a.y*(paper-0.5),0.0,1.0);
    let gap=0.5*pressure-depth;
    let k=1.0+0.25*depth;
    var outer=relief_inverse(gap/k,profile);
    if !contact_feature(32768u,style.contact_c.z!=0.0) {outer=min(1.0,outer);}
    if outer<=0.0 {return 0.0;}
    let inner=min(outer,relief_inverse((gap-1.0/style.material_a.y)/k,profile));
    let loading=1.0-style.material_a.z+style.material_a.z*pressure;
    let dx=vec2<f32>(dab.metric.x,dab.metric.z);
    let dy=vec2<f32>(dab.metric.y,dab.metric.w);
    let relative=world-dab.center+dab.motion;
    let physical=dx*relative.x+dy*relative.y;
    let origin=relief_origin(dab,physical);
    let a=origin+0.5*(dx+dy);
    let b=origin+0.5*(dx-dy);
    let c=origin-0.5*(dx+dy);
    let d=origin-0.5*(dx-dy);
    var inside=max(max(dot(a,a),dot(b,b)),max(dot(c,c),dot(d,d)))<=inner;
    if inside && contact_feature(32768u,style.contact_c.z!=0.0) {
        let aa=physical+0.5*(dx+dy);let bb=physical+0.5*(dx-dy);
        let cc=physical-0.5*(dx+dy);let dd=physical-0.5*(dx-dy);
        inside=max(max(dot(aa,aa),dot(bb,bb)),max(dot(cc,cc),dot(dd,dd)))<=1.0;
    }
    if inside {return loading;}
    let half=sqrt(outer)*length(dx)/abs(dab.metric.x*dab.metric.w-dab.metric.y*dab.metric.z);
    var center_y=relative.y;
    if contact_feature(32768u,style.contact_c.z!=0.0) {
        center_y+=(dab.metric.x*dab.material.z-dab.metric.z*dab.material.y)/(dab.metric.x*dab.metric.w-dab.metric.y*dab.metric.z);
    }
    var vertical=vec2<f32>(max(-0.5,-center_y-half),min(0.5,-center_y+half));
    if contact_feature(32768u,style.contact_c.z!=0.0) {
        let physical_half=length(dx)/abs(dab.metric.x*dab.metric.w-dab.metric.y*dab.metric.z);
        vertical=vec2<f32>(max(vertical.x,-relative.y-physical_half),min(vertical.y,-relative.y+physical_half));
    }
    if vertical.y<=vertical.x {return 0.0;}
    var integral=vec2<f32>(0.0);
    for(var row=0u;row<8u;row+=1u) {
        var node=0.1834346424956498;
        var weight=0.3626837833783620;
        switch row%4u {
            case 1u: {node=0.5255324099163290;weight=0.3137066458778873;}
            case 2u: {node=0.7966664774136267;weight=0.2223810344533745;}
            case 3u: {node=0.9602898564975363;weight=0.1012285362903763;}
            default: {}
        }
        let y=0.5*(vertical.x+vertical.y)+0.5*(vertical.y-vertical.x)*node*select(-1.0,1.0,row>=4u);
        let ray=relief_ray(origin+dy*y,dx);
        var bounds=vec2<f32>(-0.5,0.5);
        if contact_feature(32768u,style.contact_c.z!=0.0) {bounds=relief_range(relief_ray(physical+dy*y,dx),1.0,bounds);}
        let support=relief_range(ray,outer,bounds);
        if support.y<=support.x {continue;}
        let full=relief_range(ray,inner,support);
        var sum=vec2<f32>(loading,1.0-exp(-dose*loading))*(full.y-full.x);
        for(var edge=0u;edge<2u;edge+=1u) {
            let span=select(vec2<f32>(support.x,full.x),vec2<f32>(full.y,support.y),edge==1u);
            if span.y<=span.x {continue;}
            for(var column=0u;column<4u;column+=1u) {
                let extreme=column==0u || column==3u;
                let x=0.5*(span.x+span.y)+0.5*(span.y-span.x)
                    *select(0.3399810435848563,0.8611363115940526,extreme)*select(-1.0,1.0,column>=2u);
                let point=origin+dy*y+dx*x;
                let q=dot(point,point);
                let contact=loading*relief_transfer((gap-k*(profile.x+q*(profile.y+profile.z*q)))*style.material_a.y);
                sum+=vec2<f32>(contact,1.0-exp(-dose*contact))*0.5*(span.y-span.x)
                    *select(0.6521451548625461,0.3478548451374538,extreme);
            }
        }
        integral+=sum*0.5*(vertical.y-vertical.x)*weight;
    }
    if contact_uniform() {return integral.x;}
    return -log(max(1.0-integral.y,0.000001))/dose;
}

fn relief_contact(dab:Dab,world:vec2<f32>,paper:f32)->f32 {
    let axes=0.5*(dab.radii+dab.previous.xy);
    let radius=min(axes.x,axes.y);
    if !contact_feature(16384u,radius<8.0) || radius>=8.0 || contact_feature(4096u,style.material_a.x>0.0) {
        return relief_evaluate(dab,world,paper,false).x;
    }
    let dose=6.0*dab.flow*dab.color.a;
    if dose<=0.0 {return 0.0;}
    if dot(dab.motion,dab.motion)<0.000001 {
        let filtered=relief_tap_pixel(dab,world,paper,dose);
        let blend=relief_transfer((radius-4.0)*0.25);
        return mix(filtered,relief_evaluate(dab,world,paper,false).x,blend);
    }
    let relative=world-dab.center+dab.motion;
    let transverse=dot(relative,dab.metric.zw);
    let pressure=max(dab.previous_contact.x,dab.contact.x);
    let profile=min(dab.previous_contact.yzw,dab.contact.yzw);
    let depth=0.5-clamp(0.5+style.contact_a.y*(paper-0.5),0.0,1.0);
    var limit=relief_inverse((0.5*clamp(pressure,0.0,1.0)-depth)/(1.0+0.25*depth),profile);
    if !contact_feature(32768u,style.contact_c.z!=0.0) {limit=min(1.0,limit);}
    if limit<=0.0 {return 0.0;}
    var normal=normalize(dab.metric.zw);
    var field_transverse=transverse;
    if contact_feature(32768u,style.contact_c.z!=0.0) {field_transverse+=dab.material.z;}
    var bounds=vec2<f32>(-sqrt(limit)-field_transverse,sqrt(limit)-field_transverse)/length(dab.metric.zw);
    if contact_feature(32768u,style.contact_c.z!=0.0) {
        let physical=vec2<f32>(-1.0-transverse,1.0-transverse)/length(dab.metric.zw);
        bounds=vec2<f32>(max(bounds.x,physical.x),min(bounds.y,physical.y));
    }
    let a=max(abs(normal.x),abs(normal.y));
    let b=min(abs(normal.x),abs(normal.y));
    let half=0.5*(a+b);
    let flat=0.5*(a-b);
    var exposure=vec2<f32>(0.0);
    var alpha=0.0;
    for(var piece=0u;piece<3u;piece+=1u) {
        var span=vec2<f32>(-half,-flat);
        if piece==1u {span=vec2<f32>(-flat,flat);}
        if piece==2u {span=vec2<f32>(flat,half);}
        span=vec2<f32>(max(span.x,bounds.x),min(span.y,bounds.y));
        if span.y<=span.x {continue;}
        for(var i=0u;i<4u;i+=1u) {
            let outer=i==0u || i==3u;
            let node=select(0.3399810435848563,0.8611363115940526,outer)*select(-1.0,1.0,i>=2u);
            let position=0.5*(span.x+span.y)+0.5*(span.y-span.x)*node;
            let probability=select(1.0/a,(half-abs(position))/max(a*b,0.00000001),abs(position)>flat);
            let weight=0.5*(span.y-span.x)*select(0.6521451548625461,0.3478548451374538,outer)*probability;
            let sample=world+normal*position;
            let contact=relief_evaluate(dab,sample,paper,!contact_uniform());
            exposure+=weight*contact;
            alpha+=weight*(1.0-exp(-dose*contact.y));
        }
    }
    var filtered=0.0;
    if contact_uniform() {filtered=exposure.x;}
    else if exposure.y>0.0 {
        filtered=exposure.x*(-log(max(1.0-alpha,0.000001)))/(dose*exposure.y);
    }
    let blend=relief_transfer((radius-4.0)*0.25);
    if blend>0.0 {filtered=mix(filtered,relief_evaluate(dab,world,paper,false).x,blend);}
    return filtered;
}
