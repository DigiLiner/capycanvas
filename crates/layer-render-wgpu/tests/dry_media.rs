mod support;
use layer_core::*;
use layer_engine::{InstantFeedbackConfig, ViewTransform};
use layer_render::{CanvasRenderer, HostImage, PixelFormat, ViewState};
use std::sync::Arc;

const EXTENT: [u32; 2] = [256; 2];
const PRESETS: [DefaultBrushPreset; 7] = [DefaultBrushPreset::Pencil,
    DefaultBrushPreset::PointyPencil, DefaultBrushPreset::ShadingPencil,
    DefaultBrushPreset::Charcoal, DefaultBrushPreset::Chalk,
    DefaultBrushPreset::PastelBlock, DefaultBrushPreset::Crayon];
const PAPER: &str = "test:dry-contact-paper";

#[derive(Clone, Copy)]
struct Run { samples: u64, cadence: u64, feedback: bool, scale: f32, mask: bool }
impl Default for Run {
    fn default() -> Self { Self { samples: 64, cadence: 4, feedback: false, scale: 1., mask: false } }
}

fn paint(mut brush: BrushSnapshot, run: Run, pose: impl Fn(f32) -> ([f32; 2], f32, [f32; 2])) -> support::Image {
    let mut document = Document::new(PortableId::random(), EXTENT[0], EXTENT[1],
        DocumentNames { paint: "Ink".into(), paper: "Paper".into() });
    let paper = support::named_occurrence(&document, "Paper");
    document.artwork.occurrences.get_mut(paper).unwrap().visible = false;
    if run.mask {
        let owner = document.working.occurrence.unwrap();
        let mut mask = CoverageSnapshot::reveal_all(document.artwork.coverage.next_handle(), EXTENT, [0; 2]);
        mask.source.default_coverage = 0.;
        document.apply(support::mask_edit(&document, owner, mask)).unwrap();
        document.working.target = Some(SourceTarget::Coverage(document.scene().occurrence(owner).unwrap().mask.as_ref().unwrap().source));
        support::paint_mut(&mut document, owner).base = Some(layer_core::authored::PaintBase::new(
            color::source::rgba8_source(EXTENT, |_, _| [255; 4]).into()));
        brush.color_rgba_linear = [1.; 4];
    }
    let (mut engine, mut input) = support::engine(document);
    if brush.grain.as_ref().is_some_and(|g| g.asset.0.as_ref() == PAPER) {
        let heights: Vec<_> = (0..256).map(|y| if y % 16 < 8 { 255 } else { 0 }).collect();
        engine.backend_mut().prepare_asset(&AssetId::from(PAPER), HostImage { width: 1, height: 256,
            stride: 1, format: PixelFormat::R8Unorm, bytes: &heights }).unwrap();
    }
    engine.set_view(ViewState {
        width_px: EXTENT[0], height_px: EXTENT[1],
        document_to_surface: [run.scale, 0., 0., run.scale, 0., 0.],
    }, ViewTransform::IDENTITY);
    engine.set_instant_feedback(InstantFeedbackConfig { enabled: run.feedback, ..Default::default() }).unwrap();
    engine.set_brush(brush).unwrap();
    engine.render_frame_at(0).unwrap();
    let mut time = 1_000_000_000;
    for i in 0..=run.samples {
        time += 4_000_000;
        let (position, pressure, tilt) = pose(i as f32 / run.samples as f32);
        let mut event = support::pen_sample(i, run.samples, time, Point { x: position[0], y: position[1] });
        event.sequence = time;
        event.pressure = pressure;
        event.tilt_radians = tilt;
        input.push(event).unwrap();
        if i % run.cadence == 0 || i == run.samples {
            support::drain_input(&mut engine, time, true);
        }
    }
    support::image(&mut engine, time)
}

fn line(brush: BrushSnapshot, pressure: f32, tilt: f32, run: Run) -> support::Image {
    paint(brush, run, |t| ([32.5 + 192. * t, 128.5], pressure,
        [tilt * std::f32::consts::FRAC_PI_2, 0.]))
}

