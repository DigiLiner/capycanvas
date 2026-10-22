//! Native presentation of shared tool bubbles. Shared Rust owns their sources,
//! taps, rings, physics and menus; this module draws the faces and forwards
//! contacts in workspace coordinates.
use super::{Workspace, allocate_at};
use adw::prelude::*;
use gtk::{gdk, glib, graphene, gsk, subclass::prelude::*};
use layer_ui::*;
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};

const FACE_ICON: i32 = 24;
const RING_ICON: f32 = 16.;
const TRACK: f32 = 4.;
const SECOND_FINGER: f32 = 90.;

#[derive(Clone, Copy, Debug, PartialEq)]
enum Contact {
    Pending([f32; 2]),
    Moving,
    Ring,
    Beads { strip: Option<BeadStrip>, last: f32, scrolled: bool },
    Pair,
}

mod imp {
    use super::*;

    #[derive(Default)]
    pub struct Layer {
        pub(super) placements: RefCell<Vec<BubblePlacement>>,
    }
    #[glib::object_subclass]
    impl ObjectSubclass for Layer {
        const NAME: &'static str = "CapyBubbleLayer";
        type Type = super::Layer;
        type ParentType = gtk::Widget;
    }
    impl ObjectImpl for Layer {
        fn dispose(&self) {
            while let Some(child) = self.obj().first_child() {
                child.unparent();
            }
        }
    }
    impl WidgetImpl for Layer {
        fn measure(&self, _: gtk::Orientation, _: i32) -> (i32, i32, i32, i32) {
            (0, 0, -1, -1)
        }
        fn size_allocate(&self, _: i32, _: i32, _: i32) {
            let placements = self.placements.borrow();
            let mut child = self.obj().first_child();
            while let Some(widget) = child {
                child = widget.next_sibling();
                let bubble = widget.downcast_ref::<super::Bubble>().unwrap();
                let placement = placements.iter().find(|p| p.id == bubble.id()).copied();
                widget.set_child_visible(placement.is_some());
                if let Some(p) = placement {
                    bubble.set_placement(p);
                    let extent = bubble.extent();
                    allocate_at(&widget, Bounds {
                        x: p.center[0] - extent * 0.5,
                        y: p.center[1] - extent * 0.5,
                        width: extent,
                        height: extent,
                    });
                }
            }
        }
        fn contains(&self, _: f64, _: f64) -> bool {
            false
        }
    }

