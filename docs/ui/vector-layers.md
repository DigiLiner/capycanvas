# Vector layers

[Workspace and UI](README.md)

**Layer > New Vector Layer** adds a layer that stores paths instead of pixels
([model](../reference/authored-model.md), [rendering](../internals/rendering.md#vector-layers)).

## Drawing

Plain round dry brushes, such as G-Pen, draw on a vector layer. While the
pen is down the line shows as a provisional path; at pen-up it is fitted to
cubic segments with a pressure width profile and added as one path in one undo
step. Other brushes refuse the stroke with a notice, and the canvas cursor shows
the refusal before contact.

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
- A dashed box marks the selection's bounds.
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

## Tests

`move_tool_selects_drags_and_deletes_vector_paths`,
`arrange_commands_restack_selected_paths_one_step_at_a_time` and
`the_eraser_cuts_or_removes_vector_paths_in_one_undo_step` in
[`vector_line_tests.rs`](../../crates/layer-ui/src/vector_line_tests.rs) cover
the session rules. The GTK journey `native_vector_draw_move_and_delete` draws two
G-Pen lines with tablet input, drags one, undoes, selects the other with a
marquee and deletes it. Run it with `--tablet` once per theme through
`CAPY_NATIVE_TEST_THEME` ([Linux](../development/linux.md)). On a tablet,
`AndroidVectorLayerTest#gPenLinesDrawMoveAndUndoOnVectorLayers` draws, drags and
undoes a G-Pen line with stylus events in both themes and checks the screen
([Android](../development/android.md#device-tests)).
