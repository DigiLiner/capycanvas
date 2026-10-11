//! How dabs lay over paint follows the document's Blending: soft edges and
//! opacity build up on encoded values in Perceptual documents, checked against
//! a CPU reference, and a brush blend mode equals the layer blend mode.
use super::*;
use layer_core::color::{DocumentColor, RgbSpace, SampleDepth};
use layer_core::{BlendSpace, BrushBlendMode, Document, LayerBlend, StrokeId, authored::{Artwork, SourceTarget}};

const EXTENT: [u32; 2] = [128, 128];
const WHITE: [f32; 4] = [1.; 4];

struct Canvas {
    r: WgpuRasterizer,
    document: Document,
    base: SourceTarget,
    over: Option<SourceTarget>,
    space: BlendSpace,
    strokes: u64,
    extent: [u32;2],
}

impl Canvas {
    fn new(color: DocumentColor, space: BlendSpace, blend: Option<LayerBlend>) -> Self {
        Self::at(color,space,blend,EXTENT)
    }

    fn at(color:DocumentColor,space:BlendSpace,blend:Option<LayerBlend>,extent:[u32;2])->Self {
        let mut artwork = Artwork::new(extent).unwrap();
        let composition = artwork.compositions.get_mut(artwork.root).unwrap();
        composition.color = color;
        composition.blend = space;
        let over = blend.map(|blend| {
            let (owner, target) = crate::test_support::add_paint(&mut artwork, "Blend", extent);
            artwork.occurrences.get_mut(owner).unwrap().blend = blend;
            target
        });
        let (_, base) = crate::test_support::add_paint(&mut artwork, "Paint", extent);
        let document = Document::from_artwork(artwork).unwrap();
        let mut r = WgpuRasterizer::new_native_headless(color).unwrap();
        r.submit(FramePacket { reset_layers: true, blend_space: space, ..packet(document.scene(), extent) }).unwrap();
        Self { r, document, base, over, space, strokes: 0, extent }
    }

    fn paint(&mut self, target: SourceTarget, mut style: DabStyle, dab: Dab) {
        style.blend_space = self.space;
        self.strokes += 1;
        let damage = Rect { min: Point { x: 0., y: 0. }, max: Point { x: self.extent[0] as f32, y: self.extent[1] as f32 } };
        let batch = DabBatch { stroke_id: StrokeId(self.strokes), ..crate::test_support::dab_batch(target, style, damage) };
        self.r
            .submit(FramePacket { dabs: &[dab], dab_batches: &[batch], blend_space: self.space, ..packet(self.document.scene(), self.extent) })
            .unwrap();
    }

    fn fill(&mut self, target: SourceTarget, color: [f32; 4]) {
        let mut dab = test_dab([64., 64.], color, 1.);
        dab.radii = [200.; 2];
        self.paint(target, test_style(BrushExecution::Dry), dab);
    }

    /// The composite, as the document's blend space holds it.
    fn composite(&mut self) -> Vec<[f64; 4]> {
        self.r.submit(FramePacket { blend_space: self.space, ..packet(self.document.scene(), self.extent) }).unwrap();
        crate::layer_tests::page_bytes(&self.r, crate::test_support::document_texture(&self.r))
            .chunks_exact(16)
            .map(|p| std::array::from_fn(|c| f64::from(f32::from_le_bytes(p[c * 4..][..4].try_into().unwrap()))))
            .collect()
    }
}

fn soft(center: [f32; 2], color: [f32; 4], radius: f32, hardness: f32) -> Dab {
    let mut dab = test_dab(center, color, 1.);
    dab.radii = [radius; 2];
    dab.hardness = hardness;
    dab
}

/// `analytic_coverage` of a round dab at the center of pixel `[x, y]`.
fn coverage(dab: &Dab, [x, y]: [usize; 2]) -> f64 {
    let radius = f64::from(dab.radii[0]);
    let local = [(x as f64 + 0.5 - f64::from(dab.center.x)) / radius, (y as f64 + 0.5 - f64::from(dab.center.y)) / radius];
    let squared = local[0] * local[0] + local[1] * local[1];
    if squared >= 1. {
        return 0.;
    }
    let edge = (1. - f64::from(dab.hardness)).max(1. / radius);
    let solid = (1. - edge).max(0.);
    if squared <= solid * solid { 1. } else { ((1. - squared.sqrt()) / edge).clamp(0., 1.) }
}