    #[derive(Default)]
    pub struct Bubble {
        pub(super) view: RefCell<Option<BubbleView>>,
        pub(super) face: RefCell<Option<gtk::Box>>,
        pub(super) icon: RefCell<Option<gtk::Image>>,
        pub(super) preview: RefCell<Option<gtk::Picture>>,
        pub(super) value: RefCell<Option<gtk::Label>>,
        pub(super) wheel: RefCell<Option<crate::tool_panels::ColorWheel>>,
        pub(super) icons: RefCell<Vec<gtk::Image>>,
        pub(super) beads: RefCell<Vec<crate::display_color::ColorPatch>>,
        pub(super) palettes: RefCell<Option<gtk::Image>>,
        pub(super) pill: RefCell<Option<(gtk::Box, gtk::Label)>>,
        pub(super) display: Cell<Option<(crate::display_color::ViewColor, f32)>>,
        pub(super) readout: RefCell<Option<gtk::Label>>,
        pub(super) placement: Cell<Option<BubblePlacement>>,
        pub(super) colors: Cell<Option<[gdk::RGBA; 3]>>,
        pub(super) contact: Cell<Option<Contact>>,
        pub(super) touches: Cell<u32>,
        pub(super) pinch: Cell<Option<f32>>,
    }
    #[glib::object_subclass]
    impl ObjectSubclass for Bubble {
        const NAME: &'static str = "CapyToolBubble";
        type Type = super::Bubble;
        type ParentType = gtk::Widget;
        fn class_init(klass: &mut Self::Class) {
            klass.set_accessible_role(gtk::AccessibleRole::Button);
        }
    }
    impl ObjectImpl for Bubble {
        fn dispose(&self) {
            while let Some(child) = self.obj().first_child() {
                child.unparent();
            }
        }
    }
    impl WidgetImpl for Bubble {
        fn measure(&self, _: gtk::Orientation, _: i32) -> (i32, i32, i32, i32) {
            (0, 0, -1, -1)
        }
        fn size_allocate(&self, width: i32, height: i32, _: i32) {
            let view = self.view.borrow();
            let Some(view) = view.as_ref() else { return };
            let center = [width as f32 * 0.5, height as f32 * 0.5];
            let square = |size: f32, at: [f32; 2]| Bounds { x: at[0] - size * 0.5, y: at[1] - size * 0.5, width: size, height: size };
            if let Some(face) = self.face.borrow().as_ref() {
                allocate_at(face.upcast_ref(), square(view.diameter, center));
            }
            for (icon, ring) in self.icons.borrow().iter().zip(&view.rings) {
                let [x, y] = ring.arc.point(0.);
                allocate_at(icon.upcast_ref(), square(RING_ICON, [center[0] + x, center[1] + y]));
            }
            let beads = view.beads.iter().filter(|b| b.placement.bead != Bead::Palettes);
            for (patch, bead) in self.beads.borrow().iter().zip(beads) {
                let [x, y] = bead.placement.center;
                allocate_at(patch.upcast_ref(), square(BUBBLE_BEAD - 4., [center[0] + x, center[1] + y]));
            }
            if let (Some(icon), Some(bead)) = (self.palettes.borrow().as_ref(), view.beads.iter().find(|b| b.placement.bead == Bead::Palettes)) {
                let [x, y] = bead.placement.center;
                allocate_at(icon.upcast_ref(), square(RING_ICON, [center[0] + x, center[1] + y]));
            }
            if let Some((pill, _)) = self.pill.borrow().as_ref() {
                let b = palette_pill(view.diameter);
                allocate_at(pill.upcast_ref(), Bounds { x: center[0] + b.x, y: center[1] + b.y, ..b });
            }
            if let Some(readout) = self.readout.borrow().as_ref() {
                let (_, natural, _, _) = readout.measure(gtk::Orientation::Horizontal, -1);
                let (_, tall, _, _) = readout.measure(gtk::Orientation::Vertical, natural);
                allocate_at(readout.upcast_ref(), Bounds {
                    x: center[0] - natural as f32 * 0.5,
                    y: center[1] - tall as f32 * 0.5,
                    width: natural as f32,
                    height: tall as f32,
                });
            }
        }
        fn contains(&self, x: f64, y: f64) -> bool {
            let view = self.view.borrow();
            let Some(view) = view.as_ref() else { return false };
            let obj = self.obj();
            let local = [x as f32 - obj.width() as f32 * 0.5, y as f32 - obj.height() as f32 * 0.5];
            let pill = palette_pill(view.diameter);
            let second = if self.touches.get() > 0 { SECOND_FINGER } else { 0. };
            local[0].hypot(local[1]) <= self.obj().reach(view) + second
                || (view.pill.is_some() && pill.contains(local[0], local[1]))
        }
        fn snapshot(&self, snapshot: &gtk::Snapshot) {
            let obj = self.obj();
            let view = self.view.borrow();
            let Some(view) = view.as_ref() else { return };
            let center = [obj.width() as f32 * 0.5, obj.height() as f32 * 0.5];
            let placement = self.placement.get();
            let homing = placement.is_some_and(|p| p.homing);
            let scale = self.pinch.get().unwrap_or(1.)
                * if homing { 0.82 } else if placement.is_some_and(|p| p.lifted) { 1.04 } else { 1. };
            snapshot.save();
            snapshot.translate(&graphene::Point::new(center[0], center[1]));
            snapshot.scale(scale, scale);
            snapshot.translate(&graphene::Point::new(-center[0], -center[1]));
            if homing {
                snapshot.push_opacity(0.55);
            }
            if let Some(face) = self.face.borrow().as_ref() {
                let round = gtk::Snapshot::new();
                obj.snapshot_child(face, &round);
                if view.kind == BubbleKind::Layers {
                    if let Some(node) = round.to_node() {
                        snapshot.append_node(&node);
                    }
                } else {
                    crate::squircle::append_round(snapshot, round);
                }
            }
            if let Some(colors) = self.colors.get() {
                for bead in &view.beads {
                    super::bead(snapshot, center, bead, colors);
                }
                if view.kind == BubbleKind::Medium {
                    super::dots(snapshot, center, view, colors[1]);
                }
                for ring in &view.rings {
                    super::ring(snapshot, center, ring, colors, view.kind != BubbleKind::Slider);
                }
            }
            for icon in self.icons.borrow().iter().take(view.rings.len()) {
                obj.snapshot_child(icon, snapshot);
            }
            for patch in self.beads.borrow().iter() {
                obj.snapshot_child(patch, snapshot);
            }
            if let Some(icon) = self.palettes.borrow().as_ref() {
                obj.snapshot_child(icon, snapshot);
            }
            if let Some((pill, _)) = self.pill.borrow().as_ref() {
                let round = gtk::Snapshot::new();
                obj.snapshot_child(pill, &round);
                crate::squircle::append_round(snapshot, round);
            }
            if let Some(readout) = self.readout.borrow().as_ref() {
                obj.snapshot_child(readout, snapshot);
            }
            if homing {
                snapshot.pop();
            }
            snapshot.restore();
        }
    }
}

glib::wrapper! {
    pub struct Layer(ObjectSubclass<imp::Layer>) @extends gtk::Widget,
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget;
}
glib::wrapper! {
    pub struct Bubble(ObjectSubclass<imp::Bubble>) @extends gtk::Widget,
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget;
}

