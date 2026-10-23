//! Offline swatches rendered by the real GPU presets. Both frontends ship the
//! same small PNGs; opening the brush selector does not run extra brush jobs.
use super::*;
use blend_study::{Painter, line, prepared, scrub, srgb};
use std::f32::consts::{PI, TAU};

pub fn generate(directory: &Path) -> Result<(), Box<dyn Error>> {
    fs::create_dir_all(directory)?;
    for theme in ["dark", "light"] {
        let tones = Tones::of(theme);
        for choice in layer_ui::brush_catalog() {
            if let Ok(ids) = std::env::var("CAPY_BRUSH_PREVIEW_IDS")
                && !ids.split(',').any(|id| id.parse::<u32>().ok() == Some(choice.id))
            {
                continue;
            }
            if std::env::var_os("CAPY_CONTACT_PREVIEWS_ONLY").is_some()
                && !layer_core::CONTACT_BRUSH_PRESETS.iter().any(|p| *p as u32 == choice.id)
            {
                continue;
            }
            let preset = layer_ui::preset(choice.id)?;
            let mut canvas = Canvas::configured([400, 80], [0.0; 4])?;
            match choice.group {
                _ if choice.group.tool() == layer_ui::Tool::Blend => blender(&mut canvas, preset, &tones)?,
                layer_ui::ToolGroup::Liquify => liquify(&mut canvas, preset, &tones)?,
                layer_ui::ToolGroup::Retouch => retouch(&mut canvas, preset, &tones)?,
                _ if preset == Preset::Eraser => eraser(&mut canvas, &tones)?,
                _ => {
                    canvas.set_brush(brush(preset, 42.0, 1.0, tones.strong))?;
                    stroke(&mut canvas, -3.0)?;
                }
            }
            canvas.write_png(&directory.join(format!("{}-{theme}.png", choice.id)))?;
            eprintln!("swatch: {} ({theme})", choice.label);
        }
    }
    Ok(())
}

struct Tones {
    strong: [f32; 4],
}

impl Tones {
    fn of(theme: &str) -> Self {
        Self { strong: if theme == "dark" { [0.9, 0.9, 0.9, 1.0] } else { [0.012, 0.012, 0.014, 1.0] } }
    }
}

fn wave(phase: f32, offset: f32, amplitude: f32, from: f32, to: f32) -> Vec<(f32, f32)> {
    (0..=120).map(|i| {
        let t = i as f32 / 120.0;
        let x = from + t * (to - from);
        (x, 40.0 + offset + amplitude * ((x - 22.0) / 356.0 * TAU + phase).sin())
    }).collect()
}

fn tapered(points: &[(f32, f32)]) -> Vec<(f32, f32, f32)> {
    let count = points.len().max(2) - 1;
    points.iter().enumerate().map(|(i, &(x, y))| {
        let t = i as f32 / count as f32;
        (x, y, 0.16 + 0.84 * (t * PI).sin().max(0.0).powf(0.65))
    }).collect()
}

fn even(points: &[(f32, f32)], pressure: f32) -> Vec<(f32, f32, f32)> {
    points.iter().map(|&(x, y)| (x, y, pressure)).collect()
}

fn trace(canvas: &mut Canvas, points: &[(f32, f32, f32)]) -> Result<(), String> {
    for (i, &(x, y, pressure)) in points.iter().enumerate() {
        let event = canvas.next_event(Sample { x, y, pressure }, phase(i, points.len()));
        canvas.submit(&[event])?;
        if i % 8 == 0 || i + 1 == points.len() {
            canvas.draw()?;
        }
    }
    canvas.wait_idle()
}

fn paint(canvas: &mut Canvas, preset: Preset, color: [f32; 4], diameter: f32, points: &[(f32, f32, f32)]) -> Result<(), String> {
    canvas.set_brush(brush(preset, diameter, 1.0, color))?;
    trace(canvas, points)
}

fn centre(x: f32) -> f32 {
    40.0 + 6.0 * ((x - 20.0) / 360.0 * TAU).sin()
}

fn swell(x: f32) -> f32 {
    ((x - 14.0) / 372.0 * PI).sin().clamp(0.0, 1.0).powf(0.35)
}

