//! Bundled real-engine swatches, shared byte-for-byte with the browser host.
use gtk::{gdk, glib};
use layer_ui::Theme;

macro_rules! swatches {
    ($theme:literal) => {
        swatches!(
            $theme; 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22,
            23, 24, 25, 26, 27, 28, 29, 30, 31, 32, 33, 34, 35, 36, 37, 38, 39, 40, 41, 42, 43,
            44, 45, 46, 47, 48, 49, 50, 51, 52, 53, 54, 55, 56, 57, 58, 59, 60, 61, 62
        )
    };
    ($theme:literal; $($id:literal),*) => {
        [$((
            $id,
            include_bytes!(concat!("../../layer-web/brush-previews/", $id, "-", $theme, ".png"))
                as &'static [u8],
        )),*]
    };
}
fn bytes(id: u32, theme: Theme) -> Option<&'static [u8]> {
    let images = if theme == Theme::Dark {
        swatches!("dark")
    } else {
        swatches!("light")
    };
    images.into_iter().find(|(preset, _)| *preset == id).map(|(_, bytes)| bytes)
}
pub fn texture(id: u32, theme: Theme) -> gdk::Texture {
    let bytes = bytes(id, theme).expect("every brush preset has a bundled preview");
    gdk::Texture::from_bytes(&glib::Bytes::from_static(bytes))
        .expect("bundled brush preview is a valid PNG")
}

#[cfg(test)]
mod tests {
    #[test]
    fn every_brush_preset_has_a_bundled_preview_in_both_themes() {
        for choice in layer_ui::brush_catalog() {
            for theme in [layer_ui::Theme::Light, layer_ui::Theme::Dark] {
                assert!(super::bytes(choice.id, theme).is_some(), "preset {} has no {theme:?} preview", choice.id);
            }
        }
    }
}
