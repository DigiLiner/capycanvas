use crate::ImageResolution;
use crate::color::SampleDepth;
use crate::color::source::{SourceBuilder, SourceChannels, SourceImage, SourceInterpretation};
use std::collections::VecDeque;

pub const MAX_PAPER_SIDE: u32 = crate::MAX_PAPER_IMAGE_EXTENT;
pub const CMYK: &str = "CMYK images cannot be paper";
const SEAM_BAND: f64 = 0.1;
const SEAMLESS_RATIO: f64 = 1.5;
const MAX_JITTER: f64 = 1.5;

fn gray_interpretation() -> SourceInterpretation {
    SourceInterpretation { channels: SourceChannels::Gray, depth: SampleDepth::U8, profile: Default::default(), profile_assumed: false }
}

fn lightness(source: &SourceImage) -> Result<impl FnMut(u32) -> Result<Vec<f32>, String> + '_, String> {
    let interpretation = &source.interpretation;
    let channels = interpretation.channels.count();
    if interpretation.channels == SourceChannels::Cmyk {
        return Err(CMYK.into());
    }
    let depth = interpretation.depth;
    let mut rows = source.rows();
    let mut bytes = vec![0; source.row_bytes()];
    Ok(move |y: u32| {
        rows.read(y, &mut bytes)?;
        let sample = |i: usize| -> f32 {
            match depth {
                SampleDepth::U8 => f32::from(bytes[i]) / 255.,
                SampleDepth::U16 => f32::from(u16::from_le_bytes([bytes[2 * i], bytes[2 * i + 1]])) / 65535.,
                SampleDepth::F16 => crate::color::srgb_encode(half::f16::from_bits(u16::from_le_bytes([bytes[2 * i], bytes[2 * i + 1]])).to_f32().clamp(0., 1.)),
                SampleDepth::F32 => crate::color::srgb_encode(f32::from_le_bytes(bytes[4 * i..4 * i + 4].try_into().unwrap()).clamp(0., 1.)),
            }
        };
        Ok((0..source.extent[0] as usize).map(|x| {
            let at = x * channels;
            let (value, alpha) = match channels {
                1 => (sample(at), 1.),
                2 => (sample(at), sample(at + 1)),
                3 => (0.2126 * sample(at) + 0.7152 * sample(at + 1) + 0.0722 * sample(at + 2), 1.),
                _ => (0.2126 * sample(at) + 0.7152 * sample(at + 1) + 0.0722 * sample(at + 2), sample(at + 3)),
            };
            alpha * value + 1. - alpha
        }).collect())
    })
}

fn gaussian(x: u32, y: u32, salt: u32) -> [f64; 2] {
    let mut v = [x, y, salt, 0x9e37_79b9].map(|v| v.wrapping_mul(1_664_525).wrapping_add(1_013_904_223));
    for _ in 0..2 {
        v[0] = v[0].wrapping_add(v[1].wrapping_mul(v[3])); v[1] = v[1].wrapping_add(v[2].wrapping_mul(v[0]));
        v[2] = v[2].wrapping_add(v[0].wrapping_mul(v[1])); v[3] = v[3].wrapping_add(v[1].wrapping_mul(v[2]));
        v = v.map(|v| v ^ (v >> 16));
    }
    let [u, w] = [v[0], v[1]].map(|v| f64::from(v >> 8) / 16_777_216.);
    let radius = (-2. * u.max(1e-7).ln()).sqrt();
    let angle = std::f64::consts::TAU * w;
    [radius * angle.cos(), radius * angle.sin()].map(|v| v.clamp(-MAX_JITTER, MAX_JITTER))
}

