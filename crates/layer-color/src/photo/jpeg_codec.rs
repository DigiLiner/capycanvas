//! Pure Rust JPEG codec. This backend buffers full images; enforce admission
//! limits before allocating pixels, and account for retained compressed input.
use super::*;
use libjpeg_turbo_rs::{ColorSpace, Decoder, Encoder, PixelFormat, Subsampling};
use layer_core::memory::{Budget, Reservation};

pub(super) const MEMORY_ERROR: &str = "JPEG exceeds the codec memory budget; use a smaller image";
const SCRATCH_BYTES: usize = 2 * 1024 * 1024;

pub(super) struct Input {
    pub bytes: Vec<u8>,
    pub memory: Reservation<'static>,
}
impl Input {
    pub fn new(bytes: Vec<u8>, budget: usize) -> Result<Self, String> {
        let memory = reserve_input(layer_core::memory::images(), bytes.capacity(), budget)?;
        Ok(Self { bytes, memory })
    }
    pub fn into_memory(self) -> Reservation<'static> {
        drop(self.bytes);
        self.memory
    }
}

fn input_memory(capacity: usize) -> Result<usize, String> {
    capacity.checked_mul(4).and_then(|v| v.checked_add(SCRATCH_BYTES))
        .ok_or_else(|| MEMORY_ERROR.into())
}

pub(super) fn reserve_input(memory: &Budget, capacity: usize, budget: usize) -> Result<Reservation<'_>, String> {
    let bytes = input_memory(capacity)?;
    if bytes > budget { return Err(MEMORY_ERROR.into()); }
    Ok(memory.reserve(bytes)?)
}

pub(super) fn reserve_decode(
    decoder: &Decoder<'_>, capacity: usize, overlap: usize, budget: usize, memory: &mut Reservation<'_>,
) -> Result<(), String> {
    decoder.output_buffer_size().map_err(|e| format!("{MEMORY_ERROR}: {e}"))?;
    let bytes = usize::try_from(decoder.estimated_decode_memory()).map_err(err)?
        .checked_add(input_memory(capacity)?).and_then(|v| v.checked_add(overlap))
        .ok_or(MEMORY_ERROR)?;
    if bytes > budget { return Err(MEMORY_ERROR.into()); }
    Ok(memory.resize(bytes)?)
}

pub(super) fn read_bounded(mut input: impl Read, budget: usize) -> Result<Input, String> {
    let mut encoded = Input::new(Vec::new(), budget)?;
    let input_limit = (budget - SCRATCH_BYTES) / 4;
    let mut buffer = [0u8; 65536];
    loop {
        let count = match input.read(&mut buffer) {
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
            result => result.map_err(err)?,
        };
        if count == 0 {
            return Ok(encoded);
        }
        let needed = encoded.bytes
            .len()
            .checked_add(count)
            .filter(|n| *n <= input_limit)
            .ok_or(MEMORY_ERROR)?;
        if needed > encoded.bytes.capacity() {
            let capacity = needed.max(encoded.bytes.capacity().saturating_mul(2)).min(input_limit);
            let working = input_memory(capacity)?;
            if working > budget { return Err(MEMORY_ERROR.into()); }
            encoded.memory.resize(working)?;
            encoded.bytes
                .try_reserve_exact(capacity - encoded.bytes.len())
                .map_err(|_| MEMORY_ERROR)?;
            encoded.memory.resize(input_memory(encoded.bytes.capacity())?)?;
        }
        encoded.bytes.extend_from_slice(&buffer[..count]);
    }
}

pub(super) fn decoder(
    bytes: &[u8],
    input_capacity: usize,
    limits: DecodeLimits,
) -> Result<Decoder<'_>, String> {
    // Marker parsing retains selected metadata and coefficient/table snapshots.
    let budget = limits
        .codec_bytes
        .checked_sub(
            input_memory(input_capacity)?,
        )
        .ok_or(MEMORY_ERROR)?;
    let mut decoder = Decoder::new_with_limits(
        bytes,
        libjpeg_turbo_rs::DecodeLimits {
            max_width: limits.dimension.min(32768) as usize,
            max_height: limits.dimension.min(32768) as usize,
            max_scans: 256,
            max_memory: Some(budget as u64),
            ..Default::default()
        },
    )
    .map_err(err)?;
    decoder.set_stop_on_warning(true);
    decoder.set_dct_method(libjpeg_turbo_rs::DctMethod::IsLow);
    Ok(decoder)
}

