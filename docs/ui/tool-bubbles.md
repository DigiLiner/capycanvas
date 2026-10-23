# Tool bubbles

[Workspace and UI](README.md) · [Drag and reorder](drag-and-reorder.md) · [Panel transparency](panel-transparency.md)

Tool bubbles keep tools on the canvas, like brushes laid on a desk. Dragging a
title-bar button, a toolbar tool or a brush row out onto the canvas takes a
bubble of it; dropping the bubble back on a toolbar or the title bar puts it
away. Bubbles stay while painting and in Zen mode, push each other aside and
return to their places when there is room.

Shared Rust owns everything a host would otherwise decide: `bubbles.rs` holds
sources, sizes and the geometry of every part (halo arcs, color beads, the
radial chooser, brush columns, the Layers card, the count badge and readout
spots, the widget reach), which part a point lands on (`BubbleView::hit`, which
hosts use for input, hover and pressed states alike), `bubble_desk.rs` holds how
bubbles push each other, and `session_bubbles.rs` holds taps, choices, beads,
strokes, menus, history and UI text. The workspace keeps each bubble's place and
options and each brush's ring settings (`BubbleLayout`). Hosts draw the parts
at the offsets in `BubbleView`, run the frame clock and forward contacts
relative to the bubble center. GTK presents bubbles; Web and the native clients
follow after review.

## What becomes a bubble

| Source | Bubble | Face |
| --- | --- | --- |
| A tool group in the title bar or a toolbar (Brush, Eraser, Sculpt, a tool slot…) | Group dial, 56 px | The tool's current icon and the title bar's drawer mark |
| A brush row in Brushes or a tool drawer | Brush, 56 px | The brush icon over its own stroke |
| A medium (Pencil, Oil…) in Brushes or a drawer | Medium, 56 px | The medium icon and one dot per brush along the lower edge; the current brush's dot is darker |
| The color picker | Brush controls, 134 px | A 56 px circle with the color picker icon, Strength on a left arc and Size on a right arc, both always shown; **Swap sliders** in its hold menu swaps them |
| Undo or Redo | Undo and redo, 104 × 56 px | A capsule with Undo on the left and Redo on the right; each is a full 56 px circle out to the capsule's end, which answers taps and shows its hover and pressed highlight, and a side is dimmed when there is nothing to undo or redo |
| Color | Color ring, 136, 216 or 344 px (pinch: 128–352) | The Color panel's wheel on a glass plate, with beads around it |
| Layers | Layers card, opened or folded to a 56 px squircle | The current layer's thumbnail with the layer count |

Other commands, panels, sliders, Tool Options, size chips and dividers stay in
their toolbars. `BubbleSource::for_control` decides; Redo is stored as Undo, so
both make the same bubble. There is one bubble per source: taking a tool again
moves the existing bubble.

## Taking and putting back

- **Title bar and drawer rows:** drag without a hold. The contact keeps scrolling
  its list or toolbar until it leaves the bar, drawer or panel; from then on the
  bubble follows it. Releasing within 80 px (`WORKSPACE_PROXIMITY`) of where it
  came from takes nothing.
- **Toolbar tiles:** a plain drag takes a bubble the same way. Hold, then drag
  still rearranges toolbars; releasing such a drag on the canvas past the 80 px
  line takes a bubble.
- **Putting back:** drop a bubble on a toolbar, a panel or the title bar, or choose
  **Put back** in its hold menu. While the contact is over a place it would go
  back to, the bubble shrinks and fades. `BubbleAction::PutBackAll` empties the
  desk in one step.
- **Coming out:** a brush, medium or group dial comes out selected, with its
  settings halo open when its brush has settings, and Layers opens into its
  card as soon as it leaves the toolbar or title bar. Moving a bubble already on the canvas leaves it as it was.
- Each take, move or put back is one layout undo step. A drag with no net change
  records nothing; cancelling restores the arrangement.

## Moving and pushing

