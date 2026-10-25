use super::*;

#[test]
fn cold_reduced_preview_retains_its_cursor_across_pressure_and_more_than_sixty_four_sources() {
    let extent=[5120,1024];
    let source=layer_core::color::source::rgba8_source(extent,|_,_|[255,0,0,255]);
    let mut r=WgpuRasterizer::new_native_headless(Default::default()).unwrap();
    r.source_tiles.borrow_mut().admit(0);
    let mut state=ImageTransformState::new(&r.device);
    state.sources[0]=Some(TileSnapshot {pages:Default::default(),base:Some(layer_core::authored::PaintBase::new(source.clone().into())),
        backing:None,bounds:PixelRect::full(extent),decode_id:layer_core::authored::PortableId::random()});
    let attempt=layer_core::memory::generation();
    let failed=crate::object_image_mips::PreparedMipTile {attempt,id:layer_core::authored::PortableId::random(),source:source.clone(),coordinate:[0;2],
        pixels:Err(layer_core::memory::MemoryError.into())};
    r.decode_pressure.image_failed(&failed,r.device.working_space());
    for _ in 0..3 {
        let mut encoder=submission::CommandEncoder::new(&r.device,&Default::default());
        assert!(matches!(state.prepare_reduced(&mut r,&mut encoder,None,None,extent,1),Err(GpuRasterError::DeferredObjectWork)));
        assert_eq!(state.reduced.as_ref().unwrap().pending.as_ref().unwrap().len(),80);
        assert!(r.paint_layers.is_empty());
        assert!(r.moving_decode.as_ref().is_none_or(|(_,queue)|!queue.pending()));
        r.uploads.finish(&encoder);encoder.submit(&r.queue);
    }
    assert!(r.decode_pressure.blocks_background(layer_core::memory::generation(),false));
    layer_core::memory::notify_capacity_released();
    let deadline=std::time::Instant::now()+Duration::from_secs(30);
    loop {
        let mut encoder=submission::CommandEncoder::new(&r.device,&Default::default());
        let result=state.prepare_reduced(&mut r,&mut encoder,None,None,extent,1);
        r.uploads.finish(&encoder);encoder.submit(&r.queue);
        match result {
            Ok(())=>break,
            Err(GpuRasterError::DeferredObjectWork)=>{
                assert!(state.reduced.as_ref().unwrap().pending.is_some());
                assert!(std::time::Instant::now()<deadline);
                r.wait_idle().unwrap();std::thread::sleep(Duration::from_millis(1));
            },
            Err(error)=>panic!("{error}"),
        }
    }
    assert!(state.reduced.as_ref().unwrap().pending.is_none());
    assert!(r.paint_layers.is_empty());
    let image=&state.reduced.as_ref().unwrap().image;
    let pixels=crate::layer_tests::page_bytes(&r,&image.texture);
    for coordinate in page_coordinates(PixelRect::full(extent)) {
        let x=coordinate[0]*PAGE_SIZE/2+32;let y=coordinate[1]*PAGE_SIZE/2+32;
        let offset=((y*image.texture.width()+x)*16) as usize;
        let value=std::array::from_fn::<_,4,_>(|channel|f32::from_le_bytes(pixels[offset+channel*4..offset+channel*4+4].try_into().unwrap()));
        assert_eq!(value, [1.,0.,0.,1.]);
    }
    state.release_snapshot();assert!(!r.decode_pressure.memory_pressure_waiting());
}
