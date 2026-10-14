# Paper

[Developer guide](README.md) · [Texture programs](texture-programs.md) · [Brushes](../internals/brushes.md)

Paper gives a drawing a surface. A Paper layer holds the sheet's color and its
texture; every brush that reacts to tooth reads the height of the paper under
the layer it paints on. Textures come from built-in
[texture programs](texture-programs.md) or from imported images, and a small
paper library lets artists choose, import and reuse them. Brush-owned grain
textures are removed.

This plan covers what artists see, the resolution model, the layer, brushes,
the library, rendering, saved data and the order of work.

## Scope

| First version | Later |
| --- | --- |
| Paper layer with Color, Texture and Size | Visible relief on the canvas, through the planned [Paper / Canvas Texture](illustration-filters-proposal.md#paper--canvas-texture) effect reading the same paper |
| One resolution per drawing, 300 ppi by default | Texture parameters in Properties and saved variations |
| Fourteen built-in papers from eight programs | Portable user texture programs, shared with a portable custom-filter contract |
| Every grain brush follows the paper | Per-brush textures |
| Importing a paper image and an app-wide paper library | Paper that changes as it is drawn on: burnishing, sizing, absorbency |
| A Size preview on the canvas | Non-repeating full sheets and deckle edges |

## What artists see

1. **New drawing.** The dialog shows Width, Height and Resolution (300 ppi by
   default) beside the existing presets, Background and Blending. The drawing
   starts with a Paper layer under an empty layer. Its texture is the last
   paper chosen in any drawing, at its real size.
2. **Choose a paper.** Tapping the Paper layer opens Properties: Color, a
   Texture tile and Size. The Texture tile opens a picker with the papers in
   this drawing, the built-in papers and imported papers.
3. **Scale the paper.** Dragging Size, or browsing the picker, shades the paper
   relief over the canvas so the scale can be judged against the drawing. The
   shading disappears on release and never appears in the artwork or export.
4. **Draw.** Pencils, charcoal, pastels and textured paint brushes catch the
   tooth. Texture strength in Tool Settings still sets how much a brush reacts.
   Hot press barely breaks a pencil line; rough breaks it everywhere.
5. **Import a paper.** Import… in the picker accepts an image file, adds it to
   the library and applies it.
6. **Resize, crop, rotate or flip the image.** The tooth stays attached to the
   marks already drawn, and Size reports its new scale.

## Resolution and scale

Resolution is set in two places and paper scale in one, and each number means
the same thing in every drawing.

- **Resolution belongs to the drawing.** New drawing sets it, 300 ppi by
  default and kept in saved new-drawing presets, and Image Size changes it.
  Nothing else shows or asks for one.
- **Paper resolution.** Paper uses the drawing's resolution, except that a
  missing resolution or one below 150 ppi counts as 300 ppi. Cameras,
  screenshots and screen software write 72 or 96 ppi by default; nobody prints
  paper that coarse. Image Size shows 300 for a drawing without a resolution.
- **Texture resolution.** Programs define their features in millimeters. An
  imported image uses the resolution in its file under the same rule, so a
  600 dpi scan keeps its real size, and an image saved at 72 ppi or without a
  resolution maps one image pixel to one drawing pixel in a 300 ppi drawing.
- **Size compares the paper with the real sheet.** 100% means the texture
  appears at its real size when the drawing prints at its paper resolution. A
  new drawing or Paper layer starts at 100%; double-clicking the Size label
  returns to 100%; choosing another texture keeps the Size.
- **Image commands keep the paper on the marks.** Crop, Image Size, rotation
  and flips move and scale the paper with the pixels, and Size reports the
  result. Changing only the resolution changes no pixels; Size then reports how
  the texture compares with real paper at the new print size.
- **Size is one number.** After a resize with different horizontal and
  vertical factors, Size shows their geometric mean, and changing it makes the
  paper uniform again. The slider covers 25–400% on a logarithmic scale;
  typing accepts 1–10,000%, and image commands may leave Size outside the
  slider's range.
- **Brushes have no texture scale,** and brush sizes stay in pixels.

| Action | Paper on the canvas | Size shows |
| --- | --- | --- |
| New drawing at 300 ppi | Real size | 100% |
| New drawing at 72 ppi | Real size at 300 ppi | 100% |
| Open a photo saved at 72 ppi, or with no resolution | Real size at 300 ppi | 100% |
| Image Size to 200%, resolution unchanged | Grows with the marks | 200% |
| Resolution from 300 to 600 ppi only | Unchanged | 50% |
| Crop, rotate or flip | Moves with the marks | Unchanged |
| Copy a Paper from a 300 ppi drawing into a 600 ppi drawing | Same pixels | 50% |
| Set Size to 100% after any of these | Real size at the paper resolution | 100% |

| Control | Where it lives |
| --- | --- |
| Resolution | New drawing and Image Size |
| Paper color, texture and Size | Paper layer Properties |
| How strongly a brush catches tooth | Texture strength in Tool Settings |
| Imported paper resolution | Read from the file, never asked |

## Paper layer

### Model

Paper is a built-in fill program, `paper`, in the Fill category beside Solid
Color and Gradient Fill. It keeps the decision that made the default paper an
ordinary fill: masks, opacity, blending, clipping, merging, history,
thumbnails and color editing come from the general layer rules.

| Value | Kind | Rule |
| --- | --- | --- |
| `color` | Color | Opaque. Declared as the program's constant color, so Paper keeps the startup background, clear folding and color thumbnails of Solid Color. |
| `texture` | Texture (new) | A built-in program with its values, or an imported image. Defaults to the last paper chosen, or Cartridge. |

Paper's spatial reference maps paper millimeters to drawing pixels, and it is
the only stored scale. Size is a Paper property read from it: the mapping's
pixels per millimeter divided by the paper resolution's, and editing Size
rewrites the mapping's scale about the document origin. Inserting a Paper sets
the scale to the paper resolution, with the paper's origin at the document
origin. Crop, Image Size, rotation and flips compose the mapping, as they do
for Gradient Fill
([canvas geometry](../../crates/layer-core/src/canvas_geometry.rs)), so the
tooth follows the marks. Image Rotate offers only quarter turns and flips, so
the baked tile always stays aligned with the pixel grid.

`Document::new` creates a white Paper instead of a white Solid Color. One scene
query, the nearest Paper at or below a target, replaces the three ways code
finds the paper today: the first constant-color fill in
[document creation](../../crates/layer-ui/src/document_creation.rs), position 1
in [photo projects](../../crates/layer-color/src/photo_project.rs) and the
constant backdrop in the [GTK opener](../../apps/layer-linux/src/files/open.rs).

### Which paper a stroke uses

- **The nearest Paper at or below the target** in its stack, then in each
  enclosing stack. Painting into a Paper's own mask uses that Paper.
- **Appearance does not change tooth.** Visibility, opacity, blending, masks
  and clipping affect only how Paper looks. A Transparent background hides the
  Paper but keeps its tooth.
- **No Paper below** uses the default paper at 100% of the paper resolution.
- **The stroke keeps its paper.** It is resolved at pen-down and held with the
  stroke's view and brush settings, so queued contacts are unaffected by later
  edits.

### Restrictions

| Restriction | Status | Reason |
| --- | --- | --- |
| Paper draws a flat color | Adopted | Fill shaders never bind a texture and Paper keeps every constant-color shortcut. Opaque paint hides relief on the paper itself; visible relief belongs to the later appearance effect over the painting. |
| Paper color is opaque | Adopted | Transparency comes from visibility or layer opacity; the startup background stays exact. |
| Tooth ignores appearance | Adopted | Brushes read one texture per stroke and never a mask. |
| No offset or angle on Paper | Adopted | Position and orientation come from the spatial reference that image commands already update. |
| Painting on Paper requires a mask | Kept | The existing rule for fills. |
| No masks on Paper | Rejected | Masks already work on fills with no Paper-specific code. Forbidding them adds refusals to Add Mask, fill insertion with a selection, mask paste and drag, Merge and Group on five hosts. |
| One Paper per drawing, pinned to the bottom | Rejected | This is the protect-at-bottom logic removed when Paper became a fill. It needs refusals in duplicate, reorder, group, paste and merge, while the nearest-Paper query is a short stack walk. |
| Normal blend only | Rejected | Blend modes cost nothing; forbidding them adds code. |

Flatten and Merge Visible keep the bottom root Paper as a separate layer, as
Procreate keeps its background color; everything above it merges. Merging into
any other Paper bakes it into paint, as today.

## Brushes

- **Every grain brush follows the paper.** Today every grain-using preset uses
  canvas-fixed grain, and no preset uses moving grain. `BrushGrain` in
  [`layer-core`](../../crates/layer-core/src/lib.rs) shrinks to its Texture
  strength; its asset, behavior, scale, rotation and offset jitter are removed,
  together with the CPU paper generators in
  [`builtin_masks.rs`](../../crates/layer-render-wgpu/src/builtin_masks.rs).
  Watercolor transport fields and mask tips are unchanged. Transparent Glaze
  loses its inert grain.
- **Relief scales tooth.** Dry media use
  `S = contact paper × Texture strength × relief / 100 µm` in the
  [contact model](../history/brushes/dry-media-contact.md); the multiplicative
  grain of wet and stamp brushes scales by the same factor. A 100 µm paper
  behaves like today's calibration.
- **Brushes read the paper in document pixels.** `contact_paper` and
  `grain_uv` add the target layer's offset and read the tile texel at the
  document pixel. Today paper is read in layer-local pixels, so a moved layer
  shifts its tooth.
- **Recalibration.** The first phase bakes today's contact-paper recipe through
  the new path to prove an unchanged look. The default Cartridge paper is then
  tuned so a 300 ppi drawing keeps the current pencil and charcoal character,
  and the dry-media fixtures are reviewed.
- **Previews.** Toolbar strength and slider previews read the active drawing's
  paper; their cache key gains the paper's bake key. Catalog swatches are
  rendered on the default paper.

Per-brush textures stay out of the first version. They would need non-numeric
tool overrides (today `ToolMemory` stores only numbers per setting), library
references from brush settings and a fallback when a texture is deleted. The
Paper layer already gives every brush the surface a painter chose; a
deliberately stylized brush texture is the remaining case.

## Paper library

### Items and storage

| Item | Source | Actions |
| --- | --- | --- |
| Built-in paper | A catalog preset of a texture program | Use |
| Imported paper | An image, stored as gray samples with a resolution | Use, Rename, Delete |
| Paper in this drawing | An embedded image not in the library | Use, Add to Library |

Imported images are stored once by SHA-256 of their samples in a `papers`
folder of the data root, named in [`StorageRoots`](../../crates/layer-host/src/storage.rs)
and listed in the [storage guide](../internals/storage.md). Names and order
live in the workspace database beside the color library; Web uses IndexedDB.
The library allows 64 imported papers and 256 MiB. As in the
[ICC profile library](../../crates/layer-ui/src/profile_library.rs), papers over
a limit stay listed and removable, and the shared action transport carries no
file paths.

New drawings use the last paper an artist chose. That is remembered with the
other new-drawing state, not offered as a setting. If it was deleted, the
default paper is used.

### Picker

One shared view model, presented by each host: tiles in the sections In this
drawing, Built-in and Imported, then Import…. Tiles are GPU renders of the
paper under raking light with a short graphite stroke across them, produced
off the frame path like [filter previews](../../crates/layer-ui/src/filter_previews.rs).
Rename and Delete sit in the context menu, as for palettes. There is no
separate library panel.

### Importing a paper

Import runs on a worker and never on a UI thread.

1. Decode the image through the existing open path and convert it to
   luminance; light is high.
2. Read the resolution from the file under the
   [paper resolution rule](#resolution-and-scale).
3. Make the edges meet. When opposite edges differ, a band along each edge is
   cross-faded with the opposite edge in a variance-preserving blend (Heitz and
   Neyret's histogram-preserving blending operator, HPG 2018), which keeps the
   height histogram and so the pencil tone.
4. Limit each side to 4096 samples with the jittered resampler, adjusting the
   recorded resolution so the physical size is unchanged.
5. Store the result and apply it to the selected Paper.

Requiring seamless images would be simpler, but most scans are not, and a seam
repeats as a grid that pencil shading reveals.

### Drawings and the library

Choosing an imported paper embeds its image in the drawing, so files open
anywhere. A drawing whose embedded paper is not in the library lists it under
In this drawing with Add to Library; adding it reuses an equal stored image.

## Rendering and performance

- **The canvas is unaffected.** Paper composites as a constant color, so
  startup, composition and its cost stay as for Solid Color today.
- **Bakes run off the frame path.** The [bake](texture-programs.md#the-bake)
  is GPU work in 256 × 256 pages, scheduled at brush priority by the compiler
  path. It replaces today's CPU generation of contact paper, which on Web runs
  inside one browser task.
- **Bakes are cached in memory** by program, version, values and tile size,
  shared between drawings and released when unused. A rebake follows a change
  of texture, Size, Image Size or orientation, not a crop or a resolution-only
  change. A disk cache is not needed: a bake costs a few milliseconds on a
  desktop GPU and is not expected to exceed the shader preparation it already
  waits behind on a tablet.
- **Strokes wait for their paper** as they wait for shaders. The first frame
  does not.
- **Brush cost is unchanged.** Brushes still read the paper once per pixel; the
  per-batch uniform gains the tile size and relief, and no shader variant is
  added.
- **The Size preview** is an overlay in the presentation pass, like the
  [crop overlay](../internals/rendering.md), drawn only while Size is dragged or
  the picker is open.
- **Memory** is one byte per texel: 2.3 MB for a 128 mm watercolor tile at
  300 ppi, 4.3 MB for a 175 mm laid or washi tile, and at most 16 MB.

Every phase that changes brush sampling measures the dry-media rows against a
matched control by the [measuring rules](../performance/measuring.md). Bake
rows are added to the [performance targets](../PERFORMANCE_TARGETS.md).

## Saved data

| Record | Change |
| --- | --- |
| `paper` built-in effect, version 1 | Values `color` and `texture`. The spatial reference maps paper millimeters to drawing pixels and holds the scale shown as Size. |
| Effect value kind `texture` | `{"kind": "texture", "value": {"builtin": "watercolor", "version": 1, "values": {...}}}` or `{"kind": "texture", "value": {"image": {"ref": "..."}}}`. |
| `capy.image/1` | An imported paper is a `gray` image with a required resolution, read as data without profile conversion, as the [package contract](../reference/capy-package.md) plans for image inputs. |
| `capy.composition/2` | New drawings write their resolution. |

Fixed-file tests cover both texture forms, and the frozen built-in contracts
gain `paper` and each texture program. Brush presets are not stored in
drawings, so removing grain fields changes no file. Drawings made before Paper
keep their Solid Color.

## Order of work

UI is built and reviewed on GTK before it is ported, as the
[UI rules](../ui/README.md) require.

| Step | Work | Done when |
| --- | --- | --- |
| 1 | Bake executor, `texture` program kind and `tx_` library; today's contact paper as a program; brushes read the baked tile in document pixels; CPU generators deleted | Dry-media fixtures match within tolerance; periodicity tests pass; the bake runs on GTK, Web, Android, Apple and Windows; dry-media rows match the control |
| 2 | Resolution in New drawing and Image Size with the paper resolution rule; `paper` effect, `texture` value kind and physical spatial reference; `Document::new` creates Paper; nearest-Paper query; brushes follow Paper with relief scaling | Journeys 1, 4 and 6 pass on GTK in light and dark themes; recalibrated fixtures reviewed |
| 3 | The eight programs and fourteen presets, the picker and the Size preview | Statistics and moiré tests pass; journeys 2 and 3 pass on GTK; bake rows recorded on each tier |
| 4 | Import, the paper library and embedding | An imported paper survives save, reopen on another machine and Add to Library |
| 5 | Web, then Android, Apple and Windows | Each host passes the journeys in both themes |

## Measurements

The program structure rests on sourced process descriptions and scales; most
relief depths, the honeycomb geometry, grit coverage and fiber densities are
estimates.

| Measure | Method | Sets |
| --- | --- | --- |
| Relief, bearing curve and skew of each surface | Profilometry or photometric stereo of sheets the team owns | Relief and shaping in each program |
| Felt spacing on hot press, cold press and rough from two or three brands | Spectra and nearest-neighbor statistics of those height maps | `felt` and its presets |
| Honeycomb pitch, row spacing and rim depth | Raking-light scan of Mi-Teintes | `honeycomb` |
| Laid and chain spacing on modern Ingres papers | Transmitted-light scan | `laid` |
| Grit standard, coverage and protrusion | Microscopy | `sanded` |
| Canvas relief before and after each gesso coat | Profilometry of primed swatches | `canvas` |
| Graphite, charcoal and pastel coverage against pressure | Scanned swatches at controlled loads | Relief reference and Texture strength |
| Tile size at which repetition shows | Side-by-side renders | Program periods |
| Bake time per tier | Performance rows | Page size |

## Decisions

A capability stays only when it improves performance, has a meaningful
artistic effect, or makes the app simpler to use. The texture language's own
choices are in [texture program decisions](texture-programs.md#decisions).

| Question | Choice | Simpler option and why it loses, or wins |
| --- | --- | --- |
| Layer type | A fill program | A dedicated Paper occurrence. It was removed once already; a fill reuses every layer rule. |
| Visible relief on the Paper layer | No | It would make Paper a textured generator, losing the constant-color shortcuts, and paint covers it anyway. The simpler option wins. |
| Which paper a stroke uses | Nearest Paper at or below the target | One Paper per drawing needs pinning and refusals across every structural command. |
| Texture parameters in Properties | Presets only | The simpler option wins for now. Presets with Size cover the papers artists choose between; exposing parameters adds controls, text and frozen ranges for every program. |
| Size preview on the canvas | Overlay while editing | Thumbnails alone. A thumbnail cannot show scale relative to the drawing, which is the judgement Size exists for. |
| Scale model | One stored scale, read as Size against the paper resolution | A Size value stored relative to the resolution when the Paper was made: after resampling to 200% it would still read 100%, and resetting it would do nothing. A Size tied live to the current resolution: changing only the resolution would rescale the tooth under the marks already drawn. A scale in pixels: the same paper would look different at every resolution and 100% could not mean real size. |
| Low resolutions | Below 150 ppi counts as 300 | Using the value as written. Photos, screenshots and web images carry 72 or 96 ppi by default, which would make tooth four times too fine and imported textures four times too large. |
| Resolution controls | New drawing and Image Size | A resolution field on Paper or on import. New drawing and Image Size are where painting and photo apps put resolution, and a second field would let the paper disagree with the drawing. |
| App-wide library | Yes, small | Imports stored only in drawings. Every new drawing would need the file imported again, and new drawings could not start on a chosen paper. |
| Seamless import | Variance-preserving edge blend | Requiring tileable images. Most scans are not tileable, and the seam grid shows in every shaded area. |
| Brush textures | Every grain brush follows Paper | Keeping bundled grains per brush. That keeps five grain fields, the CPU generators and textures that ignore the chosen paper. |
| Per-brush textures | Later | They need typed overrides and library references for a minority case. |
| Disk cache for bakes | None | The simpler option wins while bakes stay within the shader preparation they wait behind. |