fn analytic(corner: f32, paper: f32) -> BrushSnapshot {
    let mut brush = default_brush(DefaultBrushPreset::Pencil);
    brush.diameter = 80.;
    brush.flow = 0.3;
    brush.color_rgba_linear = [0., 0., 0., 1.];
    brush.mappings = Arc::from([]);
    brush.grain.as_mut().unwrap().asset = AssetId::from(PAPER);
    brush.grain.as_mut().unwrap().scale = 1.;
    let contact = brush.contact.as_mut().unwrap();
    contact.paper = paper;
    contact.pressure_gain = 1.;
    contact.dry = Some(BrushDryMedia { poses: [BrushDryPose { radii: [1.; 2],
        relief: [0.08, 0.75, 0.3] }; 3], softness: 0.07, loading: 0.3, corner });
    brush
}

fn contact_q(position: [f64; 2], brush: &BrushSnapshot) -> f64 {
    let dry = brush.contact.unwrap().dry.unwrap();
    let radii = dry.poses[0].radii.map(|v| f64::from(v*brush.diameter*0.5));
    let (sin, cos) = f64::from(brush.angle_radians).sin_cos();
    let [x, y] = position;
    let (u, v) = (x*cos+y*sin, y*cos-x*sin);
    if dry.corner == 0. { return (u/radii[0]).powi(2)+(v/radii[1]).powi(2); }
    let rim = f64::from(dry.corner)*radii[0].min(radii[1]);
    (((u.abs()-radii[0]+rim).max(0.)+(v.abs()-radii[1]+rim).max(0.))/rim).powi(2)
}

fn reference_alpha(y: usize, pressure: f64, brush: &BrushSnapshot) -> u8 {
    let contact = brush.contact.unwrap();
    let dry = contact.dry.unwrap();
    let profile = dry.poses[0].relief.map(f64::from);
    let height = if y % 16 < 8 { 1. } else { 0. };
    let pressure = (pressure*f64::from(contact.pressure_gain)).clamp(0., 1.);
    let d = 0.5 - (0.5 + f64::from(contact.paper) * (height - 0.5)).clamp(0., 1.);
    let mut chord = 0.;
    let mut deposition = 0.;
    for i in 0..16384 {
        let x = (i as f64 + 0.5 - 8192.) / 8192. * f64::from(brush.diameter);
        let q = contact_q([x, y as f64-128.], brush);
        if q > 1. { continue; }
        chord += 1.;
        let recess = profile[0] + profile[1]*q + profile[2]*q*q;
        let x = ((0.5*pressure - recess - (1. + 0.25*recess)*d) / f64::from(dry.softness)).clamp(0., 1.);
        deposition += x*x*(3.-2.*x) * (1.-f64::from(dry.loading)*(1.-pressure));
    }
    if chord == 0. { return 0; }
    ((1. - (-6.*f64::from(brush.flow*brush.opacity*brush.color_rgba_linear[3])*deposition/chord).exp())*255.).round() as u8
}

#[test]
fn rotated_block_taps_keep_every_supported_corner_and_no_outside_pixels() {
    for angle in [0., 0.7] {
        let mut brush = analytic(0.35, 0.);
        brush.angle_radians = angle;
        for pose in &mut brush.contact.as_mut().unwrap().dry.as_mut().unwrap().poses {
            pose.radii = [1.2, 0.6];
            pose.relief = [0.; 3];
        }
        let image = paint(brush.clone(), Run { samples: 2, ..Default::default() },
            |_| ([128.5; 2], 1., [0.; 2]));
        for y in 64..193 {
            for x in 64..193 {
                let q = contact_q([x as f64-128., y as f64-128.], &brush);
                let alpha = image.rgba[(y*256+x)*4+3];
                if q < 0.99 { assert!(alpha > 0, "angle={angle} supported tap corner ({x},{y}) is clipped"); }
                if q > 1.01 { assert_eq!(alpha, 0, "angle={angle} tap outside support ({x},{y}) paints"); }
            }
        }
    }
}

