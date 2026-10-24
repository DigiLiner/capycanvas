use std::io::{self, Cursor, Read, Seek, SeekFrom};
use std::sync::{Arc, atomic::{AtomicUsize, Ordering}};
use tiff::{decoder::{Decoder, Limits, DecodingResult}, TiffError};

fn file(width: u32, height: u32, samples: u16, compression: u16, data: &[u8]) -> Vec<u8> {
    let mut bytes = b"II*\0\x08\0\0\0".to_vec();
    let payload = 134u32;
    bytes.extend_from_slice(&10u16.to_le_bytes());
    for (tag, kind, count, value) in [
        (256u16, 4u16, 1u32, width), (257, 4, 1, height),
        (258, 3, u32::from(samples), if samples == 1 { 8 } else { payload }),
        (259, 3, 1, u32::from(compression)), (262, 3, 1, if samples == 1 { 1 } else { 2 }),
        (273, 4, 1, payload + if samples == 1 { 0 } else { u32::from(samples) * 2 }),
        (277, 3, 1, u32::from(samples)), (278, 4, 1, height),
        (279, 4, 1, data.len() as u32), (284, 3, 1, 1),
    ] {
        bytes.extend_from_slice(&tag.to_le_bytes());
        bytes.extend_from_slice(&kind.to_le_bytes());
        bytes.extend_from_slice(&count.to_le_bytes());
        bytes.extend_from_slice(&value.to_le_bytes());
    }
    bytes.extend_from_slice(&0u32.to_le_bytes());
    if samples > 1 { for _ in 0..samples { bytes.extend_from_slice(&8u16.to_le_bytes()); } }
    bytes.extend_from_slice(data);
    bytes
}

struct Observed {
    bytes: Cursor<Vec<u8>>,
    payload_reads: Arc<AtomicUsize>,
}
impl Read for Observed {
    fn read(&mut self, out: &mut [u8]) -> io::Result<usize> {
        let start = self.bytes.position();
        let count = self.bytes.read(out)?;
        if start < 140 && start + count as u64 > 134 {
            self.payload_reads.fetch_add(count, Ordering::Relaxed);
        }
        Ok(count)
    }
}
impl Seek for Observed {
    fn seek(&mut self, from: SeekFrom) -> io::Result<u64> { self.bytes.seek(from) }
}

#[test]
fn initial_directory_limits_precede_value_payload_reads() {
    let reads = Arc::new(AtomicUsize::new(0));
    let source = Observed { bytes: Cursor::new(file(2, 2, 3, 1, &[127; 12])), payload_reads: reads.clone() };
    let mut limits = Limits::default(); limits.ifd_value_size = 5;
    assert!(matches!(Decoder::new_with_limits(source, limits), Err(TiffError::LimitsExceeded)));
    assert_eq!(reads.load(Ordering::Relaxed), 0);
    let mut decoder = Decoder::new(Cursor::new(file(2, 2, 3, 1, &[127; 12]))).unwrap();
    assert!(matches!(decoder.read_image().unwrap(), DecodingResult::U8(data) if data == [127; 12]));
}

#[cfg(feature = "jpeg")]
fn jpeg() -> Vec<u8> {
    libjpeg_turbo_rs::Encoder::new(&[91; 64 * 64], 64, 64, libjpeg_turbo_rs::PixelFormat::Grayscale)
        .force_baseline(true).encode().unwrap()
}

#[test]
#[cfg(feature = "jpeg")]
fn jpeg_chunk_header_must_match_tiff_before_decoding_planes() {
    let mut decoder = Decoder::new(Cursor::new(file(2, 2, 1, 7, &jpeg()))).unwrap();
    assert!(decoder.read_image().is_err());
}

#[test]
#[cfg(feature = "jpeg")]
fn jpeg_chunk_working_memory_is_admitted_before_planes() {
    let jpeg = jpeg();
    let mut limits = Limits::default();
    limits.intermediate_buffer_size = 2 * 1024 * 1024 + jpeg.len() * 4 + 1024;
    let mut decoder = Decoder::new_with_limits(Cursor::new(file(64, 64, 1, 7, &jpeg)), limits).unwrap();
    assert!(matches!(decoder.read_image(), Err(TiffError::LimitsExceeded)));
    let mut decoder = Decoder::new(Cursor::new(file(64, 64, 1, 7, &jpeg))).unwrap();
    assert!(matches!(decoder.read_image().unwrap(), DecodingResult::U8(data) if data == [91; 64 * 64]));
}

#[test]
#[cfg(feature = "jpeg")]
fn jpeg_chunk_requires_a_complete_end_marker() {
    let mut jpeg = jpeg();
    jpeg.truncate(jpeg.len() - 2);
    let mut decoder = Decoder::new(Cursor::new(file(64, 64, 1, 7, &jpeg))).unwrap();
    assert!(decoder.read_image().is_err());
}

#[test]
#[cfg(feature = "jpeg")]
fn malformed_jpeg_tables_fail_during_initial_directory_admission() {
    let mut bytes = file(64, 64, 1, 7, &jpeg());
    let entry = 10 + 9 * 12;
    bytes[entry..entry + 2].copy_from_slice(&347u16.to_le_bytes());
    bytes[entry + 2..entry + 4].copy_from_slice(&1u16.to_le_bytes());
    bytes[entry + 4..entry + 8].copy_from_slice(&4u32.to_le_bytes());
    bytes[entry + 8..entry + 12].copy_from_slice(&[0xff, 0xd8, 0, 0]);
    assert!(Decoder::new(Cursor::new(bytes)).is_err());
}

#[test]
fn bigtiff_directory_work_is_bounded_before_reading_entries() {
    let mut bytes = b"II+\0\x08\0\0\0".to_vec();
    bytes.extend_from_slice(&16u64.to_le_bytes());
    bytes.extend_from_slice(&u64::MAX.to_le_bytes());
    assert!(matches!(Decoder::new(Cursor::new(bytes)), Err(TiffError::LimitsExceeded)));
}
