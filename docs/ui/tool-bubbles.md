# Tool bubbles

[Workspace and UI](README.md) · [Drag and reorder](drag-and-reorder.md) · [Panel transparency](panel-transparency.md)

Tool bubbles keep tools on the canvas, like brushes laid on a desk. Dragging a
title-bar button, a toolbar tool or a brush row out onto the canvas takes a
bubble of it; dropping the bubble back on a toolbar or the title bar puts it
away. Bubbles stay while painting and in Zen mode, push each other aside and
return to their places when there is room.

Shared Rust owns everything a host would otherwise decide: `bubbles.rs` holds
sources, sizes and the geometry of every part (halo arcs, color beads, the
radial chooser, brush columns, the Layers card), `bubble_desk.rs` holds how
bubbles push each other, and `session_bubbles.rs` holds taps, choices, beads,
strokes, menus, history and UI text. Hosts draw the parts at the offsets in
`BubbleView`, run the frame clock and forward contacts relative to the bubble
center. GTK presents bubbles; Web and the native clients follow after review.

## What becomes a bubble

| Source | Bubble | Face |
| --- | --- | --- |
| A tool group in the title bar or a toolbar (Brush, Eraser, Sculpt, a tool slot…) | Group dial, 56 px | The tool's current icon and the title bar's drawer mark |
| A brush row in Brushes or a tool drawer | Brush, 56 px | The brush icon over its own stroke |
| A medium (Pencil, Oil…) in Brushes or a drawer | Medium, 56 px | The medium icon and one dot per brush along the lower edge; the current brush's dot is darker |
| Brush size, Brush opacity (or the Opacity tile) and Strength sliders | Slider, 96 px | The setting icon and value inside a 300° ring; the center moves it |
| Color | Color ring, 136, 176 or 216 px (pinch: 128–256) | The Color panel's wheel on a glass plate, with beads around it |
| Layers | Layers card, folded to a 56 px squircle | The current layer's thumbnail on a small stack, with the layer count |

Commands, other panels, Tool Options, size chips and dividers stay in their
toolbars. `BubbleSource::for_control` decides; the Opacity tile is stored as the
Brush opacity slider. There is one bubble per source: taking a tool again moves
the existing bubble.

## Taking and putting back

- **Title bar and drawer rows:** drag without a hold. The contact keeps scrolling
  its list or toolbar until it leaves the bar, drawer or panel; from then on the
  bubble follows it. Releasing within 80 px (`WORKSPACE_PROXIMITY`) of where it
  came from takes nothing.
- **Toolbar tiles:** a plain drag takes a bubble the same way, except for slider
  and Tool Options tiles, whose plain drags keep editing their value. Hold, then
  drag still rearranges toolbars; releasing such a drag on the canvas past the
  80 px line takes a bubble.
- **Putting back:** drop a bubble on a toolbar, a panel or the title bar, or choose
  **Put back** in its hold menu. While the contact is over a place it would go
  back to, the bubble shrinks and fades. `BubbleAction::PutBackAll` empties the
  desk in one step.
- Each take, move or put back is one layout undo step. A drag with no net change
  records nothing; cancelling restores the arrangement.

## Moving and pushing

Bubbles move without a hold: drag the face of a tool bubble, the center of a
slider, the folded Layers squircle or an open card's grab bar and edges, or the
color ring anywhere outside the wheel that isn't a bead. Two fingers move any
bubble; while one finger is on a bubble, the second may land up to 90 px outside
it. Pinching the color
ring sets its size between 128 and 256 px in one undo step. A finger on the hue
ring or disc picks a color at once.

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
  122 px for one to three settings and 188 px for four to six (2+2, 3+2, 3+3).
  Each arc is a 4 px track on a matte band that covers only the arc, with the
  setting's icon on the band just before the track starts;
  side arcs grow upward and top and bottom arcs rightward. Drag along an arc to
  set the value; its name and value show below the halo while dragging. A halo
  stays open while painting and until the bubble is tapped or another tool is
  chosen.
- **Which settings:** each brush starts with its primary settings. **Settings in
  the ring** in the hold menu lists every Tool Settings row, at most six checked.
  The choice belongs to the brush and is part of the workspace.
