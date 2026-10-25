//! Color changes prepare immutable backing and a complete private GPU canvas on
//! IO. The render owner publishes renderer + document/history in one turn.
use crate::android::{error, fail, or_throw, read, string};
use jni::{
    JNIEnv,
    objects::{JClass, JString},
    sys::{jboolean, jbyteArray, jint, jlong, jstring},
};
use layer_host::tasks::{ColorTask, Preview};

type Task = crate::inspection::Task<ColorTask>;
#[unsafe(no_mangle)]
pub extern "system" fn Java_art_capycanvas_Native_colorTask(
    mut env: JNIEnv,
    _: JClass,
    handle: jlong,
    id: jint,
    cancel: jlong,
) -> jlong {
    crate::jni_entry!(env, {
    crate::inspection::capture_task(&mut env, handle, id, cancel, ColorTask::capture)
    })
}
#[unsafe(no_mangle)]
pub extern "system" fn Java_art_capycanvas_Native_colorWork(
    mut env: JNIEnv,
    _: JClass,
    handle: jlong,
    choice: JString,
    copy: jboolean,
) -> jstring {
    crate::jni_entry!(env, {
    let result = (|| {
        let choice = serde_json::from_str(&read(&mut env, &choice)?).map_err(error)?;
        let mut t_owner = crate::inspection::borrow::<Task>(handle)?;let t=&mut *t_owner;
        crate::inspection::on_worker("capy-color", "Color worker failed", move || {
            t.task.work(choice, copy != 0, t.control.clone())?;
            serde_json::to_string(&t.task.details()).map_err(error)
        })
    })();
    string(&mut env, result)
    })
}
pub(crate) fn preview_bytes(env: &mut JNIEnv, previews: &[Preview], after: jboolean) -> jbyteArray {
    let result = (|| {
        let preview = previews
            .get(usize::from(after != 0))
            .ok_or("Comparison is unavailable")?;
        let mut bytes = Vec::with_capacity(8 + preview.pixels.len());
        for v in preview.extent {
            bytes.extend_from_slice(&v.to_le_bytes());
        }
        bytes.extend_from_slice(&preview.pixels);
        env.byte_array_from_slice(&bytes)
            .map(|a| a.into_raw())
            .map_err(error)
    })();
    or_throw(env, result, std::ptr::null_mut())
}
#[unsafe(no_mangle)]
pub extern "system" fn Java_art_capycanvas_Native_colorPreview(
    mut env: JNIEnv,
    _: JClass,
    handle: jlong,
    after: jboolean,
) -> jbyteArray {
    crate::jni_entry!(env, {
    preview_bytes(&mut env, crate::inspection::borrow::<Task>(handle)?.task.previews(), after)
    })
}
#[unsafe(no_mangle)]
pub extern "system" fn Java_art_capycanvas_Native_colorAdopt(
    mut env: JNIEnv,
    _: JClass,
    handle: jlong,
    job: jlong,
) {
    crate::jni_entry!(env, handle, capy_app, {
    let result = (|| {
        let a = &mut *capy_app;
        let mut t_owner = crate::inspection::borrow::<Task>(job)?;let t=&mut *t_owner;
        t.task
            .adopt(&mut a.host, t.control.is_cancelled(), || true)?;
        a.project_adopted();
        Ok(())
    })();
    fail(&mut env, result)
    })
}
#[unsafe(no_mangle)]
pub extern "system" fn Java_art_capycanvas_Native_colorFree(mut env: JNIEnv, _: JClass, handle: jlong) {
    crate::jni_entry!(env, {
    crate::inspection::release::<Task>(handle)?;
    })
}

struct CopyOpening {
    environment: layer_host::open::OpenEnvironment,
    epoch: u64,
    revision: u64,
    generation: u64,
    candidate: Option<Box<layer_ui::UiSession<layer_host::Renderer>>>,
    retired: Option<Box<layer_render_wgpu::WgpuRasterizer>>,
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_art_capycanvas_Native_colorCopyTask(mut env: JNIEnv, _: JClass, handle: jlong, source: jlong) -> jlong {
    crate::jni_entry!(env, handle, capy_app, {
        let result = (|| {
            let a = &mut *capy_app;
            let source = crate::inspection::borrow::<Task>(source)?;
            source.task.validate(&a.host, source.control.is_cancelled())?;
            crate::inspection::owned(CopyOpening {
                environment: layer_host::open::OpenEnvironment::capture(&a.host.session,
                    a.window.documents.admission(&a.host.session.retained_document_tiles()), a.host.renderer_options())?,
                epoch: a.host.session.state().document_file.epoch,
                revision: a.host.session.engine().document().revision,
                generation: a.gpu_generation,
                candidate: None, retired: None,
            })
        })();
        or_throw(&mut env, result, 0)

    })
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_art_capycanvas_Native_colorCopyPrepare(mut env: JNIEnv, _: JClass, handle: jlong, source: jlong) {
    crate::jni_entry!(env, {
        let mut task_owner = crate::inspection::borrow::<CopyOpening>(handle)?; let task = &mut *task_owner;
        let source = crate::inspection::borrow::<Task>(source)?;
        let result = crate::inspection::on_worker("capy-color-copy", "Converted copy preparation failed", move || {
            let document = source.task.copy_document(source.control.is_cancelled())?;
            task.candidate = Some(task.environment.prepare(document, || source.control.is_cancelled())?);
            Ok(())
        });
        fail(&mut env, result)
    })
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_art_capycanvas_Native_colorCopyReady(mut env: JNIEnv, _: JClass, handle: jlong, copy: jlong, source: jlong) -> jboolean {
    crate::jni_entry!(env, handle, capy_app, {
        let result = (|| {
            let a = &mut *capy_app;
            let mut task_owner = crate::inspection::borrow::<CopyOpening>(copy)?; let task = &mut *task_owner;
            let source = crate::inspection::borrow::<Task>(source)?;
            source.task.validate(&a.host, source.control.is_cancelled())?;
            if a.gpu_generation != task.generation || task.candidate.is_none() { return Err(a.host.session.localization().text(layer_ui::MessageId::DOCUMENTS_ERROR_STALE_CANDIDATE).to_string()); }
            a.window.adoption_ready(&a.host)
        })();
        or_throw(&mut env, result.map(u8::from), 0)
    })
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_art_capycanvas_Native_colorCopyAdopt(mut env: JNIEnv, _: JClass, handle: jlong, copy: jlong, source: jlong) {
    crate::jni_entry!(env, handle, capy_app, {
        let result = (|| {
            let a = &mut *capy_app;
            let mut task_owner = crate::inspection::borrow::<CopyOpening>(copy)?; let task = &mut *task_owner;
            let source = crate::inspection::borrow::<Task>(source)?;
            source.task.validate(&a.host, source.control.is_cancelled())?;
            if a.gpu_generation != task.generation || a.gpu_watch.failure().is_some() { return Err(a.host.session.localization().text(layer_ui::MessageId::DOCUMENTS_ERROR_STALE_CANDIDATE).to_string()); }
            let adoption = layer_host::window::OpenAdoption { epoch: task.epoch, revision: task.revision, location: None };
            task.retired = a.window.adopt(&mut a.host, &mut task.candidate, adoption, || !source.control.is_cancelled(), |session| session)?;
            a.document_retired(); a.project_adopted();
            Ok(())
        })();
        fail(&mut env, result)
    })
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_art_capycanvas_Native_colorCopyFree(mut env: JNIEnv, _: JClass, handle: jlong) {
    crate::jni_entry!(env, { crate::inspection::release::<CopyOpening>(handle)?; })
}