fn arc_path(center: [f32; 2], arc: &RingArc, from: f32, to: f32) -> gsk::Path {
    let builder = gsk::PathBuilder::new();
    let [x0, y0] = arc.point(from);
    builder.move_to(center[0] + x0, center[1] + y0);
    let sweep = arc.sweep * to;
    let mut done = arc.sweep * from;
    while (sweep - done).abs() > 1e-3 {
        let step = (sweep - done).clamp(-179., 179.);
        done += step;
        let [x, y] = arc.point(done / arc.sweep);
        builder.svg_arc_to(arc.radius, arc.radius, 0., false, step > 0., center[0] + x, center[1] + y);
    }
    builder.to_path()
}

fn ring(snapshot: &gtk::Snapshot, center: [f32; 2], ring: &BubbleRing, [chip, ink, accent]: [gdk::RGBA; 3], band: bool) {
    let stroke = |width: f32| {
        let stroke = gsk::Stroke::new(width);
        stroke.set_line_cap(gsk::LineCap::Round);
        stroke
    };
    if band {
        snapshot.append_stroke(&arc_path(center, &ring.arc, 0., 1.), &stroke(ring.arc.width + 4.), &chip);
    }
    let start = if band { (RING_ICON * 0.5 + 3.) / (ring.arc.radius * ring.arc.sweep.abs().to_radians()) } else { 0. };
    let groove = gdk::RGBA::new(ink.red(), ink.green(), ink.blue(), 0.18);
    snapshot.append_stroke(&arc_path(center, &ring.arc, start, 1.), &stroke(TRACK), &groove);
    if ring.fill > start + 0.002 {
        snapshot.append_stroke(&arc_path(center, &ring.arc, start, ring.fill), &stroke(TRACK), &accent);
    }
    let [x, y] = ring.arc.point(ring.fill);
    let thumb = gsk::PathBuilder::new();
    thumb.add_circle(&graphene::Point::new(center[0] + x, center[1] + y), if ring.active { 6. } else { 5. });
    let thumb = thumb.to_path();
    snapshot.append_fill(&thumb, gsk::FillRule::Winding, &accent);
    snapshot.append_stroke(&thumb, &gsk::Stroke::new(1.5), &chip);
}

fn bead(snapshot: &gtk::Snapshot, center: [f32; 2], bead: &BubbleBead, [chip, ink, _]: [gdk::RGBA; 3]) {
    let [x, y] = bead.placement.center;
    let disc = gsk::PathBuilder::new();
    disc.add_circle(&graphene::Point::new(center[0] + x, center[1] + y), BUBBLE_BEAD * 0.5);
    let disc = disc.to_path();
    let alpha = bead.placement.alpha;
    snapshot.append_fill(&disc, gsk::FillRule::Winding, &gdk::RGBA::new(chip.red(), chip.green(), chip.blue(), chip.alpha() * alpha));
    let rim = gdk::RGBA::new(ink.red(), ink.green(), ink.blue(), if bead.selected { 0.9 } else { 0.22 } * alpha);
    snapshot.append_stroke(&disc, &gsk::Stroke::new(if bead.selected { 2. } else { 1. }), &rim);
}

fn dots(snapshot: &gtk::Snapshot, center: [f32; 2], view: &BubbleView, ink: gdk::RGBA) {
    let radius = view.diameter * 0.5 - 7.;
    let step = (6. / radius).to_degrees();
    let first = 90. + step * (view.dots.len() as f32 - 1.) * 0.5;
    for (i, current) in view.dots.iter().enumerate() {
        let angle = (first - step * i as f32).to_radians();
        let dot = gsk::PathBuilder::new();
        dot.add_circle(&graphene::Point::new(center[0] + radius * angle.cos(), center[1] + radius * angle.sin()), 1.75);
        let alpha = if *current { 0.9 } else { 0.32 };
        snapshot.append_fill(&dot.to_path(), gsk::FillRule::Winding, &gdk::RGBA::new(ink.red(), ink.green(), ink.blue(), alpha));
    }
}

impl Bubble {
    pub fn id(&self) -> u32 {
        self.imp().view.borrow().as_ref().map_or(0, |v| v.id)
    }
    fn reach(&self, view: &BubbleView) -> f32 {
        let beads = view.beads.iter().map(|b| b.placement.center[0].hypot(b.placement.center[1]) + BUBBLE_BEAD * 0.5 + 2.);
        let pill = view.pill.as_ref().map(|_| {
            let b = palette_pill(view.diameter);
            (b.width * 0.5).hypot(b.y + b.height)
        });
        view.rings
            .iter()
            .map(|r| r.arc.radius + r.arc.width * 0.5 + 2.)
            .chain(beads)
            .chain(pill)
            .fold(view.diameter * 0.5, f32::max)
    }
    fn extent(&self) -> f32 {
        self.imp().view.borrow().as_ref().map_or(0., |view| self.reach(view) * 2. + 24.)
    }
    fn set_placement(&self, placement: BubblePlacement) {
        let previous = self.imp().placement.replace(Some(placement));
        if previous.is_none_or(|p| (p.lifted, p.homing) != (placement.lifted, placement.homing)) {
            if let Some(face) = self.imp().face.borrow().as_ref() {
                set_class(face, "lifted", placement.lifted);
            }
            self.queue_draw();
        }
    }
    fn center(&self) -> [f32; 2] {
        [self.width() as f32 * 0.5, self.height() as f32 * 0.5]
    }

