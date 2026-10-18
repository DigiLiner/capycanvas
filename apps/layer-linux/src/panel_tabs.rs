//! Retained tab buttons adapt to their actual allocation and native font metrics.
//! Shared Rust chooses which complete labels fit; native input targets survive.
use gtk::{glib, prelude::*, subclass::prelude::*};
use layer_ui::TabStyle;
use std::cell::{Cell, RefCell};
pub(crate) const TAB_RADIUS: f32 = layer_ui::SURFACE_RADIUS * crate::squircle::CORNER_FIT;
mod imp {
    use super::*;
    #[derive(Default)]
    pub struct Strip {
        pub buttons: RefCell<Vec<gtk::Button>>,
        pub style: Cell<TabStyle>,
        pub tints: RefCell<Option<[gtk::Box; 2]>>,
        pub feet: Cell<bool>,
    }
    #[glib::object_subclass]
    impl ObjectSubclass for Strip {
        const NAME: &'static str = "LayerPanelTabs";
        type Type = super::PanelTabs;
        type ParentType = gtk::Widget;
    }
    impl ObjectImpl for Strip {
        fn dispose(&self) {
            for button in self.buttons.take() {
                button.unparent();
            }
            for tint in self.tints.take().into_iter().flatten() {
                tint.unparent();
            }
        }
    }
    impl WidgetImpl for Strip {
        fn measure(&self, axis: gtk::Orientation, size: i32) -> (i32, i32, i32, i32) {
            if axis == gtk::Orientation::Horizontal && self.style.get() == TabStyle::Automatic {
                let widths = self.obj().widths();
                return (
                    widths.iter().map(|w| w[1]).sum::<f32>().ceil() as i32,
                    widths.iter().map(|w| w[0]).sum::<f32>().ceil() as i32,
                    -1,
                    -1,
                );
            }
            let mut min = 0;
            let mut natural = 0;
            for button in self.buttons.borrow().iter() {
                let (m, n, _, _) = button.measure(axis, size);
                if axis == gtk::Orientation::Horizontal {
                    min += m;
                    natural += n;
                } else {
                    min = min.max(m);
                    natural = natural.max(n);
                }
            }
            (min, natural, -1, -1)
        }
        fn size_allocate(&self, width: i32, height: i32, _: i32) {
            let widths = self.obj().widths();
            let automatic = self.style.get() == TabStyle::Automatic;
            let names = TabStyle::automatic_names(width as f32, &widths);
            let mut x = 0.;
            for (i, button) in self.buttons.borrow().iter().enumerate() {
                let w = if automatic {
                    let content = button.child().unwrap();
                    content.first_child().unwrap().set_visible(true);
                    content.last_child().unwrap().set_visible(names[i]);
                    if names[i] {
                        button.remove_css_class("icon-only-tab");
                    } else {
                        button.add_css_class("icon-only-tab");
                    }
                    widths[i][usize::from(!names[i])]
                } else {
                    button.measure(gtk::Orientation::Horizontal, -1).1 as f32
                };
                let end = x + w;
                button.allocate(
                    (end.round() - x.round()) as i32,
                    height,
                    -1,
                    Some(
                        gtk::gsk::Transform::new()
                            .translate(&gtk::graphene::Point::new(x.round(), 0.)),
                    ),
                );
                x = end;
            }
            for tint in self.tints.borrow().iter().flatten() {
                tint.allocate(0, 0, -1, None);
            }
        }
        fn snapshot(&self, snapshot: &gtk::Snapshot) {
            let obj = self.obj();
            let buttons = self.buttons.borrow();
            let bounds = gtk::graphene::Rect::new(0., 0., obj.width() as f32, obj.height() as f32);
            let Some([strip, foot]) = self.tints.borrow().clone() else { return };
            let active = buttons.iter().find(|b| b.has_css_class("selected-tool") && b.opacity() > 0.)
                .and_then(|tab| tab.compute_bounds(&*obj));
            let feet = active.filter(|_| self.feet.get());
            let draw_feet = |color: &gtk::gdk::RGBA| {
                if let Some(tab) = feet {
                    let cr = snapshot.append_cairo(&bounds);
                    cr.set_source_rgba(color.red().into(), color.green().into(), color.blue().into(), color.alpha().into());
                    super::feet(&cr, &tab, bounds.height());
                    let _ = cr.fill();
                }
            };
            snapshot.push_mask(gtk::gsk::MaskMode::InvertedAlpha);
            if let Some(tab) = active {
                let radius = gtk::graphene::Size::new(TAB_RADIUS, TAB_RADIUS);
                let square = gtk::graphene::Size::new(0., 0.);
                snapshot.push_rounded_clip(&gtk::gsk::RoundedRect::new(tab, radius, radius, square, square));
                snapshot.append_color(&gtk::gdk::RGBA::BLACK, &tab);
                snapshot.pop();
            }
            draw_feet(&gtk::gdk::RGBA::BLACK);
            snapshot.pop();
            snapshot.append_color(&strip.color(), &bounds);
            snapshot.pop();
            draw_feet(&foot.color());
            for button in buttons.iter() {
                obj.snapshot_child(button, snapshot);
            }
        }
    }
}
glib::wrapper! {
    pub struct PanelTabs(ObjectSubclass<imp::Strip>)
        @extends gtk::Widget, @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget;
}
impl PanelTabs {
    pub fn new(style: TabStyle) -> Self {
        let tabs: Self = glib::Object::new();
        tabs.imp().style.set(style);
        tabs.set_hexpand(true);
        let tints = ["tab-strip-tint", "tab-foot-tint"].map(|class| {
            let tint = gtk::Box::new(gtk::Orientation::Horizontal, 0);
            tint.add_css_class(class);
            tint.set_can_target(false);
            tint.set_parent(&tabs);
            tint
        });
        *tabs.imp().tints.borrow_mut() = Some(tints);
        tabs
    }
    pub fn with_feet(self) -> Self {
        self.imp().feet.set(true);
        self
    }
    pub fn set_style(&self, style: TabStyle) {
        if self.imp().style.replace(style) != style {
            self.queue_resize();
        }
    }
    pub fn append(&self, button: &gtk::Button) {
        button.set_parent(self);
        self.imp().buttons.borrow_mut().push(button.clone());
        self.queue_resize();
    }
    fn widths(&self) -> Vec<[f32; 2]> {
        self.imp()
            .buttons
            .borrow()
            .iter()
            .map(|button| {
                let content = button.child().unwrap().downcast::<gtk::Box>().unwrap();
                let icon = content.first_child().unwrap();
                let label = content
                    .last_child()
                    .unwrap()
                    .downcast::<gtk::Label>()
                    .unwrap();
                let text = label.layout().pixel_size().0;
                let face = content.measure(gtk::Orientation::Horizontal, -1).0;
                let minimum = if button.has_css_class("icon-only-tab") {
                    20
                } else {
                    0
                };
                let inset =
                    (button.measure(gtk::Orientation::Horizontal, -1).0 - face.max(minimum)).max(0);
                let icon = icon.measure(gtk::Orientation::Horizontal, -1).1.max(14);
                [
                    (inset + icon + content.spacing() + text) as f32,
                    (inset + icon.max(20)) as f32,
                ]
            })
            .collect()
    }
}

pub(crate) fn feet(cr: &gtk::cairo::Context, tab: &gtk::graphene::Rect, bottom: f32) {
    for (x, direction) in [(tab.x(), -1.), (tab.x() + tab.width(), 1.)] {
        crate::squircle::concave_foot(cr, f64::from(x), f64::from(bottom), 6., direction);
    }
}

pub(crate) fn strip_end(child: &impl IsA<gtk::Widget>) -> gtk::Box {
    let end = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    end.add_css_class("tab-strip-end");
    end.set_halign(gtk::Align::End);
    end.append(child);
    end
}