#[test]
fn uniform_stroke_colors_and_coverage_do_not_depend_on_contact_batching() {
    for space in BlendSpace::ALL {
        for (mode, blend) in [(DabMode::Paint, BrushBlendMode::Normal), (DabMode::Paint, BrushBlendMode::Overlay), (DabMode::Erase, BrushBlendMode::Normal)] {
            for varying in [false, true] {
                let dabs: Vec<_> = [0.2, 1., 0.4, 1.].into_iter().enumerate().map(|(i, alpha)| {
                    soft([40. + i as f32 * 11., 48. + i as f32 * 6.],
                        [0.2 + if varying { i as f32 * 0.2 } else { 0. }, 0.3, 0.8, alpha], 52., 0.3)
                }).collect();
                let draw = |chunk: usize| {
                    let mut canvas = Canvas::new(DocumentColor::default(), space, None);
                    canvas.fill(canvas.base, [0.3, 0.5, 0.1, 0.8]);
                    let mut style = test_style(BrushExecution::Dry);
                    style.blend_space = space;
                    style.mode = mode;
                    style.rendering.blend_mode = blend;
                    style.rendering.accumulation = BrushAccumulation::Uniform;
                    let damage = dabs.iter().fold(Rect::default(), |r, d| r.union(d.bounds()));
                    let mut batch = DabBatch { stroke_id: StrokeId(2), ..crate::test_support::dab_batch(canvas.base, style, damage) };
                    for (i, contacts) in dabs.chunks(chunk).enumerate() {
                        batch.dab_count = contacts.len() as u32;
                        batch.stroke_start = i == 0;
                        batch.stroke_end = (i + 1) * chunk >= dabs.len();
                        canvas.r.submit(FramePacket { dabs: contacts, dab_batches: std::slice::from_ref(&batch),
                            blend_space: space, ..packet(canvas.document.scene(), EXTENT) }).unwrap();
                    }
                    canvas.composite()
                };
                for (pixel, (a, b)) in draw(1).into_iter().zip(draw(dabs.len())).enumerate() {
                    for channel in 0..4 {
                        assert!((a[channel] - b[channel]).abs() < 2e-4,
                            "{space:?} {mode:?} {blend:?} varying={varying} pixel={pixel} channel={channel}: {a:?} != {b:?}");
                    }
                }
            }
        }
    }
}

