use crate::*;
use serde::{Deserialize, Serialize};

#[path = "bubble_desk.rs"]
mod desk;
pub(crate) use desk::{Desk, Shape};

pub const TOOL_BUBBLE: f32 = 56.;
pub const HISTORY_BUBBLE: [f32; 2] = [104., 56.];
pub const CONTROL_RINGS: [ToolbarNumericBinding; 2] = [ToolbarNumericBinding::BrushSize, ToolbarNumericBinding::BrushStrength];
pub const HISTORY_COMMANDS: [CommandId; 2] = [CommandId::Undo, CommandId::Redo];
pub const COLOR_RING_SIZES: [f32; 3] = [136., 216., 344.];
pub const COLOR_RING_RANGE: [f32; 2] = [128., 352.];
pub const RING_RIM: f32 = 7.;
pub const BUBBLE_BEAD: f32 = 24.;
pub const PALETTE_BEAD: f32 = 26.;
pub const BEAD_STEP: f32 = 28.;
pub const RECENT_BEADS: usize = 8;
pub const MAX_RING_SETTINGS: usize = 6;
pub const RING_TRACK: f32 = 4.;
pub const HALO_RINGS: [f32; 2] = [53., 88.];
pub const HALO_BANDS: [f32; 2] = [30., 30.];
pub const RING_ICON: f32 = 21.;
pub const CHOOSER_ITEM: f32 = 44.;
pub const CHOOSER_RING: f32 = 84.;
pub const BRUSH_PILL: [f32; 2] = [128., 48.];
pub const PALETTE_PILL: [f32; 2] = [184., 30.];
pub const STROKE_FADE: f32 = 48.;
const BRUSH_COLUMN: f32 = 170.;
const BRUSH_FAN: f32 = 96.;
const PILL_PITCH: f32 = BRUSH_PILL[1] + 4.;

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
    Controls,
    History,
    Color,
    Layers,
}

