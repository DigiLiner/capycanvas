use super::*;
use crate::include::common::bitdepth::{AsPrimitive as _, BitDepth};
use crate::include::dav1d::headers::{DRav1d, Rav1dFrameHeader, Rav1dPixelLayout};
use crate::include::dav1d::picture::Rav1dPicAllocator;
use crate::src::c_arc::RawArc;
use crate::src::strided::Strided as _;

const STILL: [&[u8]; 3] = [
    include_bytes!("testdata/still-8bit.obu"),
    include_bytes!("testdata/still-10bit.obu"),
    include_bytes!("testdata/still-12bit.obu"),
];

struct Decoder(Option<Dav1dContext>);

impl Decoder {
    fn new(threads: i32) -> Self {
        Self::with_allocator(threads, Rav1dPicAllocator::default())
    }

    fn with_allocator(threads: i32, allocator: Rav1dPicAllocator) -> Self {
        let mut settings = mem::MaybeUninit::<Dav1dSettings>::uninit();
        // SAFETY: settings has exclusive writable storage for the complete output.
        unsafe { dav1d_default_settings(NonNull::new(settings.as_mut_ptr()).unwrap()); }
        // SAFETY: default_settings initializes every field.
        let mut settings = unsafe { settings.assume_init() };
        settings.n_threads = threads;
        settings.max_frame_delay = threads;
        settings.frame_size_limit = 4096;
        settings.allocator = allocator.into();
        let mut context = None;
        // SAFETY: Initialized settings and exclusively owned output.
        assert_eq!(unsafe { dav1d_open(Some(NonNull::from(&mut context)), Some(NonNull::from(&mut settings))) }.0, 0);
        Self(context)
    }

    fn picture(&self) -> Result<Picture, i32> {
        let mut picture = Dav1dPicture::default();
        // SAFETY: The context is live and externally serialized; picture is exclusive output.
        let result = unsafe { dav1d_get_picture(self.0, Some(NonNull::from(&mut picture))) }.0;
        if result == 0 { Ok(Picture(picture)) } else { Err(result) }
    }

    fn push(&mut self, bytes: &[u8], pictures: &mut Vec<Picture>) -> Result<(), i32> {
        let mut packet = Dav1dData::default();
        // SAFETY: packet is exclusive output; the API owns the returned initialized allocation.
        let raw = unsafe { dav1d_data_create(Some(NonNull::from(&mut packet)), bytes.len()) };
        assert!(!raw.is_null());
        // SAFETY: The allocation is writable for bytes.len(); the source is distinct immutable data.
        unsafe { raw.copy_from_nonoverlapping(bytes.as_ptr(), bytes.len()); }
        let result = (|| {
            for _ in 0..32 {
                // SAFETY: The context and packet are live and exclusively accessed by this owner.
                let result = unsafe { dav1d_send_data(self.0, Some(NonNull::from(&mut packet))) }.0;
                if result == 0 { return Ok(()); }
                if result != -(EAGAIN as i32) { return Err(result); }
                pictures.push(self.picture()?);
            }
            panic!("bounded packet admission did not finish")
        })();
        // SAFETY: The local packet retains its original owner or has been cleared by send_data.
        unsafe { dav1d_data_unref(Some(NonNull::from(&mut packet))); }
        result
    }

    fn drain(&mut self, pictures: &mut Vec<Picture>) -> Result<(), i32> {
        let mut empty = 0;
        for _ in 0..32 {
            match self.picture() {
                Ok(picture) => { empty = 0; pictures.push(picture); }
                Err(error) if error == -(EAGAIN as i32) => {
                    empty += 1;
                    if empty == 2 { return Ok(()); }
                }
                Err(error) => return Err(error),
            }
        }
        panic!("bounded output drain did not finish")
    }

    fn flush(&mut self) {
        // SAFETY: The live context is exclusively accessed by this owner.
        unsafe { dav1d_flush(self.0.unwrap()); }
    }
}

impl Drop for Decoder {
    fn drop(&mut self) {
        // SAFETY: This is the sole close of the live context; close clears its handle.
        unsafe { dav1d_close(Some(NonNull::from(&mut self.0))); }
    }
}

struct Picture(Dav1dPicture);
impl Drop for Picture {
    fn drop(&mut self) {
        // SAFETY: Each decoded picture owner is released exactly once.
        unsafe { dav1d_picture_unref(Some(NonNull::from(&mut self.0))); }
    }
}

#[derive(Debug, PartialEq, Eq)]
struct Frame { size: [i32; 2], depth: i32, layout: i32, samples: Vec<u8> }

impl Picture {
    fn into_rust(mut self) -> Rav1dPicture {
        mem::take(&mut self.0).into()
    }

