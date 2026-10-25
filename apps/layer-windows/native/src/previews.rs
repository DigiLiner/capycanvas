//! Bounded binary UI queries. No GPU wait or pixel JSON.
use layer_host::NativeHost;
use serde::Deserialize;
use std::{
    ffi::CString,
    sync::Arc,
};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Query {
    filters: Vec<Arc<str>>,
    cache: layer_ui::FilterPreviewCache,
}
pub struct CapyPreview {
    metadata: CString,
    bytes: Vec<u8>,
}
pub fn query(host: &mut NativeHost, json: &str) -> Result<CapyPreview, String> {
    let query: Query = serde_json::from_str(json).map_err(|e| e.to_string())?;
    let update = host.poll_filter_previews(query.filters, query.cache)?;
    let mut metadata = serde_json::to_value(update.status).map_err(|e| e.to_string())?;
    let bytes = if let Some(atlas) = update.image {
        let image = atlas.image;
        metadata["atlas"] = serde_json::json!({
            "request":image.request_id.to_string(), "width":image.width,
            "height":image.height, "stride":image.stride, "filters":atlas.filters
        });
        image.bytes
    } else { Vec::new() };
    CapyPreview::packet(metadata, bytes)
}
pub(crate) fn store(packet:CapyPreview)->Result<u64,String> {
    layer_host::handles::insert_shared(Arc::new(packet)).map(layer_host::handles::Handle::raw).map_err(|error|error.to_string())
}
fn packet(value:u64)->Option<Arc<CapyPreview>> {
    layer_host::handles::Handle::from_raw(value).and_then(layer_host::handles::shared::<CapyPreview>).ok()
}
#[unsafe(no_mangle)]
pub extern "C" fn capy_preview_metadata(value:u64)->u64 {
    packet(value).map_or(0,|packet|crate::owned::text(packet.metadata.clone()))
}
#[unsafe(no_mangle)]
pub extern "C" fn capy_preview_length(value:u64)->usize {packet(value).map_or(0,|packet|packet.bytes.len())}
/// # Safety
/// Nonempty output is writable and exclusive for length bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn capy_preview_copy(value:u64,offset:usize,output:*mut u8,length:usize)->bool {
    packet(value).is_some_and(|packet|unsafe {layer_host::ffi::copy_bytes(&packet.bytes,offset,output,length)})
}
#[unsafe(no_mangle)]
pub extern "C" fn capy_preview_free(value:u64) {
    let _=layer_host::handles::Handle::from_raw(value).and_then(layer_host::handles::release::<CapyPreview>);
}

#[cfg(test)]
pub(crate) fn copied_bytes(value:u64)->Vec<u8> {
    let mut bytes=vec![0;capy_preview_length(value)];
    for offset in (0..bytes.len()).step_by(layer_core::package::MAX_RANGE_BYTES) {
        let count=(bytes.len()-offset).min(layer_core::package::MAX_RANGE_BYTES);
        assert!(unsafe {capy_preview_copy(value,offset,bytes.as_mut_ptr().add(offset),count)});
    }
    assert!(unsafe {capy_preview_copy(value,bytes.len(),std::ptr::null_mut(),0)});
    bytes
}
#[cfg(test)]
pub(crate) fn copied_metadata(value:u64)->serde_json::Value {
    serde_json::from_slice(crate::owned::take_text(capy_preview_metadata(value)).as_bytes()).unwrap()
}

/// # Safety
/// Data must point to length readable bytes; width and height must point to
/// writable storage. Callable from any thread; free the result with capy_preview_free.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn capy_decode_webp(data: *const u8, length: usize, width: *mut u32, height: *mut u32) -> u64 {
    let (Some(width), Some(height)) = (unsafe { width.as_mut() }, unsafe { height.as_mut() }) else { return 0 };
    if data.is_null() { return 0; }
    let Ok(([w, h], pixels)) = layer_color::photo::decode_rgba8_webp(unsafe { std::slice::from_raw_parts(data, length) }) else { return 0 };
    [*width, *height] = [w, h];
    store(CapyPreview { metadata: CString::default(), bytes: pixels }).unwrap_or(0)
}

