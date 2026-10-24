//! Android host boundary. The render Looper exclusively owns `App`; native UI
//! actions use Serde, while pointer history crosses JNI as packed numeric data.
#[cfg(target_os = "android")]
macro_rules! jni_entry {
    ($env:ident, $handle:ident, $body:block) => {{
        let owner = $handle as *mut crate::app::App;
        match unsafe { owner.as_ref() }.and_then(|app| app.host.poisoned()).map(str::to_owned) {
            Some(cause) => crate::android::refuse(&mut $env, cause),
            None => match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| $body)) {
                Ok(value) => value,
                Err(payload) => {
                    let cause = match unsafe { owner.as_mut() } {
                        Some(app) => app.host.poison(&*payload),
                        None => layer_host::panic_diagnostic(&*payload),
                    };
                    crate::android::refuse(&mut $env, cause)
                }
            },
        }
    }};
    ($env:ident, $body:block) => {{
        match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| $body)) {
            Ok(value) => value,
            Err(payload) => crate::android::refuse(&mut $env, layer_host::panic_diagnostic(&*payload)),
        }
    }};
}
#[cfg(target_os = "android")]
pub(crate) use jni_entry;
#[cfg(any(target_os = "android", test))]
mod display;
#[cfg(target_os = "android")]
mod android;
#[cfg(target_os = "android")]
mod app;
#[cfg(any(target_os = "android", test))]
mod launch;
#[cfg(any(target_os = "android", test))]
mod document_close;
#[cfg(target_os = "android")]
mod workspaces;

#[cfg(target_os = "android")]
mod documents;
#[cfg(target_os = "android")]
mod session_recovery;
#[cfg(target_os = "android")]
mod document_tabs;

#[cfg(target_os = "android")]
mod image_import;
#[cfg(target_os = "android")]
mod clipboard;

#[cfg(target_os = "android")]
mod inspection;
#[cfg(target_os = "android")]
mod scopes;

#[cfg(target_os = "android")]
mod color_edit;
#[cfg(target_os = "android")]
mod source_edit;
#[cfg(target_os = "android")]
mod image_objects;

#[cfg(target_os = "android")]
mod color_preferences;
#[cfg(target_os = "android")]
mod proof;
#[cfg(target_os = "android")]
mod papers;

#[cfg(target_os = "android")]
mod hdr;

#[cfg(test)]
mod launch_tests;