    fn new(w: &Rc<Workspace>, view: &BubbleView, theme: Theme) -> Self {
        let bubble: Self = glib::Object::new();
        bubble.set_widget_name(&format!("bubble-{}", view.id));
        let face = gtk::Box::new(gtk::Orientation::Vertical, 0);
        face.add_css_class("tool-bubble");
        face.set_can_target(false);
        face.set_parent(&bubble);
        let content: gtk::Widget = match view.kind {
            BubbleKind::Color => {
                let wheel = crate::tool_panels::ColorWheel::bare();
                wheel.set_bare_range((COLOR_RING_SIZES[0] as i32, COLOR_RING_SIZES[2] as i32));
                wheel.set_hexpand(true);
                wheel.set_vexpand(true);
                *bubble.imp().wheel.borrow_mut() = Some(wheel.clone());
                face.set_can_target(true);
                wheel.connect_pick(glib::clone!(#[weak] w, move |action| w.dispatch(UiAction::Color { action })));
                wheel.upcast()
            }
            _ => {
                let overlay = gtk::Overlay::new();
                overlay.set_hexpand(true);
                overlay.set_vexpand(true);
                let column = gtk::Box::new(gtk::Orientation::Vertical, 0);
                column.set_valign(gtk::Align::Center);
                column.set_halign(gtk::Align::Center);
                let icon = crate::icons::image(&format!("layer-{}-symbolic", view.icon));
                icon.set_pixel_size(match view.kind {
                    BubbleKind::Slider => 16,
                    _ if view.preview.is_some() => 20,
                    _ => FACE_ICON,
                });
                column.append(&icon);
                if view.kind == BubbleKind::Slider {
                    let value = gtk::Label::new(None);
                    value.add_css_class("bubble-value");
                    column.append(&value);
                    *bubble.imp().value.borrow_mut() = Some(value);
                }
                *bubble.imp().icon.borrow_mut() = Some(icon);
                if let Some(id) = view.preview {
                    let picture = gtk::Picture::for_paintable(&crate::previews::texture(id, theme));
                    picture.set_content_fit(gtk::ContentFit::Cover);
                    picture.set_can_shrink(true);
                    picture.set_size_request(40, 14);
                    picture.set_margin_top(2);
                    column.append(&picture);
                    *bubble.imp().preview.borrow_mut() = Some(picture);
                }
                overlay.set_child(Some(&column));
                if view.variants {
                    let marker = crate::icons::image("layer-tool-group-symbolic");
                    marker.set_halign(gtk::Align::End);
                    marker.set_valign(gtk::Align::End);
                    marker.set_margin_end(8);
                    marker.set_margin_bottom(8);
                    marker.set_can_target(false);
                    overlay.add_overlay(&marker);
                }
                overlay.upcast()
            }
        };
        face.append(&content);
        if view.kind == BubbleKind::Layers {
            face.add_css_class("layers-bubble");
        }
        let readout = gtk::Label::new(None);
        readout.add_css_class("bubble-readout");
        readout.set_can_target(false);
        readout.set_visible(false);
        readout.set_parent(&bubble);
        *bubble.imp().readout.borrow_mut() = Some(readout);
        *bubble.imp().face.borrow_mut() = Some(face);
        bubble.install_input(w, view.id, view.kind);
        w.install_context(&bubble, ContextTarget::Bubble { id: view.id });
        bubble
    }

    fn set_beads(&self, w: &Rc<Workspace>, view: &BubbleView) {
        let imp = self.imp();
        if view.kind != BubbleKind::Color {
            return;
        }
        let colors: Vec<_> = view.beads.iter().filter(|b| b.placement.bead != Bead::Palettes).collect();
        {
            let mut beads = imp.beads.borrow_mut();
            while beads.len() < colors.len() {
                let patch = crate::display_color::ColorPatch::new(true);
                patch.set_parent(self);
                beads.push(patch);
            }
            while beads.len() > colors.len() {
                beads.pop().unwrap().unparent();
            }
            let (display, headroom) = imp.display.get().unwrap_or_default();
            for (patch, bead) in beads.iter().zip(&colors) {
                if let Some(color) = bead.color {
                    patch.set_display_color(color, display, headroom);
                }
                patch.set_opacity(bead.placement.alpha as f64);
            }
        }
        let palettes = view.beads.iter().find(|b| b.placement.bead == Bead::Palettes);
        if palettes.is_some() && imp.palettes.borrow().is_none() {
            let icon = crate::icons::image(&format!("layer-{}-symbolic", Panel::Palettes.icon()));
            icon.set_pixel_size(RING_ICON as i32);
            icon.set_can_target(false);
            icon.set_parent(self);
            *imp.palettes.borrow_mut() = Some(icon);
        }
        if let Some(icon) = imp.palettes.borrow().as_ref() {
            icon.set_visible(palettes.is_some());
            icon.set_opacity(palettes.map_or(0., |b| b.placement.alpha) as f64);
        }
        if view.pill.is_some() && imp.pill.borrow().is_none() {
            let id = view.id;
            let pill = gtk::Box::new(gtk::Orientation::Horizontal, 0);
            pill.add_css_class("bubble-pill");
            let step = |icon: &str, step: i32| {
                let button = crate::icons::button(icon);
                button.add_css_class("flat");
                button.connect_clicked(glib::clone!(#[weak] w, move |_| {
                    w.dispatch(UiAction::Bubble { action: BubbleAction::Palette { id, step } });
                }));
                button
            };
            let name = gtk::Label::new(None);
            name.set_ellipsize(gtk::pango::EllipsizeMode::End);
            name.set_hexpand(true);
            let choose = gtk::Button::new();
            choose.add_css_class("flat");
            choose.set_hexpand(true);
            choose.set_child(Some(&name));
            choose.connect_clicked(glib::clone!(#[weak] w, move |button| {
                w.show_context(button.upcast_ref(), ContextTarget::BubblePalettes { id }, button.width() as f64 * 0.5, button.height() as f64);
            }));
            pill.append(&step("layer-go-previous-symbolic", -1));
            pill.append(&choose);
            pill.append(&step("layer-go-next-symbolic", 1));
            pill.set_parent(self);
            *imp.pill.borrow_mut() = Some((pill, name));
        }
        if let Some((pill, name)) = imp.pill.borrow().as_ref() {
            pill.set_visible(view.pill.is_some());
            name.set_label(view.pill.as_deref().unwrap_or_default());
        }
    }