impl BubbleSource {
    pub fn for_control(control: ToolbarControl) -> Option<Self> {
        let control = match control {
            ToolbarControl::Command { command: CommandId::Redo } => ToolbarControl::Command { command: CommandId::Undo },
            ToolbarControl::Command { command: CommandId::Undo }
            | ToolbarControl::Color
            | ToolbarControl::ColorPicker
            | ToolbarControl::Brush { .. }
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
            Self::Control { control: ToolbarControl::ColorPicker } => BubbleKind::Controls,
            Self::Control { control: ToolbarControl::Command { command: CommandId::Undo } } => BubbleKind::History,
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
        Self { size: COLOR_RING_SIZES[1], recent: false, palette: false, pair: false, full: false, fold: true }
    }
}
impl BubbleOptions {
    fn is_default(&self) -> bool {
        *self == Self::default()
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Corner {
    #[default]
    TopLeft,
    TopRight,
    BottomLeft,
    BottomRight,
}
impl Corner {
    fn point(self, viewport: [f32; 2]) -> [f32; 2] {
        match self {
            Self::TopLeft => [0., 0.],
            Self::TopRight => [viewport[0], 0.],
            Self::BottomLeft => [0., viewport[1]],
            Self::BottomRight => viewport,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ToolBubble {
    pub id: u32,
    pub source: BubbleSource,
    pub corner: Corner,
    pub offset: [f32; 2],
    #[serde(default, skip_serializing_if = "BubbleOptions::is_default")]
    pub options: BubbleOptions,
}

pub fn pinched_size(size: f32, scale: f32) -> f32 {
    (size * scale).clamp(COLOR_RING_RANGE[0], COLOR_RING_RANGE[1])
}

impl ToolBubble {
    pub(crate) fn new(id: u32, source: BubbleSource, center: [f32; 2], viewport: [f32; 2]) -> Self {
        let mut bubble = Self { id, source, corner: Corner::TopLeft, offset: center, options: BubbleOptions::default() };
        bubble.place(center, viewport);
        bubble
    }
    pub fn center(&self, viewport: [f32; 2]) -> [f32; 2] {
        let corner = self.corner.point(viewport);
        [corner[0] + self.offset[0], corner[1] + self.offset[1]]
    }
    pub(crate) fn place(&mut self, center: [f32; 2], viewport: [f32; 2]) {
        let corner = match (center[0] * 2. > viewport[0], center[1] * 2. > viewport[1]) {
            (false, false) => Corner::TopLeft,
            (true, false) => Corner::TopRight,
            (false, true) => Corner::BottomLeft,
            (true, true) => Corner::BottomRight,
        };
        let point = corner.point(viewport);
        self.corner = corner;
        self.offset = [center[0] - point[0], center[1] - point[1]];
    }
    pub fn diameter(&self, narrow: bool) -> f32 {
        match self.source.kind() {
            BubbleKind::Controls => halo_disc(CONTROL_RINGS.len()),
            BubbleKind::History => HISTORY_BUBBLE[0],
            BubbleKind::Color if narrow => self.options.size.min(COLOR_RING_SIZES[0]),
            BubbleKind::Color => self.options.size,
            _ => TOOL_BUBBLE,
        }
    }
    pub(crate) fn shape(&self, narrow: bool, pair: PairBeads) -> Shape {
        match self.source.kind() {
            BubbleKind::Color => Shape::circle(color_reach(self.diameter(narrow), self.options, pair)),
            BubbleKind::Layers => Shape::rect([TOOL_BUBBLE; 2]),
            BubbleKind::History => Shape::rect(HISTORY_BUBBLE),
            _ => Shape::circle(self.diameter(narrow) * 0.5),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RingSettings {
    pub brush: u32,
    pub settings: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct KeptOptions {
    pub source: BubbleSource,
    pub options: BubbleOptions,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct BubbleLayout {
    pub bubbles: Vec<ToolBubble>,
    pub rings: Vec<RingSettings>,
    pub kept: Vec<KeptOptions>,
}
impl BubbleLayout {
    pub fn is_empty(&self) -> bool {
        self.bubbles.is_empty() && self.rings.is_empty() && self.kept.is_empty()
    }
    pub(crate) fn put_back(&mut self, gone: impl Fn(&ToolBubble) -> bool) {
        let (removed, staying): (Vec<_>, Vec<_>) = std::mem::take(&mut self.bubbles).into_iter().partition(|b| gone(b));
        self.bubbles = staying;
        for bubble in removed {
            self.kept.retain(|k| k.source != bubble.source);
            if bubble.options != BubbleOptions::default() {
                self.kept.push(KeptOptions { source: bubble.source, options: bubble.options });
            }
        }
    }
    pub(crate) fn options_for(&self, source: BubbleSource) -> BubbleOptions {
        self.kept.iter().find(|k| k.source == source).map_or_else(BubbleOptions::default, |k| k.options)
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
                || !bubble.offset.into_iter().all(f32::is_finite)
                || !(COLOR_RING_RANGE[0]..=COLOR_RING_RANGE[1]).contains(&bubble.options.size)
            {
                return Err("Invalid tool bubble".into());
            }
            sources.push(bubble.source);
        }
        let mut kept = Vec::new();
        for k in &self.kept {
            k.source.validate()?;
            if kept.contains(&k.source) || !(COLOR_RING_RANGE[0]..=COLOR_RING_RANGE[1]).contains(&k.options.size) {
                return Err("Invalid kept bubble options".into());
            }
            kept.push(k.source);
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

fn degrees(point: [f32; 2]) -> f32 {
    point[1].atan2(point[0]).to_degrees().rem_euclid(360.)
}

fn polar(radius: f32, degrees: f32) -> [f32; 2] {
    let a = degrees.to_radians();
    [radius * a.cos(), radius * a.sin()]
}

fn turn(from: f32, to: f32) -> f32 {
    (to - from + 540.).rem_euclid(360.) - 180.
}

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

impl Bead {
    pub fn strip(self) -> Option<BeadStrip> {
        match self {
            Self::Recent { .. } => Some(BeadStrip::Recent),
            Self::Swatch { .. } => Some(BeadStrip::Palette),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
pub struct BeadPlacement {
    pub bead: Bead,
    pub center: [f32; 2],
    pub size: f32,
    pub alpha: f32,
}

fn orbit(size: f32, row: usize) -> f32 {
    size * 0.5 + 19. + row as f32 * BEAD_STEP
}

fn bead_degrees(size: f32) -> f32 {
    (BEAD_STEP / orbit(size, 0)).to_degrees()
}

fn strip_columns(strip: BeadStrip, size: f32) -> usize {
    let step = bead_degrees(size);
    match strip {
        BeadStrip::Recent => ((104. / step) as usize + 1).min(RECENT_BEADS),
        BeadStrip::Palette => (112. / step) as usize + 1,
    }
}

fn strip_angle(strip: BeadStrip, size: f32, column: f32, last: f32) -> f32 {
    let (middle, direction) = match strip {
        BeadStrip::Recent => (270., 1.),
        BeadStrip::Palette => (90., -1.),
    };
    middle + direction * (column - last * 0.5) * bead_degrees(size)
}

fn strip_rows(strip: BeadStrip) -> usize {
    match strip {
        BeadStrip::Recent => 1,
        BeadStrip::Palette => 2,
    }
}

pub(crate) fn bead_scroll_limit(strip: BeadStrip, size: f32, count: usize) -> f32 {
    count.div_ceil(strip_rows(strip)).saturating_sub(strip_columns(strip, size)) as f32
}

pub(crate) fn bead_travel(strip: BeadStrip, size: f32, from: [f32; 2], to: [f32; 2]) -> f32 {
    let beads = turn(degrees(from), degrees(to)) / bead_degrees(size);
    match strip {
        BeadStrip::Recent => -beads,
        BeadStrip::Palette => beads,
    }
}

const FADE_COLUMNS: f32 = 0.8;
const PALETTE_GAP: f32 = 1.35;
const BEAD_CLEARANCE: f32 = 3.;

fn strip_last(strip: BeadStrip, size: f32, count: usize) -> f32 {
    strip_columns(strip, size).min(count.div_ceil(strip_rows(strip))) as f32 - 1.
}

fn faded(column: f32, last: f32) -> f32 {
    let excess = (-column).max(column - last).max(0.);
    (1. - excess / FADE_COLUMNS).clamp(0., 1.)
}

fn strip_beads(beads: &mut Vec<BeadPlacement>, strip: BeadStrip, size: f32, count: usize, scroll: f32) {
    let rows = strip_rows(strip);
    let last = strip_last(strip, size, count);
    for index in 0..count {
        let column = (index / rows) as f32 - scroll;
        let alpha = faded(column, last);
        if alpha > 0. {
            let bead = if strip == BeadStrip::Palette { Bead::Swatch { index } } else { Bead::Recent { index } };
            let center = polar(orbit(size, index % rows), strip_angle(strip, size, column, last));
            beads.push(BeadPlacement { bead, center, size: BUBBLE_BEAD * (0.6 + 0.4 * alpha), alpha });
        }
    }
}

fn strip_edges(strip: BeadStrip, size: f32, count: usize) -> Vec<([f32; 2], f32)> {
    let last = strip_last(strip, size, count);
    let columns = (0..=8).flat_map(|i| [-(i as f32) * 0.1, last + i as f32 * 0.1]);
    columns
        .flat_map(|column| (0..strip_rows(strip)).map(move |row| (column, row)))
        .map(|(column, row)| {
            let radius = BUBBLE_BEAD * 0.5 * (0.6 + 0.4 * faded(column, last));
            (polar(orbit(size, row), strip_angle(strip, size, column, last)), radius)
        })
        .collect()
}

fn clear(center: [f32; 2], radius: f32, obstacles: &[([f32; 2], f32)]) -> bool {
    obstacles.iter().all(|(at, r)| (center[0] - at[0]).hypot(center[1] - at[1]) >= radius + r + BEAD_CLEARANCE)
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
pub struct PairBeads {
    pub foreground: f32,
    pub background: f32,
    pub gap: f32,
}

impl PairBeads {
    const SMALLEST: Self = Self { foreground: 30., background: 24., gap: 10. };

    pub fn for_window(width: f32) -> Self {
        let stage = Panel::Color.default_width(width) - 2. * PANEL_CONTENT_INSET;
        let Some((layout, wheel)) = ColorPanelLayout::new(stage).and_then(|l| Some((l, ColorWheelGeometry::new(l.wheel[2])?))) else {
            return Self::SMALLEST;
        };
        let center = [layout.wheel[0] + layout.wheel[2] * 0.5, layout.wheel[1] + layout.wheel[3] * 0.5];
        let gap = [layout.foreground, layout.background, layout.transparent]
            .map(|[x, y, w, _]| (x + w * 0.5 - center[0]).hypot(y + w * 0.5 - center[1]) - wheel.outer - w * 0.5)
            .into_iter()
            .fold(f32::MAX, f32::min);
        Self { foreground: layout.foreground[2], background: layout.background[2], gap }
    }

    fn offset(self) -> [f32; 2] {
        let (fg, bg) = (self.foreground, self.background);
        [0.04 * fg + 0.5 * bg, 0.5 * fg - 0.24 * bg]
    }
}

fn pair_at(size: f32, angle: f32, pair: PairBeads) -> [[f32; 2]; 2] {
    let radius = size * 0.5 + pair.gap;
    let offset = pair.offset();
    let u = polar(1., angle);
    let along = offset[0] * u[0] + offset[1] * u[1];
    let reach = (radius + pair.background * 0.5).powi(2) - (offset[0] * offset[0] + offset[1] * offset[1]) + along * along;
    let distance = (radius + pair.foreground * 0.5).max(reach.max(0.).sqrt() - along);
    let foreground = polar(distance, angle);
    [foreground, [foreground[0] + offset[0], foreground[1] + offset[1]]]
}

pub fn color_beads(size: f32, options: BubbleOptions, pair: PairBeads, recent: usize, swatches: usize, scroll: [f32; 2], front: Bead) -> Vec<BeadPlacement> {
    let step = bead_degrees(size);
    let mut beads = Vec::new();
    let mut obstacles = Vec::new();
    if options.recent {
        strip_beads(&mut beads, BeadStrip::Recent, size, recent, scroll[0]);
        obstacles.extend(strip_edges(BeadStrip::Recent, size, recent));
    }
    let mut palette = None;
    if options.palette {
        strip_beads(&mut beads, BeadStrip::Palette, size, swatches, scroll[1]);
        obstacles.extend(strip_edges(BeadStrip::Palette, size, swatches));
        let last = strip_last(BeadStrip::Palette, size, swatches);
        let angle = strip_angle(BeadStrip::Palette, size, last + PALETTE_GAP, last);
        let middle = (orbit(size, 0) + orbit(size, 1)) * 0.5;
        let center = polar(middle / (PALETTE_GAP * step).to_radians().cos(), angle);
        palette = Some(BeadPlacement { bead: Bead::Palettes, center, size: PALETTE_BEAD, alpha: 1. });
    }
    if let Some(bead) = palette {
        obstacles.push((bead.center, bead.size * 0.5));
    }
    if options.pair {
        let last = strip_last(BeadStrip::Palette, size, swatches);
        let start = if options.palette { strip_angle(BeadStrip::Palette, size, -FADE_COLUMNS, last) } else { 140. };
        let place = |angle: f32| pair_at(size, angle, pair);
        let fits = |[f, b]: [[f32; 2]; 2]| clear(f, pair.foreground * 0.5, &obstacles) && clear(b, pair.background * 0.5, &obstacles);
        let angle = (0..90).map(|i| start + i as f32).find(|a| fits(place(*a))).unwrap_or(start);
        let [foreground, background] = place(angle);
        let beads_in_order = [(Bead::Background, background, pair.background), (Bead::Foreground, foreground, pair.foreground)];
        let order = if front == Bead::Background { [beads_in_order[1], beads_in_order[0]] } else { beads_in_order };
        for (bead, center, size) in order {
            beads.push(BeadPlacement { bead, center, size, alpha: 1. });
            obstacles.push((center, size * 0.5));
        }
        let radius = size * 0.5 + pair.gap + pair.background * 0.5;
        let start = palette.map_or(45., |p| degrees(p.center) - 10.);
        let angle = (0..90).map(|i| start - i as f32).find(|a| clear(polar(radius, *a), pair.background * 0.5, &obstacles)).unwrap_or(start);
        beads.push(BeadPlacement { bead: Bead::Transparent, center: polar(radius, angle), size: pair.background, alpha: 1. });
    }
    beads.extend(palette);
    beads
}

pub fn bead_at(beads: &[BeadPlacement], point: [f32; 2]) -> Option<Bead> {
    beads
        .iter()
        .rev()
        .filter(|b| b.alpha > 0.6)
        .find(|b| (point[0] - b.center[0]).hypot(point[1] - b.center[1]) <= b.size * 0.5 + 2.)
        .map(|b| b.bead)
}

pub fn palette_pill(size: f32) -> Bounds {
    Bounds { x: -PALETTE_PILL[0] * 0.5, y: size * 0.5 + 70., width: PALETTE_PILL[0], height: PALETTE_PILL[1] }
}

pub fn color_drag_out(size: f32, point: [f32; 2]) -> bool {
    point[0].hypot(point[1]) > size
}

pub fn on_color_rim(size: f32, options: BubbleOptions, pair: PairBeads, point: [f32; 2]) -> bool {
    let radius = point[0].hypot(point[1]);
    radius > size * 0.5 - RING_RIM && radius <= color_reach(size, options, pair)
}

pub fn color_reach(size: f32, options: BubbleOptions, pair: PairBeads) -> f32 {
    let [x, y] = pair.offset();
    [
        (true, size * 0.5),
        (options.recent, orbit(size, 0) + BUBBLE_BEAD * 0.5),
        (options.palette, orbit(size, 1) + BUBBLE_BEAD * 0.5),
        (options.pair, size * 0.5 + pair.gap + (pair.foreground + pair.background) * 0.5 + x.hypot(y)),
    ]
    .into_iter()
    .filter(|(shown, _)| *shown)
    .fold(0., |reach, (_, r)| f32::max(reach, r))
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
pub struct RingArc {
    pub radius: f32,
    pub start: f32,
    pub sweep: f32,
    pub band: f32,
}
impl RingArc {
    pub fn point(&self, fraction: f32) -> [f32; 2] {
        polar(self.radius, self.start + self.sweep * fraction.clamp(0., 1.))
    }
    pub fn icon_point(&self) -> [f32; 2] {
        polar(self.radius, self.start - self.sweep.signum() * (RING_ICON / self.radius).to_degrees())
    }
    pub fn fraction(&self, point: [f32; 2]) -> f32 {
        let span = self.sweep.abs();
        let angle = degrees(point);
        let offset = (if self.sweep >= 0. { angle - self.start } else { self.start - angle }).rem_euclid(360.);
        if offset <= span {
            offset / span
        } else if offset - span < 360. - offset {
            1.
        } else {
            0.
        }
    }
}

pub fn ring_at(arcs: &[RingArc], point: [f32; 2]) -> Option<usize> {
    let radius = point[0].hypot(point[1]);
    let angle = degrees(point);
    let away = |arc: &RingArc| turn(arc.start + arc.sweep * 0.5, angle).abs();
    arcs.iter()
        .enumerate()
        .filter(|(_, arc)| (radius - arc.radius).abs() <= arc.band * 0.5)
        .min_by(|a, b| away(a.1).total_cmp(&away(b.1)))
        .map(|(index, _)| index)
}

pub fn bubble_face(kind: BubbleKind, diameter: f32) -> [f32; 2] {
    match kind {
        BubbleKind::Controls => [TOOL_BUBBLE; 2],
        BubbleKind::History => HISTORY_BUBBLE,
        _ => [diameter; 2],
    }
}

pub fn history_button(point: [f32; 2]) -> Option<usize> {
    let [width, height] = HISTORY_BUBBLE;
    (point[0].abs() <= width * 0.5 && point[1].abs() <= height * 0.5).then_some(usize::from(point[0] >= 0.))
}

fn halo_ring(count: usize, ring: usize) -> Vec<RingArc> {
    let slots: &[(f32, f32)] = match (count, ring) {
        (1, _) => &[(270., 250.)],
        (2, _) => &[(180., 116.), (0., 116.)],
        (_, 0) => &[(270., 96.), (30., 96.), (150., 96.)],
        _ => &[(90., 96.), (210., 96.), (330., 96.)],
    };
    let icon = ((RING_ICON + 10.) / HALO_RINGS[ring]).to_degrees();
    slots
        .iter()
        .map(|&(middle, span)| {
            let (sin, cos) = f32::to_radians(middle).sin_cos();
            let direction = if cos.abs() > 0.7 { -cos.signum() } else { -sin.signum() };
            RingArc {
                radius: HALO_RINGS[ring],
                start: middle - direction * (span * 0.5 - icon),
                sweep: direction * (span - icon),
                band: HALO_BANDS[ring],
            }
        })
        .collect()
}

pub fn halo_arcs(count: usize) -> Vec<RingArc> {
    let (inner, outer) = match count {
        0..=3 => (count, 0),
        4 => (2, 2),
        _ => (3, count.min(MAX_RING_SETTINGS) - 3),
    };
    let mut arcs = if inner > 0 { halo_ring(inner, 0) } else { Vec::new() };
    if outer > 0 {
        arcs.extend(halo_ring(outer, 1));
    }
    arcs
}

pub fn halo_disc(count: usize) -> f32 {
    let ring = usize::from(count > 3);
    (HALO_RINGS[ring] + HALO_BANDS[ring] * 0.5) * 2.
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ChooserKind {
    Medium,
    Tool,
    Brush,
}

fn fits(bounds: Bounds, point: [f32; 2], half: f32) -> bool {
    point[0] - half >= bounds.x
        && point[1] - half >= bounds.y
        && point[0] + half <= bounds.x + bounds.width
        && point[1] + half <= bounds.y + bounds.height
}

fn free_arc(center: [f32; 2], radius: f32, bounds: Bounds) -> Option<(f32, f32)> {
    let free: Vec<bool> = (0..72)
        .map(|i| {
            let p = polar(radius, i as f32 * 5.);
            fits(bounds, [center[0] + p[0], center[1] + p[1]], CHOOSER_ITEM * 0.5 + 2.)
        })
        .collect();
    if free.iter().all(|f| *f) {
        return Some((0., 360.));
    }
    (0..72)
        .filter(|&i| free[i] && !free[(i + 71) % 72])
        .map(|i| (i, (0..72).take_while(|n| free[(i + n) % 72]).count()))
        .max_by_key(|run| run.1)
        .map(|(start, run)| (start as f32 * 5., (run - 1) as f32 * 5.))
}

pub fn chooser_ring(count: usize, center: [f32; 2], bounds: Bounds) -> Vec<[f32; 2]> {
    let pitch = CHOOSER_ITEM + 6.;
    let base = CHOOSER_RING.max(pitch * count as f32 / std::f32::consts::TAU);
    if free_arc(center, base, bounds) == Some((0., 360.)) {
        return (0..count).map(|i| polar(base, -90. + i as f32 * 360. / count as f32)).collect();
    }
    let mut items = Vec::new();
    for radius in (0..8).map(|row| base + row as f32 * pitch) {
        let Some((start, span)) = free_arc(center, radius, bounds).filter(|(_, span)| *span < 360.) else { continue };
        let step = (pitch / radius).to_degrees();
        let room = (span / step) as usize + 1;
        let take = room.min(count - items.len());
        let middle = start + span * 0.5;
        items.extend((0..take).map(|i| polar(radius, middle + (i as f32 - (take - 1) as f32 * 0.5) * step)));
        if items.len() == count {
            break;
        }
    }
    items.resize(count, polar(base, -90.));
    items
}

pub fn chooser_column(count: usize, center: [f32; 2], medium: Option<([f32; 2], f32)>, bounds: Bounds) -> Vec<[f32; 2]> {
    let n = count as f32;
    let (radius, side, middle) = match medium {
        Some((item, reach)) => {
            let radius = (n * PILL_PITCH * 0.5 + 20.).max(BRUSH_COLUMN).max(reach + CHOOSER_ITEM * 0.5 + BRUSH_PILL[0] * 0.5 + 8.);
            let room = |side: f32| {
                if side > 0. { bounds.x + bounds.width - (center[0] + radius + BRUSH_PILL[0] * 0.5) } else { center[0] - radius - BRUSH_PILL[0] * 0.5 - bounds.x }
            };
            let side = if item[0] > -1. { 1. } else { -1. };
            let side = if room(side) < 0. && room(-side) > room(side) { -side } else { side };
            (radius, side, item[1])
        }
        None => {
            let radius = (n * PILL_PITCH * 0.5 + 18.).max(BRUSH_FAN);
            let room = |right: bool| {
                if right { bounds.x + bounds.width - (center[0] + radius + BRUSH_PILL[0] * 0.5) } else { center[0] - radius - BRUSH_PILL[0] * 0.5 - bounds.x }
            };
            let right = if room(true) >= 0. && (center[0] <= bounds.x + bounds.width * 0.5 || room(false) < 0.) {
                true
            } else if room(false) >= 0. {
                false
            } else {
                room(true) > room(false)
            };
            (radius, if right { 1. } else { -1. }, 0.)
        }
    };
    let half = n.max(1.) * PILL_PITCH * 0.5 - PILL_PITCH * 0.5;
    let top = bounds.y + BRUSH_PILL[1] * 0.5 - center[1];
    let bottom = bounds.y + bounds.height - BRUSH_PILL[1] * 0.5 - center[1];
    let mut first = (middle - half).max(top);
    if first + 2. * half > bottom {
        first = (bottom - 2. * half).max(top);
    }
    (0..count)
        .map(|i| {
            let y = first + i as f32 * PILL_PITCH;
            let dy = y - middle;
            let x = (radius * radius - dy * dy).max(0.).sqrt().max(BRUSH_PILL[0] * 0.5 + 16.);
            [side * x, y]
        })
        .collect()
}

pub fn layers_card(viewport: [f32; 2], layers: usize, measured: Option<[f32; 2]>, properties: bool) -> [f32; 2] {
    let guess = PanelScrollMeasurement { fixed_height: 92., unit_height: 40. };
    let heights = measured.unwrap_or_else(|| LayerDrawerSizing::pages(guess, layers, viewport[1], 160.));
    [Panel::Layers.default_width(viewport[0]), heights[usize::from(properties)]]
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum BubbleGrab {
    Bubble { id: u32 },
    Rim { id: u32 },
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
    Tap { id: u32, position: [f32; 2] },
    Ring { id: u32, phase: ContactPhase, position: [f32; 2] },
    Beads { id: u32, phase: ContactPhase, position: [f32; 2] },
    Choose { id: u32, phase: ContactPhase, position: [f32; 2] },
    PutBack { id: u32 },
    PutBackAll,
    SetOptions { id: u32, options: BubbleOptions },
    ToggleRingSetting { brush: u32, setting: String },
    Palette { id: u32, step: i32 },
    Resize { id: u32, scale: f32 },
    Properties { id: u32 },
    MeasureCard { id: u32, heights: [f32; 2] },
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct BubbleButton {
    pub label: String,
    pub icon: &'static str,
    pub enabled: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
pub struct BubbleStamp {
    pub center: [f32; 2],
    pub diameter: f32,
    pub color: layer_core::color::RgbColor,
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
    pub enabled: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct BubbleBead {
    pub placement: BeadPlacement,
    pub color: Option<layer_core::color::RgbColor>,
    pub label: String,
    pub selected: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct ChooserItem {
    pub kind: ChooserKind,
    pub center: [f32; 2],
    pub label: String,
    pub icon: &'static str,
    pub preview: Option<u32>,
    pub selected: bool,
    pub shown: bool,
    pub enabled: bool,
}
impl ChooserItem {
    pub fn size(&self) -> [f32; 2] {
        if self.kind == ChooserKind::Brush { BRUSH_PILL } else { [CHOOSER_ITEM; 2] }
    }
    pub fn contains(&self, point: [f32; 2]) -> bool {
        let [w, h] = self.size();
        let d = [(point[0] - self.center[0]).abs(), (point[1] - self.center[1]).abs()];
        let corner = [(d[0] - w * 0.5 + h * 0.5).max(0.), (d[1]).max(0.)];
        d[0] <= w * 0.5 && d[1] <= h * 0.5 && corner[0].hypot(corner[1]) <= h * 0.5
    }
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct BubbleCard {
    pub size: [f32; 2],
    pub full: bool,
    pub properties: bool,
    pub disclosure: String,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct BubbleView {
    pub id: u32,
    pub source: BubbleSource,
    pub kind: BubbleKind,
    pub diameter: f32,
    pub face: [f32; 2],
    pub pair: PairBeads,
    pub label: String,
    pub icon: &'static str,
    pub preview: Option<u32>,
    pub variants: bool,
    pub dots: Vec<bool>,
    pub selected: bool,
    pub enabled: bool,
    pub open: bool,
    pub halo: Option<f32>,
    pub rings: Vec<BubbleRing>,
    pub readout: Option<String>,
    pub stamp: Option<BubbleStamp>,
    pub buttons: Vec<BubbleButton>,
    pub beads: Vec<BubbleBead>,
    pub pill: Option<String>,
    pub chooser: Vec<ChooserItem>,
    pub card: Option<BubbleCard>,
    pub layer: Option<u64>,
    pub layers: usize,
    pub options: BubbleOptions,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
pub struct BubblePlacement {
    pub id: u32,
    pub center: [f32; 2],
    pub lifted: bool,
    pub homing: bool,
}

#[cfg(test)]
#[path = "bubble_tests.rs"]
mod tests;