#[test]
fn dry_flow_batch_dose_matches_ordered_contacts_and_closed_form() {
    use layer_core::{BrushContact, BrushDryMedia, BrushDryPose, Selection, SelectionPixels};
    for underpaint in [[-0.12f32,0.18,1.5,0.6],[0.;4],[-0.15*0.6,0.7*0.6,1.9*0.6,0.6]] {
    let bytes: Vec<_> = (0..PAGE_SIZE*PAGE_SIZE).flat_map(|_| underpaint.into_iter().flat_map(f32::to_le_bytes)).collect();
    let selected = Arc::new(Selection::pixels(Arc::new(SelectionPixels::new(EXTENT,
        [0, 0, EXTENT[0], EXTENT[1]], vec![0x22222222; (EXTENT[0]*EXTENT[1]/8) as usize]).unwrap())));
    for space in BlendSpace::ALL {
        let mut canvas = Canvas::new(DocumentColor { depth: if space == BlendSpace::Linear { SampleDepth::F32 } else { SampleDepth::U8 }, ..Default::default() }, space, None);
        let r = &mut canvas.r;
        let base = r.create_page_surface("dose oracle underpaint");
        crate::test_support::upload_page(r, &base.texture, &bytes);
        let output = r.create_page_surface("dose oracle output");
        for mode in [DabMode::Paint, DabMode::Erase] {
            for (varying, locked, selection) in [(false, false, false), (true, false, false),
                (false, true, false), (false, false, true)] {
                let mut style = test_style(BrushExecution::Dry);
                style.mode = mode; style.blend_space = space; style.alpha_locked = locked;
                style.selection = selection.then(|| selected.clone());
                style.rendering.accumulation = BrushAccumulation::Flow;
                style.contact = Some(BrushContact { pressure_gain: 1., dry: Some(BrushDryMedia {
                    poses: [BrushDryPose { radii: [1.; 2], relief: [0.; 3] }; 3],
                    softness: 0.05, loading: 0., corner: 0.,
                }), ..Default::default() });
                let dabs: Vec<_> = [(0., 0.1), (0.15, 0.02), (0.65, 0.3), (1., 0.12)]
                    .into_iter().enumerate().map(|(i, (alpha, flow))| {
                        let mut dab = test_dab([64.5; 2], [-0.15 + if varying { i as f32*0.12 } else { 0. }, 0.7, 1.9, alpha], flow);
                        dab.radii = [200.; 2]; dab.contact = [0.7, 0., 0., 0.]; dab.previous_contact = dab.contact;
                        dab
                    }).collect();
                let batch = DabBatch { dab_count: dabs.len() as u32,
                    ..crate::test_support::dab_batch(canvas.base, style, Rect::from_extent(EXTENT)) };
                let mut encoder = crate::submission::CommandEncoder::new(&r.device, &Default::default());
                r.prepare_uploads(FramePacket { dabs: &dabs, dab_batches: std::slice::from_ref(&batch),
                    ..packet(canvas.document.scene(), EXTENT) }, &mut [Vec::new()], &mut encoder).unwrap();
                r.prepare_target_selection(&mut encoder, &batch.style, EXTENT).unwrap();
                r.prepare_dry_records(&batch, std::iter::once((PixelRect::full(EXTENT), 0..dabs.len() as u32, [0; 16])), &mut encoder).unwrap();
                let key = WgpuRasterizer::texture_set_key(&batch.style); r.ensure_texture_set(key.clone()).unwrap();
                r.uploads.finish(&encoder);
                encoder.submit(&r.queue);
                r.queue.write_buffer(&r.target_buffer, 0, target_bytes(&TargetGpu::new([0; 2], EXTENT, EXTENT)));
                let marker = mem::offset_of!(StyleGpu, material_a)+12;
                let mut record = r.style_upload[..mem::size_of::<StyleGpu>()].to_vec();
                let enabled = !locked && !selection && (mode == DabMode::Erase || !varying);
                assert_eq!(f32::from_ne_bytes(record[marker..marker+4].try_into().unwrap()), f32::from(enabled),
                    "{space:?}/{mode:?} varying={varying} locked={locked} selection={selection}: actual batch marker");
                let sources = create_material_bind_group(&r.device, &r.material_layout, &[&base.view; 9],
                    &r.dab_buffer, &r.empty_scalar_view, &r.empty_view,
                    wgpu::BindingResource::Buffer(r.dry_records.binding()));
                let binding = r.pipelines.dry_material.output(r, &output, None, canvas.base, [0; 2]);
                let mut rendered = Vec::new();
                for reference in [false, true] {
                    if reference { record[marker..marker+4].copy_from_slice(&0f32.to_ne_bytes()); }
                    r.queue.write_buffer(&r.style_buffer, 0, &record);
                    let mut encoder = crate::submission::CommandEncoder::new(&r.device, &Default::default());
                    { let mut pass = encoder.begin_compute_pass(&Default::default());
                        pass.set_pipeline(r.pipelines.dry_material.selected_kernel(&batch.style, MaterialOperation::Deposit, false, if enabled && !reference {8192} else {0}));
                        pass.set_bind_group(0, &binding, &[0]);
                        pass.set_bind_group(1, r.paint_target_binding(&batch.style), &[0]);
                        pass.set_bind_group(2, &sources, &[0]);
                        pass.set_bind_group(3, &r.texture_sets.iter().find(|t|t.key==key).unwrap().bind_group, &[]);
                        pass.dispatch_workgroups(EXTENT[0].div_ceil(32), EXTENT[1].div_ceil(2), 1);
                    }
                    encoder.submit(&r.queue);
                    rendered.push(crate::test_support::float_pixels(r, &output.texture));
                }
                assert!(crate::test_support::max_error(&rendered[0], &rendered[1]) < 1e-6,
                    "{space:?}/{mode:?} varying={varying} locked={locked} selection={selection}: aggregate vs ordered contacts");
                let mut expected = underpaint.map(f64::from);
                if mode == DabMode::Paint && space == BlendSpace::Perceptual {
                    for c in 0..3 { expected[c] = if expected[3]>0. {RgbSpace::Srgb.encode(expected[c]/expected[3])*expected[3]} else {0.}; }
                }
                for dab in &dabs {
                    let alpha = (1.-(-6.*f64::from(dab.flow)*f64::from(dab.color_rgba_linear[3])).exp())*if selection {0.5} else {1.};
                    if mode == DabMode::Erase { if !locked { for value in &mut expected { *value *= 1.-alpha; } } }
                    else {
                        for c in 0..3 {
                            let color = if space == BlendSpace::Perceptual { RgbSpace::Srgb.encode(f64::from(dab.color_rgba_linear[c])) } else { f64::from(dab.color_rgba_linear[c]) };
                            expected[c] = expected[c]*(1.-alpha)+color*alpha*if locked {expected[3]} else {1.};
                        }
                        if !locked { expected[3] = alpha+expected[3]*(1.-alpha); }
                    }
                }
                if mode == DabMode::Paint && space == BlendSpace::Perceptual {
                    for c in 0..3 { expected[c] = if expected[3]>0. {RgbSpace::Srgb.decode(expected[c]/expected[3])*expected[3]} else {0.}; }
                }
                for y in 0..EXTENT[1] { for x in 0..EXTENT[0] {
                    let pixel = rendered[0][(y*PAGE_SIZE+x) as usize];
                    for c in 0..4 { assert!((f64::from(pixel[c])-expected[c]).abs() < 1e-6,
                        "{space:?}/{mode:?} varying={varying} locked={locked} selection={selection} channel={c}: GPU {pixel:?}, closed form {expected:?}"); }
                }}
            }
        }
    }
    }
}