    fn set_view(&self, view: BubbleView) {
        let imp = self.imp();
        if let Some(icon) = imp.icon.borrow().as_ref() {
            crate::icons::set(icon, Some(&format!("layer-{}-symbolic", view.icon)));
        }
        if let Some(face) = imp.face.borrow().as_ref() {
            set_class(face, "selected-tool", view.selected && view.kind != BubbleKind::Slider && view.kind != BubbleKind::Color);
            set_class(face, "open", view.open);
            face.set_opacity(if view.enabled { 1. } else { 0.45 });
        }
        if let Some(value) = imp.value.borrow().as_ref() {
            value.set_label(view.rings.first().map_or("", |r| r.text.as_str()));
        }
        let ring_icons = if view.kind == BubbleKind::Slider { 0 } else { view.rings.len() };
        {
            let mut icons = imp.icons.borrow_mut();
            while icons.len() < ring_icons {
                let icon = crate::icons::image("layer-settings-symbolic");
                icon.set_pixel_size(RING_ICON as i32);
                icon.set_can_target(false);
                icon.set_parent(self);
                icons.push(icon);
            }
            for (i, icon) in icons.iter().enumerate() {
                icon.set_visible(i < ring_icons);
                if let Some(ring) = view.rings.get(i) {
                    crate::icons::set(icon, Some(&format!("layer-{}-symbolic", ring.icon)));
                    icon.set_tooltip_text(Some(&ring.label));
                }
            }
        }
        if let Some(readout) = imp.readout.borrow().as_ref() {
            let active = view.rings.iter().find(|r| r.active).filter(|_| view.kind != BubbleKind::Slider);
            readout.set_visible(active.is_some());
            if let Some(ring) = active {
                readout.set_label(&ring.text);
            }
        }
        self.update_property(&[gtk::accessible::Property::Label(&view.label)]);
        *imp.view.borrow_mut() = Some(view);
        self.queue_allocate();
        self.queue_draw();
    }

