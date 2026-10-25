//! Real worker/owner proof transactions. View changes must not edit paint.
use super::*;
use layer_core::color::{ColorProfile, ProofRecipe, RgbSpace};

fn prepare(app: &App, recipe: Option<&ProofRecipe>, setup: bool) -> ProjectJob {
    let request = if setup {
        app.invoke("soft_proof_setup");
        app.state()["requests"]
            .as_array()
            .unwrap()
            .iter()
            .find(|r| r["kind"]["type"] == "soft_proof_setup")
            .unwrap()["id"]
            .as_u64()
            .unwrap() as u32
    } else {
        0
    };
    let recipe = CString::new(serde_json::to_string(&recipe).unwrap()).unwrap();
    let pointer = unsafe { capy_apple_proof_task(app.0, request, recipe.as_ptr()) };
    assert!(!(pointer==0));
    ProjectJob(pointer)
}
fn status(app: &App) -> Value {
    app.request(2, json!({"type":"proof_status"})).unwrap()
}
fn preserved(job: &ProjectJob) -> Option<Vec<u8>> {
    let mut count = 0;
    assert_eq!(
        unsafe { capy_project_proof_preservation(job.0, &mut count) },
        0
    );
    (count>0).then(||copied_bytes(count,|offset,output,length|unsafe {capy_project_proof_preservation_copy(job.0,offset,output,length)}))
}

#[test]
fn apple_proof_workers_preserve_profiles_reject_cancellation_and_leave_artwork_exact() {
    for platform in [0, 1] {
        let app = App::new(platform);
        app.borrow().host.session.renderer_mut().0 = Some(native_renderer());
        app.draw_until_idle();
        app.stroke();
        app.draw_until_idle();
        let pixels = app.pixels();
        let before = app.document().clone();
        let icc = layer_color::profile_bytes(&ColorProfile::Builtin(RgbSpace::DisplayP3)).unwrap();
        let original =
            ProofRecipe::new("Embedded P3".into(), ColorProfile::Icc(icc.clone().into()));
        app.borrow()
            .host
            .session
            .set_proof_recipe(Some(original.clone()))
            .unwrap();
        let installed = app.document().clone();
        let replacement =
            ProofRecipe::new("sRGB proof".into(), ColorProfile::Builtin(RgbSpace::Srgb));
        let job = prepare(&app, Some(&replacement), true);
        assert_eq!(
            unsafe { capy_apple_proof_apply(app.0, job.0, true) },
            -1,
            "must build first"
        );
        assert_eq!(
            unsafe { capy_project_proof_build(job.0) },
            0,
            "{:?}",
            job.error()
        );
        assert_eq!(preserved(&job), Some(icc.clone()));
        assert_eq!(unsafe { capy_apple_proof_check(app.0, job.0) }, 0);
        assert_eq!(
            unsafe { capy_apple_proof_apply(app.0, job.0, false) },
            -1,
            "must preserve ICC first"
        );
        assert_project_document(
            &app.document(),
            &installed,
        );
        assert_eq!(unsafe { capy_apple_proof_apply(app.0, job.0, true) }, 0);
        assert_eq!(status(&app)["text"], "Proof: sRGB proof");
        assert_eq!(status(&app)["needed"], false);
        assert_eq!(
            unsafe { capy_apple_proof_apply(app.0, job.0, true) },
            -1,
            "setup is consumed once"
        );
        app.draw_until_idle();
        assert_eq!(app.pixels(), pixels);
        let result = app.document().clone();
        assert_eq!(result.output().proof, Some(replacement.clone()));
        for command in ["soft_proof", "gamut_warning", "soft_proof", "gamut_warning"] {
            app.invoke(command);
            app.draw_until_idle();
            assert_project_document(&app.document(), &result);
            assert_eq!(
                app.pixels(),
                pixels,
                "view flags never reach artwork/export pixels"
            );
        }
        app.invoke("undo");
        app.draw_until_idle();
        assert_eq!(
            app.document().output().proof,
            Some(original)
        );
        app.invoke("redo");
        app.draw_until_idle();
        assert_eq!(
            app.document().output().proof,
            Some(replacement.clone())
        );

        let cancelled = prepare(&app, Some(&replacement), true);
        let id = app.state()["requests"]
            .as_array()
            .unwrap()
            .iter()
            .find(|r| r["kind"]["type"] == "soft_proof_setup")
            .unwrap()["id"]
            .clone();
        unsafe { capy_project_cancel(cancelled.0) };
        assert_eq!(unsafe { capy_project_proof_build(cancelled.0) }, -1);
        assert_eq!(unsafe { capy_apple_proof_check(app.0, cancelled.0) }, -1);
        app.action(json!({"type":"complete_request", "id":id, "error":null}));
        assert_eq!(
            unsafe { capy_apple_proof_apply(app.0, cancelled.0, true) },
            -1
        );

        // A background rebuild may outlive its original recipe. Reject its LUT
        // without changing the newer recipe, including after actual CPU work.
        let stale = prepare(&app, None, false);
        assert_eq!(preserved(&stale), None);
        assert_eq!(unsafe { capy_project_proof_build(stale.0) }, 0);
        let changed = ProofRecipe::new(
            "New target".into(),
            ColorProfile::Builtin(RgbSpace::DisplayP3),
        );
        app.borrow()
            .host
            .session
            .set_proof_recipe(Some(changed.clone()))
            .unwrap();
        assert_eq!(unsafe { capy_apple_proof_apply(app.0, stale.0, false) }, -1);
        assert_eq!(
            app.document().output().proof,
            Some(changed)
        );
        let failed = prepare(&app, None, false);
        assert_eq!(
            unsafe { capy_apple_proof_failed(app.0, failed.0, c"Unavailable profile".as_ptr()) },
            0
        );
        assert_eq!(status(&app)["needed"], false);
        assert_eq!(status(&app)["error"], "Unavailable profile");
        {
            let mut lease_owner=app.borrow();let owner=&mut *lease_owner;
            assert!(owner.host.proof.lut(&owner.host.session).is_none());
        }
        app.draw_until_idle();
        assert_eq!(app.pixels(), pixels);
        let final_document = &app.document();
        assert_eq!(final_document.artwork.paint.len(), before.artwork.paint.len());
        for (_, id, source) in before.artwork.paint.iter() {
            let current = final_document.artwork.paint.get(final_document.artwork.paint.resolve(id).unwrap()).unwrap();
            assert_eq!(raster_samples(&current.raster), raster_samples(&source.raster));
        }
    }
}
