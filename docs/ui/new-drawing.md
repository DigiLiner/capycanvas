# New drawing

[Workspace and UI](README.md)

New drawing asks for a sheet the way an art store sells one: a size in the
unit the artist thinks in, a resolution, a color mode and a paper. A new window
always opens on a white A4 landscape sheet at 300 ppi (3,508 × 2,480 px, sRGB
8-bit, Drawing paper at 0% Texture visibility).

## What artists see

- **Main page.** A preview of the sheet at its proportions, color and grain,
  captioned with the matching preset and the orientation, and how many layers
  this device can hold at that size. Beside it: Name, Size (W and H, a link
  that keeps their ratio, the unit, portrait or landscape, and Resolution with
  common choices), Color (space, depth, blending) and Paper (the texture, with
  its Texture visibility in fainter text below its name, and the paper color).
  Presets ⌃, Cancel and Create sit in the footer; clicking the preview opens
  Presets too.
- **Units.** Millimetres, centimetres and inches keep the sheet's physical size
  when the resolution changes; pixels keep the pixel size. Dragging W or H
  lands on whole pixels and millimetres, tenths of a centimetre and eighths of
  an inch; typed values keep their precision.
- **Limits.** A sheet larger than the GPU accepts, or one that leaves no memory
  for a layer, shows why instead of the size line and cannot be created. Common
  resolutions that would cross a limit are disabled.
- **Presets.** Search, category pills, tiles with a sheet thumbnail, name, size
  and details, and a summary of the picked preset in the footer, whose texture
  name is followed by its visibility, as in "Sanded 50 %, Chalkboard". Recent
  comes first once a preset was used; Your presets appears once one is saved.
  Save As… names the current sheet into a category; your own presets have
  Rename and Delete. Select applies the picked preset.
- **Paper.** The Choose Texture grid with No paper first, the built-in papers,
  then imported papers, and an Import… tile last. Texture visibility, a 0–300%
  field like W and H, sits at the left of the footer and is disabled for No
  paper. Use Texture applies the paper and its visibility; Cancel keeps both.
  Light and Shows through paint start at their defaults.
- **Texture visibility in presets.** Screens, comics pages, print sizes and
  Blueprint start at 0%: their drawings are reproduced, so a lit sheet would
  print over the real one. Physical papers start at 100%, the relief their
  brushes feel. Newsprint and Chalkboard start at 50%; the rough Full sheet and
  the handmade Journal start at 150% so their surface reads.
- **Paper color.** The Edit Color form without its copy buttons and eyedropper,
  a Color / Transparent choice, and the Paper Colors palette with + and Delete.
  Transparent keeps the Paper layer but hides it.

## Model

All state and rules are shared Rust. Hosts present the view and forward input.

| Part | Where |
| --- | --- |
| Setups, units, built-in presets, paper colors, page text, preset catalog | [`new_drawing.rs`](../../crates/layer-ui/src/new_drawing.rs) |
| The open dialog, its drawers, actions and the Create order | [`new_drawing_dialog.rs`](../../crates/layer-ui/src/new_drawing_dialog.rs) |
| Saved presets, recents, the last custom sheet and the paper palette | `NewDocumentSettings` in [`document_creation.rs`](../../crates/layer-ui/src/document_creation.rs) |
| Device memory for layer capacity | `CanvasRenderer::memory_headroom`, from Vulkan, Metal or DXGI budgets |

The dialog opens with the last used preset or custom sheet. Built-in preset
identities, saved setups and recents are stored in settings; `This screen`
takes the display's pixels and density from the host.

## Host contract

1. New drawing raises the `document` request `new`. The host dispatches
   `UiAction::NewDrawing { action: Open { screen, untitled } }` with the
   display it shows the dialog on and the next untitled drawing name.
2. While `UiState.new_drawing` is present, the host presents it and sends a
   `NewDrawingAction` for every input. It renders again whenever the HOST
   region changes.
3. The host keeps its own color editor for the paper color, like Edit Color,
   and reports each value with `PaperColorValue`. Everything else, including
   when Transparent switches back to Color, is decided by the session.
4. `Create` validates against the device's current limits, records the preset
   as recent and leaves a `NewDrawingOrder` with the setup, the name, the paper
   material and the limits. The host builds the drawing from it on its worker
   (`OpenEnvironment::new_drawing` on native hosts) for the pending New request.
   Adoption names the drawing with `inherit_new_drawing_name`.
5. `Cancel`, or `Back` on the main page, closes the dialog; the host completes
   the New request as cancelled.

GTK is the reference presentation: [`new_drawing.rs`](../../apps/layer-linux/src/new_drawing.rs)
and its `.new-drawing-*` styles. Drawers slide up over the page; inputs are
34 px high with 6 px between controls, 10 px between rows and 14 px between
sections.

## Checks

- `cargo test --locked -p layer-ui new_drawing` covers presets, units, limits,
  the dialog's actions and the Create order without a window.
- `native_new_drawing_dialog_review` walks the GTK dialog in both themes.
