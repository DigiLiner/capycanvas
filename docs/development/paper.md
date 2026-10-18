# Paper

[Developer guide](README.md) · [Texture programs](texture-programs.md) · [Brushes](../internals/brushes.md)

Paper gives a drawing a surface. A Paper layer holds the sheet's color and its
texture; every brush that reacts to tooth reads the height of the paper under
the layer it paints on. Textures come from built-in
[texture programs](texture-programs.md) or from imported images, and a small
paper library lets artists choose, import and reuse them. Brushes own no grain
textures.

This guide covers what artists see, the resolution model, the layer, brushes,
the library, rendering, saved data and what each host has.

## Scope

| First version | Later |
| --- | --- |
| Paper layer with Color, Texture and Scale | Visible relief on the canvas, through the planned [Paper / Canvas Texture](illustration-filters-proposal.md#paper--canvas-texture) effect reading the same paper |
| One resolution per drawing, 300 ppi by default | Texture parameters in Properties and saved variations |
| Thirteen built-in papers from eight programs | Portable user texture programs, shared with a portable custom-filter contract |
| Every grain brush follows the paper | Per-brush textures |
| Importing a paper image and an app-wide paper library | Paper that changes as it is drawn on: burnishing, sizing, absorbency |
| A brief preview of the paper on the canvas when it changes | Non-repeating full sheets and deckle edges |

## What artists see

1. **New drawing.** The dialog shows Width, Height and Resolution (300 ppi by
   default) beside the existing presets, Background and Blending. The drawing
   starts with a Paper layer under an empty layer. Its texture is the paper
   last chosen in the picker, at its real size.
2. **Choose a paper.** Tapping the Paper layer opens Properties, titled Paper:
   Color, a Texture tile and Scale. The Texture tile opens the paper picker;
   choosing a paper applies it.
3. **Scale the paper.** Dragging Scale shades the paper's relief over the
   canvas so its scale can be judged against the drawing. The same shading
   shows for a moment after a paper is chosen. It never appears in the artwork
   or export.
4. **Draw.** Pencils, charcoal, pastels and textured paint brushes catch the
   tooth. Texture strength in Tool Settings still sets how much a brush reacts.
   Bristol and hot press break a pencil line only lightly; rough breaks it
   everywhere.
5. **Import a paper.** Import… in the picker accepts an image file, adds it to
   the library and applies it.
6. **Resize, crop, rotate or flip the image.** The tooth stays attached to the
   marks already drawn, and Scale reports its new value.

## Controls

| Component | Controls |
| --- | --- |
| New drawing | Resolution (ppi) beside Width and Height, 300 by default and kept in saved presets. There is no paper choice: a new drawing uses the paper last chosen in the picker. |
| Image Size | Unchanged. It shows 300 ppi for a drawing without a resolution. |
| Layer › New | Paper, beside Solid Color and Gradient Fill. The filter drawer lists Paper with the other fills, from the catalog. |
| Layers panel | Paper stays at the bottom, outside groups, without a drag handle. Its color thumbnail opens Edit Color. When Paper is missing, every client shows a centered Add paper action immediately after the last layer, inside the scrolling list. It uses the standard layer row's height and padding, with regular text, square corners and native hover, pressed and focus feedback. Native keyboard activation works on the focused action; releasing a pointer outside cancels. Adding Paper again selects the existing sheet. |
| Properties | Titled Paper. Color with Use Color; Texture, a tile showing the current paper with logical-group spacing between its thumbnail and name; Scale, a logarithmic slider from 25% to 400%. Double-clicking the Scale label returns to 100%. |
| Paper picker | One grid: the thirteen built-in papers, imported papers, then Import…. The selected Paper's own image comes first when it is not in the library, with Add to Library. Imported papers have Rename and Delete. |
| Canvas | Relief shading while Scale changes and for a moment after a paper is chosen. |
| Tool Settings | Texture strength, unchanged. Brushes have no texture picker or texture scale. |

Choosing a paper is one undo step, and so is each Scale drag. The paper
feature adds no preference. A Properties title shows only the layer name when
it matches the layer type, so Paper is not titled "Paper (Paper)" and a Solid
Color named Solid Color is not repeated either.

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
- **Scale compares the paper with the real sheet.** 100% means the texture
  appears at its real size when the drawing prints at its paper resolution. A
  new drawing or Paper layer starts at 100%; double-clicking the Scale label
  returns to 100%; choosing another texture keeps the Scale.
- **Image commands keep the paper on the marks.** Crop, Image Size, rotation
  and flips move and scale the paper with the pixels, and Scale reports the
  result. Changing only the resolution changes no pixels; Scale then reports
  how the texture compares with real paper at the new print size.
- **Scale is one number.** After a resize with different horizontal and
  vertical factors, Scale shows their geometric mean, and changing it makes
  the paper uniform again. Image commands can take Scale beyond the slider's
  25–400%; the field shows the actual value.
- **Brushes have no texture scale,** and brush sizes stay in pixels.

| Action | Paper on the canvas | Scale shows |
| --- | --- | --- |
| New drawing at 300 ppi | Real size | 100% |
| New drawing at 72 ppi | Real size at 300 ppi | 100% |
| Open a photo saved at 72 ppi, or with no resolution | Real size at 300 ppi | 100% |
| Image Size to 200%, resolution unchanged | Grows with the marks | 200% |
| Resolution from 300 to 600 ppi only | Unchanged | 50% |
| Crop, rotate or flip | Moves with the marks | Unchanged |
| Copy a Paper from a 300 ppi drawing into a 600 ppi drawing | Same pixels | 50% |
| Set Scale to 100% after any of these | Real size at the paper resolution | 100% |

## Paper layer

### Model

Paper is a built-in fill program, `paper`, in the Fill category beside Solid
Color and Gradient Fill. It keeps the decision that made the default paper an
ordinary fill: masks, opacity, blending, history, thumbnails and color editing
come from the general layer rules. Paper stays at the bottom of the root stack;
other layers drop above it. It cannot be grouped or duplicated. Adding a sheet
when none exists inserts it at the bottom, regardless of the selected group.

| Value | Kind | Rule |
| --- | --- | --- |
| `color` | Color | Opaque. Declared as the program's constant color, so Paper keeps the startup background, clear folding and color thumbnails of Solid Color. |
| `texture` | Texture (new) | A built-in program with its values, or an imported image. Defaults to the paper last chosen in the picker, or Drawing paper. |

Paper's spatial reference maps paper millimeters to drawing pixels, and it is
the only stored scale. Scale is a Paper property read from it: the mapping's
pixels per millimeter divided by the paper resolution's, and editing Scale
rewrites the mapping's scale about the document origin. Inserting a Paper sets
the scale to the paper resolution, with the paper's origin at the document
origin. Crop, Image Size, rotation and flips compose the mapping, as they do
for Gradient Fill
([canvas geometry](../../crates/layer-core/src/canvas_geometry.rs)), so the
tooth follows the marks. Image Rotate offers only quarter turns and flips, so
the baked tile always stays aligned with the pixel grid.

`Document::new` creates a white Paper. One scene query, the nearest Paper at or
below a target, finds the paper for
[document creation](../../crates/layer-ui/src/document_creation.rs),
[photo projects](../../crates/layer-color/src/photo_project.rs), whose Paper is
hidden, and the [GTK opener](../../apps/layer-linux/src/files/open.rs).

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
| A selected Paper with a mask paints its mask | Adopted | Without a mask, Paper and other fills use the nearest artwork layer above them. If that layer is inside a group, there is no drawable target. |
| No masks on Paper | Rejected | Paper shares fill-mask editing, selection masks, mask paste and history. |
| One Paper per drawing, pinned to the bottom | Adopted | The sheet stays below the artwork. Shared edit validation protects its position through reorder, group, duplicate and paste. |
| Normal blend only | Rejected | Blend modes cost nothing; forbidding them adds code. |

Flatten and Merge Visible keep the bottom root Paper as a separate layer, as
Procreate keeps its background color; everything above it merges. Merging into
any other Paper bakes it into paint, as for any fill.

## Brushes

- **Every grain brush follows the paper.** `BrushGrain` in
  [`layer-core`](../../crates/layer-core/src/lib.rs) holds only Texture
  strength; brushes have no grain asset, scale, rotation, offset jitter or
  moving grain, and [`builtin_masks.rs`](../../crates/layer-render-wgpu/src/builtin_masks.rs)
  has no CPU paper generators. Watercolor transport fields and mask tips are
  separate from paper. Transparent Glaze has no grain.
- **Relief scales tooth.** Dry media use
  `S = contact paper × Texture strength × relief / 100 µm` in the
  [contact model](../history/brushes/dry-media-contact.md); the multiplicative
  grain of wet and stamp brushes scales by the same factor.
- **Brushes read the paper in document pixels.** `paper_uv` in
  [`contact_field.wgsl`](../../crates/layer-render-wgpu/src/contact_field.wgsl)
  adds the target layer's offset, so moving a layer does not shift its tooth.
- **Calibration.** Drawing paper gives every dry-media brush the tooth contrast
  of the single built-in paper that preceded Paper, so a 300 ppi drawing keeps
  its pencil, charcoal, chalk, pastel and crayon character. The other papers
  were tuned against it with every dry-media brush at three pressures. Every
  paper has tooth finer than a pencil tip, shaped like its own surface, so
  none fills a contact solid at medium pressure; smooth papers fill sooner,
  rough ones later. Program
  heights are tooth depths: a pencil reaches about 30–50 µm below the paper's
  middle, so deeper valleys stay white at any pressure.
- **Previews.** Toolbar strength and slider previews read the active drawing's
  paper; their cache key gains the paper's bake key. Catalog swatches are
  rendered on the default paper.

Per-brush textures stay out of the first version. They would need non-numeric
tool overrides (`ToolMemory` stores only numbers per setting), library
references from brush settings and a fallback when a texture is deleted. The
Paper layer already gives every brush the surface a painter chose; a
deliberately stylized brush texture is the remaining case.

## Paper library

### Items and storage

| Item | Source | Actions |
| --- | --- | --- |
| Built-in paper | A catalog preset of a texture program | Use |
| Imported paper | An image, stored as gray samples with a resolution | Use, Rename, Delete |
| The selected Paper's image, when not in the library | An image embedded in the drawing | Use, Add to Library |

Imported images are stored once by SHA-256 of their samples in a `papers`
folder of the data root, named in [`StorageRoots`](../../crates/layer-host/src/storage.rs)
and listed in the [storage guide](../internals/storage.md), with their previews
and an index of names and order beside them
([`papers.rs`](../../crates/layer-host/src/papers.rs)). The library allows 64
imported papers and 256 MiB; papers over a limit stay listed and removable. The
image's identity in a drawing comes from the same hash, so a reopened drawing
still finds its paper in the library. Hosts that have paper storage report the
library to the session ([`paper_library.rs`](../../crates/layer-ui/src/paper_library.rs));
a host without it shows only the built-in papers and no Import.

New drawings use the paper last chosen in the picker. That is remembered with
the other new-drawing state, not offered as a setting; an imported paper is
read when the library loads. If it was deleted, the default paper is used.

### Picker

One shared view model, presented by each host: a single grid with the
built-in papers (Hot press, Cold press, Rough, Drawing paper, Bristol, Ingres,
Pastel paper, Sanded pastel, Washi, Unryu, Handmade paper, Canvas and Linen),
then imported papers, then Import…. When the selected Paper
uses an embedded image that is not in the library, that paper comes first with
Add to Library. Tiles show 16 mm of each paper under raking light with its
name. Built-in previews are made from their bakes by a test and ship with the
app ([texture programs](texture-programs.md#validation-and-tests)); an imported
paper's preview is made from its image when it is imported. Choosing a tile
applies it; choosing an imported paper that is not in memory reads it on a
worker first. Imported papers take their names from their files; Rename and
Delete sit in their context menu, as for palettes, and Add to Library sits in
the drawing's own paper's menu. There is no separate library panel.

### Importing a paper

Import runs on a worker and never on a UI thread.

1. Decode the image through the existing open path and convert it to
   luminance; light is high.
2. Read the resolution from the file under the
   [paper resolution rule](#resolution-and-scale).
3. Limit each side to 4096 samples with the jittered resampler, adjusting the
   recorded resolution so the physical size is unchanged.
4. Make the edges meet. When opposite edges differ, the first tenth of each
   axis is cross-faded with the band past its end in a variance-preserving
   blend (Heitz and Neyret's histogram-preserving blending operator, HPG 2018),
   which keeps the height histogram and so the pencil tone, and that band is
   dropped.
5. Store the result and apply it to the selected Paper.

Requiring seamless images would be simpler, but most scans are not, and a seam
repeats as a grid that pencil shading reveals.

### Drawings and the library

Choosing an imported paper embeds its image in the drawing, so files open
anywhere. A paper embedded in someone else's drawing shows first in the picker
with Add to Library; adding it reuses an equal stored image.

## Rendering and performance

- **The canvas is unaffected.** Paper composites as a constant color, so
  startup, composition and its cost are those of Solid Color.
- **Bakes run off the frame path.** The [bake](texture-programs.md#the-bake)
  is GPU work in 256 × 256 pages, scheduled at brush priority by the compiler
  path, never on the CPU or in a browser task.
- **Bakes are cached in memory** by program, version, values and tile size,
  shared between drawings and released when unused. A rebake follows a change
  of texture, Size, Image Size or orientation, not a crop or a resolution-only
  change. A disk cache is not needed: a bake costs a few milliseconds on a
  desktop GPU and is not expected to exceed the shader preparation it already
  waits behind on a tablet.
- **Strokes wait for their paper** as they wait for shaders. The first frame
  does not.
- **Brush cost follows tooth contrast.** Brushes read the paper once per pixel,
  with its tile size and relief in the per-batch uniform and no extra shader
  variant. Dry media on Drawing paper run as fast as before Paper; rougher
  papers cost them a few percent more per update
  ([top tier](../performance/top-tier.md#paper)).
- **The paper preview** is a separate draw in the presentation pass with its
  own bind group, so the canvas shader and camera are unchanged. It shades the
  Paper's baked tile by its slope, white facing the light and black away from
  it, while Scale changes and for 1.2 s after a paper is chosen, then fades out
  ([`paper.rs`](../../crates/layer-ui/src/effects/paper.rs)). The visible interval
  starts after the viewport has encoded the relief; waiting for the paper bake
  or its presentation pipeline does not consume that interval. Replacing the
  renderer resends the relief and preserves its remaining visible interval.
- **Memory** is one byte per texel: 2.3 MB for a 128 mm watercolor tile at
  300 ppi, 4.3 MB for a 175 mm laid or washi tile, and at most 16 MB.

Every change to brush sampling measures the dry-media rows against a matched
control by the [measuring rules](../performance/measuring.md). Bake time counts
toward canvas readiness in [responsiveness](../performance/responsiveness.md);
it is measured on desktop only.

## Saved data

| Record | Change |
| --- | --- |
| `paper` built-in effect, version 1 | Values `color` and `texture`. The spatial reference maps paper millimeters to drawing pixels and holds the scale shown as Scale. |
| Effect value kind `texture` | `{"kind": "texture", "value": {"builtin": "watercolor", "version": 1, "values": {...}}}` or `{"kind": "texture", "value": {"image": {"ref": "..."}}}`. |
| `capy.image/1` | An imported paper is a `gray` image with a required resolution, read as data without profile conversion, as the [package contract](../reference/capy-package.md) plans for image inputs. |
| `capy.composition/2` | New drawings write their resolution. |

Fixed-file tests cover both texture forms, and the frozen built-in contracts
gain `paper` and each texture program. Brush presets are not stored in
drawings, so removing grain fields changes no file. Drawings made before Paper
keep their Solid Color.

## Status

| Area | State |
| --- | --- |
| Shared Rust | Paper layer, texture value, eight programs and thirteen papers, bake, brushes on Paper, resolution, Scale, relief preview, library policy, import |
| GTK | Properties with the paper picker, Import, Rename, Delete and Add to Library, Resolution in New drawing, relief preview and the Add paper row. `native_paper_picker_relief_and_scale` and `native_fill_drawing_targets_and_paper_order` cover the picker, drawing targets, masks and sheet creation in both themes. |
| Windows | Properties with the paper picker, Import, Rename, Delete and Add to Library, Resolution in New drawing, shared relief preview and the Add paper row. Image decoding and library files use native workers. The Layers Paper fixture covers the row in both themes. |
| Web, Android, Apple | Build and paint on Paper, offer the Add paper row and show Resolution in New drawing. Texture is a list of built-in papers, Scale is a slider and there is no library or Import. Web's `--paper-rows`, Android's missing-Paper and fill-row journeys, and Apple's shared Add Paper journeys cover the layer controls; Web's `--blending` and Android's `newDocumentCreatesTheChosenResolution` cover Resolution. |

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
| Texture parameters in Properties | Presets only | The simpler option wins for now. Presets with Scale cover the papers artists choose between; exposing parameters adds controls, text and frozen ranges for every program. |
| Paper preview on the canvas | Brief relief shading when the paper or its Scale changes | Thumbnails alone: a thumbnail cannot show scale relative to the drawing, which is the judgement Scale exists for. Shading while the picker is open: it needs hover to preview a tile, which touch screens lack, and keeps another state. |
| Scale model | One stored scale, read as Scale against the paper resolution | A scale value stored relative to the resolution when the Paper was made: after resampling to 200% it would still read 100%, and resetting it would do nothing. A scale tied live to the current resolution: changing only the resolution would rescale the tooth under the marks already drawn. A scale in pixels: the same paper would look different at every resolution and 100% could not mean real size. |
| Low resolutions | Below 150 ppi counts as 300 | Using the value as written. Photos, screenshots and web images carry 72 or 96 ppi by default, which would make tooth four times too fine and imported textures four times too large. |
| Control name | Scale | Size. Next to a paper, Size reads as the sheet's dimensions; Scale is the word texture controls use in Procreate and Photoshop. |
| Scale range | 25–400% | A separate, wider typed range. Real papers near their own size need nothing beyond it, and image commands can still take Scale outside it. |
| Picker tiles | Relief under raking light | A graphite stroke across each tile. It needs a brush render per tile, while raking light already shows the texture and its depth. |
| Picker layout | One grid, with the drawing's own paper first only when it is not in the library | Separate sections for this drawing, built-in and imported papers. The drawing's paper is usually already in the grid, so the section repeats it. |
| Built-in papers | Thirteen | Fourteen, adding Bristol vellum and raw linen. Bristol vellum sits between Drawing paper and Bristol with little visible difference, and painters draw on primed linen rather than raw. |
| Paper in New drawing | None; the paper last chosen | A picker in the dialog. It adds a second place to choose paper for a choice the Paper layer already makes. |
| Library management | Rename and Delete in the picker | A library panel: a handful of imported papers does not need one. Delete alone: imported names come from file names such as scan_0042.png. |
| Resolution controls | New drawing and Image Size | A resolution field on Paper or on import. New drawing and Image Size are where painting and photo apps put resolution, and a second field would let the paper disagree with the drawing. |
| App-wide library | Yes, small | Imports stored only in drawings. Every new drawing would need the file imported again, and new drawings could not start on a chosen paper. |
| Seamless import | Variance-preserving edge blend | Requiring tileable images. Most scans are not tileable, and the seam grid shows in every shaded area. |
| Brush textures | Every grain brush follows Paper | Keeping bundled grains per brush. That keeps five grain fields, the CPU generators and textures that ignore the chosen paper. |
| Per-brush textures | Later | They need typed overrides and library references for a minority case. |
| Disk cache for bakes | None | The simpler option wins while bakes stay within the shader preparation they wait behind. |
| Picker previews | Shipped for built-in papers, made at import for imported ones | Rendering previews on the GPU at run time. It needs a request and readback path on every host for images that never change between releases; a test keeps the shipped previews equal to their bakes. |
| Library index | A small index file beside the papers | The workspace database. Papers are app-wide, not per workspace, and the index keeps the folder self-contained. |
| Tile actions | Rename, Delete and Add to Library in each tile's menu | Buttons in the picker. They would show for every paper while applying to one. |

The Windows Paper picker marks the current tile and publishes its selected state
to accessibility clients.
The picker groups its tiles under the shared Texture label for accessibility.
Paper parameter uploads and bake admission return GPU mapping errors through
preparation, so hosts can retire a lost renderer through the usual recovery path.
