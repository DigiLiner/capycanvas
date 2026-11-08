    if settings.maximum==0u {
        if settings.scale==32u {
            if (bitcast<u32>(value.a)&0x7f800000u)==0u {store_outputs(invocation.z,pixel,vec4(0u),vec4(0.));return;}
            let finite=3.402823466e38;
            let limit=finite*value.a;
            let straight=vec4(clamp(clamp(value.rgb,vec3(-limit),vec3(limit))/value.a,vec3(-finite),vec3(finite)),value.a);
            store_outputs(invocation.z,pixel,bitcast<vec4<u32>>(straight),vec4(straight.rgb*straight.a,straight.a));
            return;
        }
        let alpha_bits=half_bits(value.a);
        if (alpha_bits&32767u)==0u {store_outputs(invocation.z,pixel,vec4(0u),vec4(0.));return;}
        let limit=65504.*value.a;
        let rgb=clamp(clamp(value.rgb,vec3(-limit),vec3(limit))/value.a,vec3(-65504.),vec3(65504.));
        let bits=vec4(half_bits(rgb.r),half_bits(rgb.g),half_bits(rgb.b),alpha_bits);
        let alpha=half_value(alpha_bits);
        store_outputs(invocation.z,pixel,bits,vec4(vec3(half_value(bits.r),half_value(bits.g),half_value(bits.b))*alpha,alpha));
        return;
    }
