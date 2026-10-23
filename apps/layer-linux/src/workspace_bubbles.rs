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
const HOLD_SLOP: f32 = 8.;
const FADED: f64 = 0.2;
const HALO_BAND: f32 = 28.;
const FADE_RETURN_MS: u64 = 180;
const UNFOLD_US: i64 = 200_000;
const ITEM_IN_US: i64 = 180_000;
const ITEM_OUT_US: i64 = 120_000;
const HALO_IN_US: i64 = 160_000;
const HALO_OUT_US: i64 = 120_000;
const BEAD_TWEEN_US: i64 = 220_000;
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
    preview: RefCell<Option<gdk::Texture>>,
    name: gtk::Label,
    tag: gtk::Label,
}

struct Tween {
    from: Vec<BubbleBead>,
    to: Vec<BubbleBead>,
    start: i64,
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct Pair {
    start: f32,
    mid: [f32; 2],
    ended: bool,
}

struct Press {
    sequence: Option<usize>,
    region: Region,
    color: Option<layer_core::color::RgbColor>,
    start: [f32; 2],
    moving: bool,
    held: bool,
    hold: Option<glib::SourceId>,
}

impl Press {
    fn forget(&mut self) {
        if let Some(source) = self.hold.take() {
            source.remove();
        }
    }
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
        pub(super) preview: RefCell<Option<gdk::Texture>>,
        pub(super) buttons: RefCell<Vec<gtk::Image>>,
        pub(super) stamp: RefCell<Option<(u64, f32, gdk::Texture)>>,
        pub(super) stamp_patch: RefCell<Option<crate::display_color::ColorPatch>>,
        pub(super) stamp_tick: RefCell<Option<gtk::TickCallbackId>>,
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
        pub(super) shadow: Cell<f32>,
        pub(super) rings_shown: RefCell<Vec<BubbleRing>>,
        pub(super) halo_fade: Cell<Option<(i64, bool)>>,
        pub(super) born: RefCell<Vec<i64>>,
        pub(super) leaving: RefCell<Vec<(super::Item, ChooserItem, i64)>>,
        pub(super) tween: RefCell<Option<super::Tween>>,
        pub(super) ticking: Cell<bool>,
        pub(super) fingers: RefCell<Vec<(usize, [f32; 2])>>,
        pub(super) pair: Cell<Option<super::Pair>>,
        pub(super) press: RefCell<Option<super::Press>>,
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
            let obj = self.obj();
            let c = [width as f32 * 0.5, height as f32 * 0.5];
            let place = |widget: &gtk::Widget, at: [f32; 2], size: [f32; 2]| {
                allocate_at(widget, Bounds { x: c[0] + at[0] - size[0] * 0.5, y: c[1] + at[1] - size[1] * 0.5, width: size[0], height: size[1] });
            };
            if let Some(face) = self.face.borrow().as_ref() {
                place(face.upcast_ref(), [0.; 2], view.face);
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
            for (icon, ring) in self.icons.borrow().iter().zip(self.rings_shown.borrow().iter()) {
                place(icon.upcast_ref(), ring.arc.icon_point(), [16.; 2]);
            }
            if let (Some(patch), Some(stamp), Some((_, extent, _))) = (self.stamp_patch.borrow().as_ref(), view.stamp, self.stamp.borrow().as_ref()) {
                place(patch.upcast_ref(), stamp.center, [stamp.diameter * extent; 2]);
            }
            if let Some(readout) = self.readout.borrow().as_ref().filter(|r| r.is_visible()) {
                let (_, natural, _, _) = readout.measure(gtk::Orientation::Horizontal, -1);
                let (_, tall, _, _) = readout.measure(gtk::Orientation::Vertical, natural);
                place(readout.upcast_ref(), [0., view.halo.unwrap_or(view.diameter) * 0.5 + 16.], [natural as f32, tall as f32]);
            }
            let shown = obj.shown_beads();
            let colors = shown.iter().filter(|b| b.placement.bead != Bead::Palettes);
            for (patch, bead) in self.beads.borrow().iter().zip(colors) {
                place(patch.upcast_ref(), bead.placement.center, [bead.placement.size; 2]);
            }
            if let (Some(palettes), Some(bead)) = (self.palettes.borrow().as_ref(), shown.iter().find(|b| b.placement.bead == Bead::Palettes)) {
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
            for (item, chooser, _) in self.leaving.borrow().iter() {
                place(item.root.upcast_ref(), chooser.center, chooser.size());
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
            if self.contact.get().is_some() || !self.fingers.borrow().is_empty() {
                let reach = obj.reach(view) + SECOND_FINGER;
                return p[0].abs() <= reach && p[1].abs() <= reach;
            }
            obj.region(view, p, None) != Region::Pass || obj.passes(view, p)
        }
        fn snapshot(&self, snapshot: &gtk::Snapshot) {
            let obj = self.obj();
            let view = self.view.borrow();
            let Some(view) = view.as_ref() else { return };
            let now = obj.now();
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
            let shadow = self.shadow.get() * if placement.is_some_and(|p| p.lifted) { 1.5 } else { 1. };
            let lift = if placement.is_some_and(|p| p.lifted) { 1. } else { 0. };
            let round = |widget: &gtk::Widget| {
                if let Some(bounds) = widget.compute_bounds(&*obj) {
                    super::soft_shadow(snapshot, &bounds, shadow, lift);
                }
                let part = gtk::Snapshot::new();
                obj.snapshot_child(widget, &part);
                crate::squircle::append_round(snapshot, part);
            };
            let grow = |at: [f32; 2], scale: f32, alpha: f32| {
                snapshot.save();
                snapshot.translate(&graphene::Point::new(at[0], at[1]));
                snapshot.scale(scale, scale);
                snapshot.translate(&graphene::Point::new(-at[0], -at[1]));
                snapshot.push_opacity(alpha as f64);
            };
            let done = || {
                snapshot.pop();
                snapshot.restore();
            };
            let colors = self.colors.get();
            let (card_shown, face_shown) = obj.fold_progress(view.card.is_some());
            if let (Some(card), Some(progress)) = (self.card.borrow().as_ref().filter(|c| c.root.is_child_visible()), card_shown) {
                grow(c, 0.6 + 0.4 * progress, progress);
                obj.snapshot_child(&card.root, snapshot);
                done();
            }
            if let Some(progress) = face_shown {
                grow(c, 0.6 + 0.4 * progress, progress);
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
                if let Some(texture) = self.preview.borrow().as_ref() {
                    snapshot.push_opacity(0.78);
                    snapshot.append_texture(texture, &graphene::Rect::new(c[0] - 16., c[1] + 7., 32., 8.));
                    snapshot.pop();
                }
                if let Some(count) = self.count.borrow().as_ref() {
                    round(count.upcast_ref());
                }
                done();
            }
            if let Some(colors) = colors {
                if view.kind == BubbleKind::Medium {
                    super::dots(snapshot, c, view, colors[1]);
                }
                let alpha = obj.halo_alpha();
                let rings = self.rings_shown.borrow();
                if alpha > 0. && !rings.is_empty() {
                    grow(c, 0.85 + 0.15 * alpha, alpha);
                    for ring in rings.iter() {
                        snapshot.push_opacity(if ring.enabled { 1. } else { 0.4 });
                        super::ring(snapshot, c, ring, colors);
                        snapshot.pop();
                    }
                    for icon in self.icons.borrow().iter().take(rings.len()) {
                        obj.snapshot_child(icon, snapshot);
                    }
                    done();
                }
            }
            let shown = obj.shown_beads();
            let colored = shown.iter().filter(|b| b.placement.bead != Bead::Palettes);
            for (patch, bead) in self.beads.borrow().iter().zip(colored) {
                super::bead_shadow(snapshot, c, bead);
                let part = gtk::Snapshot::new();
                part.push_opacity(bead.placement.alpha as f64);
                obj.snapshot_child(patch, &part);
                part.pop();
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
            let born = self.born.borrow();
            let item = |item: &super::Item, chooser: &ChooserItem, scale: f32, alpha: f32| {
                let at = [c[0] + chooser.center[0], c[1] + chooser.center[1]];
                grow(at, scale, alpha);
                round(item.root.upcast_ref());
                if let Some(texture) = item.preview.borrow().as_ref().filter(|_| chooser.kind == ChooserKind::Brush) {
                    let [w, h] = BRUSH_PILL;
                    snapshot.append_texture(texture, &graphene::Rect::new(at[0] - w * 0.34, at[1] - h * 0.5 + 5., w * 0.68, 14.));
                }
                done();
            };
            for (i, (widget, chooser)) in self.items.borrow().iter().zip(&view.chooser).enumerate() {
                let alpha = ease(now.saturating_sub(born.get(i).copied().unwrap_or(0)), ITEM_IN_US);
                let hot = if hot != Some(i) { 1. } else if chooser.kind == ChooserKind::Brush { 1.06 } else { 1.12 };
                if alpha > 0. {
                    item(widget, chooser, (0.6 + 0.4 * alpha) * hot, alpha);
                }
                if widget.tag.is_visible() && alpha >= 1. {
                    round(widget.tag.upcast_ref());
                }
            }
            for (widget, chooser, start) in self.leaving.borrow().iter() {
                let alpha = 1. - ease(now - start, ITEM_OUT_US);
                if alpha > 0. {
                    item(widget, chooser, 0.6 + 0.4 * alpha, alpha);
                }
            }
            if let Some(readout) = self.readout.borrow().as_ref().filter(|r| r.is_visible()) {
                round(readout.upcast_ref());
            }
            if let (Some(patch), Some((_, _, texture))) = (self.stamp_patch.borrow().as_ref(), self.stamp.borrow().as_ref()) {
                snapshot.push_mask(gsk::MaskMode::Alpha);
                snapshot.append_texture(texture, &patch.compute_bounds(&*obj).unwrap_or_else(|| graphene::Rect::zero()));
                snapshot.pop();
                obj.snapshot_child(patch, snapshot);
                snapshot.pop();
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

fn ease(elapsed: i64, span: i64) -> f32 {
    1. - (1. - (elapsed as f32 / span as f32).clamp(0., 1.)).powi(3)
}

fn motion() -> bool {
    gtk::Settings::default().is_none_or(|s| s.is_gtk_enable_animations())
}

fn tweened(from: &[BubbleBead], to: &[BubbleBead], t: f32) -> Vec<BubbleBead> {
    let lerp = |a: f32, b: f32| a + (b - a) * t;
    let mut out: Vec<_> = to
        .iter()
        .map(|b| {
            let end = b.placement;
            let start = from.iter().find(|f| f.placement.bead == end.bead).map_or(BeadPlacement { alpha: 0., ..end }, |f| f.placement);
            let placement = BeadPlacement {
                center: [lerp(start.center[0], end.center[0]), lerp(start.center[1], end.center[1])],
                size: lerp(start.size, end.size),
                alpha: lerp(start.alpha, end.alpha),
                ..end
            };
            BubbleBead { placement, ..b.clone() }
        })
        .collect();
    out.extend(from.iter().filter(|f| !to.iter().any(|b| b.placement.bead == f.placement.bead)).map(|f| BubbleBead {
        placement: BeadPlacement { alpha: f.placement.alpha * (1. - t), ..f.placement },
        ..f.clone()
    }));
    out
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

fn ring(snapshot: &gtk::Snapshot, c: [f32; 2], ring: &BubbleRing, [chip, ink, thumb_color]: [gdk::RGBA; 3]) {
    let stroke = |width: f32| {
        let stroke = gsk::Stroke::new(width);
        stroke.set_line_cap(gsk::LineCap::Round);
        stroke
    };
    let arc = ring.arc;
    let [x, y] = arc.icon_point();
    let start = y.atan2(x).to_degrees();
    let lead = (start - arc.start + 540.).rem_euclid(360.) - 180.;
    let banded = RingArc { start, sweep: arc.sweep - lead, ..arc };
    snapshot.append_stroke(&arc_path(c, &banded, 1.), &stroke(HALO_BAND), &chip);
    snapshot.append_stroke(&arc_path(c, &arc, 1.), &stroke(RING_TRACK), &rgba(ink, 0.18));
    if ring.fill > 0.004 {
        snapshot.append_stroke(&arc_path(c, &arc, ring.fill), &stroke(RING_TRACK), &rgba(ink, 0.5));
    }
    let thumb = circle({
        let [x, y] = arc.point(ring.fill);
        [c[0] + x, c[1] + y]
    }, if ring.active { 7.5 } else { 7. });
    snapshot.append_fill(&thumb, gsk::FillRule::Winding, &thumb_color);
    snapshot.append_stroke(&thumb, &gsk::Stroke::new(1.), &rgba(ink, 0.6));
}

fn rounded(bounds: &graphene::Rect, grow: f32) -> gsk::RoundedRect {
    let rect = graphene::Rect::new(bounds.x() - grow, bounds.y() - grow, bounds.width() + grow * 2., bounds.height() + grow * 2.);
    gsk::RoundedRect::from_rect(rect, bounds.width().min(bounds.height()) * 0.5 + grow)
}

fn soft_shadow(snapshot: &gtk::Snapshot, bounds: &graphene::Rect, strength: f32, lift: f32) {
    let inner = rounded(bounds, 0.);
    for (grow, alpha) in [(1.5, 0.05), (3., 0.04), (5., 0.03), (8., 0.02)] {
        let path = gsk::PathBuilder::new();
        let mut outer = rounded(bounds, grow * (1. + lift * 0.5));
        outer.offset(0., 1.5 + lift * 2.5);
        path.add_rounded_rect(&outer);
        path.add_rounded_rect(&inner);
        snapshot.append_fill(&path.to_path(), gsk::FillRule::EvenOdd, &gdk::RGBA::new(0., 0., 0., alpha * strength));
    }
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
        let name = gtk::Label::new(None);
        name.add_css_class("bubble-brush-name");
        name.set_ellipsize(gtk::pango::EllipsizeMode::End);
        name.set_max_width_chars(14);
        name.set_margin_start(8);
        name.set_margin_end(8);
        name.set_margin_bottom(5);
        name.set_valign(gtk::Align::End);
        name.set_vexpand(true);
        root.append(&icon);
        root.append(&name);
        root.set_parent(bubble);
        let tag = gtk::Label::new(None);
        tag.add_css_class("bubble-tag");
        tag.set_can_target(false);
        tag.set_parent(bubble);
        Self { root, icon, preview: RefCell::new(None), name, tag }
    }
    fn set(&self, item: &ChooserItem, theme: Theme, hot: bool) {
        let brush = item.kind == ChooserKind::Brush;
        self.icon.set_visible(!brush || item.preview.is_none());
        crate::icons::set(&self.icon, Some(&format!("layer-{}-symbolic", item.icon)));
        *self.preview.borrow_mut() = item.preview.filter(|_| brush).map(|id| crate::previews::texture(id, theme));
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
    fn refresh(&self, w: &Rc<Workspace>, state: &UiState, card: &BubbleCard) {
        self.layers.refresh(state);
        self.effects.refresh(w, state);
        let page = if card.properties { "properties" } else { "layers" };
        if self.stack.visible_child_name().as_deref() != Some(page) {
            let transition = if card.properties { gtk::StackTransitionType::SlideUp } else { gtk::StackTransitionType::SlideDown };
            self.stack.set_visible_child_full(page, transition);
        }
        self.disclosure_label.set_label(&card.disclosure);
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
        let card = view.card.as_ref().map(|c| corner([0.; 2], c.size));
        let halo = view.halo.map(|size| size * 0.5 + 40.);
        beads.chain(items).chain(pill).chain(card).chain(halo).fold(view.diameter * 0.5, f32::max)
    }
    fn extent(&self) -> f32 {
        self.imp().view.borrow().as_ref().map_or(0., |view| (self.reach(view) * 2. + 24.).ceil())
    }
    fn set_placement(&self, placement: BubblePlacement) {
        let previous = self.imp().placement.replace(Some(placement));
        if previous.is_none_or(|p| (p.lifted, p.homing) != (placement.lifted, placement.homing)) {
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
        if let Some(card) = view.card.as_ref() {
            if p[0].abs() <= card.size[0] * 0.5 && p[1].abs() <= card.size[1] * 0.5 {
                let handles = self.imp().card.borrow().as_ref().is_none_or(|c| c.handles(picked));
                return if handles { Region::Handle } else { Region::Pass };
            }
        }
        if view.chooser.iter().any(|item| item.contains(p)) {
            return Region::Choose;
        }
        let arcs: Vec<_> = view.rings.iter().filter(|r| r.enabled).map(|r| r.arc).collect();
        if ring_at(&arcs, p).is_some() {
            return Region::Ring;
        }
        let beads: Vec<_> = view.beads.iter().map(|b| b.placement).collect();
        if bead_at(&beads, p).is_some() {
            return Region::Beads;
        }
        let r = p[0].hypot(p[1]);
        match view.kind {
            BubbleKind::Color if on_color_rim(view.diameter, view.options, view.pair, p) => Region::Handle,
            BubbleKind::Color => Region::Pass,
            BubbleKind::Layers if view.card.is_none() && p[0].abs() <= TOOL_BUBBLE * 0.5 && p[1].abs() <= TOOL_BUBBLE * 0.5 => Region::Handle,
            BubbleKind::History if history_button(p).is_some() => Region::Handle,
            BubbleKind::History | BubbleKind::Layers => Region::Pass,
            _ if r <= view.diameter * 0.5 => Region::Handle,
            _ => Region::Pass,
        }
    }
    fn passes(&self, view: &BubbleView, p: [f32; 2]) -> bool {
        let pill = view.pill.is_some() && palette_pill(view.diameter).contains(p[0], p[1]);
        let card = view.card.as_ref().is_some_and(|c| p[0].abs() <= c.size[0] * 0.5 && p[1].abs() <= c.size[1] * 0.5);
        pill || card || (view.kind == BubbleKind::Color && p[0].hypot(p[1]) <= view.diameter * 0.5)
    }

    fn new(w: &Rc<Workspace>, view: &BubbleView, theme: Theme) -> Self {
        let bubble: Self = glib::Object::new();
        bubble.set_widget_name(&format!("bubble-{}", view.id));
        let imp = bubble.imp();
        if view.kind == BubbleKind::Layers {
            for _ in 0..2 {
                let coin = glass("tool-bubble");
                coin.add_css_class("squircle-bubble");
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
                wheel.connect_pick(glib::clone!(#[weak] w, move |action| w.dispatch(UiAction::Color { action })));
                face.set_can_target(true);
                face.append(&wheel);
                *imp.wheel.borrow_mut() = Some(wheel);
            }
            BubbleKind::Layers => {
                face.add_css_class("squircle-bubble");
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
                icon.set_pixel_size(20);
                column.append(&icon);
                match view.kind {
                    BubbleKind::Medium => icon.set_margin_bottom(8),
                    BubbleKind::History => {
                        column.set_orientation(gtk::Orientation::Horizontal);
                        column.set_homogeneous(true);
                        column.set_halign(gtk::Align::Fill);
                        column.set_hexpand(true);
                        column.remove(&icon);
                        for button in &view.buttons {
                            let image = crate::icons::image(&format!("layer-{}-symbolic", button.icon));
                            image.set_pixel_size(20);
                            column.append(&image);
                            imp.buttons.borrow_mut().push(image);
                        }
                    }
                    _ => {}
                }
                if let Some(id) = view.preview {
                    icon.set_margin_bottom(12);
                    *imp.preview.borrow_mut() = Some(crate::previews::texture(id, theme));
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
        bubble.install_input(w, view.id);
        w.install_menu(&bubble, ContextTarget::Bubble { id: view.id });
        bubble
    }

    fn set_beads(&self, w: &Rc<Workspace>, view: &BubbleView) {
        let imp = self.imp();
        if view.kind != BubbleKind::Color {
            return;
        }
        let turning = imp.contact.get() == Some(Contact::Beads(true));
        let changed = imp.view.borrow().as_ref().is_some_and(|old| old.beads != view.beads && old.diameter == view.diameter);
        let tween = (changed && !turning && motion()).then(|| Tween { from: self.shown_beads(), to: view.beads.clone(), start: self.now() });
        let shown = tween.as_ref().map_or_else(|| view.beads.clone(), |t| tweened(&t.from, &t.to, 0.));
        *imp.tween.borrow_mut() = tween;
        self.sync_patches(&shown);
        self.ensure_ticking();
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

    fn sync_patches(&self, shown: &[BubbleBead]) {
        let imp = self.imp();
        let colors: Vec<_> = shown.iter().filter(|b| b.placement.bead != Bead::Palettes).collect();
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
        }
    }

    fn shown_beads(&self) -> Vec<BubbleBead> {
        let imp = self.imp();
        match imp.tween.borrow().as_ref() {
            Some(t) => tweened(&t.from, &t.to, ease(self.now() - t.start, BEAD_TWEEN_US)),
            None => imp.view.borrow().as_ref().map(|v| v.beads.clone()).unwrap_or_default(),
        }
    }

    fn now(&self) -> i64 {
        self.frame_clock().map_or_else(glib::monotonic_time, |clock| clock.frame_time())
    }

    fn halo_alpha(&self) -> f32 {
        match self.imp().halo_fade.get() {
            None => 1.,
            Some((start, true)) => ease(self.now() - start, HALO_IN_US),
            Some((start, false)) => 1. - ease(self.now() - start, HALO_OUT_US),
        }
    }

    fn ensure_ticking(&self) {
        if self.imp().ticking.replace(true) {
            return;
        }
        self.add_tick_callback(|bubble, _| {
            bubble.queue_allocate();
            bubble.queue_draw();
            if bubble.settle() {
                glib::ControlFlow::Continue
            } else {
                bubble.imp().ticking.set(false);
                glib::ControlFlow::Break
            }
        });
    }

    fn settle(&self) -> bool {
        let imp = self.imp();
        let now = self.now();
        if let Some((start, showing)) = imp.halo_fade.get()
            && now - start >= if showing { HALO_IN_US } else { HALO_OUT_US }
        {
            imp.halo_fade.set(None);
            if !showing {
                imp.rings_shown.borrow_mut().clear();
                self.sync_ring_icons();
            }
        }
        let (gone, kept): (Vec<_>, Vec<_>) = imp.leaving.take().into_iter().partition(|(_, _, start)| now - start >= ITEM_OUT_US);
        *imp.leaving.borrow_mut() = kept;
        for (item, _, _) in gone {
            item.remove();
        }
        if imp.tween.borrow().as_ref().is_some_and(|t| now - t.start >= BEAD_TWEEN_US) {
            imp.tween.take();
            let beads = imp.view.borrow().as_ref().map(|v| v.beads.clone()).unwrap_or_default();
            self.sync_patches(&beads);
        }
        imp.halo_fade.get().is_some()
            || !imp.leaving.borrow().is_empty()
            || imp.tween.borrow().is_some()
            || imp.born.borrow().iter().any(|born| now - born < ITEM_IN_US)
    }

    fn sync_ring_icons(&self) {
        let imp = self.imp();
        let rings = imp.rings_shown.borrow();
        let count = rings.len();
        let mut icons = imp.icons.borrow_mut();
        while icons.len() < count {
            let icon = crate::icons::image("layer-settings-symbolic");
            icon.set_pixel_size(16);
            icon.set_can_target(false);
            icon.set_parent(self);
            icons.push(icon);
        }
        for (i, icon) in icons.iter().enumerate() {
            icon.set_child_visible(i < count);
            if let Some(ring) = rings.get(i) {
                crate::icons::set(icon, Some(&format!("layer-{}-symbolic", ring.icon)));
                icon.set_tooltip_text(Some(&ring.label));
                icon.set_opacity(if ring.enabled { 1. } else { 0.4 });
            }
        }
    }

    fn set_rings(&self, view: &BubbleView) {
        let imp = self.imp();
        let showing = !imp.rings_shown.borrow().is_empty() && !matches!(imp.halo_fade.get(), Some((_, false)));
        if !view.rings.is_empty() {
            *imp.rings_shown.borrow_mut() = view.rings.clone();
            if !showing {
                imp.halo_fade.set(motion().then(|| (self.now(), true)));
            }
        } else if showing && motion() {
            imp.halo_fade.set(Some((self.now(), false)));
        } else if !matches!(imp.halo_fade.get(), Some((_, false))) {
            imp.rings_shown.borrow_mut().clear();
        }
        self.ensure_ticking();
    }

    fn set_items(&self, view: &BubbleView, theme: Theme) {
        let imp = self.imp();
        let previous: Vec<ChooserItem> = imp.view.borrow().as_ref().map(|v| v.chooser.clone()).unwrap_or_default();
        let now = self.now();
        let mut items = imp.items.borrow_mut();
        if view.chooser.is_empty() && !items.is_empty() {
            let closing: Vec<_> = items.drain(..).zip(previous).collect();
            imp.born.borrow_mut().clear();
            drop(items);
            for (item, chooser) in closing {
                item.tag.set_visible(false);
                if motion() {
                    imp.leaving.borrow_mut().push((item, chooser, now));
                } else {
                    item.remove();
                }
            }
            self.ensure_ticking();
            return;
        }
        while items.len() < view.chooser.len() {
            items.push(Item::new(self));
        }
        while items.len() > view.chooser.len() {
            items.pop().unwrap().remove();
        }
        let mut born = imp.born.borrow_mut();
        born.resize(view.chooser.len(), 0);
        let mut fresh = 0;
        for (i, chooser) in view.chooser.iter().enumerate() {
            if !previous.get(i).is_some_and(|p| p.kind == chooser.kind && p.label == chooser.label) {
                let stagger = if chooser.kind == ChooserKind::Brush { 16_000 } else { 14_000 };
                born[i] = if motion() { now + fresh * stagger } else { 0 };
                fresh += 1;
            }
        }
        drop(born);
        let hot = imp.hot.get();
        for (i, (item, chooser)) in items.iter().zip(&view.chooser).enumerate() {
            item.set(chooser, theme, hot == Some(i));
        }
        drop(items);
        self.ensure_ticking();
    }

    fn set_card(&self, w: &Rc<Workspace>, state: &UiState, view: &BubbleView) {
        let imp = self.imp();
        let shown = imp.card.borrow().as_ref().is_some_and(|c| c.root.is_child_visible());
        let folding = imp.fold.get().is_some_and(|(_, opening)| !opening);
        let Some(card) = view.card.as_ref() else {
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
            set_class(face, "selected-tool", view.selected && matches!(view.kind, BubbleKind::Dial | BubbleKind::Brush | BubbleKind::Medium | BubbleKind::Controls));
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
        for (image, button) in imp.buttons.borrow().iter().zip(&view.buttons) {
            image.set_opacity(if button.enabled { 1. } else { 0.35 });
            image.set_tooltip_text(Some(&button.label));
        }
        self.set_rings(&view);
        if let Some(readout) = imp.readout.borrow().as_ref() {
            readout.set_visible(view.readout.is_some());
            readout.set_label(view.readout.as_deref().unwrap_or_default());
        }
        self.update_property(&[gtk::accessible::Property::Label(&view.label)]);
        *imp.view.borrow_mut() = Some(view);
        self.sync_ring_icons();
        self.queue_allocate();
        self.queue_draw();
    }

    fn set_stamp(&self, w: &Rc<Workspace>, view: &BubbleView) {
        let imp = self.imp();
        let Some(stamp) = view.stamp else {
            if let Some(patch) = imp.stamp_patch.take() {
                patch.unparent();
                imp.stamp.take();
                self.queue_draw();
            }
            return;
        };
        let (display, headroom) = imp.display.get().unwrap_or_default();
        imp.stamp_patch
            .borrow_mut()
            .get_or_insert_with(|| {
                let patch = crate::display_color::ColorPatch::new(false);
                patch.set_parent(self);
                patch
            })
            .set_display_color(stamp.color, display, headroom);
        if imp.stamp_tick.borrow().is_none() {
            let tick = self.add_tick_callback(glib::clone!(#[weak] w, #[upgrade_or] glib::ControlFlow::Break, move |bubble, _| {
                if bubble.poll_stamp(&w) {
                    return glib::ControlFlow::Continue;
                }
                bubble.imp().stamp_tick.take();
                glib::ControlFlow::Break
            }));
            *imp.stamp_tick.borrow_mut() = Some(tick);
        }
    }

    fn poll_stamp(&self, w: &Rc<Workspace>) -> bool {
        let imp = self.imp();
        if imp.stamp_patch.borrow().is_none() {
            return false;
        }
        let known = imp.stamp.borrow().as_ref().map(|(id, _, _)| *id);
        let stamp = w.gpu.borrow_mut().as_mut().and_then(|g| {
            let context = g.session.state().toolbar_context();
            g.session.toolbar_stamp(context, known).ok().flatten()
        });
        if let Some(stamp) = stamp.filter(|s| !s.alpha.is_empty()) {
            let size = stamp.size as i32;
            let texture = gdk::MemoryTexture::new(size, size, gdk::MemoryFormat::A8, &glib::Bytes::from_owned(stamp.alpha), stamp.size as usize);
            imp.stamp.replace(Some((stamp.request_id, stamp.extent, texture.upcast())));
            self.queue_allocate();
            self.queue_draw();
        }
        true
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

    fn install_input(&self, w: &Rc<Workspace>, id: u32) {
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
            let Some(at) = surface_point(&w, &bubble, x, y) else { return };
            let state = match bubble.begin_contact(&w, id, bubble.region(&view, position, picked.as_ref()), position, at) {
                None => gtk::EventSequenceState::Denied,
                Some(Contact::Ring | Contact::Choose) => gtk::EventSequenceState::Claimed,
                Some(_) => return,
            };
            gesture.set_state(state);
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
                Some(Contact::Pending(_)) => send(&w, BubbleAction::Tap { id, position }),
                Some(Contact::Pair) | None => {}
            }
        }));
        drag.connect_cancel(glib::clone!(#[weak] w, move |gesture, _| {
            let Some(bubble) = gesture.widget().and_downcast::<Bubble>() else { return };
            if bubble.imp().contact.get() != Some(Contact::Pair) {
                bubble.cancel_contact(&w, id, [0.; 2]);
            }
        }));
        self.add_controller(drag);

        let fingers = gtk::EventControllerLegacy::new();
        fingers.set_propagation_phase(gtk::PropagationPhase::Capture);
        fingers.connect_event(glib::clone!(#[weak] w, #[upgrade_or] glib::Propagation::Proceed, move |controller, event| {
            let Some(bubble) = controller.widget().and_downcast::<Bubble>() else { return glib::Propagation::Proceed };
            if matches!(event.event_type(), gdk::EventType::EnterNotify | gdk::EventType::MotionNotify | gdk::EventType::ButtonPress | gdk::EventType::ProximityIn) {
                let pen = event.device_tool().is_some() || event.device().is_some_and(|d| d.source() == gdk::InputSource::Pen);
                if bubble.cursor().is_some() != pen {
                    bubble.set_cursor_from_name(pen.then_some("none"));
                }
            }
            let Some(at) = w.event_point(controller) else { return glib::Propagation::Proceed };
            let sequence = || event.event_sequence().as_ptr() as usize;
            let stop = match event.event_type() {
                gdk::EventType::TouchBegin => bubble.finger_down(&w, id, sequence(), at),
                gdk::EventType::TouchUpdate => bubble.finger_moved(&w, id, sequence(), at),
                gdk::EventType::TouchEnd => bubble.finger_up(&w, id, sequence(), at, false),
                gdk::EventType::TouchCancel => bubble.finger_up(&w, id, sequence(), at, true),
                gdk::EventType::ButtonPress => {
                    if event.downcast_ref::<gdk::ButtonEvent>().is_some_and(|b| b.button() == gdk::BUTTON_PRIMARY) {
                        bubble.press_down(&w, id, None, at);
                    }
                    false
                }
                gdk::EventType::MotionNotify => bubble.press_moved(&w, id, None, at),
                gdk::EventType::ButtonRelease => {
                    bubble.press_up(&w, id, None, at, false);
                    false
                }
                _ => false,
            };
            if stop { glib::Propagation::Stop } else { glib::Propagation::Proceed }
        }));
        self.add_controller(fingers);
    }

    fn local(&self, at: [f32; 2]) -> [f32; 2] {
        let center = self.imp().placement.get().map_or([0.; 2], |p| p.center);
        [at[0] - center[0], at[1] - center[1]]
    }

    fn begin_contact(&self, w: &Rc<Workspace>, id: u32, region: Region, position: [f32; 2], at: [f32; 2]) -> Option<Contact> {
        let send = |action: BubbleAction| w.dispatch(UiAction::Bubble { action });
        let phase = ContactPhase::Down;
        let contact = match region {
            Region::Pass => return None,
            Region::Handle => Contact::Pending(at),
            Region::Ring => {
                send(BubbleAction::Ring { id, phase, position });
                Contact::Ring
            }
            Region::Choose => {
                self.hot_item(position);
                send(BubbleAction::Choose { id, phase, position });
                Contact::Choose
            }
            Region::Beads => {
                send(BubbleAction::Beads { id, phase, position });
                Contact::Beads(false)
            }
        };
        self.imp().contact.set(Some(contact));
        Some(contact)
    }

    fn cancel_contact(&self, w: &Rc<Workspace>, id: u32, at: [f32; 2]) {
        let send = |action: BubbleAction| w.dispatch(UiAction::Bubble { action });
        let (phase, position) = (ContactPhase::Cancel, [0.; 2]);
        match self.imp().contact.take() {
            Some(Contact::Ring) => send(BubbleAction::Ring { id, phase, position }),
            Some(Contact::Choose) => {
                self.imp().hot.set(None);
                send(BubbleAction::Choose { id, phase, position });
            }
            Some(Contact::Beads(_)) => send(BubbleAction::Beads { id, phase, position }),
            Some(Contact::Moving) => {
                w.bubbles.drag(w, BubbleGrab::Bubble { id }, phase, at);
            }
            _ => {}
        }
    }

    fn restore_color(&self, w: &Rc<Workspace>, color: layer_core::color::RgbColor) {
        let current = w.gpu.borrow().as_ref().map(|g| g.session.state().colors.definition());
        if current.is_some_and(|c| c != color) {
            w.dispatch(UiAction::Color { action: ColorAction::Definition { color } });
        }
    }

    fn cancel_pick(&self, w: &Rc<Workspace>, color: layer_core::color::RgbColor) {
        if let Some(wheel) = self.imp().wheel.borrow().as_ref() {
            wheel.cancel_pick();
        }
        self.restore_color(w, color);
    }

    fn press_down(&self, w: &Rc<Workspace>, id: u32, sequence: Option<usize>, at: [f32; 2]) {
        if let Some(mut previous) = self.imp().press.take() {
            previous.forget();
        }
        let Some(view) = self.imp().view.borrow().clone() else { return };
        let Some(point) = w.surface.compute_point(self, &graphene::Point::new(at[0], at[1])) else { return };
        let (x, y) = (point.x() as f64, point.y() as f64);
        let p = self.local(at);
        let region = self.region(&view, p, self.pick(x, y, gtk::PickFlags::DEFAULT).as_ref());
        let wheel = view.kind == BubbleKind::Color && region == Region::Pass && p[0].hypot(p[1]) <= view.diameter * 0.5;
        if !wheel && !super::customization::owns_context(self.upcast_ref(), x, y) {
            return;
        }
        let color = w.gpu.borrow().as_ref().map(|g| g.session.state().colors.definition()).filter(|_| wheel);
        let long = gtk::Settings::default().map_or(500, |s| s.gtk_long_press_time()) as u64;
        let delay = if view.kind == BubbleKind::Color { long * 2 } else { long };
        let hold = glib::timeout_add_local_once(
            std::time::Duration::from_millis(delay),
            glib::clone!(#[weak(rename_to = bubble)] self, #[weak] w, move || bubble.press_held(&w, id)),
        );
        *self.imp().press.borrow_mut() = Some(Press { sequence, region, color, start: at, moving: false, held: false, hold: Some(hold) });
    }

    fn press_held(&self, w: &Rc<Workspace>, id: u32) {
        let (start, color) = {
            let mut press = self.imp().press.borrow_mut();
            let Some(press) = press.as_mut() else { return };
            press.hold = None;
            if press.moving {
                return;
            }
            press.held = true;
            (press.start, press.color)
        };
        if let Some(color) = color {
            self.restore_color(w, color);
        }
        self.cancel_contact(w, id, start);
        let [x, y] = self.local(start);
        let [cx, cy] = self.center();
        w.show_held_context(self.upcast_ref(), ContextTarget::Bubble { id }, (cx + x) as f64, (cy + y) as f64);
    }

    fn press_moved(&self, w: &Rc<Workspace>, id: u32, sequence: Option<usize>, at: [f32; 2]) -> bool {
        let imp = self.imp();
        let (held, moving, color, region) = match imp.press.borrow_mut().as_mut().filter(|c| c.sequence == sequence) {
            Some(press) => {
                let far = (at[0] - press.start[0]).hypot(at[1] - press.start[1]) > HOLD_SLOP;
                if press.held && !far {
                    return true;
                }
                if far {
                    press.forget();
                }
                let held = std::mem::take(&mut press.held);
                (held, press.moving, press.color, press.region)
            }
            None => return false,
        };
        if held {
            w.dismiss_context();
            if color.is_none() {
                self.begin_contact(w, id, region, self.local(at), at);
            }
            return false;
        }
        if moving {
            w.bubbles.drag(w, BubbleGrab::Rim { id }, ContactPhase::Move, at);
            return true;
        }
        let size = imp.view.borrow().as_ref().map_or(0., |v| v.diameter);
        let Some(color) = color.filter(|_| color_drag_out(size, self.local(at))) else { return false };
        self.cancel_pick(w, color);
        let lifted = w.bubbles.drag(w, BubbleGrab::Rim { id }, ContactPhase::Down, at);
        match imp.press.borrow_mut().as_mut() {
            Some(press) if lifted => press.moving = true,
            _ => return false,
        }
        true
    }

    fn press_up(&self, w: &Rc<Workspace>, id: u32, sequence: Option<usize>, at: [f32; 2], cancel: bool) {
        let Some(mut press) = self.imp().press.borrow_mut().take_if(|c| c.sequence == sequence) else { return };
        press.forget();
        if press.held && cancel {
            w.dismiss_context();
        }
        if press.moving {
            w.bubbles.drag(w, BubbleGrab::Rim { id }, if cancel { ContactPhase::Cancel } else { ContactPhase::Up }, at);
        } else if let Some(color) = press.color.filter(|_| cancel) {
            self.cancel_pick(w, color);
        }
    }

    fn pair_points(&self) -> Option<([f32; 2], f32)> {
        let fingers = self.imp().fingers.borrow();
        let [(_, a), (_, b)] = [*fingers.first()?, *fingers.get(1)?];
        Some(([(a[0] + b[0]) * 0.5, (a[1] + b[1]) * 0.5], (a[0] - b[0]).hypot(a[1] - b[1])))
    }

    fn finger_down(&self, w: &Rc<Workspace>, id: u32, sequence: usize, at: [f32; 2]) -> bool {
        let count = {
            let mut fingers = self.imp().fingers.borrow_mut();
            fingers.push((sequence, at));
            fingers.len()
        };
        match count {
            1 => {
                self.press_down(w, id, Some(sequence), at);
                false
            }
            2 => {
                self.begin_pair(w, id);
                true
            }
            _ => true,
        }
    }

    fn finger_moved(&self, w: &Rc<Workspace>, id: u32, sequence: usize, at: [f32; 2]) -> bool {
        if let Some(finger) = self.imp().fingers.borrow_mut().iter_mut().find(|f| f.0 == sequence) {
            finger.1 = at;
        }
        match self.imp().pair.get() {
            Some(pair) => {
                if !pair.ended {
                    self.update_pair(w, id);
                }
                true
            }
            None => self.press_moved(w, id, Some(sequence), at),
        }
    }

    fn finger_up(&self, w: &Rc<Workspace>, id: u32, sequence: usize, at: [f32; 2], cancel: bool) -> bool {
        let imp = self.imp();
        imp.fingers.borrow_mut().retain(|f| f.0 != sequence);
        match imp.pair.get() {
            Some(pair) => {
                if !pair.ended {
                    self.end_pair(w, id, cancel);
                }
                if imp.fingers.borrow().is_empty() {
                    imp.pair.set(None);
                }
            }
            None => self.press_up(w, id, Some(sequence), at, cancel),
        }
        false
    }

    fn begin_pair(&self, w: &Rc<Workspace>, id: u32) {
        let imp = self.imp();
        let Some((mid, distance)) = self.pair_points() else { return };
        self.cancel_contact(w, id, mid);
        imp.contact.set(Some(Contact::Pair));
        if let Some(mut press) = imp.press.take() {
            press.forget();
            if press.held {
                w.dismiss_context();
            }
            if press.moving {
                w.bubbles.drag(w, BubbleGrab::Rim { id }, ContactPhase::Cancel, mid);
            }
            if let Some(color) = press.color {
                self.cancel_pick(w, color);
            }
        }
        imp.hot.set(None);
        let lifted = w.bubbles.drag(w, BubbleGrab::Bubble { id }, ContactPhase::Down, mid);
        if !lifted {
            imp.contact.set(None);
        }
        imp.pair.set(Some(Pair { start: distance.max(1.), mid, ended: !lifted }));
    }

    fn update_pair(&self, w: &Rc<Workspace>, id: u32) {
        let imp = self.imp();
        let (Some((mid, distance)), Some(pair)) = (self.pair_points(), imp.pair.get()) else { return };
        imp.pair.set(Some(Pair { mid, ..pair }));
        let color = imp.view.borrow().as_ref().filter(|v| v.kind == BubbleKind::Color).map(|v| v.diameter);
        if let Some(size) = color {
            imp.pinch.set(Some(pinched_size(size, distance / pair.start) / size));
            self.queue_draw();
        }
        w.bubbles.drag(w, BubbleGrab::Bubble { id }, ContactPhase::Move, mid);
    }

    fn end_pair(&self, w: &Rc<Workspace>, id: u32, cancel: bool) {
        let imp = self.imp();
        let Some(pair) = imp.pair.get() else { return };
        imp.pair.set(Some(Pair { ended: true, ..pair }));
        imp.contact.set(None);
        w.bubbles.drag(w, BubbleGrab::Bubble { id }, if cancel { ContactPhase::Cancel } else { ContactPhase::Up }, pair.mid);
        if let Some(pinch) = imp.pinch.take() {
            self.queue_draw();
            if !cancel && (pinch - 1.).abs() > 0.01 {
                w.dispatch(UiAction::Bubble { action: BubbleAction::Resize { id, scale: pinch } });
            }
        }
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
            [gdk::RGBA::new(r, g, b, a), rgb(palette.text), rgb(palette.thumb)]
        });
        let retheme = self.theme.replace(Some(state.theme)) != Some(state.theme);
        for bubble in self.layer.bubbles() {
            let keep = views.iter().find(|v| v.id == bubble.id()).is_some_and(|v| {
                bubble.imp().view.borrow().as_ref().is_some_and(|old| old.source == v.source)
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
            bubble.imp().shadow.set(if adw::StyleManager::default().is_dark() { 1.4 } else { 1. });
            if bubble.imp().colors.replace(colors) != colors {
                bubble.queue_draw();
            }
            if retheme && let Some(id) = view.preview.filter(|_| bubble.imp().preview.borrow().is_some()) {
                *bubble.imp().preview.borrow_mut() = Some(crate::previews::texture(id, state.theme));
            }
            bubble.imp().display.set(Some((w.paint_view_color(), w.picker_headroom())));
            if let (Some(thumbnail), Some(layer)) = (bubble.imp().thumbnail.borrow().as_ref(), view.layer) {
                thumbnail.set_paintable(w.layer_panel.preview(layer).as_ref());
            }
            bubble.set_card(w, state, &view);
            bubble.set_stamp(w, &view);
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