#[test]
fn swept_contact_matches_independent_quadrature_for_ellipse_and_rounded_block() {
    for corner in [0., 0.35] {
        for paper in [0., 1.4] {
            for (pressure, softness, c4) in [0.1, 0.5, 1.].into_iter().flat_map(|p| [(p, 0.07, 0.3), (p, 0.005, 4.)]) {
                for angle in [0., 0.7] {
                    let mut brush = analytic(corner, paper);
                    let dry = brush.contact.as_mut().unwrap().dry.as_mut().unwrap();
                    dry.softness = softness;
                    for pose in &mut dry.poses { pose.relief[2] = c4; }
                    brush.angle_radians = angle;
                    if angle > 0. {
                        for pose in &mut brush.contact.as_mut().unwrap().dry.as_mut().unwrap().poses { pose.radii = [1.2, 0.6]; }
                    }
                    let image = line(brush.clone(), pressure, 0., Run::default());
                    for y in 84..173 {
                        let actual = image.rgba[(y * 256 + 128)*4 + 3];
                        let expected = reference_alpha(y, pressure as f64, &brush);
                        assert!(actual.abs_diff(expected) <= 2,
                            "corner={corner} angle={angle} paper={paper} pressure={pressure} c4={c4} softness={softness} y={y}: GPU alpha {actual}, quadrature {expected}");
                        if expected == 0 { assert_eq!(actual, 0, "empty contacts must stay exact at y={y}"); }
                    }
                }
            }
        }
    }
}

#[test]
fn native_masks_evaluate_the_same_stationary_paper_and_swept_relief() {
    for corner in [0., 0.35] {
        let mut brush = analytic(corner, 1.4);
        brush.angle_radians = 0.7;
        brush.opacity = 0.6;
        for pose in &mut brush.contact.as_mut().unwrap().dry.as_mut().unwrap().poses { pose.radii = [1.2, 0.6]; }
        for samples in [1, 64] {
            let image = line(brush.clone(), 0.5, 0., Run { samples, mask: true, ..Default::default() });
            for y in 84..173 {
                let actual = image.rgba[(y*256+128)*4+3];
                let expected = reference_alpha(y, 0.5, &brush);
                assert!(actual.abs_diff(expected) <= 2,
                    "native mask corner={corner} samples={samples} y={y}: alpha {actual}, quadrature {expected}");
                if expected == 0 { assert_eq!(actual, 0, "native mask empty paper contact y={y}"); }
            }
        }
    }
}

#[test]
fn dry_exposure_preserves_path_partition_cadence_and_masks() {
    for (corner,diameter,pressure,angle,paper,check_mask) in [(0.,80.,0.55,0.,1.4,false),
        (0.35,80.,0.55,0.,1.4,false),(0.,2.,0.5,0.,0.,true),
        (0.,4.,0.25,0.7,1.4,true),(0.,6.,0.65,0.4,1.4,true)] {
        let mut brush=analytic(corner,paper);brush.diameter=diameter;brush.angle_radians=angle;
        for &mask in if check_mask {&[false,true][..]} else {&[false][..]} {
            let reference=line(brush.clone(),pressure,0.,Run {samples:2,cadence:1,mask,..Default::default()});
            for (samples,cadence) in [(64,1),(64,17),(128,9)] {
                let actual=line(brush.clone(),pressure,0.,Run {samples,cadence,mask,..Default::default()});
                actual.assert_near(&reference,2,&format!("corner={corner} diameter={diameter} pressure={pressure} angle={angle} paper={paper} mask={mask} samples={samples} cadence={cadence}"));
            }
        }
    }
}

#[test]
fn every_dry_medium_preserves_committed_pressure_and_tilt_with_prediction_and_zoom() {
    for preset in PRESETS {
        let mut brush = default_brush(preset);
        brush.diameter = 90.;
        let path = |t: f32| ([32.5 + 192.*t, 128.5 + 20.*(t*std::f32::consts::TAU).sin()],
            0.15 + 0.8*(t*std::f32::consts::PI).sin(),
            [1.3*t, 0.2*t]);
        let real = paint(brush.clone(), Run { cadence: 1, ..Default::default() }, path);
        assert!(real.rgba.chunks_exact(4).any(|p| p[3] > 0), "{preset:?} paints");
        for scale in [1., 0.1] {
            let predicted = paint(brush.clone(), Run { cadence: 8, feedback: true, scale, ..Default::default() }, path);
            predicted.assert_near(&real, 5, &format!("{preset:?} prediction scale={scale}"));
        }
    }
}