fn lens(canvas: &mut Canvas, preset: Preset, color: [f32; 4], diameter: f32, rise: f32, from: f32, to: f32) -> Result<(), String> {
    let points: Vec<_> = (0..=150).map(|i| {
        let x = from + (to - from) * i as f32 / 150.0;
        (x, centre(x) - rise * swell(x), swell(x).max(0.05))
    }).collect();
    paint(canvas, preset, color, diameter, &points)
}

struct Shades {
    light: [f32; 4],
    mid: [f32; 4],
    dark: [f32; 4],
}

impl Shades {
    fn of(tones: &Tones) -> Self {
        if tones.strong[0] > 0.5 {
            Self { light: srgb(0xececec), mid: srgb(0xa2a2a2), dark: srgb(0x5c5c5c) }
        } else {
            Self { light: srgb(0xdedede), mid: srgb(0x8c8c8c), dark: srgb(0x353535) }
        }
    }
}

fn tube(canvas: &mut Canvas, shades: &Shades, from: f32, to: f32) -> Result<(), String> {
    lens(canvas, Preset::GPen, shades.dark, 64.0, 0.0, from, to)?;
    lens(canvas, Preset::GPen, shades.mid, 46.0, 9.0, from, to)?;
    lens(canvas, Preset::GPen, shades.light, 16.0, 16.0, from, to)
}

fn tube_scrub(from: f32, to: f32, step: f32, reach: f32) -> Vec<(f32, f32, f32)> {
    let mut points: Vec<(f32, f32, f32)> = vec![];
    let mut x = from;
    let mut down = true;
    while x <= to {
        let y = centre(x) + if down { reach } else { -reach } * swell(x);
        let start = points.last().map_or((x, y), |&(x, y, _)| (x, y));
        points.extend(line(start, (x, y), 2.0).into_iter().map(|(x, y)| (x, y, 1.0)));
        down = !down;
        x += step;
    }
    points
}

