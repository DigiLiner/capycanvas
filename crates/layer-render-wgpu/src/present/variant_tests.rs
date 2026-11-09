use super::*;
use layer_core::{Affine, ImageTransform, Point};
use layer_core::authored::{OccurrenceContent, SceneScope, SourceTarget};
use std::time::{Duration, Instant};

struct VariantFixture {
    renderer: WgpuRasterizer,
    view: ViewState,
    target: wgpu::TextureView,
}

impl VariantFixture {
    fn new(mode: usize) -> Self {
        let extent = [128; 2];
        let mut document = crate::scene::scale::tests::document_at(extent);
        let mut renderer = WgpuRasterizer::new_native_headless(document.composition().color).unwrap();
        let scope = SceneScope::All;
        let initial = crate::test_support::packet(document.scene().with_scope(&scope), extent);
        crate::test_support::submit_settled(&mut renderer, initial);
        assert_eq!(renderer.display_placement()[13], 0.);
        let mut encoder = crate::submission::CommandEncoder::new(&renderer.device, &Default::default());
        let backup = renderer.selected_display_cache().unwrap().backup(&renderer, &mut encoder).unwrap();
        encoder.submit(&renderer.queue);
        assert_eq!(backup.placement[13], 0.);
        let owner = document.scene().order()[0];
        let OccurrenceContent::Paint(source) = document.scene().occurrence(owner).unwrap().content else {
            panic!("the viewport fixture requires one paint source");
        };
        let mut view = initial.view;
        if mode != 0 {
            view.document_to_surface = [0.25, 0., 0., 0.25, 16., 16.];
            if mode == 1 {
                document.artwork.occurrences.get_mut(owner).unwrap().offset = [1, 0];
                renderer.prepare_moving_layer(Some(owner));
            } else {
                renderer.set_transform_preview(Some(&layer_render::TransformPreview {
                    transaction: 1, target: SourceTarget::Paint(source), moving: true, selection: None,
                    transform: ImageTransform::affine(Affine::translation(Point { x: 10., y: 0. })),
                })).unwrap();
            }
            let frame = layer_render::FramePacket {
                view, ..crate::test_support::packet(document.scene().with_scope(&scope), extent)
            };
            crate::test_support::submit_settled(&mut renderer, frame);
        }
        assert_eq!(renderer.display_placement()[13] as usize, mode, "the source fixture must produce the actual requested mode");
        renderer.display_backup = Some(backup);
        assert_eq!(renderer.display_placement()[13], 0., "an accepted backup determines the first viewport binding");
        renderer.shader_idle(false, false);
        let (_, target) = crate::create_target(&renderer.device, extent, wgpu::TextureFormat::Rgba8UnormSrgb, "viewport variants");
        Self { renderer, view, target }
    }

    fn presenter(&self) -> ViewportPresenter {
        let presenter = ViewportPresenter::for_surface(&self.renderer, wgpu::TextureFormat::Rgba8UnormSrgb, SdrSurfaceColor::Srgb).unwrap();
        assert!(presenter.pipeline.iter().all(|pipeline| !pipeline.ready()), "a fresh renderer must leave every viewport recipe cold");
        presenter
    }

    fn expose(&mut self, mode: usize) {
        self.renderer.display_backup = None;
        assert_eq!(self.renderer.display_placement()[13] as usize, mode);
    }

    fn encode(&self, presenter: &mut ViewportPresenter) -> wgpu::SubmissionIndex {
        let mut encoder = self.renderer.device.create_command_encoder(&Default::default());
        presenter.encode(&self.renderer, &mut encoder, &self.target, self.view, [0.; 4]).unwrap();
        self.renderer.queue.submit([encoder.finish()])
    }

    fn complete(&self, submission: wgpu::SubmissionIndex) {
        self.renderer.device.poll(wgpu::PollType::Wait {
            submission_index: Some(submission), timeout: Some(Duration::from_secs(30)),
        }).unwrap();
    }

    fn assert_binding(&self, presenter: &ViewportPresenter, mode: usize) {
        assert_eq!(self.renderer.display_placement()[13] as usize, mode);
        assert_eq!(presenter.camera_data.unwrap()[53] as usize, mode);
        assert_eq!(presenter.last_artwork_pipeline.as_ref(), Some(presenter.pipeline[mode].compile()));
    }
}