fn resample(source: &SourceImage, extent: [u32; 2], cancelled: &dyn Fn() -> bool) -> Result<Vec<f32>, String> {
    let shrink = [0, 1].map(|axis| f64::from(source.extent[axis]) / f64::from(extent[axis]));
    let sigma = 0.5 * (shrink[0].max(shrink[1]) - 1.).clamp(0., 1.);
    let mut read = lightness(source)?;
    let mut window: VecDeque<(u32, Vec<f32>)> = VecDeque::new();
    let mut output = Vec::with_capacity(extent[0] as usize * extent[1] as usize);
    for j in 0..extent[1] {
        if cancelled() { return Err("Paper import cancelled".into()); }
        let reach = |offset: f64| (((f64::from(j) + 0.5 + offset) * shrink[1]).floor().max(0.) as u32).min(source.extent[1] - 1);
        let (first, last) = (reach(-MAX_JITTER * sigma), reach(MAX_JITTER * sigma));
        while window.front().is_some_and(|(y, _)| *y < first) { window.pop_front(); }
        let mut next = window.back().map_or(first, |(y, _)| y + 1);
        while next <= last { window.push_back((next, read(next)?)); next += 1; }
        for i in 0..extent[0] {
            let [dx, dy] = gaussian(i, j, 0x6a09_e667).map(|v| v * sigma);
            let x = (((f64::from(i) + 0.5 + dx) * shrink[0]).floor().max(0.) as u32).min(source.extent[0] - 1);
            let y = (((f64::from(j) + 0.5 + dy) * shrink[1]).floor().max(0.) as u32).clamp(first, last);
            output.push(window[(y - first) as usize].1[x as usize]);
        }
    }
    Ok(output)
}

fn seam_ratio(heights: &[f32], [width, height]: [usize; 2], vertical: bool) -> f64 {
    let at = |x: usize, y: usize| f64::from(if vertical { heights[x * width + y] } else { heights[y * width + x] });
    let (along, across) = if vertical { (width, height) } else { (height, width) };
    let (mut seam, mut interior) = (0., 0.);
    for y in 0..along {
        seam += (at(across - 1, y) - at(0, y)).abs();
        interior += (at(across / 2, y) - at(across / 2 - 1, y)).abs();
    }
    seam / interior.max(1e-9)
}

fn make_seamless(heights: Vec<f32>, [width, height]: [usize; 2]) -> (Vec<f32>, [usize; 2]) {
    let mean = heights.iter().map(|v| f64::from(*v)).sum::<f64>() / heights.len() as f64;
    let blend = |a: f32, b: f32, w: f64| {
        let mixed = (f64::from(a) - mean) * w + (f64::from(b) - mean) * (1. - w);
        (mean + mixed / (w * w + (1. - w) * (1. - w)).sqrt()) as f32
    };
    let (mut heights, mut size) = (heights, [width, height]);
    for vertical in [false, true] {
        let across = size[usize::from(vertical)];
        let band = ((across as f64 * SEAM_BAND) as usize).max(1);
        if across < 16 || seam_ratio(&heights, size, vertical) < SEAMLESS_RATIO { continue; }
        let kept = across - band;
        let at = |x: usize, y: usize| if vertical { heights[x * size[0] + y] } else { heights[y * size[0] + x] };
        let along = size[usize::from(!vertical)];
        let mut out = vec![0.; kept * along];
        for y in 0..along {
            for x in 0..kept {
                let value = if x < band {
                    blend(at(x, y), at(kept + x, y), (x as f64 + 0.5) / band as f64)
                } else { at(x, y) };
                if vertical { out[x * along + y] = value; } else { out[y * kept + x] = value; }
            }
        }
        heights = out;
        size[usize::from(vertical)] = kept;
    }
    (heights, size)
}

pub fn paper_image(source: &SourceImage, cancelled: &dyn Fn() -> bool) -> Result<SourceImage, String> {
    crate::memory::with_input(|| {
    let shrink = (f64::from(source.extent[0].max(source.extent[1])) / f64::from(MAX_PAPER_SIDE)).max(1.);
    let extent = source.extent.map(|v| ((f64::from(v) / shrink).round() as u32).max(1));
    let window=((f64::from(source.extent[1])/f64::from(extent[1])*MAX_JITTER).ceil() as usize+2).min(source.extent[1] as usize);
    let _scratch=crate::memory::images().reserve(extent[0] as usize*extent[1] as usize*8+window*source.extent[0] as usize*4+source.row_bytes())?;
    let heights = resample(source, extent, cancelled)?;
    let (heights, size) = make_seamless(heights, extent.map(|v| v as usize));
    let ppi = super::image_ppi(source.resolution).map(|v| v / shrink);
    let mut builder = SourceBuilder::new([size[0] as u32, size[1] as u32], gray_interpretation(), 1 << 26)?;
    for row in heights.chunks_exact(size[0]) {
        builder.push_row(&row.iter().map(|v| (v.clamp(0., 1.) * 255.).round() as u8).collect::<Vec<_>>())?;
    }
    let mut image = builder.finish()?;
    image.resolution = Some(ImageResolution { unit: crate::ResolutionUnit::Inch, density: ppi.map(|v| [(v * 100.).round() as u32, 100]) });
    Ok(image)
    })
}