fn blender(canvas: &mut Canvas, preset: Preset, tones: &Tones) -> Result<(), String> {
    let shades = Shades::of(tones);
    let mut path = tube_scrub(150.0, 380.0, 16.0, 18.0);
    match preset {
        Preset::BlendingStump | Preset::Tissue => {
            let charcoal = if tones.strong[0] > 0.5 { shades.light } else { shades.dark };
            for row in 0..5 {
                let rise = 22.0 - row as f32 * 11.0;
                lens(canvas, Preset::Charcoal, charcoal, 26.0, rise, 14.0, 386.0)?;
            }
        }
        Preset::BlendingKnife => {
            let blocks = if tones.strong[0] > 0.5 { [0x9a9a9a, 0xe4e4e4, 0x6a6a6a, 0xbdbdbd] } else { [0xb0b0b0, 0x353535, 0x7a7a7a, 0x505050] };
            for (k, color) in blocks.map(srgb).into_iter().enumerate() {
                let from = 14.0 + k as f32 * 93.0;
                let points: Vec<_> = (0..=40).map(|i| { let x = from + 93.0 * i as f32 / 40.0; (x, centre(x), swell(x).max(0.05)) }).collect();
                paint(canvas, Preset::GPen, color, 64.0, &points)?;
            }
            let mut points = vec![];
            for k in 0..4 { let x = 120.0 + k as f32 * 66.0; points.extend([(x, centre(x) - 24.0 + k as f32 * 3.0), (x + 56.0, centre(x + 56.0) + 24.0 - k as f32 * 3.0)]); }
            path = blend_study::pressured(blend_study::polyline(&points, 2.0), 0.9);
        }
        Preset::CombBlender => {
            for k in 0..11 {
                let rise = (k as f32 - 5.0) / 5.0 * 26.0;
                let color = [shades.dark, shades.mid, shades.dark, shades.light][k % 4];
                let strand: Vec<_> = (0..=150).map(|i| {
                    let x = 14.0 + 372.0 * i as f32 / 150.0;
                    (x, centre(x) + rise * swell(x) + 2.5 * ((x - 14.0) / 46.0 + k as f32 * 0.9).sin(), swell(x).max(0.05))
                }).collect();
                paint(canvas, Preset::Chalk, color, 8.0, &strand)?;
            }
        }
        Preset::Sharpen => {
            lens(canvas, Preset::GPen, shades.light, 62.0, 0.0, 14.0, 386.0)?;
            for (x, color) in [(60.0, shades.dark), (125.0, shades.mid), (190.0, shades.dark), (255.0, shades.mid), (320.0, shades.dark)] {
                let y = centre(x);
                paint(canvas, Preset::GPen, color, 40.0, &[(x, y, 1.0), (x + 0.01, y, 1.0)])?;
            }
            let mut blur = default_brush(Preset::Blur);
            blur.diameter = 70.0;
            blur.stabilization = BrushStabilization { strength: 0.0, pressure_smoothing: 0.0, ..blur.stabilization };
            Painter { canvas, origin: [0.0; 2] }.blend(&blur, &even(&(0..=190).map(|i| { let x = 10.0 + i as f32 * 2.0; (x, centre(x)) }).collect::<Vec<_>>(), 1.0));
            path = tube_scrub(150.0, 380.0, 8.0, 18.0);
        }
        Preset::HueBlender => {
            let warm = if tones.strong[0] > 0.5 { [srgb(0xeee4da), srgb(0xb3a08e), srgb(0x6e5a4a)] } else { [srgb(0xe6dbd0), srgb(0x9e8a77), srgb(0x4a3a2e)] };
            let cool = if tones.strong[0] > 0.5 { [srgb(0xdde4ee), srgb(0x8e9db3), srgb(0x4a5a6e)] } else { [srgb(0xd0dbe6), srgb(0x778a9e), srgb(0x2e3a4a)] };
            tube(canvas, &Shades { light: warm[0], mid: warm[1], dark: warm[2] }, 14.0, 200.0)?;
            tube(canvas, &Shades { light: cool[0], mid: cool[1], dark: cool[2] }, 200.0, 386.0)?;
            path = tube_scrub(110.0, 290.0, 12.0, 26.0);
        }
        _ => tube(canvas, &shades, 14.0, 386.0)?,
    }
    Painter { canvas, origin: [0.0; 2] }.blend(&prepared(default_brush(preset), 42.0), &path);
    canvas.wait_idle()
}

fn eraser(canvas: &mut Canvas, tones: &Tones) -> Result<(), String> {
    lens(canvas, Preset::GPen, Shades::of(tones).mid, 64.0, 0.0, 14.0, 386.0)?;
    paint(canvas, Preset::Eraser, tones.strong, 22.0, &tapered(&wave(PI, 0.0, 20.0, 70.0, 340.0)))
}

fn hold(at: (f32, f32), turns: f32) -> Vec<(f32, f32, f32)> {
    (0..=90).map(|i| {
        let a = i as f32 / 90.0 * turns * TAU;
        (at.0 + 2.0 * a.cos(), at.1 + 2.0 * a.sin(), 1.0)
    }).collect()
}

fn halftone(canvas: &mut Canvas, tones: &Tones) -> Result<(), String> {
    let shades = Shades::of(tones);
    for column in 0..46 {
        let x = 16.0 + column as f32 * 8.2;
        for row in -4..=4 {
            let offset = row as f32 * 7.6;
            if offset.abs() > 31.0 * swell(x) - 2.0 { continue; }
            let color = if (row + column) % 2 == 0 { shades.dark } else { shades.mid };
            let y = centre(x) + offset;
            paint(canvas, Preset::GPen, color, 6.4, &[(x, y, 1.0), (x + 0.01, y, 1.0)])?;
        }
    }
    Ok(())
}

