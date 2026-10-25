//! Histogram, Waveform and tonal plots for the photo scope panels and Properties.
use super::*;
use layer_ui::HistogramView;

#[derive(Default)]
pub(crate) struct Cache {
    views: [Option<HistogramView>; 3],
    colors: Option<[[u8; 3]; 4]>,
    identity: Option<(u64,u64)>,
}
pub struct CapyScopes {
    plots: CString,
    pixels: Vec<u8>,
}
#[derive(Default)]
#[repr(C)]
pub struct CapyScopeInfo {
    pub plots: u64,
    pub count: usize,
}

/// # Safety
/// Call on the serial editor owner. The returned plots hold no editor references.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn capy_apple_take_scopes(app: u64) -> u64 {
    let Some(mut lease_app)=handles::app(app) else { return 0; };
    let app=&mut *lease_app;
    app.perform(|app| {
        let state = app.host.session.state();
        let identity=(app.window.documents.selected(),state.document_file.epoch);
        let colors = state.palette.histogram_colors().map(|color| color.0);
        let changed = app.scopes.colors != Some(colors) || app.scopes.identity != Some(identity);
        let mut plots = serde_json::Map::new();
        let mut pixels = Vec::new();
        for (index, (name, view)) in [("histogram", &state.histogram), ("waveform", &state.waveform), ("tonal_histogram", &state.tonal_histogram)].into_iter().enumerate() {
            if !changed && app.scopes.views[index].as_ref().is_some_and(|old| old.channel == view.channel && old.logarithmic == view.logarithmic
                && old.data.as_ref().map(std::sync::Arc::as_ptr) == view.data.as_ref().map(std::sync::Arc::as_ptr)) { continue; }
            let mut value = serde_json::json!({"plot": if index == 1 { Vec::new() } else { view.histogram_plot() }});
            if index == 1 {
                let image = view.waveform_premultiplied_rgba(colors);
                value["extent"] = serde_json::json!(image.as_ref().map(|(extent, _)| *extent));
                pixels = image.map(|(_, bytes)| bytes).unwrap_or_default();
            }
            plots.insert(name.into(), value);
            app.scopes.views[index] = Some(view.clone());
        }
        app.scopes.colors = Some(colors);
        app.scopes.identity=Some(identity);
        if plots.is_empty() { return Ok(0); }
        let plots = CString::new(serde_json::json!({"plots": plots, "colors": colors,"identity":format!("{}:{}",identity.0,identity.1)}).to_string()).map_err(|e| e.to_string())?;
        Ok(handles::store(CapyScopes { plots, pixels }))
    }).unwrap_or(0)
}

/// # Safety
/// Scopes must be live; output must be aligned and exclusively writable.
/// Metadata text is owned; pixel copies retain the source for each call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn capy_scopes_read(scopes: u64, output: *mut CapyScopeInfo)->bool {
    if output.is_null() {return false;}
    unsafe {output.write(CapyScopeInfo::default());}
    let Some(lease_scopes)=handles::shared::<CapyScopes>(scopes) else { return false; };
    let scopes=&*lease_scopes;
    unsafe { output.write(CapyScopeInfo { plots: handles::text(scopes.plots.clone()), count: scopes.pixels.len() }); }
    true
}

/// # Safety
/// Nonempty output is writable and exclusive for length bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn capy_scopes_copy(scopes:u64,offset:usize,output:*mut u8,length:usize)->bool {
    handles::shared::<CapyScopes>(scopes).is_some_and(|scopes|
        unsafe {layer_host::ffi::copy_bytes(&scopes.pixels,offset,output,length)})
}

/// # Safety
/// Retire the handle after its worker calls finish.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn capy_scopes_free(scopes: u64) {
    handles::free::<CapyScopes>(scopes);
}
