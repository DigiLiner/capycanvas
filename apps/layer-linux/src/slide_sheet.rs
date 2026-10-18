//! A sheet that slides up over a dialog's page and covers it.
use adw::prelude::*;
use gtk::{gdk, glib};
use std::{cell::Cell, rc::Rc};

pub(crate) struct SlideSheet {
    pub sheet: gtk::Widget,
    open: Cell<bool>,
    progress: Rc<Cell<f64>>,
    slide: adw::TimedAnimation,
}
impl SlideSheet {
    pub fn new(overlay: &gtk::Overlay, sheet: &impl IsA<gtk::Widget>) -> Rc<Self> {
        let sheet = sheet.clone().upcast::<gtk::Widget>();
        sheet.set_visible(false);
        overlay.add_overlay(&sheet);
        overlay.set_clip_overlay(&sheet, true);
        let progress = Rc::new(Cell::new(0.));
        let target = sheet.downgrade();
        overlay.connect_get_child_position(glib::clone!(#[strong] progress, move |overlay, child| {
            if target.upgrade().as_ref() != Some(child) { return None; }
            let offset = ((1. - progress.get()) * f64::from(overlay.height())).round() as i32;
            Some(gdk::Rectangle::new(0, offset, overlay.width(), overlay.height()))
        }));
        let slide = adw::TimedAnimation::new(overlay, 0., 1., 250, adw::CallbackAnimationTarget::new(glib::clone!(#[strong] progress, #[weak] overlay, move |value| {
            progress.set(value);
            overlay.queue_allocate();
        })));
        slide.set_easing(adw::Easing::EaseOutCubic);
        let this = Rc::new(Self { sheet, open: Cell::new(false), progress, slide });
        let weak = Rc::downgrade(&this);
        this.slide.connect_done(move |_| {
            if let Some(this) = weak.upgrade().filter(|this| !this.open.get()) { this.sheet.set_visible(false); }
        });
        this
    }
    pub fn is_open(&self) -> bool { self.open.get() }
    pub fn show(&self, open: bool) -> bool {
        if self.open.replace(open) == open { return false; }
        if open { self.sheet.set_visible(true); }
        self.slide.set_value_from(self.progress.get());
        self.slide.set_value_to(if open { 1. } else { 0. });
        self.slide.play();
        true
    }
}
