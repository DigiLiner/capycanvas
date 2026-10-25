//! Actual native SDR preparation, backing, history and GPU replacement.
use super::*;
use layer_core::color::{DocumentColor, SampleDepth, RgbSpace};
use std::{io::Seek, os::fd::AsRawFd};

#[test]
fn native_new_drawing_preserves_space_depth_paper_and_the_captured_order() {
    for platform in [0, 1] {
        let app = App::new(platform);
        app.borrow().host.session.renderer_mut().0 = Some(native_renderer());
        app.draw_until_idle();
        let sheet = |color: DocumentColor, transparent: bool| vec![json!({"op": "space", "space": color.space}), json!({"op": "depth", "depth": color.depth}),
            json!({"op": "paper_color"}), json!({"op": "transparent", "transparent": transparent}), json!({"op": "use_paper_color"})];
        let adopt = |job: &ProjectJob, extent: [u32; 2], color: DocumentColor, transparent: bool| {
            assert_eq!(unsafe {
                capy_apple_project_adopt(app.0, job.0, c"New drawing".as_ptr(), c"".as_ptr())
            }, 0, "{:?} / {:?}, requests={}", job.error(), app.borrow().error, app.state()["requests"]);
            app.draw_until_idle();
            let document = &app.document();
            assert_eq!(document.composition().color, color);
            assert_eq!([document.composition().size[0], document.composition().size[1]], extent);
            assert_eq!(occurrence_at(document, 1).visible, !transparent);
            assert!(document.artwork.paint.iter().all(|(_, _, source)| raster_samples(&source.raster).0.is_empty()));
            assert!(!app.state()["document_file"]["modified"].as_bool().unwrap());
            assert!(app.state()["new_drawing"].is_null());
            assert!(!app.pixels().is_empty());
        };
        for space in RgbSpace::ALL {
            for depth in [SampleDepth::U8, SampleDepth::U16] {
                for transparent in [false, true] {
                    let color = DocumentColor { space, depth };
                    let job = ProjectJob::create_with(&app, [63, 47], &sheet(color, transparent));
                    adopt(&job, [63, 47], color, transparent);
                }
            }
        }
        let captured = DocumentColor { space: RgbSpace::AdobeRgb, depth: SampleDepth::U16 };
        app.order_new_drawing([67, 43], &sheet(captured, true));
        let job = ProjectJob::new(&app, true);
        app.order_new_drawing([73, 51], &sheet(DocumentColor::default(), false));
        assert_eq!(unsafe { capy_project_new(job.0) }, 0, "{:?}", job.error());
        adopt(&job, [67, 43], captured, true);
        assert_eq!(app.state()["settings"]["new_document"]["last"]["width"], 73_000);

        app.stroke(); app.draw_until_idle();
        let before = app.document().clone();
        let pixels = app.pixels();
        let job = ProjectJob::new(&app, true);
        assert_eq!(unsafe { capy_project_new(job.0) }, -1);
        assert!(job.error().is_some());
        assert_eq!(unsafe {
            capy_apple_project_adopt(app.0, job.0, c"Invalid".as_ptr(), c"".as_ptr())
        }, -1);
        assert_project_document(&app.document(), &before);
        assert!(app.pixels() == pixels, "Rejected New must preserve the live drawing");
    }
}

