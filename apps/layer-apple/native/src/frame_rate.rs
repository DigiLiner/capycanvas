use layer_host::CanvasFrameRate;
use std::sync::Arc;

/// # Safety
/// The app belongs to the calling render owner. Release each returned observer once.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn capy_apple_frame_rate_observer(app: u64) -> *const CanvasFrameRate {
    super::handles::app(app).map_or(std::ptr::null(), |app| Arc::into_raw(app.host.canvas_frame_rate.clone()))
}

/// # Safety
/// A nonnull observer retains the Arc returned by capy_apple_frame_rate_observer.
/// Its release must not race this call. Calls may run on any thread.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn capy_apple_frame_rate_presented(observer: *const CanvasFrameRate, presented_ns: u64, observed_ns: u64) {
    if let Some(observer) = unsafe { observer.as_ref() } { observer.presented(presented_ns, observed_ns); }
}

/// # Safety
/// A nonnull observer is an unreleased Arc returned by capy_apple_frame_rate_observer.
/// No presentation calls may use it after release.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn capy_apple_frame_rate_observer_release(observer: *const CanvasFrameRate) {
    if !observer.is_null() { drop(unsafe { Arc::from_raw(observer) }); }
}

#[cfg(test)]
mod tests {
    use super::*;
    use layer_render::FrameRateSource;

    #[test]
    fn drawable_feedback_survives_app_retirement_without_owner_access() {
        let app = super::super::capy_apple_create(1);
        assert_ne!(app, 0);
        let observer = unsafe { capy_apple_frame_rate_observer(app) };
        assert!(!observer.is_null());
        unsafe { super::super::capy_apple_destroy(app); }
        let address = observer as usize;
        std::thread::spawn(move || unsafe { capy_apple_frame_rate_presented(address as *const CanvasFrameRate, 1, 1); }).join().unwrap();
        let rate = unsafe { &*observer }.sample().unwrap();
        assert_eq!(rate.source, FrameRateSource::Presented);
        assert_eq!(rate.fps, 1.);
        unsafe { capy_apple_frame_rate_observer_release(observer); }
    }

    #[test]
    fn detaching_retires_callbacks_and_new_observers_use_the_current_surface() {
        let app = super::super::capy_apple_create(0);
        let previous = unsafe { capy_apple_frame_rate_observer(app) };
        assert!(!previous.is_null());
        assert_eq!(unsafe { super::super::capy_apple_detach(app) }, 0);
        let current = unsafe { capy_apple_frame_rate_observer(app) };
        assert!(!current.is_null());
        assert_ne!(previous, current);
        unsafe { capy_apple_frame_rate_presented(previous, 1, 1); }
        assert!(unsafe { &*current }.sample().is_none());
        unsafe { capy_apple_frame_rate_presented(current, 0, 1); capy_apple_frame_rate_presented(current, 2, 1); }
        assert!(unsafe { &*current }.sample().is_none());
        unsafe { capy_apple_frame_rate_presented(current, 1, 1); }
        assert_eq!(unsafe { &*current }.sample().unwrap().source, FrameRateSource::Presented);
        unsafe {
            capy_apple_frame_rate_observer_release(previous);
            capy_apple_frame_rate_observer_release(current);
            super::super::capy_apple_destroy(app);
        }
    }

    #[test]
    fn absent_observers_are_safe_without_a_live_session() {
        assert!(unsafe { capy_apple_frame_rate_observer(0) }.is_null());
        unsafe {
            capy_apple_frame_rate_presented(std::ptr::null(), 1, 1);
            capy_apple_frame_rate_observer_release(std::ptr::null());
        }
    }
}