    fn install_input(&self, w: &Rc<Workspace>, id: u32, kind: BubbleKind) {
        let drag = gtk::GestureDrag::new();
        drag.set_button(gdk::BUTTON_PRIMARY);
        let local = |bubble: &Bubble, x: f64, y: f64| {
            let [cx, cy] = bubble.center();
            [x as f32 - cx, y as f32 - cy]
        };
        let surface_point = |w: &Workspace, bubble: &Bubble, x: f64, y: f64| {
            bubble.compute_point(&w.surface, &graphene::Point::new(x as f32, y as f32)).map(|p| [p.x(), p.y()])
        };
        drag.connect_drag_begin(glib::clone!(#[weak] w, move |gesture, x, y| {
            let Some(bubble) = gesture.widget().and_downcast::<Bubble>() else { return };
            let point = local(&bubble, x, y);
            let view = bubble.imp().view.borrow().clone();
            let Some(view) = view else { return };
            let on_ring = view.rings.iter().any(|r| r.arc.contains(point));
            let on_bead = view.beads.iter().any(|b| bead_hit(b, point));
            let strip = bead_strip_at(view.diameter, view.options, point);
            let contact = if on_ring {
                Contact::Ring
            } else if kind == BubbleKind::Color && point[0].hypot(point[1]) <= color_rim(view.diameter)[0] {
                gesture.set_state(gtk::EventSequenceState::Denied);
                return;
            } else if on_bead || strip.is_some() {
                Contact::Beads { strip, last: x as f32, scrolled: false }
            } else if point[0].hypot(point[1]) <= if kind == BubbleKind::Color { bubble.reach(&view) } else { view.diameter * 0.5 } {
                match surface_point(&w, &bubble, x, y) {
                    Some(at) => Contact::Pending(at),
                    None => return,
                }
            } else {
                gesture.set_state(gtk::EventSequenceState::Claimed);
                bubble.imp().contact.set(None);
                return;
            };
            if contact == Contact::Ring {
                gesture.set_state(gtk::EventSequenceState::Claimed);
            }
            bubble.imp().contact.set(Some(contact));
            if contact == Contact::Ring {
                w.dispatch(UiAction::Bubble { action: BubbleAction::Ring { id, phase: ContactPhase::Down, position: point } });
            }
        }));
        drag.connect_drag_update(glib::clone!(#[weak] w, move |gesture, dx, dy| {
            let Some(bubble) = gesture.widget().and_downcast::<Bubble>() else { return };
            let Some((x0, y0)) = gesture.start_point() else { return };
            let (x, y) = (x0 + dx, y0 + dy);
            match bubble.imp().contact.get() {
                Some(Contact::Ring) => {
                    w.dispatch(UiAction::Bubble { action: BubbleAction::Ring { id, phase: ContactPhase::Move, position: local(&bubble, x, y) } });
                }
                Some(Contact::Pending(origin)) => {
                    if !w.surface.drag_check_threshold(0, 0, dx as i32, dy as i32) {
                        return;
                    }
                    let Some(at) = surface_point(&w, &bubble, x, y) else { return };
                    let grab = BubbleGrab::Bubble { id };
                    gesture.set_state(gtk::EventSequenceState::Claimed);
                    if w.bubbles.drag(&w, grab, ContactPhase::Down, origin) {
                        bubble.imp().contact.set(Some(Contact::Moving));
                        w.bubbles.drag(&w, grab, ContactPhase::Move, at);
                    }
                }
                Some(Contact::Moving) => {
                    if let Some(at) = surface_point(&w, &bubble, x, y) {
                        w.bubbles.drag(&w, BubbleGrab::Bubble { id }, ContactPhase::Move, at);
                    }
                }
                Some(Contact::Beads { strip: Some(strip), last, scrolled }) => {
                    if !scrolled && !w.surface.drag_check_threshold(0, 0, dx as i32, dy as i32) {
                        return;
                    }
                    gesture.set_state(gtk::EventSequenceState::Claimed);
                    bubble.imp().contact.set(Some(Contact::Beads { strip: Some(strip), last: x as f32, scrolled: true }));
                    w.dispatch(UiAction::Bubble { action: BubbleAction::Scroll { id, strip, pixels: x as f32 - last } });
                }
                Some(Contact::Beads { strip: None, .. } | Contact::Pair) | None => {}
            }
        }));
        drag.connect_drag_end(glib::clone!(#[weak] w, move |gesture, dx, dy| {
            let Some(bubble) = gesture.widget().and_downcast::<Bubble>() else { return };
            let Some((x0, y0)) = gesture.start_point() else { return };
            if bubble.imp().contact.get() == Some(Contact::Pair) {
                return;
            }
            let (x, y) = (x0 + dx, y0 + dy);
            match bubble.imp().contact.take() {
                Some(Contact::Ring) => {
                    w.dispatch(UiAction::Bubble { action: BubbleAction::Ring { id, phase: ContactPhase::Up, position: local(&bubble, x, y) } });
                }
                Some(Contact::Moving) => {
                    if let Some(at) = surface_point(&w, &bubble, x, y) {
                        w.bubbles.drag(&w, BubbleGrab::Bubble { id }, ContactPhase::Up, at);
                    }
                }
                Some(Contact::Pending(_)) => w.dispatch(UiAction::Bubble { action: BubbleAction::Tap { id } }),
                Some(Contact::Beads { scrolled: false, .. }) => {
                    let point = local(&bubble, x, y);
                    let bead = bubble.imp().view.borrow().as_ref().and_then(|v| v.beads.iter().find(|b| bead_hit(b, point)).map(|b| b.placement.bead));
                    if let Some(bead) = bead {
                        w.dispatch(UiAction::Bubble { action: BubbleAction::Bead { id, bead } });
                    }
                }
                Some(Contact::Beads { .. } | Contact::Pair) | None => {}
            }
        }));
        drag.connect_cancel(glib::clone!(#[weak] w, move |gesture, _| {
            let Some(bubble) = gesture.widget().and_downcast::<Bubble>() else { return };
            if bubble.imp().contact.get() == Some(Contact::Pair) {
                return;
            }
            match bubble.imp().contact.take() {
                Some(Contact::Ring) => {
                    w.dispatch(UiAction::Bubble { action: BubbleAction::Ring { id, phase: ContactPhase::Cancel, position: [0.; 2] } });
                }
                Some(Contact::Moving) => {
                    w.bubbles.drag(&w, BubbleGrab::Bubble { id }, ContactPhase::Cancel, [0.; 2]);
                }
                _ => {}
            }
        }));
        self.add_controller(drag);

        let touches = gtk::EventControllerLegacy::new();
        touches.set_propagation_phase(gtk::PropagationPhase::Capture);
        touches.connect_event(|controller, event| {
            if let Some(bubble) = controller.widget().and_downcast::<Bubble>() {
                let count = &bubble.imp().touches;
                match event.event_type() {
                    gdk::EventType::TouchBegin => count.set(count.get() + 1),
                    gdk::EventType::TouchEnd | gdk::EventType::TouchCancel => count.set(count.get().saturating_sub(1)),
                    _ => {}
                }
            }
            glib::Propagation::Proceed
        });
        self.add_controller(touches);

        let pair = gtk::GestureZoom::new();
        pair.set_propagation_phase(gtk::PropagationPhase::Capture);
        let center = move |w: &Workspace, gesture: &gtk::GestureZoom| {
            let bubble = gesture.widget().and_downcast::<Bubble>()?;
            let (x, y) = gesture.bounding_box_center()?;
            surface_point(w, &bubble, x, y)
        };
        pair.connect_begin(glib::clone!(#[weak] w, move |gesture, _| {
            let Some(bubble) = gesture.widget().and_downcast::<Bubble>() else { return };
            let Some(at) = center(&w, gesture) else { return };
            gesture.set_state(gtk::EventSequenceState::Claimed);
            match bubble.imp().contact.replace(Some(Contact::Pair)) {
                Some(Contact::Ring) => w.dispatch(UiAction::Bubble { action: BubbleAction::Ring { id, phase: ContactPhase::Cancel, position: [0.; 2] } }),
                Some(Contact::Moving) => {
                    w.bubbles.drag(&w, BubbleGrab::Bubble { id }, ContactPhase::Cancel, at);
                }
                _ => {}
            }
            if !w.bubbles.drag(&w, BubbleGrab::Bubble { id }, ContactPhase::Down, at) {
                bubble.imp().contact.set(None);
            }
        }));
        pair.connect_scale_changed(glib::clone!(#[weak] w, move |gesture, scale| {
            let Some(bubble) = gesture.widget().and_downcast::<Bubble>() else { return };
            if bubble.imp().contact.get() != Some(Contact::Pair) {
                return;
            }
            if kind == BubbleKind::Color {
                let size = bubble.imp().view.borrow().as_ref().map_or(1., |v| v.diameter);
                bubble.imp().pinch.set(Some(pinched_size(size, scale as f32) / size));
                bubble.queue_draw();
            }
            if let Some(at) = center(&w, gesture) {
                w.bubbles.drag(&w, BubbleGrab::Bubble { id }, ContactPhase::Move, at);
            }
        }));
        pair.connect_end(glib::clone!(#[weak] w, move |gesture, _| {
            let Some(bubble) = gesture.widget().and_downcast::<Bubble>() else { return };
            if bubble.imp().contact.get() != Some(Contact::Pair) {
                return;
            }
            bubble.imp().contact.set(None);
            let at = center(&w, gesture).or_else(|| bubble.compute_point(&w.surface, &graphene::Point::new(bubble.center()[0], bubble.center()[1])).map(|p| [p.x(), p.y()]));
            if let Some(at) = at {
                w.bubbles.drag(&w, BubbleGrab::Bubble { id }, ContactPhase::Up, at);
            }
            if let Some(pinch) = bubble.imp().pinch.take() {
                bubble.queue_draw();
                if (pinch - 1.).abs() > 0.01 {
                    w.dispatch(UiAction::Bubble { action: BubbleAction::Resize { id, scale: pinch } });
                }
            }
        }));
        pair.connect_cancel(glib::clone!(#[weak] w, move |gesture, _| {
            let Some(bubble) = gesture.widget().and_downcast::<Bubble>() else { return };
            if bubble.imp().contact.take() == Some(Contact::Pair) {
                w.bubbles.drag(&w, BubbleGrab::Bubble { id }, ContactPhase::Cancel, [0.; 2]);
            }
            bubble.imp().pinch.set(None);
            bubble.queue_draw();
        }));
        self.add_controller(pair);
    }
}