Only the drawn parts of a bubble take a pen, finger or mouse; everything else,
such as the gaps between halo arcs, the space between the color plate and its
beads, and the corners around round parts, belongs to the canvas. Each halo arc
answers on its band, from the round cap behind its icon to the round cap past
its end; undo and redo answer inside their circles, and the rest of the
capsule moves it.

Bubbles move without a hold: drag the face of a tool bubble or Brush controls,
the undo and redo capsule, the folded Layers squircle, an open card anywhere but
its layer rows and controls that drag (sliders, number fields, curves), or the
color plate's rim. Two fingers move any
bubble from anywhere on it, the wheel included; while one finger is on a bubble,
the second may land up to 90 px outside it. Pinching the color ring sets its
size between 128 and 352 px in one undo step. A finger on the hue ring or disc
picks a color at once; once it moves more than a radius outside the wheel, the
ring lets go of the pick, restores the color and follows the finger by the
nearest point of its rim, easing to it instead of jumping.

A dragged bubble pushes the bubbles it meets, keeping a 6 px gap. Pushed bubbles
take 12% of the push as speed and slide with friction (0.86 per 60 Hz frame);
they keep their new places. Letting go glides a short way, at most 18 px per
frame, about 130 px in all; with reduced motion there is no glide. An open halo
card or chooser pushes neighbours only while it is open, each part pushing where
it is (the chooser's media and brush pills, not the empty space between them),
and they return when it closes.
Walls sit 8 px inside the canvas area, below the title bar and clear of docked
toolbars; bounces off them keep a quarter of their speed. Saved places keep
their distance to the nearest window corner, so a bubble near the bottom right
stays there when the window shrinks.

Motion runs in fixed 240 Hz steps, so it looks the same at any display rate; the
host steps it from its frame clock while anything moves and stops ticking when
everything rests.

## Taps, halos and the chooser

- **Brush:** the first tap selects the brush. The next tap opens its settings
  halo; the one after closes it. If the brush is already selected, the first tap
  opens the halo.
- **Group dial and medium:** select, then halo, then chooser, then close. A tool
  without settings goes straight from select to the chooser.
- **Halo:** the selected brush's settings sit on arcs around the bubble, within
  134 px for one to three settings and 204 px for four to six (2+2, 3+2, 3+3).
  Each arc is a 4 px track on a 28 px matte band that covers only the arc, with
  the setting's icon on the band 21 px before the track starts, clear of the
  thumb. Each band, caps included, is centred on its slot, so a single slider
  is a symmetric gauge over the top, a pair mirrors left and right, and
  neighbouring bands keep an even gap. The set part of the track is grey like the panels' sliders and the
  thumb is a 14 px circle in the toolbars' thumb color with a thin border;
  side arcs grow upward and top and bottom arcs rightward. Drag along an arc to
  set the value; its name and value show below the halo while dragging. A
  double click or double tap on an arc resets its setting to the default
  (`BubbleAction::ResetRing`, the toolbar sliders' `ResetToolSetting`); hosts
  detect it with the platform's double-click time and distance. A halo
  stays open while painting and until the bubble is tapped or another tool is
  chosen.
- **Which settings:** each brush starts with its primary settings. Its hold menu
  lists every Tool Settings row as a check item, at most six checked. The choice
  belongs to the brush and is part of the workspace.
- **Chooser:** a dial rings its media (or, for tools like Select, its tools) in
  44 px circles 84 px out, from the top clockwise. The current medium is
  highlighted with its name, set radially outward beside its icon (as wide as
  the name, ellipsized only past 96 px) so it never covers a neighbour, and its
  brushes curve beside it as pills with their strokes, just past the name; when that side lacks
  room they open on the other side without the name's offset. Tapping a medium fans its brushes, even a single one; a press can slide through a medium
  onto one of its brushes. Picking a brush or tool closes the chooser; letting go
  on nothing keeps it. A medium bubble fans its own brushes. Near a window edge
  the media gather on the open side, in a second arc when one doesn't hold them.
- **Brush controls:** drag along the Size and Strength arcs, or tap a point on
  them. While a contact is on an arc, a dab of the
  current brush in the current color sits under the finger or pen at the
  canvas's zoom, behind every bubble, and follows each change; the pointer
  hides while it shows, and the setting's name and value show below. Halo arcs
  for the other settings that shape a single dab (opacity, hardness, angle,
  grain depth, material hardness, bristle scale and load;
  `tool_settings::shapes_dab`) show the same dab.
  Tapping its center starts or stops the color picker, like its tile.
- **Undo and redo:** tapping either half undoes or redoes; dragging anywhere on
  it moves it.
- **Hold** always opens the bubble's menu: a heading that can't be chosen
  (`ContextMenuItem::heading`) naming the tool, which for a dial or medium is its
  current brush or tool rather than the group, its ring settings or options and
  **Put back**. Checking or unchecking an option keeps the
  menu open; each item changes only its own option (`BubbleAction::Option`), so
  several changes in one visit never undo each other. On the color ring a hold anywhere opens it;
  on the wheel's ring and disc it takes twice the usual delay, so a slow pick
  stays a pick. A hold
  restores the color or setting its press changed; the menu stays open when the
  contact lifts, and dragging instead closes it and carries on the drag.
- **Showing and hiding:** halo arcs grow and fade in, chooser items arrive in a
  short stagger and fade out when it closes, and beads slide to new places; with
  reduced motion they appear at once.

## Color ring

The wheel is the Color panel's bare wheel, with the same shapes, snaps and color
management, on a glass plate; the plate's rim moves the bubble. The beads are
the Color panel's own swatch buttons (`WheelButton::swatch`) with its styles,
outline and selection ring. 24 px beads orbit it on up to two orbits 28 px
apart:

- **Recent colors** along the top, newest on the left, eight at most (fewer on a
  small ring). Slide along them to reach older colors; beads shrink and fade as
  they turn past either end, so the strip never ends in a jump.
- **Primary, secondary and transparent.** These beads have the Color drawer's
  sizes and sit its gap (about 10 px) off the grey rim, and the primary and
  secondary overlap like the drawer's pair, the chosen one in front. Tapping
  the secondary chooses it and brings it to the front; nothing swaps. They sit
  lower left, and move up the side when the palette is shown.
- **Palette** in two orbits wrapping the lower half, filled column by column;
  sliding either orbit turns both, and they settle on whole columns. Columns
  past the ends shrink and fade continuously instead of waiting behind other
  beads. The glass palette bead ends the orbits on the right, as far from the
  wheel as from the outer orbit, and the transparent bead keeps at least 9 px
  from it. While a whole column of colors is hidden past either end of a strip,
  one faint dot per orbit marks where they will emerge. Tapping it opens a fixed-width ‹ name › pill
  below; the arrows switch palettes and the name lists them with **Edit
  palettes…**. Picking a color or a contact elsewhere closes it. No bead
  overlaps another at any size or turn.

A new ring is the Medium circle alone. The hold menu turns **Recent colors**,
**Palette** and **Primary and secondary** on or off and chooses Small (136 px),
Medium (216 px) or Large (344 px). Putting the ring back remembers its options
and size for the next time it is taken.

## Layers card

Layers comes out open. Tapping the folded squircle unfolds the card in place,
pushing neighbours aside; taps inside the open card change nothing, and a hold
anywhere but a layer row or a control opens the bubble's menu, with **Close**
while the card is open.
The card shares the Layers drawer's code and styles: it hosts the drawer's
`LayerInspector` (layer list, rows and properties page) in the drawer's
`dock-panel content-drawer` container, at the drawer's width and height
(`LayerDrawerSizing`, from the heights GTK measures and reports with
`BubbleAction::MeasureCard`), with the toolbars' shadow like the drawers. **Full** is the drawer
exactly: blend, opacity and lock header, full rows, layer actions and the Layer
properties footer. **Simple** keeps the approved design with the same widgets:
rows show only each layer's eye, thumbnail and name, and one bottom row in the
drawer footer's style holds the New layer icon on the left and **Properties**
with an up chevron on the right. The properties slide up over the rows as in the
drawer. There is no grab bar: dragging anywhere on the open card moves it,
except on the layer list, whose rows keep their own drag, and on controls that
drag. Contacts elsewhere fold the card, which shrinks away before the squircle
returns; turning off **Fold when not in use** in the hold menu keeps it open
until **Close**. The folded
squircle shows the current layer's thumbnail with the layer count in a badge
centered on its upper right corner: a 22 px circle that widens into a pill for
more digits.

## Look

Bubbles look like the toolbars: faces, glass beads, pills, the readout, tags and
the count badge take the panel color (the panel glass in glass mode) with no
border, a selected part layers the toolbars' selection over it exactly as a
selected toolbar tile does, and halo bands are panel glass too, blurring the
canvas under their arc. Round parts cast the toolbars' 2 px, 8 px shadow, and beads a smaller one, each
drawn as one smooth gradient around its own shape and clipped outside it, so it never
bands, never darkens the glass and never renders offscreen when scaling or
pinching. Faces, the undo and redo halves and chooser items show hover and
pressed states. Bubble, readout, tag, count and chooser text uses the standard
UI text size; brush pills are 152 x 48 px to fit it. A
pen shows no system pointer over bubbles, as over the canvas; the mouse keeps
its pointer.

## Painting

A stroke that comes within 48 px of a bubble fades it to 20% over 140 ms and
passes under it, its glass blur fading with it so the artwork shows through;
it fades back 180 ms after the pen lifts. Painting closes the chooser, the
palette name and folding cards; a halo stays and fades with its bubble.

## Zen mode

Bubbles stay visible in Zen mode, like floating panels, and their drags, halos,
choosers, cards and menus keep working while other chrome stays hidden. **Show in Zen
mode** in a toolbar's menu keeps that toolbar (off by default), and a
one-finger swipe in from an edge peeks the toolbars docked there until the next
contact elsewhere. Dropping a bubble on a toolbar kept or peeked in Zen puts it
back.

## Validation

- Shared: `cargo test --locked -p layer-ui bubble`, which covers geometry,
  pushing, tear-off and put back, tap states, halos, the chooser and its slides,
  beads, the card, stroke fading, menus, history and workspace round trips
  without a window.
- GTK: `LAYER_NATIVE_TEST_TIMEOUT_MS=420000 bash tools/performance/workspace-motion.sh gtk --native-test=native_tool_bubbles`,
  with `LAYER_NATIVE_CAPTURE_DIR` for compositor screenshots. With mouse and
  touch in both themes it takes a title-bar tool, walks the taps, adjusts a halo
  arc, paints under it, chooses a medium and slides to a brush, pushes one
  bubble with another, opens the hold menu, drags the color picker out and sets
  Size and Strength on its arcs with the dab under the contact while the gaps
  around them stay canvas, double-clicks Size back to its default, undoes and redoes from the bubble taken from Redo and
  then Undo, takes the Layers card open, taps and holds inside it for the menu
  and its heading, opens its properties, adds a layer from its footer, folds it
  and closes it from the menu, picks on the wheel and
  moves the ring by its rim while the space around the plate stays canvas,
  holds it for its menu, uses the color beads and
  palette pill, drags out of the wheel, pinches the ring with fingers starting
  on the wheel, moves a dial with two fingers, checks Zen mode and puts
  everything back.
- Frames: `LAYER_NATIVE_CAPTURE_DIR=... bash tools/performance/workspace-motion.sh gtk --native-test=native_tool_bubble_frames`
  records tearing out, pinching and putting back the ring in both themes and
  fails on dark pixels outside the ring in any frame.
- Motion: `LAYER_NATIVE_EVENT_MS=4 bash tools/performance/workspace-motion.sh gtk --native-test=native_tool_bubble_motion`
  drags one bubble through four others with 250 Hz mouse and touch input and
  reports presentation rate, frame gaps and canvas GPU time for steady motion;
  it fails if steady motion refreshes any UI model. Compare it with
  `--workspace-motion` on the same machine. Bubbles exist only on GTK, so no
  tier row is measured yet.
