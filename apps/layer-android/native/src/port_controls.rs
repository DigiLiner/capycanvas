use crate::android::{error, read, string};
use jni::{JNIEnv, objects::{JClass, JString}, sys::jstring};

fn localizer(env: &mut JNIEnv, language: &JString) -> Result<std::sync::Arc<layer_ui::Localizer>, String> {
    let language = serde_json::from_value(serde_json::Value::String(read(env, language)?)).map_err(error)?;
    layer_ui::Localizer::prepared(language).ok_or_else(|| "android_language_not_prepared".to_owned())
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_art_capycanvas_Native_workspaceNameError(mut env: JNIEnv, _: JClass, name: JString, language: JString) -> jstring {
    crate::jni_entry!(env, {
        let result = (|| {
            let localizer = localizer(&mut env, &language)?;
            let name = read(&mut env, &name)?;
            Ok(serde_json::json!({"error":layer_workspace::validate_name(name.trim()).err().map(|reason| reason.localized_message(&localizer))}).to_string())
        })();
        string(&mut env, result)
    })
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_art_capycanvas_Native_profileMatches(mut env: JNIEnv, _: JClass, entries: JString, query: JString, language: JString) -> jstring {
    crate::jni_entry!(env, {
        let result = (|| {
            let localizer = localizer(&mut env, &language)?;
            let entries: Vec<layer_ui::profile_library::ProfileEntry> = serde_json::from_str(&read(&mut env, &entries)?).map_err(error)?;
            let query = read(&mut env, &query)?;
            serde_json::to_string(&entries.iter().filter(|entry| entry.matches(&query, &localizer)).map(|entry| &entry.id).collect::<Vec<_>>()).map_err(error)
        })();
        string(&mut env, result)
    })
}