pub fn image_preview(image: &SourceImage) -> Result<Vec<u8>, String> {
    let size = 2 * super::PREVIEW_SIZE;
    let ppi = super::image_ppi(image.resolution);
    let per_texel = ppi.map(|v| v / 25.4 * super::PREVIEW_MM / size as f64);
    let mut rows = image.rows();
    let mut bytes = vec![0; image.row_bytes()];
    let mut cached = u32::MAX;
    let mut heights = vec![0.; size * size];
    for y in 0..size {
        let source_y = ((y as f64 + 0.5) * per_texel[1]) as u64 % u64::from(image.extent[1]);
        if source_y as u32 != cached { rows.read(source_y as u32, &mut bytes)?; cached = source_y as u32; }
        for x in 0..size {
            let source_x = ((x as f64 + 0.5) * per_texel[0]) as u64 % u64::from(image.extent[0]);
            heights[y * size + x] = f32::from(bytes[source_x as usize]) / 255. * super::IMAGE_RELIEF_UM;
        }
    }
    Ok(super::preview(&heights))
}

pub fn encode_png(image: &SourceImage) -> Result<Vec<u8>, String> {
    if image.interpretation != gray_interpretation() { return Err("Papers are gray images".into()); }
    let mut bytes = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut bytes, image.extent[0], image.extent[1]);
        encoder.set_color(png::ColorType::Grayscale);
        encoder.set_depth(png::BitDepth::Eight);
        let ppi = super::image_ppi(image.resolution);
        encoder.set_pixel_dims(Some(png::PixelDimensions {
            xppu: (ppi[0] / 0.0254).round() as u32, yppu: (ppi[1] / 0.0254).round() as u32, unit: png::Unit::Meter,
        }));
        let mut writer = encoder.write_header().map_err(|e| e.to_string())?;
        let mut stream = writer.stream_writer().map_err(|e| e.to_string())?;
        let mut rows = image.rows();
        let mut row = vec![0; image.row_bytes()];
        for y in 0..image.extent[1] {
            rows.read(y, &mut row)?;
            std::io::Write::write_all(&mut stream, &row).map_err(|e| e.to_string())?;
        }
        stream.finish().map_err(|e| e.to_string())?;
    }
    Ok(bytes)
}

