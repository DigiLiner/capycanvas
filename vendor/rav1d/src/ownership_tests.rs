use super::*;
use crate::include::dav1d::headers::{DRav1d, Dav1dFrameHeader, Rav1dFrameHeader};
#[cfg(not(debug_assertions))]
use std::sync::atomic::AtomicUsize;

fn context() -> Option<Dav1dContext> {
    let mut settings = mem::MaybeUninit::<Dav1dSettings>::uninit();
    // SAFETY: settings has writable storage for the complete output.
    unsafe { dav1d_default_settings(NonNull::new(settings.as_mut_ptr()).unwrap()); }
    // SAFETY: dav1d_default_settings initializes every field.
    let mut settings = unsafe { settings.assume_init() };
    settings.n_threads = 1;
    settings.max_frame_delay = 1;
    let mut context = None;
    // SAFETY: settings is initialized; context is an exclusively owned output.
    assert_eq!(unsafe { dav1d_open(Some(NonNull::from(&mut context)), Some(NonNull::from(&mut settings))) }.0, 0);
    context
}

#[test]
fn applying_grain_preserves_the_borrowed_picture_in_both_release_orders() {
    for release_input_first in [false, true] {
        let header = Arc::new(DRav1d::<Rav1dFrameHeader, Dav1dFrameHeader>::from_rav1d(Rav1dFrameHeader::default()));
        let mut input = Dav1dPicture::from(Rav1dPicture { frame_hdr: Some(header.clone()), ..Default::default() });
        let mut output = Dav1dPicture::default();
        let mut context = context();
        // SAFETY: input owns live initialized picture metadata, output is distinct
        // writable storage, and context remains live until close.
        assert_eq!(unsafe { dav1d_apply_grain(context, Some(NonNull::from(&mut output)), Some(NonNull::from(&mut input))) }.0, 0);
        assert_eq!(Arc::strong_count(&header), 3);
        // SAFETY: each picture's raw owners are consumed exactly once; the context
        // is closed only after the operation completes.
        unsafe {
            let [first, second] = if release_input_first { [&mut input, &mut output] } else { [&mut output, &mut input] };
            dav1d_picture_unref(Some(NonNull::from(first)));
            assert_eq!(Arc::strong_count(&header), 2);
            dav1d_picture_unref(Some(NonNull::from(second)));
            assert_eq!(Arc::strong_count(&header), 1);
            dav1d_close(Some(NonNull::from(&mut context)));
        }
    }
}

#[test]
fn unreferencing_a_picture_leaves_no_raw_owners() {
    let mut picture = Dav1dPicture::from(Rav1dPicture::default());
    for _ in 0..2 {
        // SAFETY: picture is an exclusively owned live C picture, including its cleared state.
        unsafe { dav1d_picture_unref(Some(NonNull::from(&mut picture))); }
        assert!(picture.itut_t35_ref.is_none());
        assert!(picture.frame_hdr_ref.is_none());
        assert!(picture.seq_hdr_ref.is_none());
        assert!(picture.r#ref.is_none());
    }
}

#[cfg(not(debug_assertions))]
unsafe extern "C" fn free_byte(ptr: *const u8, cookie: Option<SendSyncNonNull<c_void>>) {
    // SAFETY: the tests retain the AtomicUsize cookie through final unref and
    // transfer exactly one Box<u8> allocation to each callback.
    unsafe {
        cookie.unwrap().cast::<AtomicUsize>().as_ref().fetch_add(1, Ordering::SeqCst);
        drop(Box::from_raw(ptr.cast_mut()));
    }
}

#[test]
#[cfg(not(debug_assertions))]
fn rejected_user_data_wrap_retains_packet_bytes_and_existing_user_data() {
    let data_frees = AtomicUsize::new(0);
    let user_frees = AtomicUsize::new(0);
    let mut packet = Dav1dData::default();
    let data = NonNull::from(Box::leak(Box::new(0x7f)));
    let user = NonNull::from(Box::leak(Box::new(0x31)));
    // SAFETY: live exclusive packet output, one-byte owned allocations, and
    // thread-safe callback cookies retained until all owners are released.
    unsafe {
        assert_eq!(dav1d_data_wrap(Some(NonNull::from(&mut packet)), Some(data), 1, Some(free_byte), Some(SendSyncNonNull::from_ref(&data_frees).cast())).0, 0);
        assert_eq!(dav1d_data_wrap_user_data(Some(NonNull::from(&mut packet)), Some(user), Some(free_byte), Some(SendSyncNonNull::from_ref(&user_frees).cast())).0, 0);
    }
    let mut rejected = 0u8;
    // SAFETY: rejected stays live; the missing callback is rejected before any
    // ownership transfer of this byte. Packet remains exclusively owned.
    assert_eq!(unsafe { dav1d_data_wrap_user_data(Some(NonNull::from(&mut packet)), Some(NonNull::from(&mut rejected)), None, None) }.0, -(EINVAL as i32));
    assert_eq!(data_frees.load(Ordering::SeqCst), 0);
    assert_eq!(user_frees.load(Ordering::SeqCst), 0);
    assert_eq!(packet.data, Some(data));
    assert_eq!(packet.m.user_data.data, Some(user));
    // SAFETY: the failed call preserves the packet's original live owners.
    unsafe { dav1d_data_unref(Some(NonNull::from(&mut packet))); }
    assert_eq!(data_frees.load(Ordering::SeqCst), 1);
    assert_eq!(user_frees.load(Ordering::SeqCst), 1);
    assert!(packet.data.is_none());
    assert!(packet.m.user_data.data.is_none());
}