#[test]
fn native_p3_u8_and_prophoto_u16_survive_save_open_and_gpu_replacement() {
    for platform in [0, 1] {
        for (space, depth) in [
            (RgbSpace::DisplayP3, SampleDepth::U8),
            (RgbSpace::ProPhoto, SampleDepth::U16),
        ] {
            let color = DocumentColor { space, depth };
            let project = layer_ui::NewDocumentOptions {
                extent: [128, 96],
                color,
                ..Default::default()
            }
            .project(&layer_ui::Localizer::shared(layer_ui::UiLanguage::English))
            .unwrap();
            let mut file = fixtures::tempfile();
            write_document(&project, &mut file);
            file.rewind().unwrap();
            let app = App::new(platform);
            app.borrow().host.session.renderer_mut().0 = Some(native_renderer());
            app.draw_until_idle();
            let open = ProjectJob::new(&app, true);
            assert_eq!(
                unsafe { capy_project_read(open.0, file.as_raw_fd(), c"Drawing.capy".as_ptr()) },
                0,
                "{:?}",
                open.error()
            );
            assert_eq!(
                unsafe {
                    capy_apple_project_adopt(app.0, open.0, c"Color check".as_ptr(), c"".as_ptr())
                },
                0
            );
            app.draw_until_idle();
            assert_eq!(
                app.document().composition().color,
                color
            );
            let blank = app.pixels();
            app.action(json!({"type":"color","action":{"op":"definition","color":{
                "space":space,"rgba":[0.12345678,0.45678912,0.34567891,0.654321]
            }}}));
            app.action(json!({"type":"set_brush_size","value":5}));
            app.draw_until_idle();
            app.stroke();
            app.draw_until_idle();
            let painted = app.pixels();
            assert!(
                painted != blank,
                "{platform}/{space:?}: actual stroke required"
            );
            let document = app.document().clone();
            let samples: Vec<_> = document.artwork.paint.iter()
                .flat_map(|(_, _, source)| raster_samples(&source.raster).0.into_values())
                .collect();
            assert!(!samples.is_empty());
            assert!(
                samples
                    .iter()
                    .all(|(descriptor, _)| *descriptor == color.paint_descriptor())
            );
            if depth == SampleDepth::U16 {
                assert!(
                    samples
                        .iter()
                        .flat_map(|(_, bytes)| bytes.as_chunks::<2>().0.iter())
                        .any(|sample| u16::from_le_bytes([sample[0], sample[1]]) % 257 != 0),
                    "16-bit backing must retain more than 8-bit precision"
                );
            }
            app.invoke("undo");
            app.draw_until_idle();
            assert!(app.pixels() == blank);
            app.invoke("redo");
            app.draw_until_idle();
            assert_project_document(
                &app.document(),
                &document,
            );
            assert!(app.pixels() == painted);

            {
                let save = ProjectJob::new(&app, false);
                assert!(!(save.0==0));
                file.set_len(0).unwrap();
                file.rewind().unwrap();
                assert_eq!(
                    unsafe { capy_project_write(save.0, file.as_raw_fd()) },
                    0,
                    "{:?}",
                    save.error()
                );
                file.rewind().unwrap();
                let saved = read_document(&mut file);
                assert_saved_document(&saved, &document);
                let restored = App::new(platform);
                restored.borrow().host.session.renderer_mut().0 = Some(native_renderer());
                restored.draw_until_idle();
                let open = ProjectJob::new(&restored, true);
                file.rewind().unwrap();
                assert_eq!(
                    unsafe { capy_project_read(open.0, file.as_raw_fd(), c"Drawing.capy".as_ptr()) },
                    0,
                    "{:?}",
                    open.error()
                );
                let result = unsafe { capy_apple_project_adopt(restored.0, open.0,
                    c"Saved.capy".as_ptr(), c"file:///fixture.capy".as_ptr()) };
                assert_eq!(result, 0);
                restored.draw_until_idle();
                assert_saved_document(
                    &restored.document(),
                    &document,
                );
                assert!(restored.pixels() == painted);
                assert_eq!(restored.state()["document_file"]["modified"], false);
                assert_eq!(unsafe { capy_apple_suspend_renderer(restored.0) }, 0);
                let gpu = layer_render_wgpu::WgpuRasterizer::new_native_headless(color).unwrap();
                restored.with(|owner|owner.metal.install_renderer(&mut owner.host, gpu.into()).unwrap());
                restored.draw_until_idle();
                assert_saved_document(
                    &restored.document(),
                    &document,
                );
                assert!(
                    restored.pixels() == painted,
                    "{platform}/{space:?}: GPU replacement must preserve artwork"
                );
            }
        }
    }
}

#[test]
fn apple_display_previews_keep_p3_chroma_without_changing_paint_definitions() {
    for platform in [0, 1] {
        let app = App::new(platform);
        let definition = json!({"space":"DisplayP3","rgba":[1.,0.5,0.,0.5]});
        app.action(json!({"type":"color","action":{"op":"definition","color":definition}}));
        let state = app.state();
        let snapshot = app.full_snapshot();
        let panel = &snapshot["color_panel"];
        assert_eq!(panel["marker_color"], json!([1.,0.5,0.]));
        assert_eq!(panel["swatches"][0]["rgba"], json!([1.,0.5,0.,0.5]));
        assert_eq!(app.state()["colors"], state["colors"]);
        assert_eq!(state["colors"]["foreground"], definition);
        let colors = app.with(|native|native.host.session.state().colors.clone());
        let validation = |display| layer_ui::color_validation_localized(colors.definition(), colors.rgb_space(), display, false, &layer_ui::Localizer::shared(layer_ui::UiLanguage::English)).unwrap();
        assert!(validation(DISPLAY_SPACE).ends_with("Outside the document gamut."));
        assert!(validation(layer_core::color::RgbSpace::Srgb).ends_with("Outside the sRGB preview gamut."), "An sRGB fallback still reports the actual lost gamut");
        let localizer = layer_ui::Localizer::shared(layer_ui::UiLanguage::English);
        assert_ne!(colors.view_in_localized(layer_core::color::RgbSpace::Srgb, &localizer).marker_color, colors.view_in_localized(DISPLAY_SPACE, &localizer).marker_color);
    }
}
