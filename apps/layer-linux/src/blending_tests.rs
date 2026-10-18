//! Blending with real Mutter delivery: the New drawing dialog's field, Image ›
//! Blending with its undo, and the Document Properties row, in the light and
//! dark themes.
use super::crop::fill_rect;
use super::new_drawing_review::{close_new_drawing, new_drawing_action, open_new_drawing, part};
use super::new_photo::{capture_ui, finish, response};
use super::photo_edit::{choose, document, labelled, shown, start};
use super::*;
use layer_core::BlendSpace;

fn blending(w: &Workspace) -> gtk::DropDown { part(w, "new-drawing-blending") }

#[test]
#[ignore = "isolated compositor, GPU and native mouse delivery"]
fn native_blending_new_drawing_image_menu_and_properties() {
    let (_app, w, mut input) = start("art.capycanvas.Blending");
    let directory = std::path::Path::new("../../artifacts/photo-m4/blending").join(std::process::id().to_string());
    std::fs::create_dir_all(&directory).unwrap();
    let directory = directory.canonicalize().unwrap();
    assert_eq!(document(&w).composition().blend, BlendSpace::Perceptual, "a new 8-bit drawing blends perceptually");
    fill_rect(&w, [0.2, 0.2, 0.8, 0.8]);
    w.dispatch(UiAction::SetLayerOpacity { id: None, opacity: 0.5 });
    let center = [document(&w).composition().size[0] as f32 / 2., document(&w).composition().size[1] as f32 / 2.];
    pump(300);
    let perceptual = shown(&w, center);

    choose(&w, &mut input, "Image", &["Blending", "Linear Light Blending"]);
    until(|| document(&w).composition().blend == BlendSpace::Linear, "Image › Blending › Linear Light Blending");
    pump(300);
    let linear = shown(&w, center);
    assert!(linear[0] > perceptual[0] + 15, "half-opacity paint over white is lighter in linear light: {perceptual:?} {linear:?}");
    w.dispatch(UiAction::Invoke { command: CommandId::Undo });
    until(|| document(&w).composition().blend == BlendSpace::Perceptual, "one undo step restores Perceptual");
    pump(300);
    assert_eq!(shown(&w, center), perceptual, "undo restores the composite");

    w.dispatch(UiAction::Invoke { command: CommandId::DocumentProperties });
    until(|| w.window.visible_dialog().is_some(), "Document Properties");
    let dialog = w.window.visible_dialog().unwrap();
    until(|| labelled(dialog.upcast_ref(), "Blending").is_some() && labelled(dialog.upcast_ref(), "Perceptual Blending").is_some(), "the Blending row");
    response(&w, "done");
    finish(&w);

    for theme in [Theme::Light, Theme::Dark] {
        w.dispatch(UiAction::SetTheme { theme: Some(theme) });
        pump(200);
        let name = format!("{theme:?}").to_lowercase();
        open_new_drawing(&w);
        assert_eq!(blending(&w).selected(), 0, "Perceptual by default");
        assert!(blending(&w).is_sensitive());
        capture_ui(&w, &directory, &format!("new-drawing-{name}.png"));
        new_drawing_action(&w, NewDrawingAction::Depth { depth: layer_core::color::SampleDepth::F16 });
        assert!(!blending(&w).is_sensitive(), "float documents blend in linear light");
        assert_eq!(blending(&w).selected(), 1);
        assert_eq!(blending(&w).tooltip_text().as_deref(), Some("Float documents blend in linear light"));
        pump(250);
        capture_ui(&w, &directory, &format!("new-drawing-float-{name}.png"));
        new_drawing_action(&w, NewDrawingAction::Depth { depth: layer_core::color::SampleDepth::U8 });
        assert_eq!(blending(&w).selected(), 0, "the choice returns with an 8-bit depth");
        close_new_drawing(&w, "new-drawing-cancel");
        finish(&w);
    }

    let created = Rc::new(RefCell::new(None));
    let result = created.clone();
    *w.open_document.borrow_mut() = Some(Rc::new(move |project, _| {
        result.replace(Some(project));
    }));
    open_new_drawing(&w);
    blending(&w).set_selected(1);
    pump(150);
    close_new_drawing(&w, "new-drawing-create");
    finish(&w);
    let project = created.borrow_mut().take().expect("a new drawing");
    assert_eq!(project.composition().blend, BlendSpace::Linear);
    input.finish();
    w.window.destroy();
    pump(100);
}