#[test]
fn a_soft_black_edge_over_white_fades_on_the_documents_values() {
    for space in BlendSpace::ALL {
        let mut canvas = Canvas::new(DocumentColor::default(), space, None);
        canvas.fill(canvas.base, WHITE);
        let dab = soft([61.3, 66.7], [0., 0., 0., 1.], 50., 0.1);
        canvas.paint(canvas.base, test_style(BrushExecution::Dry), dab);
        let composite = canvas.composite();
        let mut faded = 0;
        for y in 0..EXTENT[1] as usize {
            for x in 0..EXTENT[0] as usize {
                let alpha = coverage(&dab, [x, y]);
                let expected = 1. - alpha;
                let actual = composite[y * EXTENT[0] as usize + x];
                assert!((actual[1] - expected).abs() < 2e-4, "{space:?} at {x},{y}: {} != {expected}", actual[1]);
                faded += usize::from(alpha > 0.1 && alpha < 0.9);
            }
        }
        assert!(faded > 1000, "{space:?}: the profile crosses {faded} pixels of the soft edge");
    }
}

#[test]
fn black_at_half_opacity_over_white_paint_is_middle_gray_when_blending_perceptually() {
    for depth in [SampleDepth::U8, SampleDepth::U16] {
        for (space, expected) in [(BlendSpace::Perceptual, 128.), (BlendSpace::Linear, 188.)] {
            let color = DocumentColor { space: RgbSpace::Srgb, depth };
            let mut canvas = Canvas::new(color, space, None);
            canvas.fill(canvas.base, WHITE);
            canvas.paint(canvas.base, test_style(BrushExecution::Dry), soft([64., 64.], [0., 0., 0., 0.5], 40., 1.));
            canvas.composite();
            let image = canvas.r.readback_srgb_rgba8().unwrap();
            let center = (64 * EXTENT[0] as usize + 64) * 4;
            let code = f32::from(image[center]);
            assert!((code - expected).abs() <= 1., "{depth:?} {space:?}: {code}, not {expected}");
        }
    }
}

#[test]
fn a_brush_blend_mode_equals_the_layer_blend_mode_in_both_spaces() {
    let base = |canvas: &mut Canvas, layer| {
        canvas.fill(layer, [0.3, 0.55, 0.2, 1.]);
        canvas.paint(layer, test_style(BrushExecution::Dry), soft([40., 60.], [0.9, 0.1, 0.6, 1.], 70., 0.));
        canvas.paint(layer, test_style(BrushExecution::Dry), soft([100., 90.], [0.05, 0.08, 0.5, 0.8], 50., 0.3));
    };
    let stroke = soft([70., 64.], [0.85, 0.7, 0.15, 0.75], 55., 0.2);
    for space in BlendSpace::ALL {
        for mode in [BrushBlendMode::Overlay, BrushBlendMode::Multiply, BrushBlendMode::Screen, BrushBlendMode::Darken, BrushBlendMode::Lighten] {
            let mut brush = Canvas::new(DocumentColor::default(), space, None);
            let target = brush.base;
            base(&mut brush, target);
            let mut style = test_style(BrushExecution::Dry);
            style.rendering.blend_mode = mode;
            brush.paint(brush.base, style, stroke);
            let mut layered = Canvas::new(DocumentColor::default(), space, Some(LayerBlend::from(mode)));
            let target = layered.base;
            base(&mut layered, target);
            layered.paint(layered.over.unwrap(), test_style(BrushExecution::Dry), stroke);
            for (i, (a, b)) in brush.composite().into_iter().zip(layered.composite()).enumerate() {
                for c in 0..4 {
                    assert!((a[c] - b[c]).abs() < 2e-4, "{space:?} {mode:?} pixel {i} channel {c}: brush {} != layer {}", a[c], b[c]);
                }
            }
        }
    }
}

#[test]
fn erasing_is_the_same_in_both_spaces() {
    let erased = |space| {
        let mut canvas = Canvas::new(DocumentColor::default(), space, None);
        canvas.fill(canvas.base, [0.7, 0.2, 0.4, 0.9]);
        let mut style = test_style(BrushExecution::Dry);
        style.mode = DabMode::Erase;
        canvas.paint(canvas.base, style, soft([64., 64.], WHITE, 45., 0.1));
        canvas.r.readback_srgb_rgba8().unwrap()
    };
    assert_eq!(erased(BlendSpace::Perceptual), erased(BlendSpace::Linear));
}