pub fn decode_png(bytes: &[u8]) -> Result<SourceImage, String> {
    crate::memory::with_input(|| {
    let _work=crate::input_work::DECODERS.enter(&std::sync::atomic::AtomicBool::new(false))?;
    crate::memory::images().collect()?;
    let _scratch=crate::memory::images().reserve((MAX_PAPER_SIDE as usize).pow(2)+64*1024)?;
    let mut decoder = png::Decoder::new(std::io::Cursor::new(bytes));
    decoder.set_limits(png::Limits {bytes:(MAX_PAPER_SIDE as usize).pow(2)+64*1024});
    decoder.set_ignore_text_chunk(true);
    decoder.set_ignore_iccp_chunk(true);
    let mut reader = decoder.read_info().map_err(|e| e.to_string())?;
    let info = reader.info();
    if info.color_type != png::ColorType::Grayscale || info.bit_depth != png::BitDepth::Eight
        || info.width > MAX_PAPER_SIDE || info.height > MAX_PAPER_SIDE {
        return Err("Not a stored paper".into());
    }
    let extent = [info.width, info.height];
    let resolution = info.pixel_dims.filter(|d| d.unit == png::Unit::Meter && d.xppu > 0 && d.yppu > 0)
        .map(|d| ImageResolution { unit: crate::ResolutionUnit::Inch,
            density: [d.xppu, d.yppu].map(|v| [(f64::from(v) * 2.54).round() as u32, 100]) });
    let mut builder = SourceBuilder::new(extent, gray_interpretation(), 1 << 26)?;
    while let Some(row) = reader.next_row().map_err(|e| e.to_string())? {
        builder.push_row(row.data())?;
    }
    let mut image = builder.finish()?;
    image.resolution = resolution;
    Ok(image)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn source(extent: [u32; 2], resolution: Option<u32>, value: impl Fn(u32, u32) -> [u8; 3]) -> SourceImage {
        let interpretation = SourceInterpretation { channels: SourceChannels::Rgb, depth: SampleDepth::U8, profile: Default::default(), profile_assumed: false };
        let mut builder = SourceBuilder::new(extent, interpretation, 1 << 30).unwrap();
        for y in 0..extent[1] {
            builder.push_row(&(0..extent[0]).flat_map(|x| value(x, y)).collect::<Vec<_>>()).unwrap();
        }
        let mut image = builder.finish().unwrap();
        image.resolution = resolution.map(ImageResolution::ppi);
        image
    }
    fn gray(image: &SourceImage) -> Vec<u8> {
        let mut rows = image.rows();
        let mut row = vec![0; image.row_bytes()];
        (0..image.extent[1]).flat_map(|y| { rows.read(y, &mut row).unwrap(); row.clone() }).collect()
    }

    #[test]
    fn seamless_images_keep_their_samples_and_resolution() {
        let tile = source([64, 48], Some(600), |x, y| { let v = ((x % 8) * 20 + (y % 6) * 10) as u8; [v, v, v] });
        let paper = paper_image(&tile, &|| false).unwrap();
        assert_eq!(paper.extent, [64, 48]);
        assert_eq!(super::super::image_ppi(paper.resolution), [600.; 2]);
        assert_eq!(gray(&paper), gray(&paper_image(&tile, &|| false).unwrap()));
        assert_eq!(gray(&paper)[..8], [0, 20, 40, 60, 80, 100, 120, 140]);
    }

    #[test]
    fn mismatched_edges_are_blended_without_losing_contrast() {
        let ramp = source([200, 100], None, |x, y| { let v = (x * 255 / 199 + (x * 7 + y * 13) % 5).min(255) as u8; [v, v, v] });
        let paper = paper_image(&ramp, &|| false).unwrap();
        assert_eq!(paper.extent, [180, 100]);
        assert_eq!(super::super::image_ppi(paper.resolution), [300.; 2], "screen resolutions read as print");
        let samples: Vec<f32> = gray(&paper).iter().map(|v| f32::from(*v)).collect();
        assert!(seam_ratio(&samples, [180, 100], false) < SEAMLESS_RATIO, "the wrap is no longer a seam");
        let spread = |values: &[f32]| { let mean = values.iter().sum::<f32>() / values.len() as f32; (values.iter().map(|v| (v - mean).powi(2)).sum::<f32>() / values.len() as f32).sqrt() };
        let band: Vec<f32> = samples.chunks(180).flat_map(|row| row[..20].to_vec()).collect();
        assert!(spread(&band) > 0.5 * spread(&samples), "the blend keeps the band's contrast");
    }

    #[test]
    fn large_images_shrink_and_keep_their_physical_size() {
        let scan = source([5000, 100], Some(1200), |x, _| { let v = if x % 2 == 0 { 40 } else { 220 }; [v, v, v] });
        let paper = paper_image(&scan, &|| false).unwrap();
        assert_eq!(paper.extent[0], 4096);
        assert!((super::super::image_ppi(paper.resolution)[0] - 1200. * 4096. / 5000.).abs() < 0.01);
        let samples = gray(&paper);
        assert!(samples.iter().all(|v| *v == 40 || *v == 220), "samples are picked, not averaged");
        let dark = samples.iter().filter(|v| **v == 40).count() as f64 / samples.len() as f64;
        assert!((dark - 0.5).abs() < 0.05, "the histogram is kept: {dark}");
    }

    #[test]
    fn stored_papers_round_trip_through_png() {
        let paper = paper_image(&source([30, 20], Some(450), |x, y| [(x * 8) as u8, (y * 12) as u8, 90]), &|| false).unwrap();
        let reopened = decode_png(&encode_png(&paper).unwrap()).unwrap();
        assert_eq!(gray(&reopened), gray(&paper));
        assert!((super::super::image_ppi(reopened.resolution)[0] - 450.).abs() < 0.02);
        assert_eq!(image_preview(&reopened).unwrap().len(), super::super::PREVIEW_SIZE * super::super::PREVIEW_SIZE);
    }
}
