//! Shared print-view preparation on a worker; adoption stays on the editor owner.
use super::*;
use layer_ui::proof_workflow::ProofPreparation;
use std::sync::Arc;

pub(super) struct Task {
    job: ProofPreparation,
    lut: Option<Arc<layer_color::ProofLut>>,
}

/// # Safety
/// Editor owner only. Recipe is optional ProofRecipe JSON; request 0 restores a
/// view without changing the document. The returned task never borrows the app.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn capy_apple_proof_task(
    app: u64,
    request: u32,
    recipe: *const c_char,
) -> u64 {
    let Some(mut lease_app)=handles::app(app) else {
        return 0;
    };
    let app=&mut *lease_app;
    app.perform(|app| {
        let recipe = if recipe.is_null() {
            None
        } else {
            serde_json::from_str(&unsafe { read_title(recipe) }?).map_err(|e| e.to_string())?
        };
        let session = &app.host.session;
        let job = ProofPreparation::begin(session, (request != 0).then_some(request), recipe).map_err(|reason|reason.proof_message(session.localization()))?;
        Ok(handles::store(*CapyProjectTask::new(
            Payload::Proof(Box::new(Task { job, lut: None })),
            session.state().document_file.epoch,
            session.engine().document().revision,
            None,
            session.localization().clone(),
        )))
    })
    .unwrap_or(0)
}

/// # Safety
/// Worker only; cancellation may run concurrently. No session or GPU access.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn capy_project_proof_build(task: u64) -> i32 {
    let Some(lease_task)=handles::shared::<CapyProjectTask>(task) else {
        return -1;
    };
    let task=&*lease_task;
    task.perform(|payload| {
        let Payload::Proof(proof) = payload else {
            return Err("Not a proof task".into());
        };
        let start = Instant::now();
        proof.lut = Some(proof.job.build(|| {
            task.control.is_cancelled() || start.elapsed() >= Duration::from_secs(120)
        }).map_err(|reason|reason.proof_message(&task.localization))?);
        Ok(())
    })
}

/// # Safety
/// Worker only, after build. Borrowed bytes remain valid until task release;
/// copy them before another task operation. A null result means no preservation.
/// Outputs are aligned and exclusively writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn capy_project_proof_preservation(
    task: u64,
    count: *mut usize,
) -> i32 {
    if count.is_null() { return -1; }
    unsafe { count.write(0); }
    let Some(lease_task)=handles::shared::<CapyProjectTask>(task) else {
        return -1;
    };
    let task=&*lease_task;
    task.perform_inner(|payload| {
        let Payload::Proof(proof) = payload else {
            return Err("Not a proof task".into());
        };
        if let Some(value) = proof.job.preservation() {
            unsafe { count.write(value.len()); }
        }
        Ok(())
    })
}

/// # Safety
/// Nonempty output is writable and exclusive for length bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn capy_project_proof_preservation_copy(task:u64,offset:usize,output:*mut u8,length:usize)->bool {
    let Some(task)=handles::shared::<CapyProjectTask>(task) else {return false;};
    let mut copied=false;
    let status=task.perform_inner(|payload| {
        let Payload::Proof(proof)=payload else {return Err("Not a proof task".into());};
        copied=unsafe {layer_host::ffi::copy_bytes(proof.job.preservation().unwrap_or(&[]),offset,output,length)};
        Ok(())
    });
    status==0&&copied
}

/// # Safety
/// Editor owner only, after the worker finishes. Check again before writing the
/// replaced ICC; apply repeats the same shared stale-document validation.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn capy_apple_proof_check(
    app: u64,
    task: u64,
) -> i32 {
    let (Some(mut lease_app),Some(lease_task))=(handles::app(app),handles::shared::<CapyProjectTask>(task)) else {
        return -1;
    };
    let (app,task)=(&mut *lease_app,&*lease_task);
    app.perform(|app| {
        task.check_cancelled()?;
        let state = task.state.lock().unwrap_or_else(|e| e.into_inner());
        let Payload::Proof(proof) = &state.payload else {
            return Err("Not a proof task".into());
        };
        proof.job.validate(&app.host.session).map_err(|reason|reason.proof_message(app.host.session.localization()))
    })
    .map_or(-1, |_| 0)
}

/// # Safety
/// Editor owner only, after preparation and any required durable ICC copy.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn capy_apple_proof_apply(
    app: u64,
    task: u64,
    preserved: bool,
) -> i32 {
    let (Some(mut lease_app),Some(lease_task))=(handles::app(app),handles::shared::<CapyProjectTask>(task)) else {
        return -1;
    };
    let (app,task)=(&mut *lease_app,&*lease_task);
    app.perform(|app| {
        task.check_cancelled()?;
        let state = task.state.lock().unwrap_or_else(|e| e.into_inner());
        let Payload::Proof(proof) = &state.payload else {
            return Err("Not a proof task".into());
        };
        let lut = proof.lut.clone().ok_or("Proof preview is not prepared")?;
        proof.job.validate(&app.host.session).map_err(|reason|reason.proof_message(app.host.session.localization()))?;
        if proof.job.preservation().is_some() && !preserved {
            return Err("Save the original in Saved Profiles before replacing it".into());
        }
        if task.begin_commit() < 0 {
            return Err("Proof preparation cancelled".into());
        }
        let previous = app.host.session.state().revision;
        let change = proof.job.apply(&mut app.host.session, preserved).map_err(|reason|reason.proof_message(app.host.session.localization()))?;
        app.host.proof.retain(&proof.job, lut).map_err(|reason|reason.proof_message(app.host.session.localization()))?;
        app.host.apply_change(previous, change);
        app.host.dirty = true;
        Ok(())
    })
    .map_or(-1, |_| 0)
}

/// # Safety
/// Editor owner only. Report a terminal background preparation error; setup
/// errors remain in the form and do not poison the previously valid view.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn capy_apple_proof_failed(
    app: u64,
    task: u64,
    message: *const c_char,
) -> i32 {
    let (Some(mut lease_app),Some(lease_task))=(handles::app(app),handles::shared::<CapyProjectTask>(task)) else {
        return -1;
    };
    let (app,task)=(&mut *lease_app,&*lease_task);
    app.perform(|app| {
        let state = task.state.lock().unwrap_or_else(|e| e.into_inner());
        let Payload::Proof(proof) = &state.payload else {
            return Err("Not a proof task".into());
        };
        app.host.proof.fail(
            &app.host.session,
            &proof.job,
            unsafe { read_title(message) }?,
        );
        Ok(())
    })
    .map_or(-1, |_| 0)
}