fn bead_hit(bead: &BubbleBead, point: [f32; 2]) -> bool {
    let [x, y] = bead.placement.center;
    bead.placement.alpha > 0.5 && (point[0] - x).hypot(point[1] - y) <= BUBBLE_BEAD * 0.5 + 2.
}

fn set_class(widget: &impl IsA<gtk::Widget>, class: &str, on: bool) {
    if on {
        widget.add_css_class(class);
    } else {
        widget.remove_css_class(class);
    }
}

impl Layer {
    fn bubbles(&self) -> Vec<Bubble> {
        let mut out = Vec::new();
        let mut child = self.first_child();
        while let Some(widget) = child {
            child = widget.next_sibling();
            out.push(widget.downcast::<Bubble>().unwrap());
        }
        out
    }
}

pub(super) struct Bubbles {
    pub(super) layer: Layer,
    tick: RefCell<Option<gtk::TickCallbackId>>,
}

impl Bubbles {
    pub(super) fn new() -> Self {
        let layer: Layer = glib::Object::new();
        layer.set_widget_name("tool-bubbles");
        Self { layer, tick: RefCell::new(None) }
    }

    pub(super) fn refresh(&self, w: &Rc<Workspace>, state: &UiState) {
        let Some(views) = w.gpu.borrow().as_ref().map(|g| g.session.bubbles()) else { return };
        w.surface.imp().layout.borrow_mut().bubbles = state.workspace.layout.bubbles.clone();
        let colors = w.palette.get().map(|palette| {
            let [r, g, b, a] = palette.glass.chip.0;
            let rgb = |c: HexColor| gdk::RGBA::new(c.0[0] as f32 / 255., c.0[1] as f32 / 255., c.0[2] as f32 / 255., 1.);
            [gdk::RGBA::new(r, g, b, a), rgb(palette.text), rgb(palette.accent)]
        });
        let existing = self.layer.bubbles();
        for bubble in &existing {
            let keep = views.iter().find(|v| v.id == bubble.id()).is_some_and(|v| {
                bubble.imp().view.borrow().as_ref().is_some_and(|old| old.source == v.source && old.diameter == v.diameter)
            });
            if !keep {
                bubble.unparent();
            }
        }
        for view in views {
            let bubble = self.layer.bubbles().into_iter().find(|b| b.id() == view.id).unwrap_or_else(|| {
                let bubble = Bubble::new(w, &view, state.theme);
                bubble.set_parent(&self.layer);
                bubble
            });
            bubble.imp().colors.set(colors);
            bubble.imp().display.set(Some((w.paint_view_color(), w.picker_headroom())));
            if bubble.imp().view.borrow().as_ref() != Some(&view) {
                bubble.set_beads(w, &view);
                bubble.set_view(view);
            }
        }
        self.refresh_wheels(&state.preview_colors(), w.paint_view_color(), w.picker_headroom(), state.color_picker.preview.is_some());
        self.animate(w);
    }