#[test]
fn edged_brushes_lay_dabs_in_the_material_pass_in_perceptual_documents() {
    let mut style = test_style(BrushExecution::Dry);
    style.rendering.wet_edge = 0.6;
    assert!(BrushPassPlan::for_style(&style).direct.is_some());
    style.blend_space = BlendSpace::Perceptual;
    let plan = BrushPassPlan::for_style(&style);
    assert!(plan.direct.is_none() && plan.material == MaterialOperation::Deposit);
}

#[test]
fn cropped_dry_dispatch_preserves_every_untouched_hdr_pixel() {
    use layer_core::{BrushContact, BrushDryMedia, BrushDryPose};
    let source_color=[-0.15f32,0.7,1.9,0.6];
    let mut canvas=Canvas::new(DocumentColor {depth:SampleDepth::F32,..Default::default()},BlendSpace::Linear,None);
    let r=&mut canvas.r;
    let base=r.create_page_surface("cropped oracle source");
    let output=r.create_page_surface("cropped oracle destination");
    for underpaint in [[-0.12f32,0.18,1.5,0.6],[0.;4]] {
    let bytes:Vec<_>=(0..PAGE_SIZE*PAGE_SIZE).flat_map(|_|underpaint.into_iter().flat_map(f32::to_le_bytes)).collect();
    crate::test_support::upload_page(r,&base.texture,&bytes);
    for dirty in [PixelRect::new(37,41,91,84),PixelRect::new(0,0,19,17),
        PixelRect::new(0,41,91,42),PixelRect::new(37,0,38,84),PixelRect::new(31,33,50,50)] {
    for mode in [DabMode::Paint,DabMode::Erase] {
        let mut style=test_style(BrushExecution::Dry);
        style.mode=mode;style.blend_space=BlendSpace::Linear;style.rendering.accumulation=BrushAccumulation::Flow;
        style.contact=Some(BrushContact {pressure_gain:1.,dry:Some(BrushDryMedia {
            poses:[BrushDryPose {radii:[1.;2],relief:[0.;3]};3],softness:0.05,loading:0.,corner:0.,
        }),..Default::default()});
        let mut dab=test_dab([64.5;2],source_color,0.2);
        dab.radii=[200.;2];dab.contact=[0.7,0.,0.,0.];dab.previous_contact=dab.contact;
        let batch=crate::test_support::dab_batch(canvas.base,style,dirty.to_rect());
        let mut encoder=crate::submission::CommandEncoder::new(&r.device,&Default::default());
        r.prepare_uploads(FramePacket {dabs:std::slice::from_ref(&dab),dab_batches:std::slice::from_ref(&batch),
            ..packet(canvas.document.scene(),EXTENT)},&mut [Vec::new()],&mut encoder).unwrap();
        r.prepare_dry_records(&batch,std::iter::once((dirty,0..1,[0;16])),&mut encoder).unwrap();
        let key=WgpuRasterizer::texture_set_key(&batch.style);r.ensure_texture_set(key).unwrap();
        r.uploads.finish(&encoder);encoder.submit(&r.queue);
        r.queue.write_buffer(&r.target_buffer,0,target_bytes(&TargetGpu::new([0;2],EXTENT,EXTENT)));
        for inplace in [false,true] {
            let stale:Vec<_>=(0..PAGE_SIZE*PAGE_SIZE).flat_map(|_|[9f32,-7.,3.,0.9].into_iter().flat_map(f32::to_le_bytes)).collect();
            crate::test_support::upload_page(r,&output.texture,if inplace {&bytes} else {&stale});
            let source=if underpaint==[0.;4] {&r.empty_view} else {&base.view};
            let sources=create_material_bind_group(&r.device,&r.material_layout,&[source;9],&r.dab_buffer,
                &r.empty_scalar_view,&r.empty_view,wgpu::BindingResource::Buffer(r.dry_records.binding()));
            let kernels=if inplace {r.pipelines.dry_in_place.as_ref().expect("reference hardware supports in-place storage")}
                else {&r.pipelines.dry_material};
            let binding=kernels.output(r,&output,None,canvas.base,[0;2]);
            let mut encoder=crate::submission::CommandEncoder::new(&r.device,&Default::default());
            let in_place_pipelines=if inplace {None} else {r.pipelines.dry_in_place.take()};
            r.encode_dry_material_jobs(&mut encoder,0,&batch,&[(binding,sources,[0;2],false,0,false,dirty)]);
            if let Some(pipelines)=in_place_pipelines {r.pipelines.dry_in_place=Some(pipelines);}
            encoder.submit(&r.queue);
            let actual=crate::test_support::float_pixels(r,&output.texture);
            let remaining=(-6f64*f64::from(dab.flow)*f64::from(source_color[3])).exp();
            for y in 0..PAGE_SIZE {for x in 0..PAGE_SIZE {
                let pixel=actual[(y*PAGE_SIZE+x) as usize];
                let inside=x>=dirty.min_x()&&x<dirty.max_x()&&y>=dirty.min_y()&&y<dirty.max_y();
                if !inside {assert_eq!(pixel.map(f32::to_bits),underpaint.map(f32::to_bits),
                    "{mode:?} inplace={inplace} underpaint={underpaint:?} dirty={dirty:?}: unchanged HDR pixel ({x},{y})");continue;}
                let expected=if mode==DabMode::Erase {underpaint.map(|v|f64::from(v)*remaining)} else {
                    std::array::from_fn(|c|f64::from(underpaint[c])*remaining+
                        (if c==3 {1.} else {f64::from(source_color[c])})*(1.-remaining))};
                for c in 0..4 {assert!((f64::from(pixel[c])-expected[c]).abs()<1e-6,
                    "{mode:?} inplace={inplace} underpaint={underpaint:?} dirty={dirty:?}: dirty HDR pixel ({x},{y}) channel{c}: {pixel:?} != {expected:?}");}
            }}
        }
    }}}
}