#[test]
fn pressure_recruits_contacts_without_forcing_the_rim_or_zero_force_to_paint() {
    for preset in PRESETS {
        let mut brush = default_brush(preset);
        brush.diameter = 120.;
        brush.mappings = Arc::from([]);
        let mut previous = vec![0; (EXTENT[0]*EXTENT[1]*4) as usize];
        for pressure in [0., 0.15, 0.45, 1.] {
            let image = line(brush.clone(), pressure, 0.85, Run::default());
            let mut recruited = 0;
            for (before, after) in previous.chunks_exact(4).zip(image.rgba.chunks_exact(4)) {
                assert!(after[3] >= before[3], "{preset:?} pressure={pressure} loses contact: {} -> {}", before[3], after[3]);
                recruited += usize::from(before[3] == 0 && after[3] > 0);
            }
            if pressure == 0. { assert!(image.rgba.chunks_exact(4).all(|p| p[3] == 0), "{preset:?}: zero pressure paints"); }
            if pressure == 1. { assert!(recruited > 0, "{preset:?}: pressure must recruit empty contacts"); }
            previous = image.rgba;
        }
    }
    let image = line(analytic(0., 0.), 1., 0., Run::default());
    assert!(image.rgba[(128*256+128)*4+3] > 0);
    for y in [91, 92, 164, 165] {
        assert_eq!(image.rgba[(y*256+128)*4+3], 0, "full pressure leaves recessed interior rim y={y} empty");
    }
}

#[test]
fn paper_contacts_stay_at_document_coordinates_when_a_stroke_starts_elsewhere() {
    for corner in [0., 0.35] {
        let brush = analytic(corner, 1.4);
        let first = line(brush.clone(), 0.55, 0., Run::default());
        let second = paint(brush, Run::default(), |t| ([8.5 + 240.*t, 128.5], 0.55, [0.; 2]));
        first.crop([110, 88], [35, 81]).assert_near(&second.crop([110, 88], [35, 81]), 2,
            &format!("stationary paper corner={corner}"));
    }
}

#[test]
fn dry_pose_keys_have_continuous_visible_contact() {
    for preset in PRESETS {
        let mut brush = default_brush(preset);
        brush.diameter = 90.;
        for key in [0.5, 0.85] {
            let before = line(brush.clone(), 0.7, key-0.0001, Run::default());
            let after = line(brush.clone(), 0.7, key+0.0001, Run::default());
            after.assert_near(&before, 1, &format!("{preset:?} tilt key {key}"));
        }
    }
}

fn strip_pixel_area(pixel: [usize; 2], center: [f64; 2], normal: [f64; 2], half: f64) -> f64 {
    let [x, y] = pixel.map(|v| v as f64);
    let mut polygon = vec![[x, y], [x+1., y], [x+1., y+1.], [x, y+1.]];
    for side in [-1., 1.] {
        let plane = |p: [f64; 2]| side*((p[0]-center[0])*normal[0]+(p[1]-center[1])*normal[1])-half;
        let mut clipped = Vec::new();
        if polygon.is_empty() { return 0.; }
        for i in 0..polygon.len() {
            let a = polygon[i];
            let b = polygon[(i+1)%polygon.len()];
            let (fa, fb) = (plane(a), plane(b));
            if fa <= 0. { clipped.push(a); }
            if (fa <= 0.) != (fb <= 0.) {
                let t = fa/(fa-fb);
                clipped.push([a[0]+t*(b[0]-a[0]), a[1]+t*(b[1]-a[1])]);
            }
        }
        polygon = clipped;
    }
    (0..polygon.len()).map(|i| {
        let a = polygon[i]; let b = polygon[(i+1)%polygon.len()];
        a[0]*b[1]-a[1]*b[0]
    }).sum::<f64>().abs()*0.5
}