fn liquify(canvas: &mut Canvas, preset: Preset, tones: &Tones) -> Result<(), String> {
    halftone(canvas, tones)?;
    let along = |from: f32, to: f32| even(&(0..=100).map(|i| { let x = from + (to - from) * i as f32 / 100.0; (x, centre(x)) }).collect::<Vec<_>>(), 1.0);
    let paths = match preset {
        Preset::LiquifyPush => vec![tube_scrub(150.0, 370.0, 30.0, 20.0)],
        Preset::LiquifyTwirlClockwise | Preset::LiquifyTwirl | Preset::LiquifyPinch | Preset::LiquifyExpand =>
            vec![hold((200.0, centre(200.0)), 6.0), hold((310.0, centre(310.0)), 6.0)],
        Preset::LiquifyReconstruct => vec![],
        Preset::LiquifyEdge => vec![along(150.0, 370.0), along(150.0, 370.0)],
        _ => vec![along(150.0, 370.0)],
    };
    canvas.set_brush(brush(preset, 44.0, 1.0, tones.strong))?;
    for path in paths { trace(canvas, &path)?; }
    Ok(())
}

fn mark(canvas: &mut Canvas, tones: &Tones, points: &[(f32, f32)]) -> Result<(), String> {
    let halo = if tones.strong[0] > 0.5 { srgb(0x262626) } else { srgb(0xf4f4f4) };
    paint(canvas, Preset::GPen, halo, 5.5, &even(points, 1.0))?;
    paint(canvas, Preset::GPen, tones.strong, 2.5, &even(points, 1.0))
}

fn ring(canvas: &mut Canvas, tones: &Tones, at: (f32, f32), radius: f32, cross: bool) -> Result<(), String> {
    mark(canvas, tones, &(0..=72).map(|i| { let a = i as f32 / 72.0 * TAU; (at.0 + radius * a.cos(), at.1 + radius * a.sin()) }).collect::<Vec<_>>())?;
    if cross {
        for (dx, dy) in [(1.0, 0.0), (0.0, 1.0)] {
            mark(canvas, tones, &line((at.0 - 7.0 * dx, at.1 - 7.0 * dy), (at.0 + 7.0 * dx, at.1 + 7.0 * dy), 1.0))?;
        }
    }
    Ok(())
}

fn arrow(canvas: &mut Canvas, tones: &Tones, from: (f32, f32), to: f32) -> Result<(), String> {
    let mut x = from.0;
    while x + 6.0 < to - 6.0 {
        mark(canvas, tones, &line((x, from.1), (x + 6.0, from.1), 1.0))?;
        x += 11.0;
    }
    mark(canvas, tones, &[(to - 7.0, from.1 - 5.0), (to, from.1), (to - 7.0, from.1 + 5.0)])
}

fn graded(canvas: &mut Canvas, tones: &Tones) -> Result<(), String> {
    let (light, dark) = if tones.strong[0] > 0.5 { (0xb8b8b8u32, 0x5e5e5eu32) } else { (0xe2e2e2u32, 0x8e8e8eu32) };
    let mix = |t: f32| {
        let channel = |shift: u32| (((light >> shift) & 255) as f32 * (1.0 - t) + ((dark >> shift) & 255) as f32 * t) as u32;
        srgb(channel(16) << 16 | channel(8) << 8 | channel(0))
    };
    for k in 0..48 {
        let from = 14.0 + k as f32 * 7.75;
        let points: Vec<_> = (0..=8).map(|i| { let x = from + 10.0 * i as f32 / 8.0; (x, centre(x), swell(x).max(0.05)) }).collect();
        paint(canvas, Preset::GPen, mix(k as f32 / 47.0), 64.0, &points)?;
    }
    canvas.wait_idle()
}

