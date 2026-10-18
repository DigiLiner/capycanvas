//! Pixel copies: frozen on the render Looper, composed and encoded on a file
//! worker. The application keeps the clip; Kotlin shares its PNG through a
//! FileProvider URI whose clip description carries the nonce.
use crate::android::{app, error, fail, or_throw, read, string};
use jni::{
    JNIEnv,
    objects::{JClass, JString},
    sys::{jboolean, jint, jlong, jstring},
};
use layer_host::clipboard::ClipTask;
use layer_ui::PixelClip;

#[unsafe(no_mangle)]
pub extern "system" fn Java_art_capycanvas_Native_clipTask(mut env: JNIEnv, _: JClass, handle: jlong, id: jint) -> jlong {
    crate::jni_entry!(env, handle, {
    let a = unsafe { app(handle) };
    let result = ClipTask::capture(&mut a.host.session, id as u32)
        .map(|task| Box::into_raw(Box::new(task)) as jlong);
    or_throw(&mut env, result, 0)
    })
}
#[unsafe(no_mangle)]
pub extern "system" fn Java_art_capycanvas_Native_clipTaskLarge(mut env: JNIEnv, _: JClass, handle: jlong) -> jboolean {
    crate::jni_entry!(env, {
    jboolean::from(unsafe { crate::inspection::borrow_ref::<ClipTask>(handle) }.capture_details().large)
    })
}
#[unsafe(no_mangle)]
pub extern "system" fn Java_art_capycanvas_Native_documentRequestTitle(mut env: JNIEnv, _: JClass, handle: jlong, id: jint) -> jstring {
    crate::jni_entry!(env, handle, {
    let owner = unsafe { app(handle) };
    match owner.host.session.document_request(id as u32) {
        Ok(request) => string(&mut env, Ok(request.title(owner.host.session.localization()).to_string())),
        Err(_) => std::ptr::null_mut(),
    }
    })
}
/// Worker: consumes the task and returns the finished clip.
#[unsafe(no_mangle)]
pub extern "system" fn Java_art_capycanvas_Native_clipRun(
    mut env: JNIEnv,
    _: JClass,
    handle: jlong,
    control: jlong,
    nonce: JString,
) -> jlong {
    crate::jni_entry!(env, {
    let task = unsafe { Box::from_raw(handle as *mut ClipTask) };
    let result = read(&mut env, &nonce).and_then(|nonce| task.run(nonce, crate::inspection::control(control)));
    or_throw(&mut env, result.map(|clip| Box::into_raw(Box::new(clip)) as jlong), 0)
    })
}
#[unsafe(no_mangle)]
pub extern "system" fn Java_art_capycanvas_Native_clipTaskFree(mut env: JNIEnv, _: JClass, handle: jlong) {
    crate::jni_entry!(env, {
    if handle != 0 {
        drop(unsafe { Box::from_raw(handle as *mut ClipTask) });
    }
    })
}
/// Worker: write the clip's PNG for other applications.
#[unsafe(no_mangle)]
pub extern "system" fn Java_art_capycanvas_Native_clipWritePng(mut env: JNIEnv, _: JClass, handle: jlong, path: JString) {
    crate::jni_entry!(env, {
    let result = read(&mut env, &path).and_then(|path| std::fs::write(path, &unsafe { crate::inspection::borrow_ref::<PixelClip>(handle) }.png[..]).map_err(error));
    fail(&mut env, result);
    })
}
/// Keep the clip for the application and complete its copy, which erases a Cut.
#[unsafe(no_mangle)]
pub extern "system" fn Java_art_capycanvas_Native_clipAdopt(mut env: JNIEnv, _: JClass, handle: jlong, id: jint, clip: jlong, control: jlong) {
    crate::jni_entry!(env, handle, {
    let clip = unsafe { Box::from_raw(clip as *mut PixelClip) };
    let result = (|| {
        let a = unsafe { app(handle) };
        if control != 0 && crate::inspection::control(control).is_cancelled() {
            return Err("Copy cancelled".into());
        }
        a.host.session.document_request(id as u32)?;
        a.window.documents.clip.set(*clip);
        let previous = a.host.session.state().revision;
        let mut change = a.host.session.complete_document_request(id as u32, Ok(true))?;
        change.canvas_wake = true;
        a.host.apply_change(previous, change);
        Ok(())
    })();
    fail(&mut env, result);
    })
}
#[unsafe(no_mangle)]
pub extern "system" fn Java_art_capycanvas_Native_clipFree(mut env: JNIEnv, _: JClass, handle: jlong) {
    crate::jni_entry!(env, {
    if handle != 0 {
        drop(unsafe { Box::from_raw(handle as *mut PixelClip) });
    }
    })
}
#[unsafe(no_mangle)]
pub extern "system" fn Java_art_capycanvas_Native_clipNonce(mut env: JNIEnv, _: JClass, handle: jlong) -> jstring {
    crate::jni_entry!(env, handle, {
    let a = unsafe { app(handle) };
    match a.window.documents.clip.get() {
        Some(clip) => string(&mut env, Ok(clip.nonce.clone())),
        None => std::ptr::null_mut(),
    }
    })
}
