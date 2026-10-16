# Non-raster content in painting apps, and how it fits the Capy object model

[Object content extensions](../object-content-extensions-research.md) · source report, 2026-10-16

Checked against the vector design at `726cb6cdc`, or the draft before it in which recipes were separate records keyed by item ID. The design this audit produced is in [vector layers §7.2–7.4](../vector-layers-research.md#72-layer-model).

Research date: 2026-10-16. Apps: Photoshop (2026 line), Clip Studio Paint (CSP) 4.x/5.x, Krita 5.2/5.3/6.0, Procreate 5.4.x (and Dreams), Adobe Fresco, PaintTool SAI 2, MediBang (Paint and the newer Paletta/"MediBang Pro" app), ibisPaint, and Affinity only where it adds something.

Legend: **[K]** = general knowledge, not re-verified today. **(unverified)** = a claim from a weak or indirect source, or an inference. "search" = the claim comes from a search-result summary, because the page blocked fetching (Adobe helpx returns HTTP 403 to scripted fetches).

Source code read (cloned for reading only, nothing copied):
- MIT: [ag-psd](https://github.com/Agamnentzar/ag-psd) (`src/additionalInfo.ts`, `src/psd.ts`), [psd-tools](https://github.com/psd-tools/psd-tools) (`src/psd_tools/api/shape.py`), [clipparse](https://github.com/wamsoft/clipparse) (`docs/CLIP_FORMAT.md`, `docs/CLIP_TOOLS_REPORT.md`), [clip_to_psd](https://github.com/dobrokot/clip_to_psd).
- [animeops/clip-tools](https://github.com/animeops/clip-tools) (MIT license, copyright AnimeOps 2026): `clip_tools/structs/vector.py`, `sqlite_records.py`, `types.py`, `constants.py`, `structs/text_attributes.py`.
- [silicate](https://github.com/Avarel/silicate): license not checked.
- Krita's `kis_kra_tags.h` (GPL, read only): <https://github.com/KDE/krita/blob/master/plugins/impex/libkra/kis_kra_tags.h>.

That commit already maps SVG, Inkscape, Illustrator, Affinity and Figma. This report concentrates on what painting and comic apps add.

---

## 0. Overview

| Content | Photoshop | CSP | Krita | Procreate | Fresco | SAI 2 | MediBang | ibisPaint |
|---|---|---|---|---|---|---|---|---|
| Vector paths and strokes | Shape layers: one compound path per layer, per-subpath booleans | Vector layers: centerline strokes rendered by the brush engine, width and opacity per point | SVG shapes in vector layers | None stored (QuickShape is transient) | Vector brushes stored as filled outlines | Linework layers (pressure per point) | Shapes and panels as "objects" (Paletta) | Vector layers (Premium) |
| Parametric shapes | Live shapes (vogk) | Figure tools mostly bake to vector lines [K] | Rectangle, ellipse, star, etc. [K] | No | Editable shapes (2024) | Shape layer: rectangle, ellipse, polygon (unverified) | Round-corner rectangle, polygon | Shapes convertible to polylines |
| Text | TySh + EngineData, live | Text layer with ruby, TCY, vertical | SVG 2 text (5.3) | Vector text until rasterized | Not researched | Text layer (vertical in a 2026 preview) | Text layer (MDP only) | Text, vertical |
| Speech balloons | None | Balloon layers: parametric tails, union of overlaps | Vector shapes | No | No | No | Materials [K] | Brushes and rulers |
| Comic panels | Frame tool; vector masks on groups | Frame border folders that clip all child layers | Shapes plus the 5.3 panel tool; clipping is manual | No | No | No | Panel material objects | Frame Divider |
| Focus and speed lines | No | Editable parametric layers | No [K] | No | No | No | Not found | Radial Line filter (raster) |
| Screentone | No (pattern fill) | Tone layer property | Screentone fill generator | No | No | No | Halftone on 8-bit layers | Not researched |
| Fills and gradients | SoCo/GdFl/PtFl fill layers; live gradients | Fill and gradient layers | Generator fill layers | No | No | No | No [K] | No [K] |

---

## 1. Photoshop

### 1.1 Shape layers are a fill layer plus a vector mask
- **Storage.** A shape layer combines fill content (`SoCo`, `GdFl`, `PtFl`), a vector mask (`vmsk`/`vsms`), stroke data (`vstk`, `vscg`) and origination data (`vogk`) ([Adobe PSD spec](https://www.adobe.com/devnet-apps/photoshop/fileformatashtml/)).
- **One fill and at most one stroke per layer.** Capy's ordered paint list is a superset.
- **`vstk` fields** (ag-psd `vectorStroke` and its `strokeStyle*` keys):
  - width, dash offset and dash set, miter limit, cap, join;
  - alignment (inside, center or outside), scale lock, stroke adjust;
  - **the stroke's own blend mode and opacity**;
  - content (solid, gradient or pattern).
- **Path records.** These are 26-byte records with selectors 0–8: subpath length records, linked or unlinked knots, fill rule, clipboard, and initial fill rule. "Paths use even/odd ruling", and an initial fill value of 1 "means that the fill starts with all pixels" ([spec](https://www.adobe.com/devnet-apps/photoshop/fileformatashtml/)).
- **Coordinates** are fixed-point fractions of the document size (ag-psd `writeFixedPointPath32(points/width)`), so an importer must scale them.
- **Knots** carry a "linked" flag, which maps to Smooth or Corner.

### 1.2 Per-subpath booleans
- **One operation per subpath.** Each subpath length record holds an op: −1 none, 0 xor, 1 or, 2 not-or (subtract), 3 and (intersect).
  - psd-tools: "The first path element is applied to the background surface. Intersection does not have strokes" ([psd-tools vector reference](https://psd-tools.readthedocs.io/en/latest/reference/psd_tools.psd.vector.html)).
  - ag-psd: `booleanOperations = ['exclude','combine','subtract','intersect']`.
- **Each subpath has an index into the origination list** ([psd-tools](https://psd-tools.readthedocs.io/en/latest/reference/psd_tools.psd.vector.html)).
- **Fill rule: conflicting, unverified.** ag-psd also reads a per-subpath flag (`flags === 2 ? 'non-zero' : 'even-odd'`); psd-tools says even-odd.
- **UI.** Combine, Subtract Front, Intersect and Exclude apply in drawing order, and **Merge Shape Components** bakes them ([Photoshop Essentials](https://photoshopessentials.com/?p=823), [Adobe Community](https://community.adobe.com/t5/photoshop/photoshop-combine-paths/m-p/10056324)).
- **PSD stores only operands and operations, never the result.** A Capy importer that stores result plus recipe must compute cubic-Bézier booleans.
  - Combine and exclude of simple subpaths need no boolean engine: combine is nonzero with one shared orientation, and exclude is even-odd.
  - Subtract and intersect need real booleans.
- Whether Photoshop strokes the result outline or each operand is unverified.

### 1.3 Live shapes (`vogk`)
- **Fields** (psd-tools `api/shape.py`; ag-psd `VogkDescriptor`):
  - `keyOriginType`: 1 rectangle, 2 rounded rectangle, 4 line, 5 ellipse;
  - `keyOriginRRectRadii`: four corner radii;
  - `keyOriginShapeBBox`;
  - **`keyOriginBoxCorners`**: four transformed corners, so a rotated or skewed rectangle stays live;
  - line start and end, weight, arrowheads (start, end, width, length, concavity);
  - a transform, resolution and `keyOriginIndex`.
- **`keyShapeInvalidated`** marks a shape whose path was edited by hand. The geometry stays and the live properties are lost: the same rule as Capy's "drop the recipe when the result changes".
- **Polygons** became live shapes in v22 (2021) ([Adobe Community, search](https://community.adobe.com/t5/photoshop-ecosystem-discussions/properties-panel-not-showing-correct-properties/m-p/13149242)). Options are sides, corner radius, star ratio and smooth indents ([Adobe user guide, search](https://learn.adobe.com/photoshop/using/drawing-shapes.html)). A Triangle tool also exists. Their keys were not found (unverified).

### 1.4 Vector masks on any layer
- `vmsk` works on pixel layers and groups, with invert, not-linked and disabled flags (ag-psd `LayerVectorMask`).
- A layer can have a raster `LMsk` user mask at the same time ([spec](https://www.adobe.com/devnet-apps/photoshop/fileformatashtml/)).

### 1.5 Fill layers and live gradients
- **Fill layers** (SoCo, GdFl, PtFl) fill the whole canvas and are shaped by masks.
- **Live gradients (May 2023).** In Gradient mode the tool creates a live Gradient Fill layer with on-canvas stops, midpoints, angle and size ([Photoshop Essentials](https://www.photoshopessentials.com/?p=34781), [Adobe](https://www.adobe.com/products/photoshop/gradient-tool.html)).
- **Interpolation:** Classic, Linear and Perceptual (plus Smooth and Stripes [K]) ([luminousworks](https://luminousworks.substack.com/p/grokking-gradients), [Adobe Community](https://community.adobe.com/t5/photoshop-ecosystem-discussions/soft-transitions-in-gradients/m-p/13991378)).
- **Styles:** linear, radial, angle, reflected and diamond [K].

### 1.6 Text (`TySh`)
- **Container.** A 2×3 transform; a text descriptor whose `EngineData` (a PostScript-like dictionary) holds style runs and paragraph runs; a warp descriptor; and bounds. `Txt2` holds document-wide engine data ([spec](https://www.adobe.com/devnet-apps/photoshop/fileformatashtml/)).
- **Layout** (ag-psd `LayerTextData`): point or box text, text on a path (Bézier control points, read-only in ag-psd), horizontal or vertical orientation.
- **Warp:** 17 named styles plus custom, with bend and perspective; the custom envelope is 16 mesh points with quilt slices.
- **Run attributes:** font (PostScript name), size, faux bold and italic, leading, tracking, kerning, baseline shift, caps, super- and subscript, underline, strike, ligatures, `tsume`, `baselineDirection`, fill and stroke colors with flags, `fillFirst` (paint order), outline width, language.
- **Paragraph attributes:** justification, indents, spacing, hyphenation, `burasagari`, `kinsokuOrder`, composer.
- **Japanese features.** Tate-chu-yoko exists [K] (unverified). No ruby was found; Adobe documents ruby for InDesign/InCopy ([InCopy CJK](https://helpx.adobe.com/incopy/using/formatting-cjk-characters.html)), so "no ruby in Photoshop" is unverified.
- **Fonts** are referenced by name [K].

### 1.7 Smart objects
- **Storage.** `PlLd`/`SoLd` reference embedded or linked files (`lnkD`, `lnk2`, `lnk3`) by GUID.
- **Fields** (ag-psd `PlacedLayer`):
  - type (vector, raster or image stack);
  - **`transform` as four corners** plus `nonAffineTransform`, so perspective is possible;
  - **warp** (a style or a custom mesh);
  - crop, page, and **comp**: which layer comp of the inner document shows;
  - **smart filters**, each with its own blend and opacity, plus a filter mask (`FMsk`).
- **Nesting and sharing.** A smart object can hold a whole layered PSB, and duplicates share their source [K].

### 1.8 Frames and artboards
- **Frame tool (2019)** is scripted as a `framedGroupSection` with a `framedGroupRect` ([CC developer forum](https://forums.creativeclouddeveloper.com/t/how-to-tell-if-layer-has-an-existing-frame-frame-tool/5586.md)): a group that clips one placed image to a rectangle or ellipse. Its PSD encoding is unverified.
- **Artboards** (`artb`, `artd`/`abdd`) are groups that clip their children to a rectangle and have a background [K].

### 1.9 Layer styles, fill opacity, blending options
- **`lfx2`** effects (ag-psd `LayerEffectsInfo`):
  - can repeat: drop shadow, inner shadow, color overlay, gradient overlay, stroke;
  - single: outer glow, inner glow, bevel, satin, pattern overlay.
- **Fill opacity** (`iOpa`) is separate from layer opacity.
- **Blending options:** `clbl`, `infx`, `knko`, `brst` ([spec](https://www.adobe.com/devnet-apps/photoshop/fileformatashtml/)).

### 1.10 Layer comps
- Image resource 1065 is a descriptor ([spec](https://www.adobe.com/devnet-apps/photoshop/fileformatashtml/)). It records per-layer visibility, position and style [K].

### 1.11 Unknown blocks and the raster fallback
- **Skipping.** Blocks are length-prefixed, so readers can skip them ([spec](https://www.adobe.com/devnet-apps/photoshop/fileformatashtml/)).
- **Preserving unknown blocks on save is undocumented.** Photoshop drops what it cannot interpret [K] (unverified).
- **The real compatibility mechanism is the raster fallback:**
  - Every layer, type and shape layers included, carries its rendered pixels [K].
  - An optional merged composite is written when Maximize Compatibility is on ([spec](https://www.adobe.com/devnet-apps/photoshop/fileformatashtml/)).
  - psd-tools composites from these cached pixels and lists "Font rendering" as not supported ([psd-tools docs](https://github.com/psd-tools/psd-tools/blob/main/docs/index.rst)).
- **Other tagged data seen in ag-psd:** `GenI` (generative layers), `CAI ` (C2PA) and OCIO.

---

## 2. Clip Studio Paint

### 2.1 The `.clip` container
- **Layout.** A `CSFCHUNK` header, then `CHNKHead`, zlib `CHNKExta` chunks, `CHNKSQLi` (a complete SQLite database) and `CHNKFoot`. Binaries are found through `ExternalChunk` and `ExternalTableAndColumnName` ([clipparse CLIP_FORMAT.md](https://github.com/wamsoft/clipparse/blob/main/docs/CLIP_FORMAT.md), measured on real files).
- **Tables:** Canvas, Layer (about 200 columns), Offscreen, Mipmap, VectorObjectList, BrushStyle, BrushPatternImage, Track, TimeLine, LayerComp, CameraInfo, Canvas3DModelBank, RulerPerspective, SpecialRulerManager, SmallObjectInfo, TimeLapseBlob and CanvasPreview (a full-canvas PNG shown on open).
- **The layer tree is a linked list** (`LayerFirstChildIndex`, `LayerNextIndex`).
- **Layer columns** (clip-tools `types.py`; clipparse §2.2 and §6):
  - text: `TextLayerString`, `TextLayerAttributes`;
  - vector: `VectorNormal{Stroke,Fill,Balloon}Index`;
  - placement: `ResizableImageInfo` (a homography from four corners);
  - other: `FilterLayerInfo`, `LayerEffectInfo`, `MonochromeFillInfo`, `SpecialRulerManager`, animation columns, `MaterialContentType`.
- **CSP's own text attributes use tag-length-value records:** font size, outline color, bounding box, skew, box size and quad vertices (clip-tools `text_attributes.py`).
- **Raster caches conflict across sources:**
  - Cropped rasters exist for text and vector layers ([CLIP_TOOLS_REPORT.md](https://github.com/wamsoft/clipparse/blob/main/docs/CLIP_TOOLS_REPORT.md)).
  - Vector layers store no raster ([CLIP_FORMAT.md §7](https://github.com/wamsoft/clipparse/blob/main/docs/CLIP_FORMAT.md)).
  - "Newer versions of CSP drop the pixel data for Vector layers" ([clip_to_psd](https://github.com/dobrokot/clip_to_psd)).

### 2.2 Vector layers: brush strokes as data
- **Per stroke (88-byte header)** (clip-tools `vector.py`):
  - control-point count and bounds;
  - **main color and sub color**, opacity;
  - **brush id**, a row in the per-document `BrushStyle` table (about 69 columns: tip, effector curves, spray, texture);
  - **base size** and **random seed**.
- **Per control point (88 bytes):**
  - x and y (F64);
  - flags (size lock, flow lock);
  - **pressure, velocity, smooth, angle, tilt, size factor, flow factor, opacity factor**.
- **Segment kinds:** STANDARD (a spline), CURVE (a quadratic handle, +16 bytes) or BEZIER (in and out handles, +32 bytes).
- **Editing:**
  - The Control point tool moves, adds and deletes points, switches corners, adjusts **width** and **opacity** per point, and cuts lines ([CSP tips #600](https://tips.clip-studio.com/q/articles/600)).
  - It works on "vector layers, balloon layers, frame border folders, and drawn lines on rulers" ([CSP manual C](https://help.clip-studio.com/en-us/manual_en/810_subtools/C.htm)).
  - Brush shape, size, color and opacity can change after drawing ([tips #600](https://tips.clip-studio.com/q/articles/600)).
- **Why raw input is stored.** Swapping the brush of existing lines only works because raw pressure, tilt and velocity survive and the new brush's dynamics are re-applied. A baked width profile could not do that.
- **Non-destructive erasing.** Transparent vector lines are erase strokes kept as vector data ([CSP manual: Vector layers](https://help.clip-studio.com/en-us/manual_en/180_layers/Vector_layers.htm), via `docs/history/vector-layers-research/02-vector-drawing-apps.md`).
- **No fills.** Fill and Gradient do not work on vector layers.
- **5.0 (Mar 2026)** added Smart Shapes and velocity dynamics, with no storage change ([CSP 5.0](https://www.clipstudio.net/en/news/202603/11_01/)).

### 2.3 Text layers
- **Direction and fitting:** vertical or horizontal; **Auto TateChuYoko** (none to 4 letters) plus manual TCY; wrap at frame; frame alignment.
- **Appearance:** **Edge** (outline) with its color; **background color** with opacity.
- **Transform:** scale and rotate, **skew W/H**, flips.
  ([CSP manual T](https://help.clip-studio.com/en-us/manual_en/810_subtools/T.htm) for all of the above.)
- **Ruby ("reading")** has size, position and spacing ([CSP manual, search](https://help.clip-studio.com/en-us/manual_en/480_text/Vertical_text_and_readings.htm)).
- **Circular text** arrived in 3.0 ([CSP FAQ](https://support.clip-studio.com/en-us/faq/articles/20240015)). No general text-on-path was found.
- **Storage:** the UTF-8 string in `TextLayerString`, run-length style blocks, and font aliases as display/PostScript name pairs. Fonts are not embedded (clip-tools).

### 2.4 Balloon layers
- **Creation.** Drawing a balloon makes a balloon layer, and drawing over text turns the text layer into one.
- **Combining.** "Balloons on the same balloon layer can be combined by placing them on top of each other": overlaps become one outline, a live union.
- **Shapes:** ellipse, curve, balloon pen and materials. **Flash** balloons create a *focus line layer*.
- **Editing.** Tails are dragged out from inside the balloon and edited by clicking inside them. Outlines are edited with control points "the same as with vector layers".
- **Text binding.** When a balloon is resized, "the placement of the text will change, but the font size will not", so text is bound to its balloon.
  ([CSP manual: Balloons](https://help.clip-studio.com/en-us/manual_en/540_comic/Balloons.htm) for the above.)
- **Tails:** Normal or Rounded; bend straight, polyline or spline; thought balloons have their own tails ([CSP tips #863, search](https://tips.clip-studio.com/q/articles/863)).
- **Storage.** The `VectorNormalBalloonIndex` column suggests balloons share the vector infrastructure (inferred).
- **No fit-balloon-to-text** was found (unverified).

### 2.5 Frame border folders
- **Creation.** "A layer mask is also created in the frame border folder."
- **Clipping.** Layers inside show only within the frame; layers above the folder show outside it.
- **Several frames per folder.** Dividing can make a folder per piece, an empty folder per piece, or keep one folder.
- **Draw border off:** "only the ruler is created". The geometry is ruler-like vector data with an optional stroked border.
- **Editing.** The Object tool and Correct line tools reshape frames; brush size and shape set the border.
- **"Keep gutters aligned"** links neighbouring frames' edges.
  ([CSP manual: Frames and Panels](https://help.clip-studio.com/en-us/manual_en/540_comic/Frames_and_Panels.htm) for the above.)
- **Masking per panel.** Divide Frame Folder gives each panel its own folder and mask ([CSP tips #9422](https://tips.clip-studio.com/en-us/articles/9422)).
- **The frame mask cannot be painted.** Artists nest the folder to get an editable mask ([CSP tips 9425, search](https://tips.clip-studio.com/es-es/articles/9425?org=1)).

### 2.6 Focus and speed lines: parametric
- **Their own layers.** A speed lines or focus line layer has a blue **reference line**, a red **shape line** and, for focus lines, a **center point**. All stay editable with the Object tool ([CSP manual: Speed and focus lines](https://help.clip-studio.com/en-us/manual_en/540_comic/Speed_lines_and_Focus_lines.htm)).
- **Parameters**, shared with Flash ([CSP manual D](https://help.clip-studio.com/en-us/manual_en/810_subtools/D.htm)):
  - **Spacing:** gap by angle or distance, disarray %, grouping, group disarray, group gap, maximum line count.
  - **Length:** length, length disarray, and **Extend lines** past the frame ("Without a frame border folder, they extend to the crop marks or canvas edge").
  - **Position:** reference position, gap from it, and uneven reference with count and height.
- **Brush.** Line style is "the same as for brush tools".
- **Implications:** a seeded generator rendered with a brush, whose extent depends on another layer and on page settings.

### 2.7 Tones
- **Tone is a Layer Property effect** ([CSP manual: Layer properties](https://help.clip-studio.com/en-us/manual_en/180_layers/Layer_properties.htm)):
  - frequency in lines per inch;
  - density from color or brightness, or a specified density for fill layers;
  - reflect layer opacity (dot size), posterization;
  - dot shape (circle, square, line, noise with size and factor; about 24 in all ([CSP Screentones, search](https://help.clip-studio.com/en-us/manual_en/540_comic/Screentones.htm)));
  - angle, dot position X/Y.
- **Tone layers** come from a selection ([CSP Tone layers, search](https://help.clip-studio.com/en-us/manual_en/180_layers/Tone_layers.htm)).
- **The canvas** stores a default tone frequency (`DefaultPageToneLine`).
- **Other Layer Property effects:** Border effect (Edge: thickness, color, anti-alias; Watercolor edge), Extract lines, Layer color, Expression color (color, gray or monochrome) and Display decrease color.

### 2.8 Rulers
- **Per layer.** `SpecialRulerManager` rows are keyed by LayerId (clip-tools).
- **Kinds:** perspective (with `RulerVanishPoint`), parallel, emit (radial), curve emit, curve parallel, multi-curve, concentric circle, symmetry and guide.

### 2.9 Other layer kinds
- **Gradient layers** (clip_to_psd):
  - repeat mode: clip, repeat, mirror or empty;
  - shape: linear, circle or ellipse;
  - stops with "use main/sub color" flags and color curves.
- **Fill layers:** `DrawColorEnable` and `DrawColorMain*`.
- **Adjustment layers:** `FilterLayerInfo` kinds 1–9.
- **Also present:** image material layers (homography), file object layers, 3D layers, 2D camera folders, animation cels (`Track`/`TimeLine`).

### 2.10 Comic page settings
- `Canvas` holds double page, binding, crop frame, paper color, default tone frequency, template, and page numbers ("nombre": font, size, position, prefix, suffix) (clip-tools `CanvasRecord`).

---

## 3. Krita
- **File layout.** `.kra` is a zip with `maindoc.xml` [K] and `mergedimage.png` ([Krita docs: KRA](https://docs.krita.org/en/general_concepts/file_formats/file_kra.html)).
- **Node types** ([kis_kra_tags.h](https://github.com/KDE/krita/blob/master/plugins/impex/libkra/kis_kra_tags.h)):
  - layers: paintlayer, grouplayer, shapelayer (SVG content [K]), generatorlayer (`generatorname`/`generatorversion`), clonelayer (`clonefromuuid`), filelayer, adjustmentlayer, referenceimages;
  - masks: filtermask, transparencymask, selectionmask (with vector `.shapeselection`), **transformmask**, **colorizemask**.
- **Layer styles are stored as Photoshop ASL** (`/annotations/layerstyles.asl`).
- **Text, 5.3/6.0** ([release notes, page dated 2026-02-05](https://krita.org/en/release-notes/krita-5-3-release-notes/); [Text tool](https://docs.krita.org/en/reference_manual/tools/text.html)):
  - On-canvas editing with IME, and "full SVG 2 text support".
  - Text types: Preformatted, Pre-positioned (old), Inline Wrapped, **Text in Shape** (a chain of shapes plus subtract shapes, padding and margin), and **Text on Path**.
  - Fonts are tagged resources, including variable fonts.
- **Text properties** ([docker](https://docs.krita.org/en/reference_manual/dockers/text_properties.html)):
  - CSS properties that inherit.
  - Writing Mode (vertical); "Text orientation ... Krita does not current support this feature".
  - OpenType features "inherit as one list"; East-Asian only "enable[s] glyphs meant for ruby annotations". There is no ruby layout and no text-combine-upright.
  - Hanging punctuation, line-break and word-break.
  - Fill and stroke cannot be set from the docker.
- **Comic panels, 5.3.** The Comic Panel Editing tool cuts vector shapes with gutter presets and merges gutters; panels are plain shapes ([doc](https://docs.krita.org/en/_sources/reference_manual/tools/comic_panel_editing_tool.rst.txt), [monthly update 33](https://krita.org/en/posts/2025/monthly-update-33/)). "Gutters are just empty space" ([krita-artists](https://krita-artists.org/t/new-tool-for-comic-panels-knife-tool-asking-for-feedback/116713)). Clipping is manual [K].
- **Fill layers:** Flat color, Gradient, Multigrid, Pattern, **Screentone**, SeExpr, Simplex Noise ([fill layers](https://docs.krita.org/en/reference_manual/layers_and_masks/fill_layers.html)).
- **Screentone generator** ([docs](https://docs.krita.org/en/reference_manual/layers_and_masks/fill_layer_generators/screentone.html)):
  - pattern (dots or lines) with shapes: round, elliptical, diamond, square, straight, sine, triangular, sawtooth, curtain;
  - interpolation, equalization;
  - size in lines per inch/cm (with resolution) or pixels, separate X/Y;
  - a transform with shear, align to pixel grid;
  - foreground and background colors, invert, brightness, contrast.
- **PSD:** 5.3 "can now load and save shapes, vector masks and guides". Text import works; saving text is limited ([release notes](https://krita.org/en/release-notes/krita-5-3-release-notes/)).

## 4. Procreate 5.4.x and Dreams
- **Text** stays vector until rasterized. Selections, transforms, adjustments and merging require **Rasterize**, which is one-way ([handbook: text interface](https://help.procreate.com/procreate/handbook/text/text-interface)).
- **Text styles:** kerning, tracking, leading, baseline, alignment, underline, Outline, Orientation (vertical) ([handbook: edit style](https://help.procreate.com/procreate/handbook/text/text-edit)). 5.4.6 allowed negative slider values ([AlternativeTo](https://alternativeto.net/news/2025/10/procreate-5-4-6-update-text-sliders-enhanced-visibility-and-performance-boost)).
- **QuickShape** snaps strokes to a line, arc, polyline, ellipse, triangle or quadrilateral. Edit Shape offers nodes only right after drawing ([handbook](https://help.procreate.com/procreate/handbook/guides/quickshape)); after that it is paint [K].
- **File:** a zip of `Document.archive` and raster chunks; no stroke data ([silicate](https://github.com/Avarel/silicate)). Text storage was not found (unverified).
- **Dreams** is raster animation [K].

## 5. Adobe Fresco
- **Vector brushes produce filled outlines.** "Vector in Fresco has always created shapes rather than lines" ([Adobe Community](https://community.adobe.com/t5/fresco-discussions/vector-lines-turn-to-filled-compound-paths-from-fresco-to-illustrator/td-p/12534525)).
- **Live brushes are raster.** Editing is mostly transforms, with editable shapes since 2024. PSD export rasterizes (earlier Capy research, `docs/history/vector-layers-research/02-vector-drawing-apps.md` §2).
- **Fit with Capy:** a filled path.

## 6. PaintTool SAI 2
- **Status:** the official page lists a 2026-10-11 Technical Preview with **vertical text** bugs, and no CFF fonts ([SYSTEMAX](https://www.systemax.jp/en/sai/devdept.html)).
- **Layer types** (fan wiki, from preview data; [layer types](https://painttoolsai.readthedocs.io/en/latest/_sources/features/layer-types-and-properties.rst.txt)):
  - Linework layers keep control points and pressure after drawing.
  - Shape layers (Ver.2): rectangle, ellipse, polygon, with fill and stroke.
  - Text layers (Ver.2).
- **Linework tools:** Pen, Curve, Line, Edit, **Pressure** (edit pressure after drawing), **Weight** ([wiki](https://painttoolsai.readthedocs.io/en/latest/_sources/features/linework-tools.rst.txt), [Wikipedia](https://en.wikipedia.org/wiki/PaintTool_SAI)).
- **Curve type:** the wiki says Bézier; SAI's curve tool is generally described as a spline [K]. Unverified.
- **PSD:** shape layers reportedly rasterize ([softwarehow, search](https://softwarehow.com/?p=26475); unverified).

## 7. MediBang
- **Panels.** Panel Materials are objects on a layer ("@X"), edited with the Operation tool. Gutters are set before splitting, line width is in mm, and Rasterize Material ends editing ([MediBang panels](https://medibangpaint.com/en/use/2016/12/how-to-create-panel)). Clipping content to panels is undocumented.
- **Halftone** is an 8-bit layer effect, and type, line count and depth stay editable ([MediBang halftone](https://medibangpaint.com/en/use/2016/08/use-halftone-layers-medibang-paint-iphone/)).
- **Text** stays editable only in MDP files ([MediBang text, search](https://medibangpaint.com/en/use/2023/11/protext)).
- **The newer Paletta app** has Shape, Comic Frame, Text and Materials objects ([text](https://medibangpaint.com/en/medibang-pro/manual/objects/text/), [comic frame](https://medibangpaint.com/en/medibang-pro/manual/objects/comic-frame/), [shape](https://medibangpaint.com/en/medibang-pro/manual/objects/shape/)):
  - text: vertical, edge color and width;
  - shapes: round corners, "Frame Only";
  - frames: stroke width, spacing, divide.
- **No vector-stroke layer was found** (unverified).

## 8. ibisPaint
- **Vector layers** ([lecture 191](https://admin.ibispaint.com/lecture/index.jsp?no=191)):
  - Storage: vertices and paths.
  - Pen type, thickness and color can change later; shapes reorder; Edit Vertices.
  - Shapes convert to polylines or curves.
  - The vector eraser removes by path, the whole line or up to an intersection.
  - **The raster eraser creates "eraser shapes"** that hide lines below them: non-destructive erase items.
  - Rasterizing is irreversible.
- **Frame Divider:** margins, thickness, intervals. The **Radial Line Filter** makes focus lines as a raster filter. Balloons use brushes and the elliptical ruler ([lecture 185](https://admin.ibispaint.com/lecture/index.jsp?no=185)).
- **Text** is vertical or horizontal with stroke ([App Store](https://apps.apple.com/jo/app/id441179131)). Vertical writing fails with fonts that lack vertical metrics (lecture 185).

## 9. Affinity (new here only)
- **Parametric callout shapes:**
  - Callout Ellipse: tail height, tail end and tail angle;
  - Callout Rectangle: tail start and end;
  - Callout Rounded Rectangle: per-corner radii and depth.
  ([Callout Ellipse](https://affinity.help/photo2ipad/en-US.lproj/pages/Tools/tools_calloutEllipse.html), [Callout Rounded Rectangle](https://affinity.help/photo/en-US.lproj/pages/Tools/tools_calloutRoundedRectangle.html).)
- These are the only parametric balloon shapes found outside CSP.

---

## 10. Comic and manga elements across apps

| Element | Editable parameters | Vector | Raster |
|---|---|---|---|
| Speech balloons | CSP (tails, union, bound text); Affinity callouts | CSP outlines; Krita and Photoshop shapes | MediBang and ibisPaint materials and brushes |
| Panel frames | CSP folders (geometry, border brush, gutter links, clipping); MediBang panel objects; Photoshop Frame | Krita shapes (gutters are gaps) | MediBang after Rasterize Material |
| Screentones | CSP tone property; Krita Screentone generator; MediBang halftone | n/a | Photoshop pattern fills or Bitmap halftone [K] |
| Sound effects | Photoshop live warped text | Photoshop Convert to Shape [K]; CSP text converted to a vector layer, then usually rasterized and mesh-warped ([CSP tips #3996](https://tips.clip-studio.com/en-us/articles/3996)) | Most apps |
| Focus and speed lines | CSP (reference and shape lines, center, spacing, disarray, grouping, length, extend-to-frame, brush) | n/a | ibisPaint Radial Line filter |
| Vertical text, ruby, tate-chu-yoko | CSP (vertical, ruby, auto and manual TCY); Photoshop (vertical, TCY [K], Japanese composition); Krita (vertical only); others vertical only | n/a | n/a |
| Bleed, trim, page numbers | CSP canvas | n/a | n/a |

## 11. Interchange
- **CSP → PSD:**
  - Vector layers, tones and frames export as raster ([clip_to_psd](https://github.com/dobrokot/clip_to_psd)).
  - Text stays editable only via Save Duplicate > psd > "Text only" (since 1.12); Save and Save As rasterize text. "Layer image and text" adds a hidden text layer, and balloon plus text become one raster layer ([CSP FAQ](https://support.clip-studio.com/en-us/faq/articles/20220034), [CSP tips #6549](https://tips.clip-studio.com/en-us/articles/6549)).
  - Reopening that PSD in CSP rasterizes the text.
  - The third-party clip_to_psd exports text as editable `TySh`, fills as `SoCo`/`GdFl`, and the Edge effect as a stroke style.
- **CSP → SVG** keeps line shape only ([CSP tips #3714](https://tips.clip-studio.com/en-us/articles/3714)).
- **Krita ↔ PSD (5.3):** shapes, vector masks and guides load and save; text loads, saving is limited. Styles round-trip as ASL.
- **Fresco:** PSD rasterizes; Illustrator receives filled compound paths.
- **SAI 2:** shape layers rasterize (unverified).
- **Procreate → PSD:** text handling unknown.
- **Photoshop:** pixels on every layer let other readers show text and shapes [K].
- **Lesson:** comic objects never survive interchange as editable data. A Capy `.clip` importer is the only route, and it would need CSP's brush-dynamics semantics.

---

## 12. Gaps vs the planned Capy model

Classes: **(a)** structural; **(b)** additive later, with the slot named; **(c)** out of scope. Slot names follow the 726cb6cdc vocabulary.

### Focus questions
1. **Frames and panels: layer-level, yes.**
   - **Why an item can't do it:** CSP folders clip raster, vector, text and balloon layers, and Photoshop vector masks, Frame and artboards are group-level. Item Group clip children can't clip paint layers.
   - **Why plain clipping is not enough:** "clip to the layer below" duplicates the geometry (base below, border on top) and can't hold several panels in one folder.
   - **Why `mask.source` alone is not enough:** it takes the single mask slot, but Photoshop layers carry a raster *and* a vector mask. And with single-use content, a separate border object can't share the geometry.
   - **Classification: (b)**, an occurrence field such as `frame` (a vector object ref) on stacks and objects. Its fills clip the children and its strokes draw above them. One field covers CSP frames, Photoshop group vector masks and artboard clipping; the raster `mask` can coexist.
   - **Gutter links across folders:** a document-level record **(b)**, or bake on import **(c)**.
2. **Balloons: result plus recipe.**
   - A Group holds the union outline and the bound Text child.
   - The recipe holds the body, the tails (kind, bend, base, tip, width, points) and the text child id.
   - The CSP overlap-union is a live boolean whose operands are balloons with their own recipes (needs S2). Affinity callouts map to the same recipe.
   - No new kind is needed.
   **Focus and speed lines:** a group recipe holding reference line, shape line, center, spacing and disarray settings, and a seed. Bake the extend-to-frame extent. Brush rendering triggers S3, otherwise **(b)**.
   **Tones:** layer-level everywhere.
   - Capy's Halftone has angle, contrast, ink, paper and size in pixels (`crates/layer-core/src/package/codec/fixtures/builtin-contracts.json`).
   - It lacks dot shape, lines per inch relative to `resolution`, density source, reflect-opacity, posterization and offset. A Screentone generator is also missing.
   - **(b)** new parameters or new effect/generator IDs; the group `effects` slot covers a tone on items.
3. **Per-point data:**
   - CSP per point: size, flow/opacity, raw pressure, velocity, tilt, angle, locks. Per stroke: main/sub color, base size, brush id, seed. SAI: editable pressure.
   - **(b)** a stroke `opacity_profile` (same knots as width; required).
   - **(b)** Brush path kind with a multi-channel per-anchor table: raw inputs separate from user factors, plus main/sub color and seed. Raw pressure is what lets CSP swap brushes on existing lines.
   - **(c)** per-point color: no app stores it.
4. **Photoshop booleans:**
   - Use the live-boolean group recipe with one operand item per component **(b)**. `vogk` origination is per component, so components must be items anyway.
   - PSD stores no result, so import needs a cubic boolean engine for subtract and intersect; combine and exclude use orientation plus the fill rule.
   - The per-subpath fill-rule flag (if real) becomes the operand path's own rule; the path-level fill rule fits, and an earlier draft's `Fill{paint, rule}` would not.
   - "Fill starts with all pixels" becomes a recipe flag, or the mask `inverted`.

### (a) Structural: none; settle three things before freeze
- **S1. Unknown anchor kinds.** These are `u8` values in the required geometry property, so an auto-smooth anchor would make the file Preserved. Decide now that unknown anchor kinds render and edit as Corner, since handles are explicit. Unknown segment kinds stay unsupported.
- **S2. Recipe values that embed items.** Operands of booleans, balloon unions and warped text are items with their own recipes. Define one rule: a recipe value may hold an item stream in the same codec, with End markers and the ignorable bits applying recursively.
- **S3. Recipes that need a slot are required data.** This covers focus lines with a brush and texture or pattern generators, so comic pages would be Preserved in older versions. Option: an ignorable recipe may use only slots its own result items use.

### (b) Additive later

| Gap | Apps | Slot |
|---|---|---|
| Frame clipping a stack, border drawn above | CSP, Photoshop group vector masks, Frame tool, artboards | Occurrence field `frame`, separate from `mask` |
| Gutter links across folders | CSP | Document record, or bake |
| Per-subpath live booleans | Photoshop, CSP balloon union | Group recipe with operand items (S2); importer boolean engine |
| Live rect, rounded rect, ellipse, line with arrowheads, polygon, star, triangle | Photoshop vogk (box corners), Affinity, MediBang Paletta | Path recipe carrying its own affine |
| Balloons | CSP, Affinity | Group recipe; Text child |
| Flash, focus and speed lines | CSP | Group recipe, seed, baked extent (S3) |
| Per-point opacity | CSP | Stroke `opacity_profile` |
| Brush raw channels, sub color, shared brush | CSP, SAI | Brush path body; brush in the slot list |
| Erase strokes in an object | CSP transparent lines, ibisPaint eraser shapes | New blend name "erase" on path or group; isolated objects only |
| Stroke opacity and blend | Photoshop vstk | Paint-entry properties (planned) |
| Stroke align, dash, non-scaling | Photoshop vstk | Stroke properties (planned) |
| Gradients: conic, diamond, reflected, repeat/mirror/empty spread, ellipse ratio, interpolation method, midpoints | Photoshop, CSP | Gradient paint kinds and properties |
| Pattern paint and pattern fill generator | Photoshop PtFl, Krita | Pattern paint (planned); generator ID |
| Screentone generator; extra Halftone parameters | Krita, CSP, MediBang | Effect IDs and parameters |
| Layer styles, especially stroke/edge | Photoshop lfx2, CSP Border effect, Krita ASL | Effect IDs |
| Fill opacity | Photoshop iOpa | Occurrence `fill_opacity`, once layer styles exist |
| Ruby | CSP | Text annotation table `(range, string, size, position, spacing)` |
| TCY, kinsoku, mojikumi, tsume, burasagari, hanging punctuation | CSP, Photoshop, Krita | Text run and paragraph attributes |
| Text background box and edge | CSP, Procreate, MediBang | Text style list; frame property |
| Live warped text | Photoshop | Envelope group recipe (S2) |
| Text in several shapes with subtract shapes; text on a separate path; circular text | Krita, Photoshop, CSP 3.0 | Required layout properties; circle recipe |
| Several text frames per layer | CSP, ibisPaint | Several Text items |
| Fonts by name | Photoshop, CSP | Name-only form of the `fonts` slot (embedding raises font-license limits) |
| Projective and mesh image placement | Photoshop SoLd, CSP ResizableImageInfo | Required image property |
| Nested documents, shared instances | Photoshop smart objects, Krita clones | Many-use definition plus Instance (planned) |
| Layer comps | Photoshop 1065, CSP LayerComp | Document record |
| Page settings: trim, bleed, safe area, binding, double page, page numbers, tone frequency | CSP Canvas | Document record |
| Rulers attached to layers | CSP SpecialRulerManager | Extend `capy.guides/1`; occurrence attachment |
| Per-occurrence raster fallback | Photoshop layer pixels, CSP cropped offscreens | Optional ancillary record; risk of stale caches |
| Vector selections | Krita | Existing `capy.selection/1` contours |

### (c) Out of scope
- **Per-point color:** no app stores it.
- **Tool behaviors with no stored data:** magnet, erase-to-intersection, connect, simplify, redraw.
- **Kept out by policy:** layer transforms (Krita transform masks; bake them) and linked files (Photoshop lnk2/lnk3, Krita file layers, CSP file objects; embed them).
- **Animation:** CSP Track/TimeLine, Procreate Dreams.
- **Other layer kinds:** 3D layers, colorize masks, Photoshop generative layers (GenI).
- **Advanced blending:** knockout, blend clipped as group, blend interior, channel restrictions.
- **Raster brush engines:** Fresco live brushes.
- **CSP gradient stop curves:** bake them.
- **CSP Expression color:** already covered by Capy's `color_mode`.

### Not verified
- Photoshop: how unknown blocks are saved, whether strokes follow the boolean result, the per-subpath fill-rule flag, polygon/star/triangle vogk keys, ruby, and the frame PSD encoding.
- SAI 2: curve type and how shape layers save to PSD.
- Procreate: text storage and PSD export of text.
- MediBang: whether vector layers exist.
- ibisPaint: how frames clip, and vector pressure.
- CSP: whether focus lines regenerate when the frame moves, and whether a balloon can fit its text.