#[test]
fn cropped_display_restores_hdr_blocks_at_partial_document_edges() {
    use layer_core::{BrushContact,BrushDryMedia,BrushDryPose};
    let extent=[131,129];
    let underpaint=|x:u32,y:u32|[-0.12+x as f32*0.002,0.18+y as f32*0.003,1.5+(x+y) as f32*0.001,0.6];
    let bytes:Vec<_>=(0..PAGE_SIZE).flat_map(|y|(0..PAGE_SIZE).flat_map(move|x|
        underpaint(x,y).into_iter().flat_map(f32::to_le_bytes))).collect();
    let mut canvas=Canvas::at(DocumentColor {depth:SampleDepth::F32,..Default::default()},BlendSpace::Linear,None,extent);
    let r=&mut canvas.r;
    let base=r.create_page_surface("display HDR source");
    crate::test_support::upload_page(r,&base.texture,&bytes);
    let dirty=PixelRect::new(37,41,extent[0],extent[1]);
    let color=[-0.15f32,0.7,1.9,0.6];
    for block in [2u32,4] {for (mode,contribution) in [(DabMode::Paint,false),(DabMode::Erase,false),(DabMode::Paint,true)] {
        r.preview_level=block.trailing_zeros();r.preview_contribution=contribution;
        let mut style=test_style(BrushExecution::Dry);style.mode=mode;style.blend_space=BlendSpace::Linear;
        style.rendering.accumulation=BrushAccumulation::Flow;
        style.contact=Some(BrushContact {pressure_gain:1.,dry:Some(BrushDryMedia {
            poses:[BrushDryPose {radii:[1.;2],relief:[0.;3]};3],softness:0.05,loading:0.,corner:0.,
        }),..Default::default()});
        let mut dab=test_dab([64.5;2],color,0.2);dab.radii=[200.;2];
        dab.contact=[0.7,0.,0.,0.];dab.previous_contact=dab.contact;
        let mut batch=crate::test_support::dab_batch(canvas.base,style,dirty.to_rect());batch.kind=DabBatchKind::Preview;
        let mut encoder=crate::submission::CommandEncoder::new(&r.device,&Default::default());
        r.prepare_uploads(FramePacket {dabs:std::slice::from_ref(&dab),dab_batches:std::slice::from_ref(&batch),
            ..packet(canvas.document.scene(),extent)},&mut [Vec::new()],&mut encoder).unwrap();
        r.prepare_dry_records(&batch,std::iter::once((dirty,0..1,[0;16])),&mut encoder).unwrap();
        let key=WgpuRasterizer::texture_set_key(&batch.style);r.ensure_texture_set(key).unwrap();
        r.uploads.finish(&encoder);encoder.submit(&r.queue);
        r.queue.write_buffer(&r.target_buffer,0,target_bytes(&TargetGpu::new([0;2],extent,extent)));
        let size=[PAGE_SIZE/block;2];
        let output=create_page_surface(&r.device,&r.texture_layout,&r.sampler,size,r.device.working_format(),"stale display HDR output");
        let stale:Vec<_>=(0..size[0]*size[1]).flat_map(|_|[9f32,-7.,3.,0.9].into_iter().flat_map(f32::to_le_bytes)).collect();
        r.queue.write_texture(output.texture.as_image_copy(),&stale,wgpu::TexelCopyBufferLayout {
            offset:0,bytes_per_row:Some(size[0]*16),rows_per_image:Some(size[1]),
        },output.texture.size());
        let sources=create_material_bind_group(&r.device,&r.material_layout,&[&base.view;9],&r.dab_buffer,
            &r.empty_scalar_view,&r.empty_view,wgpu::BindingResource::Buffer(r.dry_records.binding()));
        let binding=r.pipelines.dry_display.output(r,&output,None,canvas.base,[0;2]);
        let mut encoder=crate::submission::CommandEncoder::new(&r.device,&Default::default());
        r.encode_dry_material_jobs(&mut encoder,0,&batch,&[(binding,sources,[0;2],false,0,false,dirty)]);
        encoder.submit(&r.queue);
        let actual=crate::test_support::float_pixels(r,&output.texture);
        let remaining=(-6f64*f64::from(dab.flow)*f64::from(color[3])).exp();
        for y in 0..size[1] {for x in 0..size[0] {
            let start=[x*block,y*block];
            let end=[(start[0]+block).min(extent[0]),(start[1]+block).min(extent[1])];
            let mut expected=[0f64;4];
            if !contribution && start[0]<end[0] && start[1]<end[1] {
                for py in start[1]..end[1] {for px in start[0]..end[0] {
                    let pixel=underpaint(px,py);for c in 0..4 {expected[c]+=f64::from(pixel[c]);}
                }}
                let count=f64::from((end[0]-start[0])*(end[1]-start[1]));for v in &mut expected {*v/=count;}
            }
            let center=[start[0]+block/2,start[1]+block/2];
            if center[0]>=dirty.min_x() && center[0]<dirty.max_x() && center[1]>=dirty.min_y() && center[1]<dirty.max_y() {
                expected=if mode==DabMode::Erase {expected.map(|v|v*remaining)} else {
                    std::array::from_fn(|c|expected[c]*remaining+(if c==3 {1.} else {f64::from(color[c])})*(1.-remaining))};
            }
            let pixel=actual[(y*size[0]+x) as usize];
            for c in 0..4 {assert!((f64::from(pixel[c])-expected[c]).abs()<1e-6,
                "block{block} {mode:?} contribution={contribution} HDR({x},{y}) channel{c}: {pixel:?} != {expected:?}");}
        }}
    }}
}


