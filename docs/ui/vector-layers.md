# Vector layers

[Workspace and UI](README.md)

**Layer > New Vector Layer** adds a layer that stores paths instead of pixels
([model](../reference/authored-model.md), [rendering](../internals/rendering.md#vector-layers)).

## Drawing

Plain round dry brushes, such as G-Pen, draw on a vector layer. While the
pen is down the line shows as a provisional path; at pen-up it is fitted to
cubic segments with a pressure width profile and added as one path in one undo
step. The eraser and other brushes refuse the stroke with a notice, and the
canvas cursor shows the refusal before contact.

## Moving paths

With **Move** on a vector layer and no pixel selection:

- Pressing a path selects the front-most visible, unlocked path under the pen
  within 6 screen pixels and drags it. Pressing a selected path drags the whole
  selection. Shift adds a path to the selection or removes it.
- Pressing empty canvas and dragging draws a marquee that selects the paths it
  fully encloses; Shift adds them. A click on empty canvas clears the selection.
- A dashed box marks the selection's bounds.
- **Clear Selected** (Delete) removes the selected paths.

A drag commits one translation edit at pen-up. During the drag the moving paths
draw above the rest of their layer; they return to their place in the layer's
order when the move commits. Path selections are not undo steps and are not
restored with a session.

## Tests

`move_tool_selects_drags_and_deletes_vector_paths` in
[`vector_line_tests.rs`](../../crates/layer-ui/src/vector_line_tests.rs) covers
the session rules. The GTK journey `native_vector_draw_move_and_delete` draws two
G-Pen lines with tablet input, drags one, undoes, selects the other with a
marquee and deletes it. Run it with `--tablet` once per theme through
`CAPY_NATIVE_TEST_THEME` ([Linux](../development/linux.md)). On a tablet,
`AndroidVectorLayerTest#gPenLinesDrawMoveAndUndoOnVectorLayers` draws, drags and
undoes a G-Pen line with stylus events in both themes and checks the screen
([Android](../development/android.md#device-tests)).