    fn frame(&self) -> Frame {
        let p = &self.0;
        let mut samples = Vec::new();
        let bytes = if p.p.bpc > 8 { 2 } else { 1 };
        for plane in 0..if p.p.layout == 0 { 1 } else { 3 } {
            let sx = usize::from(plane > 0 && p.p.layout != 3);
            let sy = usize::from(plane > 0 && p.p.layout == 1);
            let width = (p.p.w as usize + sx) >> sx;
            let height = (p.p.h as usize + sy) >> sy;
            let stride = p.stride[usize::from(plane > 0)];
            assert!(stride.unsigned_abs() >= width * bytes);
            for row in 0..height {
                // SAFETY: Decoded metadata bounds each visible row; the picture retains its allocation.
                let data = unsafe { p.data[plane].unwrap().as_ptr().cast::<u8>().offset(row as isize * stride) };
                // SAFETY: The row is initialized, in bounds, and only read while this owner is live.
                samples.extend_from_slice(unsafe { slice::from_raw_parts(data, width * bytes) });
            }
        }
        Frame { size: [p.p.w, p.p.h], depth: p.p.bpc, layout: p.p.layout as i32, samples }
    }
}

fn moving_frames() -> Vec<&'static [u8]> {
    let bytes = include_bytes!("testdata/moving-8bit.ivf");
    assert_eq!(&bytes[..4], b"DKIF");
    let mut at = 32;
    let mut frames = Vec::new();
    while at < bytes.len() {
        let len = u32::from_le_bytes(bytes[at..at + 4].try_into().unwrap()) as usize;
        at += 12;
        frames.push(&bytes[at..at + len]);
        at += len;
    }
    frames
}

fn decode(packets: &[&[u8]], threads: i32) -> Vec<Frame> {
    let mut decoder = Decoder::new(threads);
    let mut pictures = Vec::new();
    for bytes in packets { decoder.push(bytes, &mut pictures).unwrap(); }
    decoder.drain(&mut pictures).unwrap();
    drop(decoder);
    pictures.iter().map(Picture::frame).collect()
}

#[test]
fn invalid_tiles_are_rejected_and_flush_allows_a_new_image() {
    for threads in [1, 4] {
        let mut decoder = Decoder::new(threads);
        let mut pictures = Vec::new();
        let result = decoder.push(include_bytes!("testdata/invalid-tile.obu"), &mut pictures)
            .and_then(|_| decoder.drain(&mut pictures));
        assert!(result.is_err());
        pictures.clear();
        decoder.flush();
        decoder.push(STILL[0], &mut pictures).unwrap();
        decoder.drain(&mut pictures).unwrap();
        assert_eq!(pictures.iter().map(Picture::frame).collect::<Vec<_>>(), decode(&[STILL[0]], 1));
    }
}

#[test]
fn independent_decoders_keep_exact_pixels_and_owned_output_after_close() {
    let packets = moving_frames();
    let expected = decode(&packets, 1);
    assert_eq!(expected.len(), 6);
    std::thread::scope(|scope| {
        for _ in 0..4 {
            let expected = &expected;
            let packets = &packets;
            scope.spawn(move || {
                assert_eq!(decode(packets, 4), *expected);
                for bytes in STILL { assert_eq!(decode(&[bytes], 4), decode(&[bytes], 1)); }
            });
        }
    });
}

#[test]
fn close_releases_worker_context_owners_before_returning() {
    for (threads, drain) in [1, 4].into_iter().flat_map(|threads| [None, Some(false), Some(true)].map(|drain| (threads, drain))) {
        let mut decoder = Decoder::new(threads);
        // SAFETY: Transfer the single public raw owner into an Arc, then publish it once again.
        let owner = unsafe { decoder.0.take().unwrap().into_arc() };
        decoder.0 = Some(RawArc::from_arc(Arc::clone(&owner)));
        let mut pictures = Vec::new();
        if let Some(drain) = drain {
            decoder.push(STILL[0], &mut pictures).unwrap();
            if drain { decoder.drain(&mut pictures).unwrap(); }
        }
        drop(decoder);
        for tc in owner.tc.iter() {
            if let Rav1dContextTaskType::Worker(slot) = &tc.task {
                assert!(slot.lock().is_none());
            }
        }
        assert_eq!(Arc::strong_count(&owner), 1);
        assert_eq!(pictures.len(), usize::from(drain == Some(true)));
        for picture in &pictures { assert_eq!(picture.frame().depth, 8); }
    }
}

#[derive(Default)]
struct AllocatorState {
    inner: Rav1dPicAllocator,
    allocations: std::sync::atomic::AtomicUsize,
    releases: std::sync::atomic::AtomicUsize,
    after_close: std::sync::atomic::AtomicUsize,
    closed: std::sync::atomic::AtomicBool,
}