impl CapyPreview {
    pub(crate) fn packet(metadata: serde_json::Value, bytes: Vec<u8>) -> Result<Self, String> {
        Ok(Self {
            metadata: CString::new(metadata.to_string()).map_err(|e| e.to_string())?,
            bytes,
        })
    }
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct LayerMenuQuery {
    epoch: String,
    id: Option<String>,
    mask: Option<bool>,
    #[serde(default)]
    blend: bool,
}

pub fn layer_menu(host: &mut NativeHost, json: &str) -> Result<CapyPreview, String> {
    let query: LayerMenuQuery = serde_json::from_str(json).map_err(|e| e.to_string())?;
    let epoch = host.session.state().document_file.epoch;
    let document = host.session.engine().document();
    let id = query
        .id
        .map(|id| id.parse::<u64>().map_err(|e| e.to_string()))
        .transpose()?
        .unwrap_or(document.working.occurrence.map(layer_ui::occurrence_token).unwrap_or(0));
    let mask = query.mask.unwrap_or(document.working.target.is_some_and(|t| matches!(t,layer_core::authored::SourceTarget::Coverage(_))));
    let epoch_matches = query.epoch.parse::<u64>().map_err(|e| e.to_string())? == epoch;
    let quick_mask = id == 0 && !mask && host.session.state().layer_tools.quick_mask;
    let exists = quick_mask || layer_ui::occurrence_handle(id).ok()
        .and_then(|h| document.scene().occurrence(h)).is_some_and(|o| !mask || o.mask.is_some());
    let menu = if epoch_matches && exists && query.blend {
        serde_json::to_value(host.session.layer_blend_menu(id)?).map_err(|e| e.to_string())?
    } else if epoch_matches && exists {
        serde_json::to_value(host.session.layer_menu(id, mask)?).map_err(|e| e.to_string())?
    } else {
        serde_json::Value::Null
    };
    CapyPreview::packet(
        serde_json::json!({"epoch":epoch.to_string(),"id":id.to_string(),"mask":mask,"menu":menu}),
        Vec::new(),
    )
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ThumbnailQuery {
    epoch: String,
    requests: Vec<(String, String)>,
}

impl ThumbnailQuery {
    fn requests(self, epoch: u64) -> Result<Vec<(u64, u64)>, String> {
        if self.requests.len() > 8 {
            return Err("Too many thumbnail requests".into());
        }
        if self.epoch.parse::<u64>().map_err(|e| e.to_string())? != epoch {
            return Ok(Vec::new());
        }
        self.requests
            .into_iter()
            .map(|(request, target)| {
                Ok((
                    request.parse::<u64>().map_err(|e| e.to_string())?,
                    target.parse::<u64>().map_err(|e| e.to_string())?,
                ))
            })
            .collect()
    }
}
#[cfg_attr(not(target_os = "windows"), expect(dead_code, reason = "Used by the Windows host"))]
pub fn layer_thumbnails(host: &mut NativeHost, json: &str) -> Result<CapyPreview, String> {
    let query: ThumbnailQuery = serde_json::from_str(json).map_err(|e| e.to_string())?;
    let epoch = host.session.state().document_file.epoch;
    let wait_ms = host.layer_thumbnail_wait_ms();
    let (accepted, images) = host.layer_thumbnails(query.requests(epoch)?)?;
    pack_layer_thumbnails(epoch, wait_ms, accepted, images)
}
fn pack_layer_thumbnails(
    epoch: u64,
    wait_ms: u64,
    accepted: Vec<u64>,
    images: Vec<layer_render::ReadbackImage>,
) -> Result<CapyPreview, String> {
    if images.len() > 8 {
        return Err("Oversized thumbnail completion batch".into());
    }
    let mut bytes = Vec::with_capacity(images.len() * 32 * 32 * 4);
    let mut rows = Vec::new();
    for image in images {
        if image.width != 32
            || image.height != 32
            || image.stride != 128
            || image.bytes.len() != 4096
        {
            return Err("Invalid layer thumbnail".into());
        }
        rows.push(serde_json::json!({"request":image.request_id.to_string(),
            "width":image.width,"height":image.height,"offset":bytes.len(),"length":image.bytes.len()}));
        bytes.extend(image.bytes);
    }
    CapyPreview::packet(
        serde_json::json!({"epoch":epoch.to_string(),"wait_ms":wait_ms,
        "accepted":accepted.into_iter().map(|id|id.to_string()).collect::<Vec<_>>(),"images":rows}),
        bytes,
    )
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PaperQuery {
    ids: Vec<String>,
}
pub(crate) fn paper_previews(host: &NativeHost, json: &str) -> Result<CapyPreview, String> {
    let query: PaperQuery = serde_json::from_str(json).map_err(|error| error.to_string())?;
    if query.ids.len() > 8 { return Err(std::io::Error::from(std::io::ErrorKind::InvalidInput).to_string()); }
    let mut bytes = Vec::new();
    let mut images = Vec::new();
    let size = layer_core::paper::PREVIEW_SIZE;
    for id in query.ids {
        let Some(preview) = host.session.paper_preview(&id) else { continue };
        if preview.len() != size * size { return Err(std::io::Error::from(std::io::ErrorKind::InvalidData).to_string()); }
        let offset = bytes.len();
        bytes.extend(preview.iter().flat_map(|&value| [value, value, value, 255]));
        images.push(serde_json::json!({"id":id, "offset":offset}));
    }
    CapyPreview::packet(serde_json::json!({"size":size, "images":images}), bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn preview_transport_uses_shared_identity_and_cache_bounds() {
        let mut host = NativeHost::new(layer_ui::Platform::Windows).unwrap();
        let request = |filters: Vec<&str>, rows: Vec<&str>| serde_json::json!({
            "filters": filters,
            "cache": {"key": null, "rows": rows}
        }).to_string();
        let json = request(vec!["curves"], vec![]);
        let packet = query(&mut host, &json).unwrap();
        let before: serde_json::Value = serde_json::from_str(packet.metadata.to_str().unwrap()).unwrap();
        host.session.reset_filter_previews();
        let packet = query(&mut host, &json).unwrap();
        let after: serde_json::Value = serde_json::from_str(packet.metadata.to_str().unwrap()).unwrap();
        assert_ne!(before["key"], after["key"]);
        assert!(query(&mut host, &request(vec![], vec!["curves"; 65])).is_err());
        assert!(query(&mut host, r#"{"filters":[],"request":"1"}"#).is_err());
    }
    #[test]
    fn owned_binary_lifetime() {
        let p=store(CapyPreview {metadata:CString::new("{}").unwrap(),bytes:vec![1,2,3,4]}).unwrap();
        let mut bytes=[93;4];
        assert_eq!(capy_preview_length(p),4);
        assert!(unsafe {capy_preview_copy(p,0,bytes.as_mut_ptr(),4)});
        assert_eq!(bytes,[1,2,3,4]);
        let metadata=capy_preview_metadata(p);
        capy_preview_free(metadata);
        assert_eq!(crate::owned::take_text(metadata).to_str().unwrap(),"{}");
        assert!(!unsafe {capy_preview_copy(p,usize::MAX,bytes.as_mut_ptr(),1)});
        assert!(!unsafe {capy_preview_copy(p,3,bytes.as_mut_ptr(),2)});
        capy_preview_free(p);capy_preview_free(p);
        assert_eq!(capy_preview_length(p),0);
        assert!(!unsafe {capy_preview_copy(p,0,bytes.as_mut_ptr(),0)});
        assert_eq!(capy_preview_metadata(p),0);
        assert_eq!(bytes,[1,2,3,4]);
    }
    #[test]
    fn bundled_webp_decodes_to_straight_rgba() {
        let bytes = include_bytes!("../../../layer-web/filter-previews/curves.webp");
        let (mut width, mut height) = (0, 0);
        let packet = unsafe { capy_decode_webp(bytes.as_ptr(), bytes.len(), &mut width, &mut height) };
        assert_eq!([width, height], layer_render::FILTER_PREVIEW_SIZE);
        assert_eq!(copied_bytes(packet).len(), (width * height * 4) as usize);
        capy_preview_free(packet);
        assert_eq!(capy_preview_length(packet), 0);
        assert_eq!(unsafe { capy_decode_webp(bytes.as_ptr(), 12, &mut width, &mut height) }, 0);
    }

    #[test]
    fn layer_menus_match_shared_policy_and_reject_stale_targets() {
        let mut host = NativeHost::new(layer_ui::Platform::Windows).unwrap();
        let epoch = host.session.state().document_file.epoch;
        let id = host.session.engine().document().working.occurrence.map(layer_ui::occurrence_token).unwrap();
        let query = |epoch: u64, id: u64, mask| {
            serde_json::json!({
                "epoch":epoch.to_string(),"id":id.to_string(),"mask":mask
            })
            .to_string()
        };
        let before = host.session.engine().document().revision;
        let packet = layer_menu(&mut host, &query(epoch, id, false)).unwrap();
        assert!(packet.bytes.is_empty());
        let metadata: serde_json::Value =
            serde_json::from_str(packet.metadata.to_str().unwrap()).unwrap();
        assert_eq!(
            metadata["menu"],
            serde_json::to_value(host.session.layer_menu(id, false).unwrap()).unwrap()
        );
        assert_eq!(before, host.session.engine().document().revision);
        let blend = serde_json::json!({"epoch":epoch.to_string(),"id":id.to_string(),"blend":true}).to_string();
        let metadata: serde_json::Value =
            serde_json::from_str(layer_menu(&mut host, &blend).unwrap().metadata.to_str().unwrap()).unwrap();
        assert_eq!(
            metadata["menu"],
            serde_json::to_value(host.session.layer_blend_menu(id).unwrap()).unwrap()
        );
        for json in [
            query(epoch + 1, id, false),
            query(epoch, u64::MAX, false),
            query(epoch, id, true),
        ] {
            let packet = layer_menu(&mut host, &json).unwrap();
            let metadata: serde_json::Value =
                serde_json::from_str(packet.metadata.to_str().unwrap()).unwrap();
            assert!(metadata["menu"].is_null());
        }
        host.dispatch(layer_ui::UiAction::Layer {
            action: layer_ui::LayerAction::AddMask { id, replace: false },
        })
        .unwrap();
        let packet = layer_menu(&mut host, &query(epoch, id, true)).unwrap();
        let metadata: serde_json::Value =
            serde_json::from_str(packet.metadata.to_str().unwrap()).unwrap();
        assert_eq!(
            metadata["menu"],
            serde_json::to_value(host.session.layer_menu(id, true).unwrap()).unwrap()
        );
        let current =
            serde_json::json!({"epoch":epoch.to_string(),"id":null,"mask":null}).to_string();
        let packet = layer_menu(&mut host, &current).unwrap();
        let metadata: serde_json::Value =
            serde_json::from_str(packet.metadata.to_str().unwrap()).unwrap();
        assert_eq!(metadata["id"], id.to_string());
        assert_eq!(metadata["mask"], true);
        host.dispatch(layer_ui::UiAction::Layer {
            action: layer_ui::LayerAction::New {
                group: true,
                clipped: false,
            },
        })
        .unwrap();
        let target = host.session.engine().document().working.occurrence.map(layer_ui::occurrence_token).unwrap();
        let packet = layer_menu(&mut host, &current).unwrap();
        let metadata: serde_json::Value =
            serde_json::from_str(packet.metadata.to_str().unwrap()).unwrap();
        assert_eq!(metadata["id"], target.to_string());
        assert_eq!(
            metadata["menu"],
            serde_json::to_value(host.session.layer_menu(target, false).unwrap()).unwrap()
        );
    }
    #[test]
    fn object_layers_open_their_shared_menu_and_reject_stale_layers() {
        let mut document = layer_core::Document::new(layer_core::authored::PortableId::random(), 64, 48,
            layer_core::DocumentNames { paint: "Current ink".into(), paper: "Paper".into() });
        let image = layer_core::color::source::rgba8_source([4, 3], |_, _| [10, 20, 30, 255]).into();
        let (layer, edit) = document.create_object_layer_edit("Photo", layer_core::ImageObject::new(image), None, 0).unwrap();
        document.apply(edit).unwrap();
        let gpu = layer_render_wgpu::WgpuRasterizer::new_native_headless(document.composition().color).unwrap();
        let mut host = NativeHost::new(layer_ui::Platform::Windows).unwrap();
        *host.session = layer_ui::UiSession::new(layer_host::Renderer(Some(gpu.into())), document, [64, 48], layer_ui::Platform::Windows).unwrap();
        let epoch = host.session.state().document_file.epoch;
        let id = layer_ui::occurrence_token(layer);
        let menu = |host: &mut NativeHost, json: serde_json::Value| -> serde_json::Value {
            let packet = layer_menu(host, &json.to_string()).unwrap();
            serde_json::from_str::<serde_json::Value>(packet.metadata.to_str().unwrap()).unwrap()["menu"].clone()
        };
        let before = host.session.engine().document().revision;
        assert_eq!(menu(&mut host, serde_json::json!({"epoch":epoch.to_string(),"id":id.to_string(),"mask":false})),
            serde_json::to_value(host.session.layer_menu(id, false).unwrap()).unwrap());
        assert_eq!(menu(&mut host, serde_json::json!({"epoch":epoch.to_string(),"id":id.to_string(),"blend":true})),
            serde_json::to_value(host.session.layer_blend_menu(id).unwrap()).unwrap());
        assert_eq!(before, host.session.engine().document().revision);
        for stale in [

            serde_json::json!({"epoch":(epoch + 1).to_string(),"id":id.to_string(),"mask":false}),
            serde_json::json!({"epoch":epoch.to_string(),"id":id.to_string(),"mask":true}),
            serde_json::json!({"epoch":epoch.to_string(),"id":(id + 1).to_string(),"mask":false}),
        ] {
            assert!(menu(&mut host, stale).is_null());
        }
        drop(host);
        layer_render_wgpu::finish_shader_compiler_shutdown();
    }
    #[test]
    fn quick_mask_row_menu_targets_reserved_id() {
        let mut host = NativeHost::new(layer_ui::Platform::Windows).unwrap();
        let epoch = host.session.state().document_file.epoch.to_string();
        let query = |mask: bool| serde_json::json!({"epoch":epoch,"id":"0","mask":mask}).to_string();
        let menu = |host: &mut NativeHost, mask| -> serde_json::Value {
            let packet = layer_menu(host, &query(mask)).unwrap();
            serde_json::from_str::<serde_json::Value>(packet.metadata.to_str().unwrap()).unwrap()["menu"].clone()
        };
        assert!(menu(&mut host, false).is_null());
        host.dispatch(layer_ui::UiAction::Invoke {
            command: layer_ui::CommandId::QuickMask,
        })
        .unwrap();
        assert!(host.session.state().layer_tools.quick_mask);
        assert_eq!(
            menu(&mut host, false),
            serde_json::to_value(host.session.layer_menu(0, false).unwrap()).unwrap()
        );
        assert!(menu(&mut host, false).to_string().contains("Save as Selection Layer"));
        assert!(menu(&mut host, true).is_null());
    }
    #[test]
    fn thumbnail_identity_bounds_and_epoch_are_lossless() {
        let query = || ThumbnailQuery {
            epoch: "7".into(),
            requests: vec![(u64::MAX.to_string(), (u64::MAX - 1).to_string()); 8],
        };
        assert_eq!(
            query().requests(7).unwrap(),
            vec![(u64::MAX, u64::MAX - 1); 8]
        );
        assert!(query().requests(8).unwrap().is_empty());
        let mut invalid = query();
        invalid.requests.push(("1".into(), "2".into()));
        assert!(invalid.requests(7).is_err());
        let mut invalid = query();
        invalid.requests[0].0 = "-1".into();
        assert!(invalid.requests(7).is_err());
    }
    #[test]
    fn thumbnail_packets_keep_pixels_binary_and_validate_completions() {
        let image = layer_render::ReadbackImage {
            request_id: u64::MAX,
            width: 32,
            height: 32,
            stride: 128,
            bytes: vec![17; 4096],
        };
        let packet =
            pack_layer_thumbnails(u64::MAX, 0, vec![u64::MAX], vec![image.clone(), image.clone()])
                .unwrap();
        let metadata: serde_json::Value =
            serde_json::from_str(packet.metadata.to_str().unwrap()).unwrap();
        assert_eq!(metadata["epoch"], u64::MAX.to_string());
        assert_eq!(metadata["wait_ms"], 0);
        let deferred = pack_layer_thumbnails(0, 50, vec![], vec![]).unwrap();
        let deferred: serde_json::Value = serde_json::from_str(deferred.metadata.to_str().unwrap()).unwrap();
        assert_eq!(deferred["wait_ms"], 50);
        assert_eq!(metadata["accepted"][0], u64::MAX.to_string());
        assert_eq!(metadata["images"][1]["offset"], 4096);
        assert_eq!(packet.bytes, vec![17; 8192]);
        assert!(metadata.to_string().len() < 512);
        assert!(pack_layer_thumbnails(0, 0, vec![], vec![image.clone(); 9]).is_err());
        for field in 0..4 {
            let mut invalid = image.clone();
            match field {
                0 => invalid.width = 31,
                1 => invalid.height = 33,
                2 => invalid.stride = 256,
                _ => {
                    invalid.bytes.pop();
                }
            }
            assert!(pack_layer_thumbnails(0, 0, vec![], vec![invalid]).is_err());
        }
    }
    #[test]
    fn paper_packets_are_bounded_binary_and_skip_unknown_papers() {
        let host = NativeHost::new(layer_ui::Platform::Windows).unwrap();
        let expected = host.session.paper_preview("rough").unwrap();
        let query = serde_json::json!({"ids":["missing", "rough"]}).to_string();
        let packet = paper_previews(&host, &query).unwrap();
        let metadata: serde_json::Value = serde_json::from_str(packet.metadata.to_str().unwrap()).unwrap();
        assert_eq!(metadata["images"], serde_json::json!([{"id":"rough","offset":0}]));
        assert_eq!(packet.bytes.len(), expected.len()*4);
        for (pixel, value) in packet.bytes.as_chunks::<4>().0.iter().zip(expected.iter()) { assert_eq!(pixel, &[*value,*value,*value,255]); }
        for query in [serde_json::json!({"ids":vec!["rough";9]}),serde_json::json!({"ids":["rough"],"extra":true}),serde_json::json!({"indices":[0]})] {
            assert!(paper_previews(&host, &query.to_string()).is_err());
        }
    }

}