pub(super) fn channels(
    decoder: &Decoder<'_>,
    adobe: Option<u8>,
) -> Result<(SourceChannels, PixelFormat), String> {
    match decoder.jpeg_color_space() {
        ColorSpace::Grayscale => Ok((SourceChannels::Gray, PixelFormat::Grayscale)),
        ColorSpace::YCbCr | ColorSpace::Rgb => Ok((SourceChannels::Rgb, PixelFormat::Rgb)),
        ColorSpace::Cmyk | ColorSpace::Ycck if matches!(adobe, Some(0 | 2)) => {
            Ok((SourceChannels::Cmyk, PixelFormat::Cmyk))
        }
        ColorSpace::Cmyk | ColorSpace::Ycck => {
            Err("This CMYK JPEG needs an explicit sample-polarity interpretation".into())
        }
        _ => Err("Unsupported JPEG color encoding".into()),
    }
}

/// Pixels, coding planes, entropy output and metadata insertion copies coexist
/// within the caller's available-memory budget. Returns the packed pixel length.
pub(super) fn admit(
    extent: [u32; 2],
    channels: usize,
    metadata: usize,
    budget: usize,
) -> Result<usize, String> {
    super::validate_extent(extent, 32768)?;
    let len = (extent[0] as usize)
        .checked_mul(extent[1] as usize)
        .and_then(|n| n.checked_mul(channels))
        .ok_or(MEMORY_ERROR)?;
    let working = len
        .checked_mul(8)
        .and_then(|v| v.checked_add(SCRATCH_BYTES))
        .ok_or(MEMORY_ERROR)?
        .saturating_add(metadata.saturating_mul(4));
    if working > budget {
        return Err(MEMORY_ERROR.into());
    }
    Ok(len)
}

/// APP1 payloads: an Exif TIFF block and an XMP packet, without identifiers.
#[derive(Clone, Copy, Default)]
pub(super) struct Markers<'a> {
    pub exif: Option<&'a [u8]>,
    pub xmp: Option<&'a [u8]>,
}
pub(super) const METADATA_TOO_LARGE: &str =
    "This photo's metadata is too large for JPEG. Choose Copyright & Contact or None under Metadata.";

/// Baseline, full-chroma coding of packed 8-bit samples.
pub(super) fn encode(
    pixels: &[u8],
    extent: [u32; 2],
    format: PixelFormat,
    quality: u8,
    icc: &[u8],
    markers: Markers<'_>,
    resolution: Option<layer_core::ImageResolution>,
) -> Result<Vec<u8>, String> {
    let [width, height] = extent.map(|v| v as usize);
    if pixels.len() != width * height * format.bytes_per_pixel() {
        return Err("Incomplete JPEG output".into());
    }
    if icc.len() > crate::MAX_ICC_BYTES.min(255 * 65519) {
        return Err("JPEG ICC profile is too large".into());
    }
    let mut encoder = Encoder::new(pixels, width, height, format)
        .quality(quality)
        .subsampling(Subsampling::S444)
        .force_baseline(true)
        .icc_profile(icc);
    for (payload, identifier) in [(markers.exif, 6), (markers.xmp, super::jpeg_markers::XMP.len())] {
        if payload.is_some_and(|p| p.len() + identifier > 65533) {
            return Err(METADATA_TOO_LARGE.into());
        }
    }
    if let Some(exif) = markers.exif {
        encoder = encoder.exif_data(exif);
    }
    if let Some(xmp) = markers.xmp {
        encoder = encoder.xmp_data(xmp);
    }
    if let Some(resolution) = resolution {
        let (unit, [x, y]) = resolution.jfif_density()?;
        encoder = encoder.density(unit, x, y);
    }
    encoder.encode().map_err(err)
}