#[test]
fn independent_full_page_dispatch_preserves_contact_hull_pixels() {
    use layer_core::DefaultBrushPreset as P;
    for (name,preset,center,radii,motion,angle,old_angle,large) in [
        ("tangent",P::Pencil,[255.49,256.49],[4.1,4.1],[180.,0.],0.37,0.37,false),
        ("thin-rotating",P::RoughGPen,[260.,258.],[41.,1.2],[180.,160.],1.1,-0.7,false),
        ("pooled-offcanvas",P::BlottyInk,[1.,260.],[3.,49.],[-140.,180.],-0.8,0.6,false),
        ("fine-AA",P::Pencil,[257.,255.],[2.6,1.3],[119.,-85.],0.87,-0.42,false),
        ("rounded",P::PastelBlock,[256.7,257.1],[72.,23.],[150.,100.],0.73,-0.37,false),
        ("wide-turn",P::Charcoal,[253.,260.],[132.,28.],[210.,-140.],0.94,-0.9,false),
        ("large-coordinates",P::Pencil,[16128.49,256.49],[11.,3.],[180.,-90.],0.51,-0.66,true),
    ] {
        let angle:f32=angle;let old_angle:f32=old_angle;
        let extent=if large {[16384,512]} else {[512,512]};
        let mut canvas=Canvas::at(DocumentColor {depth:SampleDepth::F32,..Default::default()},BlendSpace::Linear,None,extent);
        let mut style=crate::layer_tests::preset_style(preset);
        style.rendering.accumulation=BrushAccumulation::Flow;
        if style.contact.unwrap().dry.is_none() {let c=style.contact.as_mut().unwrap();c.edge_roughness=0.9;c.pooling=1.;}
        let mut dab=test_dab(center,[-0.15,0.7,1.9,0.6],0.41);
        dab.radii=radii;dab.motion=motion;dab.rotation=[angle.cos(),angle.sin()];
        dab.previous=[radii[0]*0.82,radii[1]*1.17,old_angle.cos(),old_angle.sin()];
        dab.contact=[0.73,0.2,0.4,17.];dab.previous_contact=[0.41,0.7,0.1,17.];
        if let Some(d)=style.contact.unwrap().dry {let v=d.pose(0.2).relief;let old=d.pose(0.7).relief;
            dab.contact=[0.73,v[0],v[1],v[2]];dab.previous_contact=[0.41,old[0],old[1],old[2]];}
        canvas.paint(canvas.base,style.clone(),dab);
        let batch=crate::test_support::dab_batch(canvas.base,style.clone(),Rect {min:Point{x:0.,y:0.},max:Point{x:extent[0] as f32,y:512.}});
        let r=&mut canvas.r;let base=r.create_page_surface("independent zero source");let output=r.create_page_surface("uncropped oracle");
        crate::test_support::upload_page(r,&base.texture,&vec![0;256*256*16]);
        let coordinates:Vec<_>=if large {(62..64).flat_map(|x|(0..2).map(move|y|[x,y])).collect()}
            else {(0..2).flat_map(|x|(0..2).map(move|y|[x,y])).collect()};
        let actual:Vec<_>=coordinates.iter().map(|coord|r.paint_layers.iter().find(|l|l.id==canvas.base)
            .and_then(|l|l.pages.iter().find(|p|p.coordinate==*coord)).map(|p|crate::layer_tests::page_bytes(r,&p.active().texture))
            .unwrap_or_else(||vec![0;256*256*16])).collect();
        let mut encoder=crate::submission::CommandEncoder::new(&r.device,&Default::default());
        r.prepare_uploads(FramePacket {dabs:&[dab],dab_batches:std::slice::from_ref(&batch),..packet(canvas.document.scene(),extent)},&mut [Vec::new()],&mut encoder).unwrap();
        r.prepare_dry_records(&batch,std::iter::once((PixelRect::full([256;2]),0..1,[0;16])),&mut encoder).unwrap();
        let key=WgpuRasterizer::texture_set_key(&style);r.ensure_texture_set(key.clone()).unwrap();r.uploads.finish(&encoder);encoder.submit(&r.queue);
        let marker=mem::offset_of!(StyleGpu,material_a)+12;
        let aggregate=f32::from_ne_bytes(r.style_upload[marker..marker+4].try_into().unwrap())>0.5&&style.contact.unwrap().dry.is_some();
        let fine=style.contact.unwrap().dry.is_some_and(|d|d.corner==0.)&&(0..2).any(|i|0.5*(dab.radii[i]+dab.previous[i])<8.);
        let mut painted=0usize;let mut maximum=0f32;
        for (i,coord) in coordinates.iter().enumerate() {
            r.queue.write_buffer(&r.target_buffer,0,target_bytes(&TargetGpu::new([coord[0]*256,coord[1]*256],extent,extent)));
            let sources=create_material_bind_group(&r.device,&r.material_layout,&[&base.view;9],&r.dab_buffer,&r.empty_scalar_view,&r.empty_view,wgpu::BindingResource::Buffer(r.dry_records.binding()));
            let kernels=&r.pipelines.dry_material;let binding=kernels.output(r,&output,None,canvas.base,*coord);
            let mut encoder=crate::submission::CommandEncoder::new(&r.device,&Default::default());
            {let mut pass=encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {label:Some("full independent page"),timestamp_writes:None});
                pass.set_pipeline(kernels.selected_kernel(&style,MaterialOperation::Deposit,false,if aggregate {8192}else{0}|if fine{16384}else{0}));
                pass.set_bind_group(0,&binding,&[0]);pass.set_bind_group(1,r.paint_target_binding(&style),&[0]);pass.set_bind_group(2,&sources,&[0]);
                pass.set_bind_group(3,&r.texture_sets.iter().find(|s|s.key==key).unwrap().bind_group,&[]);pass.dispatch_workgroups(8,128,1);}
            encoder.submit(&r.queue);let reference=crate::test_support::float_pixels(r,&output.texture);
            painted+=reference.iter().filter(|p|p[3]>0.).count();
            for (pixel,(a,b)) in actual[i].chunks_exact(16).zip(reference).enumerate() {let a:[f32;4]=std::array::from_fn(|c|f32::from_le_bytes(a[c*4..c*4+4].try_into().unwrap()));
                assert_eq!(a[3]==0.,b[3]==0.,"{name} tile{coord:?} pixel{pixel}: strict zero support");
                for c in 0..4 {maximum=maximum.max((a[c]-b[c]).abs());assert!((a[c]-b[c]).abs()<=1e-6,"{name} tile{coord:?} pixel{pixel} channel{c}: clipped{a:?} != uncropped{b:?}");}}
        }
        assert!(painted>16,"{name}: nonempty independent reference");println!("{name}: painted={painted} maximum_RGBA_error={maximum}");
    }
}