unsafe extern "C" fn tracked_alloc(picture: *mut Dav1dPicture,
    cookie: Option<SendSyncNonNull<c_void>>) -> Dav1dResult
{
    let cookie = cookie.unwrap().cast::<AllocatorState>();
    // SAFETY: The test retains the synchronized allocator state through final release.
    let state = unsafe { cookie.as_ref() };
    // SAFETY: The decoder provides exclusive picture output; the default allocator owns each allocation.
    let result = unsafe { (state.inner.alloc_picture_callback)(picture, state.inner.cookie) };
    if result.0 == 0 { state.allocations.fetch_add(1, Ordering::SeqCst); }
    result
}

unsafe extern "C" fn tracked_release(picture: *mut Dav1dPicture,
    cookie: Option<SendSyncNonNull<c_void>>)
{
    let cookie = cookie.unwrap().cast::<AllocatorState>();
    // SAFETY: The test retains the synchronized allocator state through final release.
    let state = unsafe { cookie.as_ref() };
    // SAFETY: Each default allocation is passed back exactly once for final release.
    unsafe { (state.inner.release_picture_callback)(picture, state.inner.cookie); }
    state.releases.fetch_add(1, Ordering::SeqCst);
    if state.closed.load(Ordering::SeqCst) { state.after_close.fetch_add(1, Ordering::SeqCst); }
}

#[test]
fn retained_picture_releases_allocator_on_another_thread_after_close() {
    for threads in [1, 4] {
        let state = AllocatorState::default();
        let allocator = Rav1dPicAllocator {
            cookie: Some(SendSyncNonNull::from_ref(&state).cast()),
            alloc_picture_callback: tracked_alloc,
            release_picture_callback: tracked_release,
        };
        let mut decoder = Decoder::with_allocator(threads, allocator);
        let mut pictures = Vec::new();
        decoder.push(STILL[0], &mut pictures).unwrap();
        decoder.drain(&mut pictures).unwrap();
        assert_eq!(pictures.len(), 1);
        let retained = pictures.pop().unwrap().into_rust();
        drop(decoder);
        state.closed.store(true, Ordering::SeqCst);
        assert!(state.allocations.load(Ordering::SeqCst) > state.releases.load(Ordering::SeqCst));
        std::thread::spawn(move || {
            let picture = Picture(retained.into());
            assert_eq!(picture.frame(), decode(&[STILL[0]], 1)[0]);
        }).join().unwrap();
        assert_eq!(state.after_close.load(Ordering::SeqCst), 1);
        assert_eq!(state.allocations.load(Ordering::SeqCst), state.releases.load(Ordering::SeqCst));
    }
}

#[test]
fn outer_thread_traits_follow_owned_components() {
    fn shared<T: Send + Sync>() {}
    shared::<Rav1dContext>();
    shared::<crate::src::internal::TaskThreadDataDelayedFg>();
    shared::<crate::include::dav1d::picture::Rav1dPicture>();
}

fn grain_input<BD: BitDepth>(width: i32, depth: u8, layout: Rav1dPixelLayout,
    from_luma: bool, overlap: bool) -> Rav1dPicture
{
    let mut sequence = rav1d_parse_sequence_header(STILL[0]).unwrap().rav1d;
    sequence.layout = layout;
    sequence.hbd = (depth - 8) / 2;
    let mut header = Rav1dFrameHeader::default();
    let grain = &mut header.film_grain.data;
    grain.seed = 12345;
    grain.num_y_points = 2;
    grain.y_points[..2].copy_from_slice(&[[0, 8], [255, 64]]);
    grain.chroma_scaling_from_luma = from_luma;
    grain.num_uv_points = if from_luma { [0; 2] } else { [2; 2] };
    for points in &mut grain.uv_points { points[..2].copy_from_slice(&[[0, 12], [255, 72]]); }
    grain.scaling_shift = 8;
    grain.ar_coeff_shift = 6;
    grain.uv_mult = [16, 32];
    grain.uv_luma_mult = [48, 32];
    grain.uv_offset = [7, -9];
    grain.overlap_flag = overlap;
    let picture = Rav1dPicAllocator::default().alloc_picture_data(width, 35,
        Arc::new(DRav1d::from_rav1d(sequence)), Some(Arc::new(DRav1d::from_rav1d(header)))).unwrap();
    for (plane, data) in picture.data.as_ref().unwrap().data.iter().enumerate() {
        for (i, pixel) in data.slice_mut::<BD, _>(..).iter_mut().enumerate() {
            *pixel = ((((i * 29 + plane * 53 + 17) & 255) << (depth - 8)) as i32).as_();
        }
    }
    picture
}

fn allocation<BD: BitDepth>(picture: &Rav1dPicture) -> [Vec<BD::Pixel>; 3] {
    picture.data.as_ref().unwrap().data.each_ref().map(|data| data.slice::<BD, _>(..).to_vec())
}