fn retouch(canvas: &mut Canvas, preset: Preset, tones: &Tones) -> Result<(), String> {
    let flaw = if tones.strong[0] > 0.5 { srgb(0x161616) } else { srgb(0x2a2a2a) };
    match preset {
        Preset::CloneStamp => {
            let leaf: Vec<_> = (0..=60).map(|i| {
                let t = i as f32 / 60.0;
                (26.0 + 110.0 * t, 66.0 - 46.0 * (t * PI * 0.85).sin() + 8.0 * t, (t * PI).sin().max(0.08).powf(0.55))
            }).collect();
            let shades = Shades::of(tones);
            paint(canvas, Preset::GPen, shades.dark, 44.0, &leaf)?;
            paint(canvas, Preset::GPen, shades.light, 4.0, &leaf[10..52].iter().map(|&(x, y, _)| (x, y + 2.0, 1.0)).collect::<Vec<_>>())?;
            let shift = 226.0;
            let copy = scrub(250.0, 316.0, 10.0, 70.0, 6.0);
            canvas.engine.set_retouch(Some(layer_core::RetouchSource::Editing));
            canvas.engine.set_clone_source(layer_core::CloneSource { point: Some(Point { x: copy[0].0 - shift, y: copy[0].1 }), offset: Some([-shift, 0.0]), aligned: true, flip: [false; 2] });
            canvas.set_brush(brush(preset, 30.0, 1.0, tones.strong))?;
            trace(canvas, &copy)?;
            canvas.engine.set_retouch(None);
            ring(canvas, tones, (324.0 - shift, 40.0), 19.0, true)?;
            ring(canvas, tones, (324.0, 40.0), 19.0, false)?;
            arrow(canvas, tones, (324.0 - shift + 25.0, 40.0), 300.0)
        }
        Preset::HealingBrush => {
            graded(canvas, tones)?;
            let scratch: Vec<_> = (0..=40).map(|i| { let x = 262.0 + 76.0 * i as f32 / 40.0; (x, centre(x) + 6.0 * ((x - 262.0) / 18.0).sin(), 1.0) }).collect();
            paint(canvas, Preset::GPen, flaw, 6.0, &scratch)?;
            canvas.engine.set_retouch(Some(layer_core::RetouchSource::Editing));
            canvas.engine.set_clone_source(layer_core::CloneSource { point: Some(Point { x: 150.0, y: 40.0 }), offset: Some([-150.0, 0.0]), aligned: true, flip: [false; 2] });
            canvas.set_brush(brush(preset, 34.0, 1.0, tones.strong))?;
            trace(canvas, &even(&(0..=40).map(|i| { let x = 256.0 + 50.0 * i as f32 / 40.0; (x, centre(x)) }).collect::<Vec<_>>(), 1.0))?;
            canvas.engine.set_retouch(None);
            ring(canvas, tones, (160.0, centre(160.0)), 18.0, true)?;
            ring(canvas, tones, (310.0, centre(310.0)), 18.0, false)?;
            arrow(canvas, tones, (184.0, 40.0), 286.0)
        }
        _ => {
            graded(canvas, tones)?;
            let spots = [(128.0, -4.0, 13.0), (212.0, 6.0, 12.0), (288.0, -8.0, 10.0), (346.0, 7.0, 13.0)];
            for (x, dy, size) in spots {
                let y = centre(x) + dy;
                paint(canvas, Preset::GPen, flaw, size, &[(x, y, 1.0), (x + 0.01, y, 1.0)])?;
            }
            canvas.engine.set_retouch(Some(layer_core::RetouchSource::Editing));
            canvas.set_brush(brush(preset, 34.0, 1.0, tones.strong))?;
            let (x, dy, _) = spots[0];
            let y = centre(x) + dy;
            trace(canvas, &[(x - 1.0, y, 1.0), (x, y, 1.0), (x + 1.0, y, 1.0)])?;
            canvas.engine.set_retouch(None);
            let (x, dy, _) = spots[1];
            ring(canvas, tones, (x, centre(x) + dy), 13.0, false)
        }
    }
}

fn stroke(canvas: &mut Canvas, offset: f32) -> Result<(), String> {
    for i in 0..=120 {
        let t = i as f32 / 120.0;
        let sample = Sample {
            x: 26.0 + t * 348.0,
            y: 40.0 + (t * std::f32::consts::TAU).sin() * 13.0 + offset,
            pressure: 0.16 + 0.84 * (t * std::f32::consts::PI).sin().max(0.0).powf(0.65),
        };
        let mut event = canvas.next_event(sample, phase(i, 121));
        if canvas.engine.brush().contact.is_some_and(|c| c.dry.is_some()) {
            event.tilt_radians = [std::f32::consts::FRAC_PI_4, 0.];
        }
        canvas.submit(&[event])?;
        if i % 8 == 0 || i == 120 {
            canvas.draw()?;
        }
    }
    canvas.wait_idle()
}
