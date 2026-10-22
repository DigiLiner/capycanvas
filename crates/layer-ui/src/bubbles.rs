//! Tool bubbles: toolbar tools and brushes kept on the canvas. Shared rules own
//! sources, sizes, ring geometry and how bubbles push each other; hosts only
//! present them and forward contacts.
use crate::*;
use serde::{Deserialize, Serialize};

pub const TOOL_BUBBLE: f32 = 56.;
pub const SLIDER_BUBBLE: f32 = 96.;
pub const COLOR_RING_SIZES: [f32; 3] = [128., 192., 256.];
pub const MAX_RING_SETTINGS: usize = 6;
pub const RING_WIDTH: f32 = 18.;
const RING_STEP: f32 = RING_WIDTH + 6.;
const RING_GAP: f32 = 12.;
const FRICTION: f32 = 0.963;
const RETURN: f32 = 0.054;
const MAX_KICK: f32 = 3.5;
const STEP_NS: u64 = 4_166_667;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum BubbleSource {
    Control { control: ToolbarControl },
    Medium { group: ToolGroup },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum BubbleKind {
    Dial,
    Brush,
    Medium,
    Slider,
    Color,
    Layers,
}

impl BubbleSource {
    pub fn for_control(control: ToolbarControl) -> Option<Self> {
        let control = match control {
            ToolbarControl::Opacity => ToolbarControl::BrushOpacitySlider,
            ToolbarControl::Color
            | ToolbarControl::Brush { .. }
            | ToolbarControl::BrushSizeSlider
            | ToolbarControl::BrushOpacitySlider
            | ToolbarControl::BrushStrengthSlider
            | ToolbarControl::Panel { panel: Panel::Layers } => control,
            control if control.has_variants() => control,
            _ => return None,
        };
        Some(Self::Control { control })
    }
    pub fn tears_without_hold(control: ToolbarControl) -> bool {
        Self::for_control(control).is_some() && !control.is_component()
    }
    pub fn for_variant(variant: ToolVariant) -> Option<Self> {
        match variant {
            ToolVariant::BrushGroup { group } => Some(Self::Medium { group }),
            ToolVariant::BrushPreset { id } => Some(Self::Control { control: ToolbarControl::Brush { id } }),
            _ => None,
        }
    }
    pub fn kind(self) -> BubbleKind {
        match self {
            Self::Medium { .. } => BubbleKind::Medium,
            Self::Control { control: ToolbarControl::Brush { .. } } => BubbleKind::Brush,
            Self::Control { control: ToolbarControl::Color } => BubbleKind::Color,
            Self::Control { control: ToolbarControl::Panel { .. } } => BubbleKind::Layers,
            Self::Control { control } if control.slider().is_some() => BubbleKind::Slider,
            Self::Control { .. } => BubbleKind::Dial,
        }
    }
    pub fn control(self) -> Option<ToolbarControl> {
        match self {
            Self::Control { control } => Some(control),
            Self::Medium { .. } => None,
        }
    }
    pub fn label(self, localization: &Localizer) -> String {
        match self {
            Self::Control { control } => customization::tool_choice_localized(control, localization).label,
            Self::Medium { group } => group.localized_label(localization).to_string(),
        }
    }
    pub(crate) fn validate(self) -> Result<(), String> {
        let valid = match self {
            Self::Control { control } => {
                control.validate().is_ok()
                    && Self::for_control(control) == Some(self)
                    && !matches!(control, ToolbarControl::Brush { id } if preset(id).is_err())
            }
            Self::Medium { .. } => true,
        };
        valid.then_some(()).ok_or_else(|| "Invalid tool bubble".into())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct BubbleOptions {
    pub size: f32,
    pub recent: bool,
    pub palette: bool,
    pub pair: bool,
    pub full: bool,
    pub fold: bool,
}
impl Default for BubbleOptions {
    fn default() -> Self {
        Self { size: COLOR_RING_SIZES[1], recent: true, palette: true, pair: true, full: false, fold: true }
    }
}
impl BubbleOptions {
    fn is_default(&self) -> bool {
        *self == Self::default()
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ToolBubble {
    pub id: u32,
    pub source: BubbleSource,
    pub center: [f32; 2],
    #[serde(default, skip_serializing_if = "BubbleOptions::is_default")]
    pub options: BubbleOptions,
}
impl ToolBubble {
    pub fn diameter(&self) -> f32 {
        match self.source.kind() {
            BubbleKind::Slider => SLIDER_BUBBLE,
            BubbleKind::Color => self.options.size,
            _ => TOOL_BUBBLE,
        }
    }
    pub(crate) fn reach(&self) -> f32 {
        match self.source.kind() {
            BubbleKind::Color => color_ring_reach(self.options.size, self.options),
            BubbleKind::Layers => TOOL_BUBBLE * 0.54,
            _ => self.diameter() * 0.5,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RingSettings {
    pub brush: u32,
    pub settings: Vec<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct BubbleLayout {
    pub bubbles: Vec<ToolBubble>,
    pub rings: Vec<RingSettings>,
}
impl BubbleLayout {
    pub fn is_empty(&self) -> bool {
        self.bubbles.is_empty() && self.rings.is_empty()
    }
    pub fn get(&self, id: u32) -> Option<&ToolBubble> {
        self.bubbles.iter().find(|b| b.id == id)
    }
    pub(crate) fn get_mut(&mut self, id: u32) -> Result<&mut ToolBubble, String> {
        self.bubbles.iter_mut().find(|b| b.id == id).ok_or_else(|| "The bubble no longer exists".into())
    }
    pub fn find(&self, source: BubbleSource) -> Option<&ToolBubble> {
        self.bubbles.iter().find(|b| b.source == source)
    }
    pub fn ring(&self, brush: u32) -> Option<&[String]> {
        self.rings.iter().find(|r| r.brush == brush).map(|r| r.settings.as_slice())
    }
    pub(crate) fn set_ring(&mut self, brush: u32, settings: Vec<String>) {
        self.rings.retain(|r| r.brush != brush);
        self.rings.push(RingSettings { brush, settings });
    }
    pub(crate) fn validate(&self, ids: &mut std::collections::BTreeSet<u32>) -> Result<(), String> {
        let mut sources = Vec::new();
        for bubble in &self.bubbles {
            bubble.source.validate()?;
            if !ids.insert(bubble.id)
                || sources.contains(&bubble.source)
                || !bubble.center.into_iter().all(f32::is_finite)
                || !(COLOR_RING_SIZES[0]..=COLOR_RING_SIZES[2]).contains(&bubble.options.size)
            {
                return Err("Invalid tool bubble".into());
            }
            sources.push(bubble.source);
        }
        let mut brushes = Vec::new();
        for ring in &self.rings {
            let mut seen = Vec::new();
            if preset(ring.brush).is_err()
                || brushes.contains(&ring.brush)
                || ring.settings.len() > MAX_RING_SETTINGS
                || ring.settings.iter().any(|s| seen.contains(&s) || { seen.push(s); !tool_settings::defined(s) })
            {
                return Err("Invalid bubble ring settings".into());
            }
            brushes.push(ring.brush);
        }
        Ok(())
    }
}

pub const BUBBLE_BEAD: f32 = 22.;
pub const RECENT_BEADS: usize = 8;
const BEAD_STEP: f32 = BUBBLE_BEAD + 4.;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Bead {
    Recent { index: usize },
    Swatch { index: usize },
    Foreground,
    Background,
    Transparent,
    Palettes,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BeadStrip {
    Recent,
    Palette,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
pub struct BeadPlacement {
    pub bead: Bead,
    pub center: [f32; 2],
    pub alpha: f32,
}

const RIM: f32 = 10.;

fn bead_orbit(size: f32) -> f32 {
    size * 0.49 + RIM + BUBBLE_BEAD * 0.5
}

fn palette_top(size: f32) -> f32 {
    bead_orbit(size) + 2.
}

pub fn color_rim(size: f32) -> [f32; 2] {
    [size * 0.49, size * 0.49 + RIM]
}

fn palette_columns(size: f32) -> usize {
    ((size / BEAD_STEP).floor() as usize).max(3)
}

pub(crate) fn bead_scroll_limit(strip: BeadStrip, size: f32, recent: usize, swatches: usize) -> f32 {
    match strip {
        BeadStrip::Recent => recent.saturating_sub(RECENT_BEADS) as f32,
        BeadStrip::Palette => (swatches + 1).div_ceil(2).saturating_sub(palette_columns(size)) as f32,
    }
}

pub(crate) fn bead_scroll(scroll: f32, pixels: f32, limit: f32) -> f32 {
    (scroll - pixels / BEAD_STEP).clamp(0., limit)
}

pub fn bead_strip_at(size: f32, options: BubbleOptions, point: [f32; 2]) -> Option<BeadStrip> {
    let radius = point[0].hypot(point[1]);
    if options.palette && point[1] > palette_top(size) - BEAD_STEP * 0.5 && point[1] < palette_top(size) + BEAD_STEP * 1.5 {
        return Some(BeadStrip::Palette);
    }
    (options.recent && point[1] < 0. && (radius - bead_orbit(size)).abs() <= BUBBLE_BEAD * 0.5 + 1.).then_some(BeadStrip::Recent)
}

pub fn color_beads(size: f32, options: BubbleOptions, recent: usize, swatches: usize, scroll: [f32; 2]) -> Vec<BeadPlacement> {
    let orbit = bead_orbit(size);
    let at = |degrees: f32| {
        let a = degrees.to_radians();
        [orbit * a.cos(), orbit * a.sin()]
    };
    let fade = |offset: f32, visible: usize| (1. - (offset.abs() - (visible as f32 - 1.) * 0.5).max(0.) * 0.65).clamp(0., 1.);
    let mut beads = Vec::new();
    if options.recent {
        let step = (BEAD_STEP / orbit).to_degrees();
        let first = scroll[0].floor() as isize - 1;
        for index in (first..first + RECENT_BEADS as isize + 3).filter(|i| *i >= 0 && (*i as usize) < recent) {
            let offset = index as f32 - scroll[0] - (RECENT_BEADS as f32 - 1.) * 0.5;
            let alpha = fade(offset, RECENT_BEADS);
            if alpha > 0.05 {
                beads.push(BeadPlacement { bead: Bead::Recent { index: index as usize }, center: at(270. + offset * step), alpha });
            }
        }
    }
    if options.pair {
        let (foreground, transparent) = if options.palette { (174., 10.) } else { (132., 48.) };
        for (bead, angle) in [(Bead::Background, foreground + 13.), (Bead::Foreground, foreground), (Bead::Transparent, transparent)] {
            beads.push(BeadPlacement { bead, center: at(angle), alpha: 1. });
        }
    }
    if options.palette {
        let columns = palette_columns(size);
        let top = palette_top(size);
        for k in 0..=swatches {
            let offset = (k / 2) as f32 - scroll[1] - (columns as f32 - 1.) * 0.5;
            let alpha = fade(offset, columns);
            if alpha > 0.05 {
                let bead = if k == swatches { Bead::Palettes } else { Bead::Swatch { index: k } };
                beads.push(BeadPlacement { bead, center: [offset * BEAD_STEP, top + (k % 2) as f32 * BEAD_STEP], alpha });
            }
        }
    }
    beads
}

pub fn palette_pill(size: f32) -> Bounds {
    let top = palette_top(size) + BEAD_STEP + BUBBLE_BEAD * 0.5 + 8.;
    Bounds { x: -74., y: top, width: 148., height: 28. }
}

pub(crate) fn color_ring_reach(size: f32, options: BubbleOptions) -> f32 {
    let ring = bead_orbit(size) + BUBBLE_BEAD * 0.5;
    if !options.palette {
        return ring;
    }
    let half = (palette_columns(size) as f32 - 1.) * 0.5 * BEAD_STEP;
    ring.max(half.hypot(palette_top(size) + BEAD_STEP) + BUBBLE_BEAD * 0.5)
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
pub struct RingArc {
    pub radius: f32,
    pub width: f32,
    pub start: f32,
    pub sweep: f32,
}
impl RingArc {
    pub fn point(&self, fraction: f32) -> [f32; 2] {
        let angle = (self.start + self.sweep * fraction.clamp(0., 1.)).to_radians();
        [self.radius * angle.cos(), self.radius * angle.sin()]
    }
    fn offset(&self, point: [f32; 2]) -> f32 {
        let angle = point[1].atan2(point[0]).to_degrees();
        (if self.sweep >= 0. { angle - self.start } else { self.start - angle }).rem_euclid(360.)
    }
    pub fn fraction(&self, point: [f32; 2]) -> f32 {
        let span = self.sweep.abs();
        let offset = self.offset(point);
        if offset <= span {
            offset / span
        } else if offset - span < 360. - offset {
            1.
        } else {
            0.
        }
    }
    pub fn contains(&self, point: [f32; 2]) -> bool {
        let reach = self.width * 0.5 + 4.;
        let offset = self.offset(point);
        let slack = reach / self.radius.max(1.) * 57.3;
        ((point[0].hypot(point[1])) - self.radius).abs() <= reach
            && (offset <= self.sweep.abs() + slack || offset >= 360. - slack)
    }
}

pub fn slider_ring(diameter: f32) -> RingArc {
    RingArc { radius: diameter * 0.5 - RING_WIDTH * 0.5 - 4., width: RING_WIDTH, start: 120., sweep: 300. }
}

pub fn halo_arcs(count: usize, diameter: f32) -> Vec<RingArc> {
    let inner = diameter * 0.5 + RING_GAP + RING_WIDTH * 0.5;
    let outer = inner + RING_STEP;
    let arc = |radius: f32, start: f32, sweep: f32| RingArc { radius, width: RING_WIDTH, start, sweep };
    let three = |radius| [arc(radius, 142., 76.), arc(radius, 232., 76.), arc(radius, 38., -76.)];
    let two = |radius| [arc(radius, 112., 136.), arc(radius, 68., -136.)];
    match count {
        0 => Vec::new(),
        1 => vec![arc(inner, 135., 270.)],
        2 => two(inner).to_vec(),
        3 => three(inner).to_vec(),
        4 => [two(inner), two(outer)].concat(),
        5 => [three(inner).to_vec(), two(outer).to_vec()].concat(),
        _ => [three(inner), three(outer)].concat(),
    }
}

pub(crate) fn halo_reach(count: usize, diameter: f32) -> f32 {
    halo_arcs(count, diameter).iter().map(|a| a.radius + a.width * 0.5).fold(diameter * 0.5, f32::max)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum BubbleGrab {
    Bubble { id: u32 },
    Tile { panel: Panel, tile: u32 },
    Header { id: u32 },
    Variant { variant: ToolVariant },
}

impl BubbleGrab {
    pub fn for_choice(action: &UiAction) -> Option<Self> {
        let variant = match action {
            UiAction::SelectBrush { id } => ToolVariant::BrushPreset { id: *id },
            UiAction::SelectToolGroup { group } | UiAction::SelectBrushSet { group } => ToolVariant::BrushGroup { group: *group },
            UiAction::ChooseToolVariant { variant, .. } => *variant,
            _ => return None,
        };
        BubbleSource::for_variant(variant).map(|_| Self::Variant { variant })
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum BubbleAction {
    Drag { grab: BubbleGrab, phase: ContactPhase, position: [f32; 2], viewport: [f32; 2] },
    Tap { id: u32 },
    Ring { id: u32, phase: ContactPhase, position: [f32; 2] },
    PutBack { id: u32 },
    Bead { id: u32, bead: Bead },
    Scroll { id: u32, strip: BeadStrip, pixels: f32 },
    Palette { id: u32, step: i32 },
    SetOptions { id: u32, options: BubbleOptions },
    ToggleRingSetting { brush: u32, setting: String },
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct BubbleRing {
    pub arc: RingArc,
    pub setting: &'static str,
    pub label: std::sync::Arc<str>,
    pub icon: &'static str,
    pub fill: f32,
    pub text: String,
    pub active: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct BubbleView {
    pub id: u32,
    pub source: BubbleSource,
    pub kind: BubbleKind,
    pub diameter: f32,
    pub label: String,
    pub icon: &'static str,
    pub preview: Option<u32>,
    pub variants: bool,
    pub dots: Vec<bool>,
    pub selected: bool,
    pub enabled: bool,
    pub open: bool,
    pub rings: Vec<BubbleRing>,
    pub beads: Vec<BubbleBead>,
    pub pill: Option<String>,
    pub options: BubbleOptions,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct BubbleBead {
    pub placement: BeadPlacement,
    pub color: Option<layer_core::color::RgbColor>,
    pub label: String,
    pub selected: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
pub struct BubblePlacement {
    pub id: u32,
    pub center: [f32; 2],
    pub lifted: bool,
    pub homing: bool,
}

#[derive(Clone, Copy, Debug)]
struct Body {
    id: u32,
    pos: [f32; 2],
    rest: [f32; 2],
    vel: [f32; 2],
    reach: f32,
    kicked: bool,
}

#[derive(Clone, Debug, Default)]
pub(crate) struct Desk {
    bodies: Vec<Body>,
    grabbed: Option<u32>,
    grown: Option<(u32, f32)>,
    homing: bool,
    bounds: Option<Bounds>,
    clock: Option<u64>,
    moved: bool,
}

fn add(a: [f32; 2], b: [f32; 2]) -> [f32; 2] {
    [a[0] + b[0], a[1] + b[1]]
}
fn sub(a: [f32; 2], b: [f32; 2]) -> [f32; 2] {
    [a[0] - b[0], a[1] - b[1]]
}
fn scale(a: [f32; 2], s: f32) -> [f32; 2] {
    [a[0] * s, a[1] * s]
}
fn length(a: [f32; 2]) -> f32 {
    a[0].hypot(a[1])
}

impl Desk {
    pub(crate) fn matches(&self, layout: &BubbleLayout, bounds: Bounds) -> bool {
        self.bounds == Some(bounds)
            && self.bodies.len() == layout.bubbles.len()
            && self.bodies.iter().zip(&layout.bubbles).all(|(body, bubble)| {
                body.id == bubble.id
                    && body.reach == bubble.reach()
                    && (body.rest == bubble.center || body.kicked || self.grabbed == Some(body.id))
            })
    }
    pub(crate) fn home(&mut self, homing: bool) {
        if self.homing != homing {
            self.homing = homing;
            self.moved = true;
        }
    }
    pub(crate) fn sync(&mut self, layout: &BubbleLayout, bounds: Bounds) {
        self.bounds = Some(bounds);
        let previous = std::mem::take(&mut self.bodies);
        self.bodies = layout
            .bubbles
            .iter()
            .map(|bubble| {
                let reach = bubble.reach();
                match previous.iter().find(|b| b.id == bubble.id) {
                    Some(body) => Body {
                        reach,
                        rest: if body.kicked || self.grabbed == Some(body.id) { body.rest } else { bubble.center },
                        ..*body
                    },
                    None => Body { id: bubble.id, pos: bubble.center, rest: bubble.center, vel: [0.; 2], reach, kicked: false },
                }
            })
            .collect();
        if self.grabbed.is_some_and(|id| !self.bodies.iter().any(|b| b.id == id)) {
            self.grabbed = None;
        }
        self.moved = true;
        self.separate();
    }
    pub(crate) fn grab(&mut self, id: u32) {
        self.grabbed = self.bodies.iter().any(|b| b.id == id).then_some(id);
        if let Some(body) = self.body_mut(id) {
            body.kicked = false;
            body.vel = [0.; 2];
        }
    }
    pub(crate) fn drag(&mut self, center: [f32; 2]) {
        let Some(id) = self.grabbed else { return };
        if let Some(body) = self.body_mut(id) {
            body.pos = center;
            body.rest = center;
        }
        self.moved = true;
        for _ in 0..4 {
            self.separate();
        }
    }
    pub(crate) fn release(&mut self) {
        if let Some(id) = self.grabbed.take()
            && let Some(body) = self.body_mut(id)
        {
            body.rest = body.pos;
        }
        self.moved = true;
    }
    pub(crate) fn grow(&mut self, grown: Option<(u32, f32)>) {
        if self.grown != grown {
            self.grown = grown;
            self.moved = true;
        }
    }
    pub(crate) fn center(&self, id: u32) -> Option<[f32; 2]> {
        self.bodies.iter().find(|b| b.id == id).map(|b| b.pos)
    }
    pub(crate) fn placements(&self) -> Vec<BubblePlacement> {
        self.bodies
            .iter()
            .map(|b| {
                let lifted = self.grabbed == Some(b.id);
                BubblePlacement { id: b.id, center: b.pos, lifted, homing: lifted && self.homing }
            })
            .collect()
    }
    pub(crate) fn moving(&self) -> bool {
        self.moved || self.grabbed.is_some() || self.bodies.iter().any(|b| b.kicked)
    }
    pub(crate) fn advance(&mut self, now_ns: u64) -> bool {
        let last = *self.clock.get_or_insert(now_ns.saturating_sub(STEP_NS));
        let steps = (now_ns.saturating_sub(last) / STEP_NS).min(32);
        self.clock = Some(if steps == 32 { now_ns } else { last + steps * STEP_NS });
        let before: Vec<_> = self.bodies.iter().map(|b| b.pos).collect();
        for _ in 0..steps {
            self.step();
        }
        if steps > 0 {
            self.moved = self.bodies.iter().zip(before).any(|(b, p)| length(sub(b.pos, p)) > 0.01);
        }
        if !self.moving() {
            self.clock = None;
        }
        self.moving()
    }
    pub(crate) fn settled(&self) -> Vec<(u32, [f32; 2])> {
        let mut desk = self.clone();
        desk.release();
        for _ in 0..2400 {
            if !desk.bodies.iter().any(|b| b.kicked) {
                break;
            }
            desk.step();
        }
        desk.bodies.iter().map(|b| (b.id, if b.kicked { b.pos } else { b.rest })).collect()
    }
    fn body_mut(&mut self, id: u32) -> Option<&mut Body> {
        self.bodies.iter_mut().find(|b| b.id == id)
    }
    fn reach(&self, body: &Body) -> f32 {
        match self.grown {
            Some((id, reach)) if id == body.id => reach.max(body.reach),
            _ => body.reach,
        }
    }
    fn priority(&self, body: &Body) -> u8 {
        if self.grabbed == Some(body.id) {
            3
        } else if self.grown.is_some_and(|(id, _)| id == body.id) {
            2
        } else {
            u8::from(body.kicked)
        }
    }
    fn step(&mut self) {
        for body in &mut self.bodies {
            if self.grabbed == Some(body.id) {
                continue;
            }
            if body.kicked {
                body.pos = add(body.pos, body.vel);
                body.vel = scale(body.vel, FRICTION);
                if length(body.vel) < 0.05 {
                    body.vel = [0.; 2];
                    body.kicked = false;
                    body.rest = body.pos;
                }
            } else {
                let towards = sub(body.rest, body.pos);
                body.pos = if length(towards) < 0.25 { body.rest } else { add(body.pos, scale(towards, RETURN)) };
            }
        }
        for _ in 0..4 {
            self.separate();
        }
    }
    fn separate(&mut self) {
        let count = self.bodies.len();
        for i in 0..count {
            for j in i + 1..count {
                let (a, b) = (self.bodies[i], self.bodies[j]);
                let delta = sub(b.pos, a.pos);
                let distance = length(delta);
                let overlap = self.reach(&a) + self.reach(&b) + WORKSPACE_SPACING - distance;
                if overlap <= 0. {
                    continue;
                }
                let normal = if distance > 1e-3 { scale(delta, 1. / distance) } else { [1., 0.] };
                let (pa, pb) = (self.priority(&a), self.priority(&b));
                let mut push = |index: usize, direction: [f32; 2], amount: f32, kick: bool| {
                    let body = &mut self.bodies[index];
                    body.pos = add(body.pos, scale(direction, amount));
                    if kick {
                        body.kicked = true;
                        body.vel = scale(direction, amount.min(MAX_KICK));
                    }
                };
                if pa > pb {
                    push(j, normal, overlap, pa != 2);
                } else if pb > pa {
                    push(i, scale(normal, -1.), overlap, pb != 2);
                } else {
                    push(j, normal, overlap * 0.5, false);
                    push(i, scale(normal, -1.), overlap * 0.5, false);
                }
            }
        }
        let Some(bounds) = self.bounds else { return };
        for index in 0..count {
            let reach = self.reach(&self.bodies[index]);
            let body = &mut self.bodies[index];
            let lo = [bounds.x + reach, bounds.y + reach];
            let hi = [bounds.x + bounds.width - reach, bounds.y + bounds.height - reach];
            for axis in 0..2 {
                let clamped = if lo[axis] > hi[axis] { (lo[axis] + hi[axis]) * 0.5 } else { body.pos[axis].clamp(lo[axis], hi[axis]) };
                if clamped != body.pos[axis] {
                    body.pos[axis] = clamped;
                    body.vel[axis] = 0.;
                }
            }
        }
    }
}

#[cfg(test)]
#[path = "bubble_tests.rs"]
mod tests;
