# Tool bubbles

[Workspace and UI](README.md) · [Drag and reorder](drag-and-reorder.md) · [Panel transparency](panel-transparency.md)

Tool bubbles keep tools on the canvas, like brushes laid on a desk. Dragging a
title-bar button, a toolbar tool or a brush row out onto the canvas takes a
round bubble of it; dropping the bubble back on a toolbar or the title bar puts
it away. Bubbles stay while painting and in Zen mode, push each other aside and
return to their places when there is room.

Shared Rust (`bubbles.rs`, `session_bubbles.rs`) owns what can become a bubble,
sizes, tap states, ring geometry, color beads, how bubbles push each other, menus,
history and UI text. Hosts draw the faces, run the frame clock and forward
contacts in workspace coordinates. GTK presents bubbles; Web and the native
clients follow after review.

## What becomes a bubble

| Source | Bubble | Face |
| --- | --- | --- |
| A tool group in the title bar or a toolbar (a tool slot, Brush, Eraser, Sculpt…) | Group dial, 56 px | The tool's current icon and the group triangle |
| A brush row in Brushes or a tool drawer | Brush, 56 px | The brush icon over its stroke preview |
| A medium (Pencil, Oil…) in Brushes or a drawer | Medium, 56 px | The medium icon and one dot per brush along the lower edge; the current brush's dot is darker |
| Brush size, Brush opacity (or the Opacity tile) and Strength sliders | Slider, 96 px | The setting icon and value inside a 300° ring |
| Color | Color ring, 128, 192 or 256 px | The hue ring and disc from the Color panel, with beads around them |
| Layers | Layers card, a 56 px squircle | The Layers icon |

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
  back to, the bubble shrinks and fades.
- Each take, move or put back is one layout undo step. A drag with no net change
  records nothing; cancelling restores the arrangement.

## Moving and pushing

Bubbles move without a hold: drag the face of a tool bubble, the center of a
slider, the folded Layers squircle, or the space between the color ring and its
beads. A dragged bubble pushes the bubbles it meets; they slide with friction
(0.86 per 60 Hz frame) and keep their new places. An open ring pushes neighbours
only while it is open, and they return when it closes. Pushes against a window
edge slide along it, and every bubble stays inside the window below the title
bar. A smaller window keeps bubbles inside without changing their saved places.

Motion runs in fixed 240 Hz steps, so it looks the same at any display rate; the
host steps it from its frame clock while anything moves and stops ticking when
everything rests. Moving bubbles requests no canvas frames of its own beyond the
glass update.

## Taps and rings

- **Brush:** the first tap selects the brush. The next tap opens its settings
  ring; the one after closes it. If the brush is already selected, the first tap
  opens the ring.
- **Group dial and medium:** select, then settings ring, then the chooser, then
  close. The chooser is the tool's Brushes drawer beside the bubble; picking a
  brush closes it, picking a medium shows its brushes. A tool without settings
  goes straight from select to the chooser.
- **Settings ring:** the selected brush's settings sit on arcs around the bubble,
  each with its icon at the start of its track. One setting is a gauge over the
  top; two sit left and right; three add one at the top; four, five and six add an
  outer ring (2+2, 3+2, 3+3). Side arcs grow upward and top arcs rightward. Drag
  along an arc to set the value; the face shows the value while dragging. The ring
  closes on a contact elsewhere.
- **Which settings:** each brush starts with its primary settings (for example
  Paint load and Color pickup for wet paint, Material hardness and Texture
  strength for dry media, the blender's own setting for blenders). **Settings in
  the ring** in the hold menu lists every Tool Settings row, at most six checked.
  The choice belongs to the brush and is part of the workspace.
- **Slider:** drag the ring or tap a point on it; the center moves the bubble.
- **Hold** always opens the bubble's menu: tool choices for dials, **Settings in
  the ring**, the bubble's options and **Put back**.

## Color ring

The ring and disc are the Color panel's bare wheel with the same shapes, snaps
and color management. Beads orbit it:

- **Recent colors** along the top: eight at a time from the 64-color history.
  Slide along them to see older colors; partly scrolled beads fade at the ends.
- **Foreground, background and transparent** at the lower left and lower right.
  With the palette shown they move up the sides.
- **Palette** in two rows under the ring, filled column by column; sliding either
  row moves both. The last bead opens a pill with the palette's name between ‹ and
  ›, at a fixed place below the rows. The arrows switch palettes; the name lists
  them.

The hold menu turns Recent colors, Palette and Foreground and background on or
off and chooses Small, Medium or Large.

## Layers card

Tapping the folded squircle opens the Layers drawer beside it, with Layer
properties sliding up from its footer as in the docked drawer. Contacts elsewhere
fold it; **Fold when not in use** in the hold menu keeps it open until tapped.
**Simple** and **Full** choose how much each row shows.

## Zen mode

Bubbles stay visible in Zen mode, like floating panels, and their drags and
menus keep working while other chrome is hidden.

## Validation

- Shared: `cargo test --locked -p layer-ui bubble`, which covers geometry,
  pushing, tear-off and put back, tap states, rings, menus, beads, history and
  workspace round trips without a window.
- GTK: `bash tools/performance/workspace-motion.sh gtk --native-test=native_tool_bubbles`,
  with `LAYER_NATIVE_CAPTURE_DIR` for compositor screenshots. It drags a title-bar
  tool and a chooser row out with mouse and touch, walks the taps, adjusts a ring,
  pushes one bubble with another, opens the hold menu and puts both back, in both
  themes.
