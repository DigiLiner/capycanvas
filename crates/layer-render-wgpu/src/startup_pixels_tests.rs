#[test]
fn staged_startup_matches_eager_pixels_and_waits_for_catalog() {
    use crate::WgpuRasterizer;
    use layer_render::CanvasRenderer;
    let instance = WgpuRasterizer::headless_instance();
    let adapter = pollster::block_on(instance.request_adapter(&Default::default())).unwrap();
    let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
        required_features: crate::rendering_features(&adapter),
        ..Default::default()
    }))
    .unwrap();
    let color = layer_core::color::DocumentColor::default();
    let mut reference = WgpuRasterizer::new_native_headless(color).unwrap();
    let doc = layer_core::Document::new(layer_core::PortableId::random(), 64, 64, layer_core::DocumentNames { paint: "Current ink".into(), paper: "Paper".into() });
    let brush = layer_core::default_brush(layer_core::DefaultBrushPreset::GPen);
    let dabs = [layer_render::Dab {
        center: layer_core::Point { x: 32., y: 32. },
        radii: [16.; 2],
        rotation: [1., 0.],
        motion: [0.; 2],
        color_rgba_linear: [0.8, 0.1, 0.3, 1.],
        flow: 1.,
        hardness: 1.,
        texture_sign: [1.; 2],
        material: [0.; 4],
        previous: [0.0; 4],
        contact: [0.0; 4],
        previous_contact: [0.0; 4],
    }];
    let batches = [crate::test_support::dab_batch(
        doc.working.target.unwrap(),
        layer_render::DabStyle::for_brush(&brush, layer_core::StrokeTool::Brush),
        layer_core::Rect { min: layer_core::Point { x: 16., y: 16. }, max: layer_core::Point { x: 48., y: 48. } },
    )];
    let packet = layer_render::FramePacket {
        view: crate::test_support::view([64; 2]),
        dabs: &dabs,
        dab_batches: &batches,
        reset_layers: true,
        ..crate::test_support::packet(doc.scene(), [64; 2])
    };
    reference.submit(packet).unwrap();
    let expected = reference.readback_srgb_rgba8().unwrap();
    assert_ne!(
        &expected[..4],
        &expected[(32 * 64 + 32) * 4..(32 * 64 + 32) * 4 + 4],
        "test must paint visible pixels"
    );
    for _ in 0..2 {
        let mut renderer = WgpuRasterizer::from_wgpu_native_staged(
            adapter.clone(),
            device.clone(),
            queue.clone(),
            color,
        )
        .unwrap();
        renderer.prepare_startup(&doc, &brush, false).unwrap();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
        loop {
            let progress = renderer.poll_startup().unwrap();
            assert!(!progress.complete);
            if progress.brush_ready && renderer.startup.as_ref().unwrap().compiler.pending() == 0 { break; }
            assert!(std::time::Instant::now() < deadline, "Startup compilation timed out: {progress:?}");
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
        renderer.finish_startup_catalog();
        crate::test_support::wait_startup(&mut renderer, deadline, |progress| progress.complete, format_args!("Startup compilation timed out"));
        renderer.submit(packet).unwrap();
        assert_eq!(renderer.readback_srgb_rgba8().unwrap(), expected);
    }
}