    pub(super) fn refresh_wheels(&self, colors: &ColorState, view: crate::display_color::ViewColor, headroom: f32, preview: bool) {
        for bubble in self.layer.bubbles() {
            if let Some(wheel) = bubble.imp().wheel.borrow().as_ref() {
                wheel.set_state(colors, view, headroom, preview);
            }
        }
    }

    pub(super) fn drag(&self, w: &Rc<Workspace>, grab: BubbleGrab, phase: ContactPhase, position: [f32; 2]) -> bool {
        let viewport = [w.surface.width() as f32, w.surface.height() as f32];
        let action = UiAction::Bubble { action: BubbleAction::Drag { grab, phase, position, viewport } };
        let result = w.gpu.borrow_mut().as_mut().map(|g| g.session.dispatch(action));
        let accepted = matches!(result, Some(Ok(_)));
        match result {
            Some(Ok(change)) if change.regions == 0 && !change.canvas_wake => {}
            Some(Ok(change)) => w.changed(Ok(change)),
            _ => {}
        }
        w.dragging.set(matches!(phase, ContactPhase::Down | ContactPhase::Move) && accepted);
        self.animate(w);
        accepted
    }

    pub(super) fn animate(&self, w: &Rc<Workspace>) {
        if self.tick.borrow().is_some() {
            return;
        }
        let tick = self.layer.add_tick_callback(glib::clone!(
            #[weak] w,
            #[upgrade_or] glib::ControlFlow::Break,
            move |layer, clock| {
                let now = clock.frame_time().max(0) as u64 * 1000;
                let frame = w.gpu.borrow_mut().as_mut().map(|g| {
                    let moving = g.session.step_bubbles(now);
                    (moving, g.session.bubble_placements())
                });
                let Some((moving, placements)) = frame else {
                    w.bubbles.tick.take();
                    return glib::ControlFlow::Break;
                };
                *layer.imp().placements.borrow_mut() = placements;
                layer.queue_allocate();
                if moving {
                    glib::ControlFlow::Continue
                } else {
                    w.bubbles.tick.take();
                    glib::ControlFlow::Break
                }
            }
        ));
        *self.tick.borrow_mut() = Some(tick);
    }
}