fn grain_edge_case<BD: BitDepth>(width: i32, depth: u8, layout: Rav1dPixelLayout,
    from_luma: bool, overlap: bool)
{
    let input = grain_input::<BD>(width, depth, layout, from_luma, overlap);
    let before = allocation::<BD>(&input);
    let pad = i32::from(layout != Rav1dPixelLayout::I444 && width & 1 != 0);
    let reference = grain_input::<BD>(width + pad, depth, layout, from_luma, overlap);
    if pad != 0 {
        let luma = &reference.data.as_ref().unwrap().data[0];
        let stride = luma.pixel_stride::<BD>() as usize;
        let mut pixels = luma.slice_mut::<BD, _>(..);
        for row in pixels.chunks_mut(stride) {
            row[width as usize] = row[width as usize - 1];
        }
    }
    let context = rav1d_open(&Rav1dSettings { n_threads: 1, max_frame_delay: 1, ..Default::default() }).unwrap();
    let mut expected = Rav1dPicture::default();
    rav1d_apply_grain(&context, &mut expected, &reference).unwrap();
    let mut serial = Rav1dPicture::default();
    rav1d_apply_grain(&context, &mut serial, &input).unwrap();
    assert!(allocation::<BD>(&input) == before, "serial grain changed source allocation");
    rav1d_close(context);
    expected.p.w = width;
    let expected = Picture(expected.into()).frame();
    assert_eq!(Picture(serial.into()).frame(), expected);
    assert_ne!(expected.samples, Picture(input.clone().into()).frame().samples);
    let barrier = std::sync::Barrier::new(2);
    std::thread::scope(|scope| {
        for threads in [1, 4] {
            let (input, barrier, expected) = (&input, &barrier, &expected);
            scope.spawn(move || {
                let context = rav1d_open(&Rav1dSettings { n_threads: threads, max_frame_delay: 1, ..Default::default() }).unwrap();
                let mut output = Rav1dPicture::default();
                barrier.wait();
                rav1d_apply_grain(&context, &mut output, input).unwrap();
                rav1d_close(context);
                assert_eq!(Picture(output.into()).frame(), *expected,
                    "width={width}, depth={depth}, layout={}, from_luma={from_luma}, overlap={overlap}", layout as u8);
            });
        }
    });
    assert!(allocation::<BD>(&input) == before, "source allocation changed");
}

#[test]
fn shared_grain_input_keeps_visible_pixels_and_padding_unchanged() {
    for width in [1, 31, 33, 32] {
        for layout in [Rav1dPixelLayout::I420, Rav1dPixelLayout::I422, Rav1dPixelLayout::I444] {
            for from_luma in [false, true] {
                for overlap in [false, true] {
                    grain_edge_case::<BitDepth8>(width, 8, layout, from_luma, overlap);
                    for depth in [10, 12] {
                        grain_edge_case::<BitDepth16>(width, depth, layout, from_luma, overlap);
                    }
                }
            }
        }
    }
}

#[test]
fn grained_c_picture_keeps_borrowed_owners_in_both_release_orders() {
    for release_input_first in [false, true] {
        let template = grain_input::<BitDepth8>(33, 8, Rav1dPixelLayout::I420, true, true);
        let original_context = rav1d_open(&Rav1dSettings { n_threads: 1, ..Default::default() }).unwrap();
        let input = original_context.allocator.alloc_picture_data(33, 35,
            template.seq_hdr.clone().unwrap(), template.frame_hdr.clone()).unwrap();
        for (destination, source) in input.data.as_ref().unwrap().data.iter()
            .zip(&template.data.as_ref().unwrap().data)
        {
            // SAFETY: The fresh destination is exclusive and disjoint from the retained template.
            unsafe { destination.copy_from(source); }
        }
        rav1d_close(original_context);
        let data = Arc::downgrade(input.data.as_ref().unwrap());
        let mut source = Picture(input.into());
        let before = source.frame();
        let decoder = Decoder::new(4);
        let mut output = Picture(Dav1dPicture::default());
        // SAFETY: The live context is serialized, source retains its owners, and output is distinct exclusive storage.
        assert_eq!(unsafe { dav1d_apply_grain(decoder.0,
            Some(NonNull::from(&mut output.0)), Some(NonNull::from(&mut source.0))) }.0, 0);
        assert_eq!(source.frame(), before);
        assert_ne!(output.frame().samples, before.samples);
        if release_input_first {
            drop(source);
            assert!(data.upgrade().is_none());
            drop(decoder);
            assert_eq!(output.frame().size, before.size);
        } else {
            drop(output);
            drop(decoder);
            assert_eq!(source.frame(), before);
            assert!(data.upgrade().is_some());
        }
    }
}