#[test]
fn fine_contact_alpha_matches_independent_pixel_area_for_rotated_sweeps() {
    for (diameter, angle, direction, phase, flow) in [
        (2., 0., 0., 0., 0.3), (2., 0., 0., 0.5, 1.),
        (2., 0.7, 0.4, 0.25, 0.3), (2., 0.7, 0.9, 0.5, 1.),
        (4., 0.7, 0., 0.375, 1.), (4., 0., 0.7, 0.625, 0.3),
    ] {
        let mut brush = analytic(0., 0.);
        brush.diameter = diameter;
        brush.flow = flow;
        brush.angle_radians = angle;
        for pose in &mut brush.contact.as_mut().unwrap().dry.as_mut().unwrap().poses {
            pose.radii = [1., 0.5]; pose.relief = [0.; 3];
        }
        let (s, c) = f64::from(direction).sin_cos();
        let normal = [-s, c];
        let center = [128.5+phase*normal[0], 128.5+phase*normal[1]];
        let image = paint(brush.clone(), Run::default(), |t| {
            let distance = 192.*f64::from(t)-96.;
            ([(center[0]+distance*c) as f32, (center[1]+distance*s) as f32], 0.5, [0.; 2])
        });
        let (s, c) = f64::from(angle).sin_cos();
        let radius = f64::from(diameter)*0.5;
        let half = radius*((normal[0]*c+normal[1]*s).powi(2)
            +0.25*(-normal[0]*s+normal[1]*c).powi(2)).sqrt();
        let alpha = 1.-(-6.*f64::from(flow)*0.85).exp();
        for y in 120..137 { for x in 120..137 {
            let expected = (255.*alpha*strip_pixel_area([x, y], center, normal, half)).round() as u8;
            let actual = image.rgba[(y*256+x)*4+3];
            assert!(actual.abs_diff(expected) <= 2,
                "diameter={diameter} angle={angle} direction={direction} phase={phase} flow={flow} ({x},{y}): GPU {actual}, independently clipped pixel area {expected}");
            if expected == 0 { assert_eq!(actual, 0, "outside finite contact at ({x},{y})"); }
        }}
    }
}

#[test]
fn fine_stationary_contact_matches_independent_two_dimensional_pixel_integration() {
    for (diameter, angle, center, relief, pressure, paper) in [
        (1.,0.,[128.5,128.5],[0.;3],0.5,0.),
        (2.,0.7,[128.75,128.625],[0.,0.75,0.3],0.5,0.),
        (4.,0.4,[128.25,128.875],[0.,0.75,0.3],0.15,1.4),
    ] {
        let mut brush=analytic(0.,paper);brush.diameter=diameter;brush.angle_radians=angle;
        for pose in &mut brush.contact.as_mut().unwrap().dry.as_mut().unwrap().poses {pose.relief=relief;}
        let image=paint(brush.clone(),Run {samples:2,..Default::default()},|_|(center,pressure,[0.;2]));
        let material=brush.contact.unwrap();let dry=material.dry.unwrap();
        for y in 124..133 {for x in 124..133 {
            let height=if y%16<8 {1.} else {0.};
            let depth=0.5-(0.5+f64::from(paper)*(height-0.5)).clamp(0.,1.);
            let p=f64::from(pressure);
            let dose=6.*f64::from(brush.flow*brush.opacity*brush.color_rgba_linear[3]);
            let loading=1.-f64::from(dry.loading)*(1.-p);
            let mut alpha=0.;
            for sy in 0..256 {for sx in 0..256 {
                let q=contact_q([x as f64+(sx as f64+0.5)/256.-f64::from(center[0]),
                    y as f64+(sy as f64+0.5)/256.-f64::from(center[1])],&brush);
                if q>1. {continue;}
                let recess=f64::from(relief[0])+q*(f64::from(relief[1])+f64::from(relief[2])*q);
                let t=((0.5*p-recess-(1.+0.25*recess)*depth)/f64::from(dry.softness)).clamp(0.,1.);
                alpha+=1.-(-dose*loading*t*t*(3.-2.*t)).exp();
            }}
            let expected=(255.*alpha/65536.).round() as u8;
            let actual=image.rgba[(y*256+x)*4+3];
            assert!(actual.abs_diff(expected)<=2,
                "stationary diameter={diameter} angle={angle} p={pressure} paper={paper} ({x},{y}): GPU {actual}, independent2D {expected}");
        }}
    }
}