fn prepare_mode(renderer: &WgpuRasterizer, presenter: &ViewportPresenter, mode: usize) {
    pollster::block_on(renderer.compilation.prepare_all([
        &presenter.pipeline[mode], &presenter.surround_pipeline,
        &presenter.cursor_pipeline, &presenter.recipes.transition.pipeline,
    ])).unwrap();
}

fn wait_for_mode(renderer: &WgpuRasterizer, presenter: &ViewportPresenter) {
    let deadline = Instant::now() + Duration::from_secs(30);
    while !presenter.pipelines_ready(renderer) {
        renderer.compilation.check().unwrap();
        assert!(Instant::now() < deadline, "the bound viewport recipe must become ready");
        std::thread::sleep(Duration::from_millis(1));
    }
}

#[test]
fn actual_viewport_sources_match_the_predicted_and_bound_variant() {
    for mode in 0..3 {
        let mut fixture = VariantFixture::new(mode);
        let batch = fixture.renderer.compilation.batch();
        let mut presenter = fixture.presenter();
        fixture.expose(mode);
        assert!(!presenter.pipelines_ready(&fixture.renderer));
        prepare_mode(&fixture.renderer, &presenter, mode);
        assert!(presenter.pipelines_ready(&fixture.renderer));
        for unused in (0..3).filter(|index| *index != mode) {
            assert!(!presenter.pipeline[unused].ready(), "unused viewport variants must not hold this frame");
        }
        let submission = fixture.encode(&mut presenter);
        fixture.assert_binding(&presenter, mode);
        fixture.complete(submission);
        drop(batch);
    }
    crate::finish_shader_compiler_shutdown();
}

#[test]
fn backup_reveal_promotes_only_the_new_cold_viewport_variant() {
    for mode in [1, 2] {
        let mut fixture = VariantFixture::new(mode);
        let batch = fixture.renderer.compilation.batch();
        let mut presenter = fixture.presenter();
        prepare_mode(&fixture.renderer, &presenter, 0);
        assert!(presenter.pipelines_ready(&fixture.renderer));
        assert!(!presenter.pipeline[1].ready() && !presenter.pipeline[2].ready());
        let submission = fixture.encode(&mut presenter);
        fixture.complete(submission);
        fixture.assert_binding(&presenter, 0);
        fixture.expose(mode);
        assert!(!presenter.pipelines_ready(&fixture.renderer));
        let previous = presenter.last_artwork_pipeline.clone();
        let camera = presenter.camera_data;
        let submission = fixture.encode(&mut presenter);
        assert_eq!(presenter.last_artwork_pipeline, previous);
        assert_eq!(presenter.camera_data, camera);
        assert!(!presenter.pipeline[mode].ready(), "encoding cannot compile the cold viewport inline");
        fixture.complete(submission);
        prepare_mode(&fixture.renderer, &presenter, mode);
        let other = 3 - mode;
        assert!(!presenter.pipeline[other].ready());
        assert!(presenter.pipelines_ready(&fixture.renderer), "a mode change cannot wait for the other unused variant");
        let submission = fixture.encode(&mut presenter);
        fixture.assert_binding(&presenter, mode);
        fixture.complete(submission);
        drop(batch);
    }
    crate::finish_shader_compiler_shutdown();
}

#[test]
fn immediate_viewport_variant_switch_prepares_on_the_coordinator() {
    for mode in [1, 2] {
        let mut fixture = VariantFixture::new(mode);
        let mut presenter = fixture.presenter();
        wait_for_mode(&fixture.renderer, &presenter);
        assert!(!presenter.pipeline[1].ready() && !presenter.pipeline[2].ready());
        let submission = fixture.encode(&mut presenter);
        fixture.complete(submission);
        fixture.expose(mode);
        wait_for_mode(&fixture.renderer, &presenter);
        let submission = fixture.encode(&mut presenter);
        fixture.complete(submission);
        fixture.assert_binding(&presenter, mode);
    }
    crate::finish_shader_compiler_shutdown();
}
