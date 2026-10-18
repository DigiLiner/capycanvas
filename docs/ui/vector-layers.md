# Vector layers

[Workspace and UI](README.md)

**Layer > New Vector Layer** adds a layer that stores paths instead of pixels
([model](../reference/authored-model.md), [rendering](../internals/rendering.md#vector-layers)).

## Drawing

Plain round dry brushes draw on a vector layer: G-Pen, whose pressure sets the
width, and Monoline, which keeps one width. The line follows the same Smoothing,
pressure curve, rulers and late stylus pressure updates as the brush does on a
paint layer. While the pen is down it shows as a provisional path; at pen-up it
is fitted to as few cubic segments as stay within half a screen pixel of the
drawn line, with smooth anchors where curves meet and corners where the line
turns sharply, and added as one path in one undo step. Its width profile keeps
pressure changes to the same half pixel. Pressure updates that arrive after
pen-up reshape that path within its undo step. Other brushes refuse the stroke
with a notice, and the canvas cursor shows the refusal before contact.

## Figures

On a vector layer, Figure's Line, Rectangle and Ellipse add one path each,
selected and in front of the others, instead of painting pixels. Line draws with
round caps. Rectangle and Ellipse keep their frame as a live shape, so later
moves, rotations and uniform scaling keep them editable shapes. Outline strokes
with the foreground color at the line width; Fill fills with it; Outline and
Fill fills with the background and strokes with the foreground above it.
Rectangles get sharp corners. Shift makes squares, circles and 45° lines, and
brush opacity sets the color's alpha. Transparent paint is refused.

## Properties

With paths selected, the layer's Properties add Fill, Fill color, Outline,
Outline color, Outline width, Line ends (Flat, Round, Square) and Corners
(Sharp, Round, Beveled). They show the front-most selected path, and a label
ends in "(mixed)" when the selected paths differ. An edit applies
to every selected path: colors change the first fill and the top outline, a path
whose style holds more fills or outlines keeps the others, and paths that shared
a style keep sharing one. Turning Fill or Outline on adds the background or
foreground color. Each change, or each drag of a number or color, is one undo
step.

## Edit Points

**Edit Points** (A), beside Move and Scale-Rotate, edits the anchors of the
selected paths; pressing another path selects it. Squares mark the anchors,
filled when selected, and selected anchors show their handles.

- Click an anchor to select it, Shift-click to add or remove it, or drag a
  marquee on empty canvas to select the anchors inside.
- Drag selected anchors to move them (Shift keeps 45° steps). Drag a handle to
  turn the curve; a smooth anchor keeps its handles in line and a symmetric one
  mirrors them, unless Alt breaks them into a corner.
- Drag a segment to bend it; click a segment to add an anchor without changing
  the curve.
- Double-click an anchor to switch it between corner and smooth.
- **Clear Selected** (Delete) removes the selected anchors and joins their
  neighbors; a path left with one anchor goes. Width profiles keep the widths
  at the remaining anchors.

Editing an anchor turns a rectangle or ellipse into a plain path. Each drag,
click or deletion is one undo step.

## Path

Figure's **Path** draws a path anchor by anchor on a vector layer. A click adds
a corner; a press and drag pulls out symmetric handles for a smooth anchor, and
Alt drags only the outgoing handle. Shift keeps the new anchor at 45° steps from
the last. The canvas bar's Remove Point (Backspace), Cancel (Escape) and Finish
(Enter) work as for polygon selections; clicking the first anchor closes the
path. The finished path uses Figure's paint mode, line width and colors, with
sharp corners and flat ends, and is selected, in one undo step. Paint layers
refuse Path with a notice.

## Erasing

The Eraser, transparent paint and the pen's eraser end erase paths on a vector
layer. By default the eraser cuts paths where it covers their centerline: the
part under the eraser goes, the rest keeps its exact curve and widths, and a cut
closed path becomes open. The first piece keeps the path's identity and the
others get new ones. With **Erase whole paths** checked in the tool options, the
eraser removes every path it touches instead. The eraser's width is the brush
size, and one stroke is one undo step.

## Moving paths

With **Move** on a vector layer and no pixel selection:

- Pressing a path selects the front-most visible, unlocked path under the pen
  within 6 screen pixels and drags it. Pressing a selected path drags the whole
  selection. Shift adds a path to the selection or removes it.
- Pressing empty canvas and dragging draws a marquee that selects the paths it
  fully encloses; Shift adds them. A click on empty canvas clears the selection.
- A dashed box marks the selection's bounds, with the eight scale handles and
  the rotate handle image layers use. Dragging a side or corner scales the
  selected paths and their outline widths from the opposite edge (Shift keeps
  the proportions, Alt scales about the centre); the rotate handle turns them
  about their centre (Shift snaps to 15°). Each drag previews live and commits
  one undo step. Rectangles and ellipses stay live shapes while the transform
  keeps them rectangles and ellipses.
- Arrow keys nudge the selected paths by one pixel, ten with Shift; a held key
  previews the move and commits it as one undo step on release.
- Alt-dragging a path drags copies of the selected paths, placed in front, and
  leaves the originals; the copies become the selection.
- **Copy** and **Cut** with selected paths and no pixel selection copy those
  paths. **Paste** onto an unlocked vector layer adds them in front with new
  identities and selects them; elsewhere it adds a vector layer holding them.
  Other applications receive a PNG of the copied paths.
- **Clear Selected** (Delete) removes the selected paths.
- **Edit > Arrange** and the Move tool options restack the selected paths within
  their layer: Bring to front, Bring forward, Send backward and Send to back.
  Forward and backward step a run of selected paths past one neighboring path;
  unknown content from a newer app version is never stepped over. These commands
  have no default keys.

A drag commits one translation edit at pen-up. During the drag the moving paths
draw above the rest of their layer; they return to their place in the layer's
order when the move commits. Path selections are not undo steps and are not
restored with a session.

## Rasterize Layer

**Rasterize Layer** turns a vector layer into a paint layer holding what its
paths show, at the document grid, in one undo step. The layer keeps its opacity,
blend, mask, clipping and attached effects, which still apply once, as for image
layers ([Object layers](image-objects.md)).

## Layers from a newer version

A vector layer saved by a newer version that uses paths this version cannot
read opens locked, showing the picture the newer version saved with it, and
its paths are written back unchanged when the drawing is saved.

## Tests

`vector_strokes_smooth_and_weigh_input_like_raster_strokes` in
[`canvas.rs`](../../crates/layer-engine/src/canvas.rs) checks that a vector line
takes the raster stroke's dabs, and
`hand_drawn_lines_keep_few_anchors_within_tolerance` and
`late_pressure_corrections_replay_the_line` in
[`vector_stroke.rs`](../../crates/layer-engine/src/vector_stroke.rs) bound the
fitted anchors, widths and corrections.
`vector_lines_land_under_the_pen_on_moved_layers`,
`late_pressure_corrections_reshape_the_drawn_line_in_its_undo_step`,
`move_tool_selects_drags_and_deletes_vector_paths`,
`arrange_commands_restack_selected_paths_one_step_at_a_time`,
`figure_shapes_on_vector_layers_become_selected_editable_paths`,
`move_tool_nudges_duplicates_and_copies_paths`,
`properties_restyle_every_selected_path_in_one_undo_step`,
`rasterize_layer_turns_a_vector_layer_into_paint_in_one_undo_step`,
`monoline_draws_constant_width_vector_lines`,
`move_frame_scales_and_rotates_selected_paths_in_one_undo_step`,
`path_tool_builds_open_and_closed_paths_anchor_by_anchor`,
`edit_points_moves_adds_deletes_and_switches_anchors` and
`the_eraser_cuts_or_removes_vector_paths_in_one_undo_step` in
[`vector_line_tests.rs`](../../crates/layer-ui/src/vector_line_tests.rs) cover
the session rules. The GTK journey `native_vector_draw_move_and_delete` draws two
G-Pen lines with tablet input, drags one, undoes, selects the other with a
marquee and deletes it. `native_vector_tools` draws a Figure rectangle and a
Path, drags an anchor with Edit Points, restyles the path, scales the rectangle
with the Move frame and cuts a G-Pen line with the eraser. Run them with `--tablet` once per theme through
`CAPY_NATIVE_TEST_THEME` ([Linux](../development/linux.md)); both write their
captures to the run's `input` directory. On a tablet,
`AndroidVectorLayerTest#gPenLinesDrawMoveAndUndoOnVectorLayers` draws, drags and
undoes a G-Pen line with stylus events in both themes and checks the screen, and
`vectorToolsDrawEditRestyleScaleAndErase` walks the same tools as
`native_vector_tools` ([Android](../development/android.md#device-tests)). On
Web, `tools/performance/workspace-motion.sh web --vector-tools` walks them with
pen events in both themes and checks presented pixels; headless Chrome omits
WebGPU pixels from screenshots, so run it headed ([Web](../development/web.md)).
