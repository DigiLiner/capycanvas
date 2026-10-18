//! Paper library requests: captured on the render owner, read and written on
//! IO, then adopted by the owner.
use crate::android::{app, fail, or_throw, read};
use jni::{JNIEnv, objects::{JClass, JString}, sys::{jboolean, jint, jlong}};
use layer_host::papers::{PaperResult, PaperTask};
use std::{os::fd::FromRawFd, sync::{Mutex, atomic::{AtomicBool, Ordering}}};

struct Job {
    task: PaperTask,
    cancelled: AtomicBool,
    result: Mutex<Option<Result<PaperResult, String>>>,
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_art_capycanvas_Native_paperTask(mut env: JNIEnv, _: JClass, handle: jlong, request: jint, directory: JString) -> jlong {
    let result = (|| {
        let directory = read(&mut env, &directory)?;
        let task = PaperTask::capture(&unsafe { app(handle) }.host.session, u32::try_from(request).ok(), directory.into())?;
        Ok(Box::into_raw(Box::new(Job { task, cancelled: AtomicBool::new(false), result: Mutex::new(None) })) as jlong)
    })();
    or_throw(&mut env, result, 0)
}
#[unsafe(no_mangle)]
pub extern "system" fn Java_art_capycanvas_Native_paperImports(_: JNIEnv, _: JClass, job: jlong) -> jboolean {
    jboolean::from(unsafe { crate::inspection::borrow_ref::<Job>(job) }.task.imports())
}
#[unsafe(no_mangle)]
pub extern "system" fn Java_art_capycanvas_Native_paperWork(mut env: JNIEnv, _: JClass, job: jlong, fd: jint, name: JString) {
    let name = read(&mut env, &name).unwrap_or_default();
    let job = unsafe { crate::inspection::borrow_ref::<Job>(job) };
    let file = (fd >= 0).then(|| unsafe { std::fs::File::from_raw_fd(fd) });
    let result = crate::inspection::on_worker("capy-paper", "Paper library worker failed",
        || job.task.work(file.as_ref().map(|file| (file, name.as_str())), &job.cancelled));
    *job.result.lock().unwrap_or_else(|e| e.into_inner()) = Some(result);
}
#[unsafe(no_mangle)]
pub extern "system" fn Java_art_capycanvas_Native_paperApply(mut env: JNIEnv, _: JClass, handle: jlong, job: jlong) {
    let result = (|| {
        let job = unsafe { crate::inspection::borrow_ref::<Job>(job) };
        let result = job.result.lock().unwrap_or_else(|e| e.into_inner()).take().ok_or("Paper library worker failed")?;
        job.task.apply(&mut unsafe { app(handle) }.host, result, job.cancelled.load(Ordering::Relaxed))
    })();
    fail(&mut env, result)
}
#[unsafe(no_mangle)]
pub extern "system" fn Java_art_capycanvas_Native_paperCancel(_: JNIEnv, _: JClass, job: jlong) {
    unsafe { crate::inspection::borrow_ref::<Job>(job) }.cancelled.store(true, Ordering::Relaxed);
}
#[unsafe(no_mangle)]
pub extern "system" fn Java_art_capycanvas_Native_paperFree(_: JNIEnv, _: JClass, job: jlong) {
    unsafe { crate::inspection::release::<Job>(job) };
}
