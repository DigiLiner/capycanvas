#[test]
fn packaged_language_tags_match_the_shared_shipped_inventory() {
    let xml = include_str!("../../app/src/main/res/xml/locales_config.xml");
    let tags = xml.split("android:name=\"").skip(1)
        .map(|value| value.split_once('"').unwrap().0).collect::<Vec<_>>();
    let shipped = layer_ui::localization::SHIPPED_LANGUAGES.iter()
        .map(|language| language.tag()).collect::<Vec<_>>();
    assert_eq!(tags, shipped);
}
#[test]
fn android_launch_uses_supplied_preferences_before_first_view_without_gpu() {
    for (saved, tag) in [("", "ja"), ("broken", "ja"), (r#"{"language":{"Explicit":"en"},"pan_speed":"broken"}"#, "en"),
        (r#"{"language":{"Explicit":"ja"}}"#, "ja")] {
        let host = layer_host::NativeHost::launch(layer_ui::Platform::Android, saved, &["ja-JP", "zh-Hant", "en-US"]).unwrap();
        let expected = layer_ui::launch_localization(saved, &["ja-JP", "zh-Hant", "en-US"]);
        assert!(std::sync::Arc::ptr_eq(host.session.localization(), &expected));
        assert_eq!(host.bootstrap_view().active_tag, tag);
        assert_eq!(host.bootstrap_view().shipped_tags, layer_ui::localization::SHIPPED_LANGUAGES.iter().map(|language| language.tag()).collect::<Vec<_>>());
        assert!(host.session.engine().backend().0.is_none());
    }
}
#[test]
fn every_jni_function_runs_inside_jni_entry() {
    let mut pending = vec![std::path::PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/src"))];
    let mut unguarded = Vec::new();
    let mut functions = 0;
    while let Some(path) = pending.pop() {
        if path.is_dir() {
            pending.extend(std::fs::read_dir(path).unwrap().map(|entry| entry.unwrap().path()));
            continue;
        }
        if path.extension().is_none_or(|extension| extension != "rs") { continue; }
        let source = std::fs::read_to_string(&path).unwrap();
        for function in source.split("extern \"system\" fn ").skip(1) {
            functions += 1;
            let (signature, body) = function.split_once('{').unwrap();
            if !body.trim_start().starts_with("crate::jni_entry!(") {
                unguarded.push(signature.split('(').next().unwrap().to_owned());
            }
        }
    }
    assert!(functions > 100);
    assert!(unguarded.is_empty(), "{unguarded:?}");
}

#[test]
fn jni_entry_returns_load_failure_after_a_panic() {
    let value = crate::jni_entry!(result => { result.unwrap_or(-1) }; {
        std::panic::resume_unwind(Box::new("load failed"))
    });
    assert_eq!(value, -1);
}

#[test]
#[cfg(unix)]
fn jni_entry_writes_photo_failures_to_the_error_pipe() {
    use std::io::{Read, Write};
    for panic in [false, true] {
        let (mut reader, mut writer) = std::os::unix::net::UnixStream::pair().unwrap();
        let value = crate::jni_entry!(result => {
            match result {
                Ok(()) => 1,
                Err(error) => { writer.write_all(error.as_bytes()).unwrap(); 0 }
            }
        }; {
            if panic { std::panic::resume_unwind(Box::new("decoder failed")); }
            Err("decoder refused input".to_owned())
        });
        drop(writer);
        let mut error = String::new();
        reader.read_to_string(&mut error).unwrap();
        assert_eq!(value, 0);
        assert_eq!(error, if panic { "Native operation panicked: decoder failed" } else { "decoder refused input" });
    }
}

#[test]
fn android_workspace_startup_ticks_on_storage_replies_without_a_timer() {
    struct Directory(std::path::PathBuf);
    impl Drop for Directory {
        fn drop(&mut self) { let _ = std::fs::remove_dir_all(&self.0); }
    }
    let directory = Directory(std::env::temp_dir().join(format!("capy-android-workspaces-{}", layer_workspace::new_id())));
    std::fs::create_dir_all(&directory.0).unwrap();
    let mut host = layer_host::NativeHost::launch(layer_ui::Platform::Android, "", &["en-US"]).unwrap();
    let (sender, wakes) = std::sync::mpsc::channel();
    let sender = std::sync::Mutex::new(sender);
    let mut controller = crate::launch::start_workspaces(&mut host, &directory.0, 1_000,
        std::sync::Arc::new(move || { let _ = sender.lock().unwrap().send(()); })).unwrap();
    let mut ticks = 0;
    while !controller.view.ready {
        let before = host.session.state().revision;
        let change = controller.tick(&mut host.session, 1_000);
        host.apply_change(before, change);
        ticks += 1;
        if !controller.view.ready {
            wakes.recv_timeout(std::time::Duration::from_secs(10)).expect("a storage reply must wake the workspace owner");
        }
    }
    assert!(host.session.state().workspace_ready);
    assert!(ticks > 1);
}

#[test]
fn canceled_ready_frame_cannot_complete_a_same_digest_reveal() {
    use std::sync::atomic::Ordering;
    let mut frame = crate::launch::LaunchFrame::default();
    let stale = frame.submit(Some(17)).unwrap();
    frame.cancel(None);
    assert_eq!(frame.take_completed(),None);
    let current = frame.submit(Some(17)).unwrap();
    assert!(!std::sync::Arc::ptr_eq(&stale,&current));
    stale.store(true,Ordering::Release);
    assert_eq!(frame.take_completed(),None);
    current.store(true,Ordering::Release);
    assert_eq!(frame.take_completed(),Some(17));
    assert_eq!(frame.take_completed(),None);
}

#[test]
fn launch_completion_matches_decreasing_digest_and_surface_ownership() {
    use std::sync::atomic::Ordering;
    let mut frame = crate::launch::LaunchFrame::default();
    let stale = frame.submit(Some(91)).unwrap();
    frame.cancel(Some(7));
    let current = frame.submit(Some(7)).unwrap();
    stale.store(true,Ordering::Release);
    assert_eq!(frame.take_completed(),None);
    current.store(true,Ordering::Release);
    assert_eq!(frame.take_completed(),Some(7));
    let destroyed = frame.submit(Some(7)).unwrap();
    let mut replacement = crate::launch::LaunchFrame::default();
    let current = replacement.submit(Some(7)).unwrap();
    destroyed.store(true,Ordering::Release);
    assert_eq!(replacement.take_completed(),None);
    current.store(true,Ordering::Release);
    assert_eq!(replacement.take_completed(),Some(7));
}