- **Chooser:** a dial rings its media (or, for tools like Select, its tools) in
  44 px circles 84 px out, from the top clockwise. The current medium is
  highlighted with its name, and its brushes curve beside it as pills with their
  strokes. Tapping a medium fans its brushes; a press can slide through a medium
  onto one of its brushes. Picking a brush or tool closes the chooser; letting go
  on nothing keeps it. A medium bubble fans its own brushes. Near a window edge
  the media gather on the open side, in a second arc when one doesn't hold them.
- **Slider:** drag the ring or tap a point on it; the center moves the bubble.
- **Hold** always opens the bubble's menu: **Settings in the ring**, the
  bubble's options and **Put back**.

## Color ring

The wheel is the Color panel's bare wheel, with the same shapes, snaps and color
management, on a glass plate; its rim and the space between it and the beads move
the bubble. 24 px beads orbit it on up to two orbits 28 px apart:

- **Recent colors** along the top, newest on the left, eight at most (fewer on a
  small ring). Slide along them to reach older colors; faint beads at the ends
  show there are more.
- **Foreground, background and transparent.** The 30 px foreground bead lies
  over the background bead; tapping the background swaps them. They sit lower
  left, and move up the side when the palette is shown.
- **Palette** in two orbits wrapping the lower half, filled column by column;
  sliding either orbit turns both, and they settle on whole columns. The glass
  palette bead ends the orbits on the right. Tapping it opens a fixed-width
  ‹ name › pill below; the arrows switch palettes and the name lists them with
  **Edit palettes…**. Picking a color or a contact elsewhere closes it.

The hold menu turns Recent colors, Palette and Foreground and background on or
off and chooses Small, Medium or Large. Windows under 600 px wide show rings at
Small.

## Layers card

Tapping the folded squircle unfolds the card in place, 244 px wide (300 in
Full), pushing neighbours aside. A grab bar sits on top; rows show up to five
layers, each with its eye, thumbnail and name. **Full** adds each row's details
and the blend, opacity, alpha lock and lock header. The footer holds the New
layer icon (and Delete in Full) on the left and **Layer properties** with an up
chevron on the right; the properties slide up over the rows and the button turns
into **All layers** with a down chevron. Contacts elsewhere fold the card, which
shrinks away before the squircle returns; **Fold when not in use** in the hold
menu keeps it open until its grab bar is tapped.

## Painting

A stroke that comes within 48 px of a bubble fades it to 20% and passes under
it; it comes back 180 ms after the pen lifts. Painting closes the chooser, the
palette name and folding cards; a halo stays and fades with its bubble.

## Zen mode

Bubbles stay visible in Zen mode, like floating panels, and their drags, halos,
choosers and cards keep working while other chrome is hidden. **Show in Zen
mode** in a toolbar's menu keeps that toolbar (off by default), and a
one-finger swipe in from an edge peeks the toolbars docked there until the next
contact elsewhere. Dropping a bubble on a toolbar kept or peeked in Zen puts it
back.

## Validation

- Shared: `cargo test --locked -p layer-ui bubble`, which covers geometry,
  pushing, tear-off and put back, tap states, halos, the chooser and its slides,
  beads, the card, stroke fading, menus, history and workspace round trips
  without a window.
- GTK: `LAYER_NATIVE_TEST_TIMEOUT_MS=300000 bash tools/performance/workspace-motion.sh gtk --native-test=native_tool_bubbles`,
  with `LAYER_NATIVE_CAPTURE_DIR` for compositor screenshots. With mouse and
  touch in both themes it takes a title-bar tool, walks the taps, adjusts a halo
  arc, paints under it, chooses a medium and slides to a brush, pushes one
  bubble with another, opens the hold menu, unfolds the Layers card and its
  properties, adds a layer from its footer and folds it, picks on the wheel and
  moves the ring by its glass, uses the color beads and palette pill, pinches
  the ring, checks Zen mode and puts everything back.
- Motion: `LAYER_NATIVE_EVENT_MS=4 bash tools/performance/workspace-motion.sh gtk --native-test=native_tool_bubble_motion`
  drags one bubble through four others with 250 Hz mouse and touch input and
  reports presentation rate, frame gaps and canvas GPU time for steady motion;
  it fails if steady motion refreshes any UI model. Compare it with
  `--workspace-motion` on the same machine. Bubbles exist only on GTK, so no
  tier row is measured yet.
