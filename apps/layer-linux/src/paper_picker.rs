use adw::prelude::*;
use gtk::{gdk, glib};
use layer_ui::{EffectAction, PaperPickerView};
use std::{cell::{Cell, RefCell}, rc::Rc, sync::Arc};

const TILE: i32 = 72;
const THUMBNAIL: i32 = 28;

pub struct PaperPicker {
    pub button: gtk::MenuButton,
    thumbnail: gtk::Image,
    name: gtk::Label,
    popover: gtk::Popover,
    grid: gtk::FlowBox,
    menu: gtk::Popover,
    view: RefCell<Option<PaperPickerView>>,
    selected: Cell<Option<u32>>,
    act: Rc<dyn Fn(EffectAction)>,
}
impl Drop for PaperPicker {
    fn drop(&mut self) {
        if self.menu.parent().is_some() { self.menu.unparent(); }
    }
}

fn texture(preview: &Arc<[u8]>) -> gdk::Texture {
    let size = layer_core::paper::PREVIEW_SIZE;
    gdk::MemoryTexture::new(size as i32, size as i32, gdk::MemoryFormat::G8, &glib::Bytes::from(&preview[..]), size).upcast()
}

fn picture(preview: Option<&Arc<[u8]>>, size: i32) -> gtk::Image {
    let image = match preview {
        Some(preview) => gtk::Image::from_paintable(Some(&texture(preview))),
        None => crate::icons::image("layer-fill-symbolic"),
    };
    image.set_pixel_size(size);
    image.add_css_class("paper-preview");
    image
}

impl PaperPicker {
    pub fn new(select: impl Fn(u32) + 'static, act: impl Fn(EffectAction) + 'static) -> Rc<Self> {
        let thumbnail = picture(None, THUMBNAIL);
        let name = gtk::Label::builder().xalign(0.).hexpand(true).ellipsize(gtk::pango::EllipsizeMode::End).build();
        let content = gtk::Box::new(gtk::Orientation::Horizontal, crate::panel_controls::CONTROL_SPACING);
        content.append(&thumbnail);
        content.append(&name);
        let grid = gtk::FlowBox::builder()
            .selection_mode(gtk::SelectionMode::Single)
            .activate_on_single_click(true)
            .homogeneous(true)
            .min_children_per_line(4)
            .max_children_per_line(4)
            .row_spacing(6)
            .column_spacing(6)
            .build();
        grid.add_css_class("paper-picker");
        let scroller = crate::input::pen_scroller(gtk::ScrolledWindow::builder()
            .hscrollbar_policy(gtk::PolicyType::Never)
            .propagate_natural_height(true)
            .max_content_height(520)
            .child(&grid)
            .build());
        let popover = gtk::Popover::builder().child(&scroller).build();
        let button = gtk::MenuButton::builder().child(&content).popover(&popover).hexpand(true).build();
        button.set_widget_name("property-texture");
        let menu = gtk::Popover::builder().has_arrow(false).build();
        menu.set_widget_name("paper-tile-menu");
        let picker = Rc::new(Self { button, thumbnail, name, popover, grid, menu, view: RefCell::default(), selected: Cell::new(None), act: Rc::new(act) });
        picker.grid.connect_child_activated(glib::clone!(#[weak] picker, move |_, child| {
            picker.popover.popdown();
            let Ok(index) = usize::try_from(child.index()) else { return };
            let import = picker.view.borrow().as_ref().filter(|view| index == view.tiles.len()).and_then(|view| view.import.clone());
            match import {
                Some(import) => (picker.act)(import.action),
                None => select(index as u32),
            }
        }));
        picker
    }

    fn open_menu(self: &Rc<Self>, index: usize) {
        let Some(actions) = self.view.borrow().as_ref().and_then(|view| view.tiles.get(index)).map(|tile| tile.actions.clone()) else { return };
        let Some(child) = self.grid.child_at_index(index as i32).filter(|_| !actions.is_empty()) else { return };
        let items = gtk::Box::new(gtk::Orientation::Vertical, 0);
        for action in actions {
            let button = gtk::Button::with_label(&action.label);
            button.add_css_class("flat");
            button.connect_clicked(glib::clone!(#[weak(rename_to = picker)] self, move |_| {
                picker.menu.popdown();
                picker.popover.popdown();
                (picker.act)(action.action.clone());
            }));
            items.append(&button);
        }
        self.menu.set_child(Some(&items));
        if self.menu.parent().as_ref() != Some(child.upcast_ref()) {
            if self.menu.parent().is_some() { self.menu.unparent(); }
            self.menu.set_parent(&child);
        }
        self.menu.popup();
    }

    fn tile(&self, label: &str, content: &gtk::Box) -> gtk::FlowBoxChild {
        let caption = gtk::Label::builder().label(label).wrap(true).lines(2).justify(gtk::Justification::Center)
            .ellipsize(gtk::pango::EllipsizeMode::End).width_chars(9).max_width_chars(9).build();
        caption.add_css_class("caption");
        content.append(&caption);
        let child = gtk::FlowBoxChild::builder().child(content).tooltip_text(label).build();
        child.update_property(&[gtk::accessible::Property::Label(label)]);
        self.grid.append(&child);
        child
    }

    pub fn update(self: &Rc<Self>, view: &PaperPickerView, selected: u32, label: &str) {
        if self.view.borrow().as_ref() != Some(view) {
            self.menu.popdown();
            if self.menu.parent().is_some() { self.menu.unparent(); }
            self.grid.remove_all();
            for tile in &view.tiles {
                let content = gtk::Box::new(gtk::Orientation::Vertical, 4);
                content.append(&picture(tile.preview.as_ref(), TILE));
                let child = self.tile(&tile.label, &content);
                if !tile.actions.is_empty() {
                    let index = self.grid.observe_children().n_items() as usize - 1;
                    let click = gtk::GestureClick::builder().button(3).build();
                    click.connect_pressed(glib::clone!(#[weak(rename_to = picker)] self, move |_, _, _, _| picker.open_menu(index)));
                    child.add_controller(click);
                    let hold = gtk::GestureLongPress::new();
                    hold.connect_pressed(glib::clone!(#[weak(rename_to = picker)] self, move |_, _, _| picker.open_menu(index)));
                    child.add_controller(hold);
                }
            }
            if let Some(import) = &view.import {
                let content = gtk::Box::new(gtk::Orientation::Vertical, 4);
                let icon = crate::icons::image(import.icon.as_deref().unwrap_or("layer-folder-open-symbolic"));
                icon.set_pixel_size(TILE / 2);
                icon.set_size_request(TILE, TILE);
                content.append(&icon);
                self.tile(&import.label, &content).set_widget_name("paper-import");
            }
            *self.view.borrow_mut() = Some(view.clone());
            self.selected.set(None);
        }
        let tile = view.tiles.get(selected as usize);
        match self.grid.child_at_index(selected as i32) {
            Some(child) => self.grid.select_child(&child),
            None => self.grid.unselect_all(),
        }
        if self.selected.replace(Some(selected)) != Some(selected) {
            match tile.and_then(|tile| tile.preview.as_ref()) {
                Some(preview) => self.thumbnail.set_paintable(Some(&texture(preview))),
                None => crate::icons::set(&self.thumbnail, Some("layer-fill-symbolic")),
            }
        }
        self.name.set_label(tile.map_or("", |tile| tile.label.as_ref()));
        self.button.set_tooltip_text(Some(label));
        self.button.update_property(&[gtk::accessible::Property::Label(label),
            gtk::accessible::Property::Description(tile.map_or("", |tile| tile.label.as_ref()))]);
    }
}
