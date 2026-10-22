use super::{Workspace, allocate_at};
use crate::layers::LayerPanel;
use adw::prelude::*;
use gtk::{gdk, glib, graphene, gsk, subclass::prelude::*};
use layer_ui::*;
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};

const SECOND_FINGER: f32 = 90.;
const FADED: f64 = 0.2;
const HALO_BAND: f32 = 22.;
const FADE_RETURN_MS: u64 = 180;
const UNFOLD_US: i64 = 200_000;
const FOLD_US: i64 = 150_000;
const FACE_IN_US: i64 = 180_000;

#[derive(Clone, Copy, Debug, PartialEq)]
enum Contact {
    Pending([f32; 2]),
    Moving,
    Ring,
    Beads(bool),
    Choose,
    Pair,
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Region {
    Handle,
    Ring,
    Beads,
    Choose,
    Pass,
}

struct Item {
    root: gtk::Box,
    icon: gtk::Image,
    preview: gtk::Picture,
    name: gtk::Label,
    tag: gtk::Label,
}

struct Card {
    root: gtk::Box,
    nub: gtk::Box,
    footer: gtk::Box,
    full: bool,
    layers: Rc<LayerPanel>,
    effects: Rc<crate::effects::EffectPanels>,
    stack: gtk::Stack,
    add: gtk::Button,
    delete: gtk::Button,
    disclosure: gtk::Button,
    disclosure_label: gtk::Label,
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
        pub(super) coins: RefCell<Vec<gtk::Box>>,
        pub(super) icon: RefCell<Option<gtk::Image>>,
        pub(super) preview: RefCell<Option<gtk::Picture>>,
        pub(super) value: RefCell<Option<gtk::Label>>,
        pub(super) wheel: RefCell<Option<crate::tool_panels::ColorWheel>>,
        pub(super) thumbnail: RefCell<Option<gtk::Picture>>,
        pub(super) count: RefCell<Option<gtk::Label>>,
        pub(super) icons: RefCell<Vec<gtk::Image>>,
        pub(super) readout: RefCell<Option<gtk::Label>>,
        pub(super) beads: RefCell<Vec<crate::display_color::ColorPatch>>,
        pub(super) palettes: RefCell<Option<gtk::Box>>,
        pub(super) pill: RefCell<Option<(gtk::Box, gtk::Label)>>,
        pub(super) items: RefCell<Vec<super::Item>>,
        pub(super) card: RefCell<Option<super::Card>>,
        pub(super) fold: Cell<Option<(i64, bool)>>,
        pub(super) card_size: Cell<[f32; 2]>,
        pub(super) display: Cell<Option<(crate::display_color::ViewColor, f32)>>,
        pub(super) placement: Cell<Option<BubblePlacement>>,
        pub(super) colors: Cell<Option<[gdk::RGBA; 3]>>,
        pub(super) contact: Cell<Option<Contact>>,
        pub(super) hot: Cell<Option<usize>>,
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
            let c = [width as f32 * 0.5, height as f32 * 0.5];
            let place = |widget: &gtk::Widget, at: [f32; 2], size: [f32; 2]| {
                allocate_at(widget, Bounds { x: c[0] + at[0] - size[0] * 0.5, y: c[1] + at[1] - size[1] * 0.5, width: size[0], height: size[1] });
            };
            if let Some(face) = self.face.borrow().as_ref() {
                place(face.upcast_ref(), [0.; 2], [view.diameter; 2]);
            }
            for (i, coin) in self.coins.borrow().iter().enumerate() {
                let scale = if i == 0 { 0.84 } else { 0.92 };
                place(coin.upcast_ref(), [0., if i == 0 { -7. } else { -4. }], [TOOL_BUBBLE * scale; 2]);
            }
            if let Some(count) = self.count.borrow().as_ref() {
                let (_, natural, _, _) = count.measure(gtk::Orientation::Horizontal, -1);
                let width = (natural as f32).max(18.);
                place(count.upcast_ref(), [TOOL_BUBBLE * 0.5 + 2. - width * 0.5, -TOOL_BUBBLE * 0.5 + 7.], [width, 18.]);
            }
            for (icon, ring) in self.icons.borrow().iter().zip(&view.rings) {
                place(icon.upcast_ref(), ring.arc.icon_point(), [16.; 2]);
            }
            if let Some(readout) = self.readout.borrow().as_ref().filter(|r| r.is_visible()) {
                let (_, natural, _, _) = readout.measure(gtk::Orientation::Horizontal, -1);
                let (_, tall, _, _) = readout.measure(gtk::Orientation::Vertical, natural);
                place(readout.upcast_ref(), [0., view.halo.unwrap_or(view.diameter) * 0.5 + 16.], [natural as f32, tall as f32]);
            }
            let colors = view.beads.iter().filter(|b| b.placement.bead != Bead::Palettes);
            for (patch, bead) in self.beads.borrow().iter().zip(colors) {
                place(patch.upcast_ref(), bead.placement.center, [bead.placement.size; 2]);
            }
            if let (Some(palettes), Some(bead)) = (self.palettes.borrow().as_ref(), view.beads.iter().find(|b| b.placement.bead == Bead::Palettes)) {
                place(palettes.upcast_ref(), bead.placement.center, [bead.placement.size; 2]);
            }
            if let Some((pill, _)) = self.pill.borrow().as_ref() {
                let b = palette_pill(view.diameter);
                place(pill.upcast_ref(), [0., b.y + b.height * 0.5], [b.width, b.height]);
            }
            for (item, chooser) in self.items.borrow().iter().zip(&view.chooser) {
                place(item.root.upcast_ref(), chooser.center, chooser.size());
                if item.tag.is_visible() {
                    let (_, natural, _, _) = item.tag.measure(gtk::Orientation::Horizontal, -1);
                    let (_, tall, _, _) = item.tag.measure(gtk::Orientation::Vertical, natural);
                    let [x, y] = chooser.center;
                    let above = if y <= 0. || x.abs() > y { -1. } else { 1. };
                    let at = [x, y + above * (CHOOSER_ITEM * 0.5 + 4. + tall as f32 * 0.5)];
                    place(item.tag.upcast_ref(), at, [natural as f32, tall as f32]);
                }
            }
            if let Some(card) = self.card.borrow().as_ref().filter(|c| c.root.is_child_visible()) {
                place(card.root.upcast_ref(), [0.; 2], self.card_size.get());
            }
        }
        fn contains(&self, x: f64, y: f64) -> bool {
            let view = self.view.borrow();
            let Some(view) = view.as_ref() else { return false };
            let obj = self.obj();
            let p = [x as f32 - obj.width() as f32 * 0.5, y as f32 - obj.height() as f32 * 0.5];
            if self.contact.get().is_some() {
                let reach = obj.reach(view) + SECOND_FINGER;
                return p[0].abs() <= reach && p[1].abs() <= reach;
            }
            obj.region(view, p, None) != Region::Pass || obj.passes(view, p)
        }
        fn snapshot(&self, snapshot: &gtk::Snapshot) {
            let obj = self.obj();
            let view = self.view.borrow();
            let Some(view) = view.as_ref() else { return };
            let c = [obj.width() as f32 * 0.5, obj.height() as f32 * 0.5];
            let placement = self.placement.get();
            let homing = placement.is_some_and(|p| p.homing);
            let scale = self.pinch.get().unwrap_or(1.) * if homing { 0.82 } else if placement.is_some_and(|p| p.lifted) { 1.04 } else { 1. };
            snapshot.save();
            snapshot.translate(&graphene::Point::new(c[0], c[1]));
            snapshot.scale(scale, scale);
            snapshot.translate(&graphene::Point::new(-c[0], -c[1]));
            if homing {
                snapshot.push_opacity(0.55);
            }
            let round = |widget: &gtk::Widget| {
                let part = gtk::Snapshot::new();
                obj.snapshot_child(widget, &part);
                crate::squircle::append_round(snapshot, part);
            };
            let colors = self.colors.get();
            let grow = |progress: f32| {
                snapshot.save();
                snapshot.translate(&graphene::Point::new(c[0], c[1]));
                let s = 0.6 + 0.4 * progress;
                snapshot.scale(s, s);
                snapshot.translate(&graphene::Point::new(-c[0], -c[1]));
                snapshot.push_opacity(progress as f64);
            };
            let (card_shown, face_shown) = obj.fold_progress(view.card.is_some());
            if let (Some(card), Some(progress)) = (self.card.borrow().as_ref().filter(|c| c.root.is_child_visible()), card_shown) {
                grow(progress);
                obj.snapshot_child(&card.root, snapshot);
                snapshot.pop();
                snapshot.restore();
            }
            if let Some(progress) = face_shown {
                grow(progress);
                for coin in self.coins.borrow().iter() {
                    obj.snapshot_child(coin, snapshot);
                }
                if let Some(face) = self.face.borrow().as_ref() {
                    if view.kind == BubbleKind::Layers {
                        obj.snapshot_child(face, snapshot);
                    } else {
                        round(face.upcast_ref());
                    }
                }
                if let Some(count) = self.count.borrow().as_ref() {
                    round(count.upcast_ref());
                }
                snapshot.pop();
                snapshot.restore();
            }
            if let Some(colors) = colors {
                if view.kind == BubbleKind::Medium {
                    super::dots(snapshot, c, view, colors[1]);
                }
                for ring in &view.rings {
                    super::ring(snapshot, c, ring, colors, view.kind != BubbleKind::Slider);
                }
            }
            for icon in self.icons.borrow().iter().take(view.rings.len()) {
                obj.snapshot_child(icon, snapshot);
            }
            let colored = view.beads.iter().filter(|b| b.placement.bead != Bead::Palettes);
            for (patch, bead) in self.beads.borrow().iter().zip(colored) {
                super::bead_shadow(snapshot, c, bead);
                let part = gtk::Snapshot::new();
                obj.snapshot_child(patch, &part);
                crate::squircle::append_round(snapshot, part);
                if let Some(colors) = colors {
                    super::bead_rim(snapshot, c, bead, colors[1]);
                }
            }
            if let Some(palettes) = self.palettes.borrow().as_ref().filter(|p| p.is_visible()) {
                round(palettes.upcast_ref());
            }
            if let Some((pill, _)) = self.pill.borrow().as_ref().filter(|(p, _)| p.is_visible()) {
                round(pill.upcast_ref());
            }
            let hot = self.hot.get();
            for (i, (item, chooser)) in self.items.borrow().iter().zip(&view.chooser).enumerate() {
                let grow = if hot != Some(i) { 1. } else if chooser.kind == ChooserKind::Brush { 1.06 } else { 1.12 };
                let at = [c[0] + chooser.center[0], c[1] + chooser.center[1]];
                snapshot.save();
                snapshot.translate(&graphene::Point::new(at[0], at[1]));
                snapshot.scale(grow, grow);
                snapshot.translate(&graphene::Point::new(-at[0], -at[1]));
                round(item.root.upcast_ref());
                snapshot.restore();
            }
            for item in self.items.borrow().iter().take(view.chooser.len()).filter(|item| item.tag.is_visible()) {
                round(item.tag.upcast_ref());
            }
            if let Some(readout) = self.readout.borrow().as_ref().filter(|r| r.is_visible()) {
                round(readout.upcast_ref());
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

fn rgba(color: gdk::RGBA, alpha: f32) -> gdk::RGBA {
    gdk::RGBA::new(color.red(), color.green(), color.blue(), color.alpha() * alpha)
}

fn circle(center: [f32; 2], radius: f32) -> gsk::Path {
    let path = gsk::PathBuilder::new();
    path.add_circle(&graphene::Point::new(center[0], center[1]), radius);
    path.to_path()
}

fn arc_path(c: [f32; 2], arc: &RingArc, to: f32) -> gsk::Path {
    let builder = gsk::PathBuilder::new();
    let [x0, y0] = arc.point(0.);
    builder.move_to(c[0] + x0, c[1] + y0);
    let sweep = arc.sweep * to;
    let mut done = 0f32;
    while (sweep - done).abs() > 1e-3 {
        let step = (sweep - done).clamp(-179., 179.);
        done += step;
        let [x, y] = arc.point(done / arc.sweep);
        builder.svg_arc_to(arc.radius, arc.radius, 0., false, step > 0., c[0] + x, c[1] + y);
    }
    builder.to_path()
}

fn ring(snapshot: &gtk::Snapshot, c: [f32; 2], ring: &BubbleRing, [chip, ink, accent]: [gdk::RGBA; 3], band: bool) {
    let stroke = |width: f32| {
        let stroke = gsk::Stroke::new(width);
        stroke.set_line_cap(gsk::LineCap::Round);
        stroke
    };
    let arc = ring.arc;
    if band {
        let [x, y] = arc.icon_point();
        let start = y.atan2(x).to_degrees();
        let lead = (start - arc.start + 540.).rem_euclid(360.) - 180.;
        let banded = RingArc { start, sweep: arc.sweep - lead, ..arc };
        snapshot.append_stroke(&arc_path(c, &banded, 1.), &stroke(HALO_BAND), &chip);
    }
    snapshot.append_stroke(&arc_path(c, &arc, 1.), &stroke(RING_TRACK), &rgba(ink, 0.18));
    if ring.fill > 0.004 {
        snapshot.append_stroke(&arc_path(c, &arc, ring.fill), &stroke(RING_TRACK), &accent);
    }
    let thumb = circle({
        let [x, y] = arc.point(ring.fill);
        [c[0] + x, c[1] + y]
    }, if ring.active { 6. } else { 5. });
    snapshot.append_fill(&thumb, gsk::FillRule::Winding, &accent);
    snapshot.append_stroke(&thumb, &gsk::Stroke::new(1.5), &chip);
}

fn bead_shadow(snapshot: &gtk::Snapshot, c: [f32; 2], bead: &BubbleBead) {
    let [x, y] = bead.placement.center;
    let radius = bead.placement.size * 0.5;
    for (grow, alpha) in [(2., 0.05), (1., 0.08)] {
        let shadow = circle([c[0] + x, c[1] + y + 1.], radius + grow);
        snapshot.append_fill(&shadow, gsk::FillRule::Winding, &gdk::RGBA::new(0., 0., 0., alpha * bead.placement.alpha));
    }
}

fn bead_rim(snapshot: &gtk::Snapshot, c: [f32; 2], bead: &BubbleBead, ink: gdk::RGBA) {
    let [x, y] = bead.placement.center;
    let at = [c[0] + x, c[1] + y];
    let radius = bead.placement.size * 0.5;
    snapshot.append_stroke(&circle(at, radius - 0.5), &gsk::Stroke::new(1.), &gdk::RGBA::new(0.5, 0.5, 0.5, 0.4 * bead.placement.alpha));
    if bead.selected {
        snapshot.append_stroke(&circle(at, radius + 1.), &gsk::Stroke::new(2.), &ink);
    }
}

fn dots(snapshot: &gtk::Snapshot, c: [f32; 2], view: &BubbleView, ink: gdk::RGBA) {
    let count = view.dots.len().min(9);
    if count < 2 {
        return;
    }
    for (i, current) in view.dots.iter().take(count).enumerate() {
        let angle = (90. + ((count - 1) as f32 * 0.5 - i as f32) * 12.).to_radians();
        let at = [c[0] + 19. * angle.cos(), c[1] + 19. * angle.sin()];
        snapshot.append_fill(&circle(at, 1.75), gsk::FillRule::Winding, &rgba(ink, if *current { 0.9 } else { 0.3 }));
    }
}

fn set_class(widget: &impl IsA<gtk::Widget>, class: &str, on: bool) {
    if on {
        widget.add_css_class(class);
    } else {
        widget.remove_css_class(class);
    }
}

fn glass(class: &str) -> gtk::Box {
    let part = gtk::Box::new(gtk::Orientation::Vertical, 0);
    part.add_css_class(class);
    part.set_can_target(false);
    part
}

impl Item {
    fn new(bubble: &Bubble) -> Self {
        let root = glass("tool-bubble");
        let icon = crate::icons::image("layer-settings-symbolic");
        icon.set_pixel_size(18);
        icon.set_vexpand(true);
        let preview = gtk::Picture::new();
        preview.set_content_fit(gtk::ContentFit::Fill);
        preview.set_can_shrink(true);
        preview.set_size_request(76, 14);
        preview.set_halign(gtk::Align::Center);
        preview.set_margin_top(5);
        let name = gtk::Label::new(None);
        name.add_css_class("bubble-brush-name");
        name.set_ellipsize(gtk::pango::EllipsizeMode::End);
        name.set_max_width_chars(14);
        name.set_margin_start(8);
        name.set_margin_end(8);
        root.append(&icon);
        root.append(&preview);
        root.append(&name);
        root.set_parent(bubble);
        let tag = gtk::Label::new(None);
        tag.add_css_class("bubble-tag");
        tag.set_can_target(false);
        tag.set_parent(bubble);
        Self { root, icon, preview, name, tag }
    }
    fn set(&self, item: &ChooserItem, theme: Theme, hot: bool) {
        let brush = item.kind == ChooserKind::Brush;
        self.icon.set_visible(!brush || item.preview.is_none());
        self.icon.set_vexpand(!brush);
        crate::icons::set(&self.icon, Some(&format!("layer-{}-symbolic", item.icon)));
        self.preview.set_visible(brush && item.preview.is_some());
        if let Some(id) = item.preview.filter(|_| brush) {
            self.preview.set_paintable(Some(&crate::previews::texture(id, theme)));
        }
        self.name.set_visible(brush);
        self.name.set_label(&item.label);
        self.tag.set_label(&item.label);
        self.tag.set_visible(!brush && (item.shown || hot));
        set_class(&self.root, "selected-tool", item.selected);
        set_class(&self.root, "bubble-brush", brush);
        self.root.set_opacity(if item.enabled { 1. } else { 0.45 });
        self.root.update_property(&[gtk::accessible::Property::Label(&item.label)]);
    }
    fn remove(self) {
        self.root.unparent();
        self.tag.unparent();
    }
}

impl Card {
    fn new(w: &Rc<Workspace>, bubble: &Bubble, id: u32, full: bool) -> Self {
        let root = gtk::Box::new(gtk::Orientation::Vertical, 0);
        root.add_css_class("bubble-card");
        root.set_widget_name("bubble-card");
        root.set_overflow(gtk::Overflow::Hidden);
        let nub = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        nub.add_css_class("bubble-nub");
        nub.set_widget_name("bubble-nub");
        nub.set_halign(gtk::Align::Center);
        root.append(&nub);
        let layers = Rc::new(w.layer_panel.sharing(w.localization()));
        layers.set_simple(!full);
        layers.bind(w);
        layers.footer.set_visible(false);
        layers.root.add_css_class("customizable-target");
        let effects = Rc::new(crate::effects::EffectPanels::with_filter_types(false));
        effects.properties.add_css_class("customizable-target");
        let properties = gtk::ScrolledWindow::builder().hscrollbar_policy(gtk::PolicyType::Never).child(&effects.properties).build();
        let stack = gtk::Stack::builder().vhomogeneous(false).transition_duration(260).build();
        stack.set_vexpand(true);
        stack.add_named(&layers.root, Some("layers"));
        stack.add_named(&properties, Some("properties"));
        root.append(&stack);
        let footer = gtk::Box::new(gtk::Orientation::Horizontal, 2);
        footer.add_css_class("bubble-card-footer");
        let icon_button = |icon: &str, action: UiAction| {
            let button = crate::icons::button(icon);
            button.add_css_class("flat");
            button.add_css_class("customizable-target");
            w.bind_action_tooltip(&button, action.clone());
            button.connect_clicked(glib::clone!(#[weak] w, move |_| w.dispatch(action.clone())));
            footer.append(&button);
            button
        };
        let add = icon_button("layer-add-layer-symbolic", UiAction::Layer { action: LayerAction::New { group: false, clipped: false } });
        add.set_widget_name("bubble-new-layer");
        let delete = icon_button("layer-delete-symbolic", UiAction::Layer { action: LayerAction::DeleteSelected });
        delete.set_visible(full);
        let spacer = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        spacer.set_hexpand(true);
        footer.append(&spacer);
        let content = gtk::Box::new(gtk::Orientation::Horizontal, 4);
        let disclosure_label = gtk::Label::new(None);
        disclosure_label.set_ellipsize(gtk::pango::EllipsizeMode::End);
        disclosure_label.set_width_chars(1);
        let chevron = crate::icons::image("layer-chevron-down-symbolic");
        chevron.add_css_class("layer-properties-chevron");
        content.append(&disclosure_label);
        content.append(&chevron);
        let disclosure = gtk::Button::new();
        disclosure.add_css_class("flat");
        disclosure.add_css_class("customizable-target");
        disclosure.set_child(Some(&content));
        disclosure.set_widget_name("bubble-layer-properties");
        disclosure.connect_clicked(glib::clone!(#[weak] w, move |_| w.dispatch(UiAction::Bubble { action: BubbleAction::Properties { id } })));
        footer.append(&disclosure);
        root.append(&footer);
        root.set_parent(bubble);
        Self { root, nub, footer, full, layers, effects, stack, add, delete, disclosure, disclosure_label }
    }
    fn refresh(&self, w: &Rc<Workspace>, state: &UiState, card: BubbleCard) {
        self.layers.refresh(state);
        self.effects.refresh(w, state);
        let page = if card.properties { "properties" } else { "layers" };
        if self.stack.visible_child_name().as_deref() != Some(page) {
            let transition = if card.properties { gtk::StackTransitionType::SlideUp } else { gtk::StackTransitionType::SlideDown };
            self.stack.set_visible_child_full(page, transition);
        }
        let disclosure = &state.layer_properties.disclosure;
        self.disclosure_label.set_label(if card.properties { &disclosure.return_label } else { &disclosure.label });
        set_class(&self.disclosure, "open", card.properties);
        self.disclosure.set_sensitive(state.layer_properties.drawer_toggle_enabled(card.properties));
        self.delete.set_visible(self.full && !card.properties);
        self.add.set_opacity(if card.properties { 0. } else { 1. });
        self.add.set_can_target(!card.properties);
    }
    fn handles(&self, picked: Option<&gtk::Widget>) -> bool {
        picked.is_none_or(|p| p == self.root.upcast_ref::<gtk::Widget>() || p == self.nub.upcast_ref::<gtk::Widget>() || p == self.footer.upcast_ref::<gtk::Widget>())
    }
}

impl Bubble {
    pub fn id(&self) -> u32 {
        self.imp().view.borrow().as_ref().map_or(0, |v| v.id)
    }
    fn reach(&self, view: &BubbleView) -> f32 {
        let corner = |at: [f32; 2], size: [f32; 2]| (at[0].abs() + size[0] * 0.5).max(at[1].abs() + size[1] * 0.5);
        let beads = view.beads.iter().map(|b| corner(b.placement.center, [b.placement.size; 2]));
        let items = view.chooser.iter().map(|i| corner(i.center, i.size()) + if i.kind == ChooserKind::Brush { 0. } else { 60. });
        let pill = view.pill.as_ref().map(|_| {
            let b = palette_pill(view.diameter);
            corner([0., b.y + b.height * 0.5], [b.width, b.height])
        });
        let card = view.card.map(|c| corner([0.; 2], c.size));
        let halo = view.halo.map(|size| size * 0.5 + 40.);
        beads.chain(items).chain(pill).chain(card).chain(halo).fold(view.diameter * 0.5, f32::max)
    }
    fn extent(&self) -> f32 {
        self.imp().view.borrow().as_ref().map_or(0., |view| (self.reach(view) * 2. + 24.).ceil())
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
    fn fold_progress(&self, open: bool) -> (Option<f32>, Option<f32>) {
        let ease = |elapsed: i64, span: i64| 1. - (1. - (elapsed as f32 / span as f32).clamp(0., 1.)).powi(3);
        let Some((start, opening)) = self.imp().fold.get() else {
            return if open { (Some(1.), None) } else { (None, Some(1.)) };
        };
        let elapsed = self.frame_clock().map_or(i64::MAX / 2, |clock| clock.frame_time()) - start;
        match opening {
            true => (Some(ease(elapsed, UNFOLD_US)), None),
            false if elapsed < FOLD_US => (Some(1. - ease(elapsed, FOLD_US)), None),
            false => (None, Some(ease(elapsed - FOLD_US, FACE_IN_US))),
        }
    }
    fn animate_fold(&self, opening: bool) {
        let Some(clock) = self.frame_clock().filter(|_| gtk::Settings::default().is_some_and(|s| s.is_gtk_enable_animations())) else {
            self.imp().fold.set(None);
            self.finish_fold(opening);
            return;
        };
        self.imp().fold.set(Some((clock.frame_time(), opening)));
        self.add_tick_callback(move |bubble, clock| {
            bubble.queue_draw();
            let Some((start, current)) = bubble.imp().fold.get().filter(|(_, current)| *current == opening) else {
                return glib::ControlFlow::Break;
            };
            if clock.frame_time() - start < if opening { UNFOLD_US } else { FOLD_US + FACE_IN_US } {
                return glib::ControlFlow::Continue;
            }
            bubble.imp().fold.set(None);
            bubble.finish_fold(current);
            glib::ControlFlow::Break
        });
    }
    fn finish_fold(&self, opening: bool) {
        if let Some(card) = self.imp().card.borrow().as_ref().filter(|_| !opening) {
            card.root.set_child_visible(false);
        }
        self.queue_draw();
    }
    fn region(&self, view: &BubbleView, p: [f32; 2], picked: Option<&gtk::Widget>) -> Region {
        if let Some(card) = view.card {
            if p[0].abs() <= card.size[0] * 0.5 && p[1].abs() <= card.size[1] * 0.5 {
                let handles = self.imp().card.borrow().as_ref().is_none_or(|c| c.handles(picked));
                return if handles { Region::Handle } else { Region::Pass };
            }
        }
        if view.chooser.iter().any(|item| item.contains(p)) {
            return Region::Choose;
        }
        let arcs: Vec<_> = view.rings.iter().map(|r| r.arc).collect();
        if ring_at(&arcs, p).is_some() {
            return Region::Ring;
        }
        let beads: Vec<_> = view.beads.iter().map(|b| b.placement).collect();
        if bead_at(&beads, p).is_some() {
            return Region::Beads;
        }
        let r = p[0].hypot(p[1]);
        match view.kind {
            BubbleKind::Color if on_color_rim(view.diameter, view.options, p) => Region::Handle,
            BubbleKind::Color => Region::Pass,
            BubbleKind::Layers if view.card.is_none() && p[0].abs() <= TOOL_BUBBLE * 0.5 && p[1].abs() <= TOOL_BUBBLE * 0.5 => Region::Handle,
            BubbleKind::Slider if r <= SLIDER_CENTER * 0.5 => Region::Handle,
            BubbleKind::Slider | BubbleKind::Layers => Region::Pass,
            _ if r <= view.diameter * 0.5 => Region::Handle,
            _ => Region::Pass,
        }
    }
    fn passes(&self, view: &BubbleView, p: [f32; 2]) -> bool {
        let pill = view.pill.is_some() && palette_pill(view.diameter).contains(p[0], p[1]);
        let card = view.card.is_some_and(|c| p[0].abs() <= c.size[0] * 0.5 && p[1].abs() <= c.size[1] * 0.5);
        pill || card || (view.kind == BubbleKind::Color && p[0].hypot(p[1]) <= view.diameter * 0.5)
    }

    fn new(w: &Rc<Workspace>, view: &BubbleView, theme: Theme) -> Self {
        let bubble: Self = glib::Object::new();
        bubble.set_widget_name(&format!("bubble-{}", view.id));
        let imp = bubble.imp();
        if view.kind == BubbleKind::Layers {
            for _ in 0..2 {
                let coin = glass("tool-bubble");
                coin.add_css_class("layers-bubble");
                coin.set_opacity(0.85);
                coin.set_parent(&bubble);
                imp.coins.borrow_mut().push(coin);
            }
        }
        let face = glass("tool-bubble");
        face.set_parent(&bubble);
        match view.kind {
            BubbleKind::Color => {
                face.add_css_class("bubble-plate");
                let wheel = crate::tool_panels::ColorWheel::bare();
                let inner = (COLOR_RING_RANGE[0] - RING_RIM * 2.) as i32;
                wheel.set_bare_range((inner, (COLOR_RING_RANGE[1] - RING_RIM * 2.) as i32));
                wheel.set_margin_start(RING_RIM as i32);
                wheel.set_margin_end(RING_RIM as i32);
                wheel.set_margin_top(RING_RIM as i32);
                wheel.set_margin_bottom(RING_RIM as i32);
                wheel.set_vexpand(true);
                wheel.add_css_class("customizable-target");
                wheel.connect_pick(glib::clone!(#[weak] w, move |action| w.dispatch(UiAction::Color { action })));
                face.set_can_target(true);
                face.append(&wheel);
                *imp.wheel.borrow_mut() = Some(wheel);
            }
            BubbleKind::Layers => {
                face.add_css_class("layers-bubble");
                let thumbnail = gtk::Picture::new();
                thumbnail.add_css_class("bubble-thumbnail");
                thumbnail.set_content_fit(gtk::ContentFit::Cover);
                thumbnail.set_can_shrink(true);
                thumbnail.set_overflow(gtk::Overflow::Hidden);
                thumbnail.set_size_request(46, 46);
                thumbnail.set_halign(gtk::Align::Center);
                thumbnail.set_valign(gtk::Align::Center);
                thumbnail.set_vexpand(true);
                face.append(&thumbnail);
                *imp.thumbnail.borrow_mut() = Some(thumbnail);
                let count = gtk::Label::new(None);
                count.add_css_class("bubble-count");
                count.set_can_target(false);
                count.set_parent(&bubble);
                *imp.count.borrow_mut() = Some(count);
            }
            _ => {
                let overlay = gtk::Overlay::new();
                overlay.set_vexpand(true);
                let column = gtk::Box::new(gtk::Orientation::Vertical, 0);
                column.set_valign(gtk::Align::Center);
                column.set_halign(gtk::Align::Center);
                let icon = crate::icons::image(&format!("layer-{}-symbolic", view.icon));
                icon.set_pixel_size(match view.kind {
                    BubbleKind::Slider => 16,
                    _ => 20,
                });
                column.append(&icon);
                match view.kind {
                    BubbleKind::Slider => {
                        icon.set_opacity(0.75);
                        let value = gtk::Label::new(None);
                        value.add_css_class("bubble-value");
                        value.set_margin_top(2);
                        column.append(&value);
                        *imp.value.borrow_mut() = Some(value);
                    }
                    BubbleKind::Medium => icon.set_margin_bottom(8),
                    _ => {}
                }
                if let Some(id) = view.preview {
                    icon.set_margin_bottom(2);
                    let picture = gtk::Picture::for_paintable(&crate::previews::texture(id, theme));
                    picture.set_content_fit(gtk::ContentFit::Fill);
                    picture.set_can_shrink(true);
                    picture.set_size_request(34, 8);
                    picture.set_opacity(0.78);
                    column.append(&picture);
                    *imp.preview.borrow_mut() = Some(picture);
                }
                *imp.icon.borrow_mut() = Some(icon);
                overlay.set_child(Some(&column));
                if view.variants {
                    let marker = crate::icons::image("layer-tool-group-symbolic");
                    marker.set_halign(gtk::Align::End);
                    marker.set_valign(gtk::Align::End);
                    marker.set_margin_end(9);
                    marker.set_margin_bottom(9);
                    overlay.add_overlay(&marker);
                }
                face.append(&overlay);
            }
        }
        *imp.face.borrow_mut() = Some(face);
        let readout = gtk::Label::new(None);
        readout.add_css_class("bubble-readout");
        readout.set_can_target(false);
        readout.set_visible(false);
        readout.set_parent(&bubble);
        *imp.readout.borrow_mut() = Some(readout);
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
            let bead = glass("bubble-glass");
            let icon = crate::icons::image(&format!("layer-{}-symbolic", Panel::Palettes.icon()));
            icon.set_pixel_size(13);
            icon.set_vexpand(true);
            bead.append(&icon);
            bead.set_parent(self);
            *imp.palettes.borrow_mut() = Some(bead);
        }
        if let Some(bead) = imp.palettes.borrow().as_ref() {
            bead.set_visible(palettes.is_some());
            set_class(bead, "selected-tool", palettes.is_some_and(|b| b.selected));
        }
        if view.pill.is_some() && imp.pill.borrow().is_none() {
            let id = view.id;
            let pill = gtk::Box::new(gtk::Orientation::Horizontal, 2);
            pill.add_css_class("bubble-pill");
            let step = |icon: &str, step: i32| {
                let button = crate::icons::button(icon);
                button.add_css_class("flat");
                button.add_css_class("customizable-target");
                button.connect_clicked(glib::clone!(#[weak] w, move |_| {
                    w.dispatch(UiAction::Bubble { action: BubbleAction::Palette { id, step } });
                }));
                button
            };
            let name = gtk::Label::new(None);
            name.set_ellipsize(gtk::pango::EllipsizeMode::End);
            name.set_width_chars(1);
            let choose = gtk::Button::new();
            choose.add_css_class("flat");
            choose.add_css_class("customizable-target");
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

    fn set_items(&self, view: &BubbleView, theme: Theme) {
        let imp = self.imp();
        let mut items = imp.items.borrow_mut();
        while items.len() < view.chooser.len() {
            items.push(Item::new(self));
        }
        while items.len() > view.chooser.len() {
            items.pop().unwrap().remove();
        }
        let hot = imp.hot.get();
        for (i, (item, chooser)) in items.iter().zip(&view.chooser).enumerate() {
            item.set(chooser, theme, hot == Some(i));
        }
    }

    fn set_card(&self, w: &Rc<Workspace>, state: &UiState, view: &BubbleView) {
        let imp = self.imp();
        let shown = imp.card.borrow().as_ref().is_some_and(|c| c.root.is_child_visible());
        let folding = imp.fold.get().is_some_and(|(_, opening)| !opening);
        let Some(card) = view.card else {
            if shown && !folding {
                self.animate_fold(false);
            }
            return;
        };
        if imp.card.borrow().as_ref().is_some_and(|c| c.full != card.full) {
            imp.card.take().unwrap().root.unparent();
        }
        if imp.card.borrow().is_none() {
            *imp.card.borrow_mut() = Some(Card::new(w, self, view.id, card.full));
        }
        imp.card_size.set(card.size);
        let opening = !imp.card.borrow().as_ref().is_some_and(|c| c.root.is_child_visible()) || folding;
        if let Some(body) = imp.card.borrow().as_ref() {
            body.root.set_child_visible(true);
            body.refresh(w, state, card);
        }
        if opening {
            self.animate_fold(true);
        }
    }

    fn set_view(&self, view: BubbleView) {
        let imp = self.imp();
        if let Some(icon) = imp.icon.borrow().as_ref() {
            crate::icons::set(icon, Some(&format!("layer-{}-symbolic", view.icon)));
        }
        if let Some(face) = imp.face.borrow().as_ref() {
            set_class(face, "selected-tool", view.selected && matches!(view.kind, BubbleKind::Dial | BubbleKind::Brush | BubbleKind::Medium));
            face.set_opacity(if view.enabled { 1. } else { 0.45 });
            face.set_child_visible(view.card.is_none());
        }
        for coin in imp.coins.borrow().iter() {
            coin.set_child_visible(view.card.is_none() && view.layers > 1);
        }
        if let Some(count) = imp.count.borrow().as_ref() {
            count.set_label(&view.layers.to_string());
            count.set_child_visible(view.card.is_none());
        }
        if let Some(value) = imp.value.borrow().as_ref() {
            value.set_label(view.rings.first().map_or("", |r| r.text.as_str()));
        }
        let ring_icons = if view.kind == BubbleKind::Slider { 0 } else { view.rings.len() };
        {
            let mut icons = imp.icons.borrow_mut();
            while icons.len() < ring_icons {
                let icon = crate::icons::image("layer-settings-symbolic");
                icon.set_pixel_size(16);
                icon.set_can_target(false);
                icon.set_parent(self);
                icons.push(icon);
            }
            for (i, icon) in icons.iter().enumerate() {
                icon.set_child_visible(i < ring_icons);
                if let Some(ring) = view.rings.get(i) {
                    crate::icons::set(icon, Some(&format!("layer-{}-symbolic", ring.icon)));
                    icon.set_tooltip_text(Some(&ring.label));
                }
            }
        }
        if let Some(readout) = imp.readout.borrow().as_ref() {
            readout.set_visible(view.readout.is_some() && view.kind != BubbleKind::Slider);
            readout.set_label(view.readout.as_deref().unwrap_or_default());
        }
        self.update_property(&[gtk::accessible::Property::Label(&view.label)]);
        *imp.view.borrow_mut() = Some(view);
        self.queue_allocate();
        self.queue_draw();
    }

    fn hot_item(&self, p: [f32; 2]) {
        let view = self.imp().view.borrow();
        let hot = view.as_ref().and_then(|v| v.chooser.iter().position(|i| i.contains(p)));
        if self.imp().hot.replace(hot) != hot {
            for (i, (item, chooser)) in self.imp().items.borrow().iter().zip(view.iter().flat_map(|v| &v.chooser)).enumerate() {
                item.tag.set_visible(chooser.kind != ChooserKind::Brush && (chooser.shown || hot == Some(i)));
            }
            self.queue_allocate();
            self.queue_draw();
        }
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
        let send = |w: &Rc<Workspace>, action: BubbleAction| w.dispatch(UiAction::Bubble { action });
        drag.connect_drag_begin(glib::clone!(#[weak] w, move |gesture, x, y| {
            let Some(bubble) = gesture.widget().and_downcast::<Bubble>() else { return };
            let position = local(&bubble, x, y);
            let Some(view) = bubble.imp().view.borrow().clone() else { return };
            let picked = bubble.pick(x, y, gtk::PickFlags::DEFAULT);
            let contact = match bubble.region(&view, position, picked.as_ref()) {
                Region::Pass => {
                    gesture.set_state(gtk::EventSequenceState::Denied);
                    return;
                }
                Region::Handle => match surface_point(&w, &bubble, x, y) {
                    Some(at) => Contact::Pending(at),
                    None => return,
                },
                Region::Ring => Contact::Ring,
                Region::Beads => Contact::Beads(false),
                Region::Choose => Contact::Choose,
            };
            bubble.imp().contact.set(Some(contact));
            match contact {
                Contact::Ring => {
                    gesture.set_state(gtk::EventSequenceState::Claimed);
                    send(&w, BubbleAction::Ring { id, phase: ContactPhase::Down, position });
                }
                Contact::Choose => {
                    gesture.set_state(gtk::EventSequenceState::Claimed);
                    bubble.hot_item(position);
                    send(&w, BubbleAction::Choose { id, phase: ContactPhase::Down, position });
                }
                Contact::Beads(_) => send(&w, BubbleAction::Beads { id, phase: ContactPhase::Down, position }),
                _ => {}
            }
        }));
        drag.connect_drag_update(glib::clone!(#[weak] w, move |gesture, dx, dy| {
            let Some(bubble) = gesture.widget().and_downcast::<Bubble>() else { return };
            let Some((x0, y0)) = gesture.start_point() else { return };
            let (x, y) = (x0 + dx, y0 + dy);
            let position = local(&bubble, x, y);
            match bubble.imp().contact.get() {
                Some(Contact::Ring) => send(&w, BubbleAction::Ring { id, phase: ContactPhase::Move, position }),
                Some(Contact::Choose) => {
                    bubble.hot_item(position);
                    send(&w, BubbleAction::Choose { id, phase: ContactPhase::Move, position });
                }
                Some(Contact::Beads(claimed)) => {
                    if !claimed && w.surface.drag_check_threshold(0, 0, dx as i32, dy as i32) {
                        gesture.set_state(gtk::EventSequenceState::Claimed);
                        bubble.imp().contact.set(Some(Contact::Beads(true)));
                    }
                    send(&w, BubbleAction::Beads { id, phase: ContactPhase::Move, position });
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
                    } else {
                        bubble.imp().contact.set(None);
                    }
                }
                Some(Contact::Moving) => {
                    if let Some(at) = surface_point(&w, &bubble, x, y) {
                        w.bubbles.drag(&w, BubbleGrab::Bubble { id }, ContactPhase::Move, at);
                    }
                }
                Some(Contact::Pair) | None => {}
            }
        }));
        drag.connect_drag_end(glib::clone!(#[weak] w, move |gesture, dx, dy| {
            let Some(bubble) = gesture.widget().and_downcast::<Bubble>() else { return };
            let Some((x0, y0)) = gesture.start_point() else { return };
            if bubble.imp().contact.get() == Some(Contact::Pair) {
                return;
            }
            let (x, y) = (x0 + dx, y0 + dy);
            let position = local(&bubble, x, y);
            match bubble.imp().contact.take() {
                Some(Contact::Ring) => send(&w, BubbleAction::Ring { id, phase: ContactPhase::Up, position }),
                Some(Contact::Choose) => {
                    bubble.imp().hot.set(None);
                    send(&w, BubbleAction::Choose { id, phase: ContactPhase::Up, position });
                }
                Some(Contact::Beads(_)) => send(&w, BubbleAction::Beads { id, phase: ContactPhase::Up, position }),
                Some(Contact::Moving) => {
                    if let Some(at) = surface_point(&w, &bubble, x, y) {
                        w.bubbles.drag(&w, BubbleGrab::Bubble { id }, ContactPhase::Up, at);
                    }
                }
                Some(Contact::Pending(_)) => send(&w, BubbleAction::Tap { id }),
                Some(Contact::Pair) | None => {}
            }
        }));
        drag.connect_cancel(glib::clone!(#[weak] w, move |gesture, _| {
            let Some(bubble) = gesture.widget().and_downcast::<Bubble>() else { return };
            if bubble.imp().contact.get() == Some(Contact::Pair) {
                return;
            }
            let position = [0.; 2];
            match bubble.imp().contact.take() {
                Some(Contact::Ring) => send(&w, BubbleAction::Ring { id, phase: ContactPhase::Cancel, position }),
                Some(Contact::Choose) => {
                    bubble.imp().hot.set(None);
                    send(&w, BubbleAction::Choose { id, phase: ContactPhase::Cancel, position });
                }
                Some(Contact::Beads(_)) => send(&w, BubbleAction::Beads { id, phase: ContactPhase::Cancel, position }),
                Some(Contact::Moving) => {
                    w.bubbles.drag(&w, BubbleGrab::Bubble { id }, ContactPhase::Cancel, position);
                }
                _ => {}
            }
        }));
        self.add_controller(drag);

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
            let cancelled = [0.; 2];
            match bubble.imp().contact.replace(Some(Contact::Pair)) {
                Some(Contact::Ring) => send(&w, BubbleAction::Ring { id, phase: ContactPhase::Cancel, position: cancelled }),
                Some(Contact::Beads(_)) => send(&w, BubbleAction::Beads { id, phase: ContactPhase::Cancel, position: cancelled }),
                Some(Contact::Choose) => send(&w, BubbleAction::Choose { id, phase: ContactPhase::Cancel, position: cancelled }),
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
            let middle = bubble.center();
            let at = center(&w, gesture).or_else(|| surface_point(&w, &bubble, middle[0] as f64, middle[1] as f64));
            if let Some(at) = at {
                w.bubbles.drag(&w, BubbleGrab::Bubble { id }, ContactPhase::Up, at);
            }
            if let Some(pinch) = bubble.imp().pinch.take() {
                bubble.queue_draw();
                if (pinch - 1.).abs() > 0.01 {
                    send(&w, BubbleAction::Resize { id, scale: pinch });
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

pub(crate) struct Bubbles {
    pub(super) layer: Layer,
    tick: RefCell<Option<gtk::TickCallbackId>>,
    restore: Rc<RefCell<Option<glib::SourceId>>>,
    theme: Cell<Option<Theme>>,
}

impl Bubbles {
    pub(super) fn new() -> Self {
        let layer: Layer = glib::Object::new();
        layer.set_widget_name("tool-bubbles");
        Self { layer, tick: RefCell::new(None), restore: Rc::default(), theme: Cell::new(None) }
    }

    pub(super) fn refresh(&self, w: &Rc<Workspace>, state: &UiState) {
        let animations = gtk::Settings::default().is_none_or(|s| s.is_gtk_enable_animations());
        let Some((views, (stroke, faded))) = w.gpu.borrow_mut().as_mut().map(|g| {
            g.session.reduce_bubble_motion(!animations);
            (g.session.bubbles(), g.session.bubble_fade())
        }) else {
            return;
        };
        w.surface.imp().layout.borrow_mut().bubbles = state.workspace.layout.bubbles.clone();
        let colors = w.palette.get().map(|palette| {
            let [r, g, b, a] = palette.glass.chip.0;
            let rgb = |c: HexColor| gdk::RGBA::new(c.0[0] as f32 / 255., c.0[1] as f32 / 255., c.0[2] as f32 / 255., 1.);
            [gdk::RGBA::new(r, g, b, a), rgb(palette.text), rgb(palette.accent)]
        });
        let retheme = self.theme.replace(Some(state.theme)) != Some(state.theme);
        for bubble in self.layer.bubbles() {
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
            if bubble.imp().colors.replace(colors) != colors {
                bubble.queue_draw();
            }
            if retheme && let (Some(picture), Some(id)) = (bubble.imp().preview.borrow().as_ref(), view.preview) {
                picture.set_paintable(Some(&crate::previews::texture(id, state.theme)));
            }
            bubble.imp().display.set(Some((w.paint_view_color(), w.picker_headroom())));
            if let (Some(thumbnail), Some(layer)) = (bubble.imp().thumbnail.borrow().as_ref(), view.layer) {
                thumbnail.set_paintable(w.layer_panel.preview(layer).as_ref());
            }
            bubble.set_card(w, state, &view);
            if bubble.imp().view.borrow().as_ref() != Some(&view) || retheme {
                bubble.set_beads(w, &view);
                bubble.set_items(&view, state.theme);
                bubble.set_view(view);
            }
        }
        self.fade(stroke, &faded);
        self.refresh_wheels(&state.preview_colors(), w.paint_view_color(), w.picker_headroom(), state.color_picker.preview.is_some());
        self.animate(w);
    }

    fn fade(&self, stroke: bool, faded: &[u32]) {
        let show = |bubble: &Bubble, faded: bool| {
            bubble.set_opacity(if faded { FADED } else { 1. });
            bubble.set_can_target(!faded);
        };
        if stroke {
            if let Some(source) = self.restore.take() {
                source.remove();
            }
            for bubble in self.layer.bubbles() {
                show(&bubble, faded.contains(&bubble.id()));
            }
            return;
        }
        if self.restore.borrow().is_some() || self.layer.bubbles().iter().all(|b| b.can_target()) {
            return;
        }
        let layer = self.layer.downgrade();
        let restore = self.restore.clone();
        let source = glib::timeout_add_local_once(std::time::Duration::from_millis(FADE_RETURN_MS), move || {
            restore.take();
            for bubble in layer.upgrade().iter().flat_map(Layer::bubbles) {
                show(&bubble, false);
            }
        });
        *self.restore.borrow_mut() = Some(source);
    }

    pub(crate) fn layer_panels(&self) -> Vec<Rc<LayerPanel>> {
        self.layer.bubbles().iter().filter_map(|b| b.imp().card.borrow().as_ref().filter(|c| c.root.is_child_visible()).map(|c| c.layers.clone())).collect()
    }

    pub(crate) fn thumbnails(&self) -> Vec<(u64, gtk::Picture)> {
        self.layer
            .bubbles()
            .iter()
            .filter_map(|b| Some((b.imp().view.borrow().as_ref()?.layer?, b.imp().thumbnail.borrow().clone()?)))
            .collect()
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
