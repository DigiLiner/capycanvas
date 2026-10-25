use super::*;

pub(super) struct Capacity {
    pub dab_bytes: u64,
    pub style_records: usize,
    pub style_bytes: usize,
}

fn grow(current:u64,required:u64,maximum:u64)->Result<u64,GpuRasterError> {
    if required>maximum || current>maximum {
        return Err(GpuRasterError::UploadLimit {required:required.max(current),limit:maximum});
    }
    if required<=current {return Ok(current);}
    Ok(required.checked_next_power_of_two().unwrap_or(maximum).min(maximum))
}

pub(super) fn plan(limits:&wgpu::Limits,dab_bytes:u64,style_records:usize,stride:u64,dabs:usize,styles:usize)->Result<Capacity,GpuRasterError> {
    let required=u64::try_from(dabs).ok().and_then(|count|count.checked_mul(mem::size_of::<DabGpu>() as u64))
        .ok_or(GpuRasterError::SizeOverflow)?;
    let dab_bytes=grow(dab_bytes,required,limits.max_buffer_size.min(limits.max_storage_buffer_binding_size))?;
    let max_records=limits.max_buffer_size.checked_div(stride).ok_or(GpuRasterError::SizeOverflow)?
        .min(u64::from(u32::MAX)/stride+1).min(usize::MAX as u64);
    let style_records=grow(style_records as u64,styles as u64,max_records).map_err(|error|match error {
        GpuRasterError::UploadLimit {required,limit}=>GpuRasterError::UploadLimit {
            required:required.saturating_mul(stride),limit:limit.saturating_mul(stride),
        },
        error=>error,
    })?;
    let style_bytes=style_records.checked_mul(stride).and_then(|bytes|usize::try_from(bytes).ok())
        .ok_or(GpuRasterError::SizeOverflow)?;
    Ok(Capacity {dab_bytes,style_records:style_records as usize,style_bytes})
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn growth_stays_within_each_limit_when_rounding_would_exceed_it() {
        assert_eq!(grow(64,65,96).unwrap(),96);
        assert_eq!(grow(64,64,96).unwrap(),64);
        assert_eq!(grow(64,65,128).unwrap(),128);
        assert!(matches!(grow(64,97,96),Err(GpuRasterError::UploadLimit {required:97,limit:96})));
        assert_eq!(grow(0,u64::MAX,u64::MAX).unwrap(),u64::MAX);
        assert!(grow(128,1,96).is_err());
    }

    #[test]
    fn dab_and_style_growth_checks_the_actual_binding_and_offset_limits() {
        let limits=wgpu::Limits {max_buffer_size:768,max_storage_buffer_binding_size:384,..wgpu::Limits::downlevel_defaults()};
        let size=mem::size_of::<DabGpu>();
        let capacity=plan(&limits,0,0,256,384/size,3).unwrap();
        assert!(capacity.dab_bytes<=384);
        assert_eq!((capacity.style_records,capacity.style_bytes),(3,768));
        assert!(plan(&limits,0,0,256,384/size+1,0).is_err());
        assert!(plan(&limits,0,0,256,0,4).is_err());
        assert!(plan(&limits,0,0,0,0,0).is_err());
        assert!(plan(&limits,0,0,256,usize::MAX,0).is_err());
        let wide=wgpu::Limits {max_buffer_size:u64::MAX,..limits};
        assert!(plan(&wide,0,0,256,0,(u32::MAX as u64/256+2) as usize).is_err());
    }
}
