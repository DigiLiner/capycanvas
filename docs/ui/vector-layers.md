# Vector layers

[Workspace and UI](README.md)

A vector layer stores paths instead of pixels
([model](../reference/authored-model.md), [rendering](../internals/rendering.md#vector-layers)).
**New vector layer** is the second button in the Layers panel footer, the item
after New layer in **Layer › New**, and a command with Ctrl+Alt+Shift+N that
command search also finds by words such as ink, line art and linework. Krita's
Shift+Insert stays Paste, as in every preset. The New submenu is there in every layer state,
including while a mask or selection layer is edited and with no layer. Vector
rows carry a path badge on their thumbnail, and Properties names the layer
type "Vector layer" under its name.

A vector layer's mask takes brush strokes like any mask, and the vector tools
leave the paths alone while it is edited. Rotate, Flip and Image Size move and
scale the paths with the canvas, scaling outline widths and keeping rectangles
and ellipses live. Clear Layer removes every path in one undo step; Fill
Selection is unavailable. Copy with a pixel selection, and the Magic Wand and
Select Color set to Editing, read what the layer shows.

## Drawing

Plain round dry brushes draw on a vector layer: G-Pen, whose pressure sets the
width, and Monoline, which keeps one width. The line follows the same Smoothing,
pressure curve, rulers and late stylus pressure updates as the brush does on a
paint layer. While the pen is down it shows as a provisional path whose tip
reaches ahead of the stylus with the paint layer's prediction; at pen-up it
becomes one path in one undo step, made of as few cubic segments as stay within
half a screen pixel of the drawn line, with smooth anchors where curves meet and
corners where the line turns sharply. Its width profile keeps pressure changes
to the same half pixel with the fewest width points. Pressure updates that
arrive after pen-up reshape that path within its undo step.

Making or selecting a vector layer while a brush that cannot draw paths is
active switches to the last pen that drew paths, G-Pen at first; selecting a
paint layer again brings that brush back unless another was chosen meanwhile.
On a vector layer the Tool panel dims brushes and brush groups that cannot draw
paths, including a pen whose settings, such as size jitter or grain, stop it.
A refused stroke shows the canvas cursor's refusal before contact and a notice
naming the layer, with **Use G-Pen** (the last pen that drew paths), **New Paint
Layer** above it and **Rasterize Layer**. When a setting keeps a pen from
drawing paths, the notice names the setting and offers to reset it instead.
Tools that need pixels, such as Gradient, offer **New Paint Layer Below** and
Rasterize Layer.

The line is fitted while it is drawn, a bounded amount each frame, so pen-up
fits only its last segments. Each segment reaches as far as one cubic stays
within the tolerance, with a minimax refinement when least squares narrowly
misses it. The tolerance holds between samples too: each chord's farthest
point from the curve is solved exactly, and chords beside a sharp turn are
also checked for overshoot. Each smooth anchor is placed by trying the last few points before
that reach and a few tangents between the arriving and leaving directions, and
keeping the one that lets the next segment reach farthest. A decision that
would depend on input not yet drawn waits, and late pressure updates undo only
the decisions that saw the changed samples, so the result matches fitting the
whole line at pen-up. Width points are the fewest that keep every sample within
the tolerance, found as a shortest path.

## Figures

Figure offers Line, Rectangle, Ellipse, Polygon, Star and Path. Its tool
options add **Corner radius** for Rectangle, **Corners** for Polygon, and
**Points** and **Inner radius** for Star (3 to 100 corners or points; 5 and 50%
by default). On a vector layer each figure adds one path, selected and in front
of the others, instead of painting pixels. Line draws with round caps.
Rectangle, Ellipse, Polygon and Star keep their frame and options as a live
shape, so later moves, rotations, flips and uniform scaling keep them editable
shapes; the first corner or point of a polygon or star sits at the top of its
frame. Outline strokes with the foreground color at the line width; Fill fills
with it; Outline and Fill fills with the background and strokes with the
foreground above it. Rectangles, polygons and stars get sharp corners. Shift
makes squares, circles, regular shapes and 45° lines, and brush opacity sets
the color's alpha. Transparent paint is refused.

On a paint layer, Line, square-cornered Rectangle and Ellipse paint as before.
Rounded rectangles, polygons, stars and Path paint the same path the vector
layer would hold into the layer's pixels, in one undo step that re-bakes the
layer; an alpha-locked layer keeps its transparency, and the pixel selection
does not limit them. While drawing, they show their outline.

## Properties

With paths selected, the layer's Properties add Fill, Fill color, Outline,
Outline color, Outline width, Line ends (Flat, Round, Square) and Corners
(Sharp, Round, Beveled), with **Overlaps** (Filled, Holes) for filled paths and
**Sharp corner limit** for sharp corners. They show the front-most selected path, and a label
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
click or deletion is one undo step. The Edit Points tool options offer Split at
point, Join paths, Close path and Reverse direction.

## Path commands

**Edit › Path** acts on the selected paths, each command in one undo step, and
keeps outline widths and width profiles along the result:

- **Join paths** connects the nearest ends of two open paths, merging ends
  within 6 screen pixels and otherwise adding a straight segment; the front
  path keeps its identity and style. With one open path selected it closes it.
- **Close path** adds a segment from each selected open path's end to its
  start.
- **Split at point** cuts the selected paths at the anchors selected in Edit
  Points; a closed path opens there, and the pieces stay selected.
- **Reverse direction** runs each path the other way.
- **Make compound path** combines the selected paths into one that cuts holes
  where they overlap, with the back-most path's style and place. **Release
  compound path** makes each part its own path again.

Starting G-Pen or Monoline within 6 screen pixels of an open end of a selected
path continues that path, in the same undo step as the line; the Path tool
does the same with its first click, and clicking the path's other end closes
it.

## Path

Figure's **Path** draws a path anchor by anchor. A click adds
a corner; a press and drag pulls out symmetric handles for a smooth anchor, and
Alt drags only the outgoing handle. Shift keeps the new anchor at 45° steps from
the last. The canvas bar's Remove Point (Backspace), Cancel (Escape) and Finish
(Enter) work as for polygon selections; clicking the first anchor closes the
path. The finished path uses Figure's paint mode, line width and colors, with
sharp corners and flat ends, and is selected, in one undo step. On a paint
layer the draft shows as an outline and Finish paints it into the layer.

## Erasing

The Eraser, transparent paint and the pen's eraser end erase paths on a vector
layer. By default the eraser cuts paths where it covers their centerline: the
part under the eraser goes, the rest keeps its exact curve and widths, and a cut
closed path becomes open. The first piece keeps the path's identity and the
others get new ones. With **Erase whole paths** checked in the tool options, the
eraser removes every path it touches instead. The eraser's width is the brush
size, and one stroke is one undo step.

## Moving paths

With **Move** on a vector layer, whether or not pixels are selected:

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
- **Select All** (Ctrl+A) on a vector layer with paths selects every path and
  switches to Move; on an empty vector layer it selects pixels as usual.
  **Deselect** and Escape clear the path selection, along with any pixel
  selection. **Edit › Transform** (Ctrl+T) with paths selected switches to Move,
  whose frame transforms them.
- With paths selected, the Move tool options add Flip Horizontal, Flip
  Vertical, Rotate Left and Rotate Right, which turn the selection about its
  centre in one undo step, and **Scale outline widths**, on by default. Turned
  off, dragging the frame scales the paths but keeps their outline widths.
- Enter or a double-click on a path opens **Edit Points** for the selection;
  Escape in Edit Points returns to Move.
- **Copy** and **Cut** with selected paths and no pixel selection copy those
  paths. **Paste** onto an unlocked vector layer adds them in front with new
  identities and selects them; elsewhere it adds a vector layer holding them.
  Other applications receive a PNG of the copied paths.
- **Clear Selected** (Delete), labelled **Delete** while paths or anchors are
  selected, removes them.
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

`vector_strokes_smooth_weigh_and_predict_input_like_raster_strokes` in
[`canvas.rs`](../../crates/layer-engine/src/canvas.rs) checks that a vector line
takes the raster stroke's dabs and shows its predicted preview on every frame,
`vector_line_previews_reach_predicted_samples_without_drawing_them` checks the
session forwards platform predictions to the preview only, and
`hand_drawn_lines_keep_few_anchors_within_tolerance`,
`recorded_lines_stay_within_tolerance_between_samples`,
`fitting_while_drawing_matches_fitting_at_pen_up` and
`late_pressure_corrections_replay_the_line` in
[`vector_stroke.rs`](../../crates/layer-engine/src/vector_stroke.rs) bound the
fitted anchors, widths and corrections, check recorded Movink strokes against
the drawn line between samples, and check that fitting while drawing gives the
same path as fitting at pen-up.
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
`edit_points_moves_adds_deletes_and_switches_anchors`,
`vector_layer_masks_take_brush_strokes_while_paths_stay`,
`move_and_edit_points_reach_paths_while_pixels_are_selected`,
`clear_layer_removes_every_path_in_one_undo_step`,
`copy_and_the_wand_read_what_a_vector_layer_shows`,
`new_vector_layers_switch_to_a_pen_and_paint_layers_switch_back`,
`the_layer_menu_offers_new_vector_layer_in_every_state`,
`painting_brushes_on_vector_layers_offer_a_pen_a_paint_layer_or_rasterizing`,
`a_pen_setting_that_stops_paths_is_named_and_reset_from_the_refusal`,
`fill_tools_on_vector_layers_offer_a_paint_layer_below`,
`select_all_transform_flip_escape_and_delete_act_on_paths`,
`path_commands_join_split_close_reverse_and_compound_selected_paths`,
`pens_and_the_path_tool_continue_a_selected_paths_open_end`,
`figure_options_shape_rectangles_polygons_and_stars_on_both_layer_kinds`,
`edit_transform_shows_the_move_frame_and_outline_widths_can_stay` and
`the_eraser_cuts_or_removes_vector_paths_in_one_undo_step` in
[`vector_line_tests.rs`](../../crates/layer-ui/src/vector_line_tests.rs) cover
the session rules. `vector_paths_follow_rotation_flip_and_resize` in
[`canvas_geometry_tests.rs`](../../crates/layer-core/src/canvas_geometry_tests.rs),
`merging_a_vector_layer_down_bakes_its_paths` in
[`merge_tests.rs`](../../crates/layer-core/src/merge_tests.rs) and the GPU test
`rasterizing_a_vector_layer_bakes_what_it_shows` in
[`vector_tests.rs`](../../crates/layer-render-wgpu/src/scene/vector_tests.rs)
cover canvas edits and baking; `paths_paint_over_a_paint_layer_through_a_bake_of_the_layer_and_a_reserved_path_layer`
and the GPU test `paths_painted_onto_paint_layers_match_the_path_drawn_above_them`
cover paths painted onto paint layers, and
[`ops_tests.rs`](../../crates/layer-core/src/authored/vector/ops_tests.rs) covers
the path commands' geometry and width profiles. The GTK journey `native_vector_draw_move_and_delete` draws two
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
