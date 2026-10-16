# Object content extensions: format audits and later additions

[Technical documentation](../README.md) · [Design and validation history](README.md)

First audit: **2026-10-15**, source baseline **`37fabf996`**. Reader fixes and
model revision: **2026-10-16**, source baseline **`86a0fd0e4`**. Second audit and
framing revision: **2026-10-16**, source baseline **`726cb6cdc`**.

This record checks whether the `.capy` format and the object-layer model can
grow without a redesign. The target is full SVG editing, plus the shapes, text
and comic elements that artists put in illustrations, manga and designs. It also
maps each later feature onto the slot that would hold it. It describes intended
work, not implemented capabilities. The decisions it produced are in the
[vector-layer design](vector-layers-research.md#72-layer-model):
- one item tree;
- framed, tagged records with PNG-style rules for unknown data;
- ordered paint lists;
- transforms on groups and non-path items;
- results with recipes for live features.

Two audits combined eleven research streams. Their write-ups, with every source URL, are in [`object-content-extensions/`](object-content-extensions):

| # | Stream | File |
|---|---|---|
| 1 | The `.capy` package and authored model, read in code | [01-capy-format-audit.md](object-content-extensions/01-capy-format-audit.md) |
| 2 | SVG 2 and its companion specs: features, model constraints, usvg | [02-svg-data-model.md](object-content-extensions/02-svg-data-model.md) |
| 3 | Inkscape 1.4.4 and unreleased 1.5: storage, live path effects, round trip | [03-inkscape-data-model.md](object-content-extensions/03-inkscape-data-model.md) |
| 4 | Illustrator 30.8: object model, Appearance, SVG writers | [04-illustrator-object-model.md](object-content-extensions/04-illustrator-object-model.md) |
| 5 | Krita, Affinity, Figma, Graphite, PDF and Clip Studio object models; Rust crates | [05-reference-object-models.md](object-content-extensions/05-reference-object-models.md) |
| 6 | Affinity 3.3, CorelDRAW and Xara: object models, fallbacks, Xara's record classes | [06-affinity-coreldraw-xara.md](object-content-extensions/06-affinity-coreldraw-xara.md) |
| 7 | Figma, Sketch, Penpot, Lunacy, Linearity Curve and Graphite: node, paint, mask and instance models | [07-figma-sketch-penpot.md](object-content-extensions/07-figma-sketch-penpot.md) |
| 8 | Photoshop, Clip Studio Paint, Krita, Procreate, Fresco, SAI, MediBang and ibisPaint: shapes, text, balloons, panels, tones | [08-painting-app-content.md](object-content-extensions/08-painting-app-content.md) |
| 9 | Text objects, fonts and the Rust text stack | [09-text-and-fonts.md](object-content-extensions/09-text-and-fonts.md) |
| 10 | DrawingML and ODF shapes, whiteboards, Lottie, After Effects, Rive, DXF, cutters and PDF | [10-diagram-motion-cad-shapes.md](object-content-extensions/10-diagram-motion-cad-shapes.md) |
| 11 | How long-lived formats stay extensible: PNG, glTF, OOXML, PDF, Xara, protobuf, Rive and others | [11-format-extensibility.md](object-content-extensions/11-format-extensibility.md) |

**Caveats on the evidence**
- **Adobe help pages** refuse automated clients. Illustrator and Photoshop facts therefore come from:
  - Wayback snapshots;
  - SDK headers mirrored on GitHub;
  - the published PSD specification;
  - MIT-licensed PSD parsers;
  - exported files found by code search.
  Adobe documents almost none of its SVG import; those claims are forum reports.
- **Unreleased or unofficial sources:**
  - Inkscape 1.5 is unreleased; its facts come from master at `f093808f` (2026-10-15).
  - Affinity's object model comes from its beta 3.3 scripting SDK and a reverse-engineered `.af` reader.
  - Figma's `.fig` details and Clip Studio's `.clip` layout come from community parsers.
- **Unreachable sources:** Xara's format specification PDF was unreachable, so its rules come from the Xara LX source. CorelDRAW's API pages refuse automated clients. Keynote's format has no public specification and was skipped.
- **Measurements and support data:**
  - Browser support comes from MDN browser-compat-data 8.1.6.
  - Font counts and wasm sizes in stream 9 were measured on a Fedora 44 workstation.
- **The code audits** read the code and ran its tests; they built no vector feature.

---

## 1. Verdict

- **The package can hold this content.** Typed records, binary resources with local slots, references, the Preserved rule and ancillary records are all the machinery it needs. Nothing in the codec blocks groups, transforms, gradients, text, symbols or comic elements.
- **The first vector design could not have grown that far.** It had:
  - a flat path list;
  - no transforms anywhere;
  - one optional fill and stroke per style;
  - a fixed binary layout;
  - a style table in the manifest's JSON;
  - nowhere to keep editing intent.

  The first audit fixed all six before any vector record shipped (section 2.3).
- **The second audit found no need for a redesign, but the record framing needed fixing before it froze** (section 2.4):
  - a reader that did not know a later container kind would have called the file damaged;
  - unknown data could not name items or slots;
  - recipes had nowhere to keep their inputs;
  - the width profile was one-sided.
- **The reader called some well-formed future files damaged.** That contradicted the package contract's own rule. Eight cases are fixed (section 2.2).
- **Whole-document Preserved stays.** That is glTF's required-extension model and OOXML's MustUnderstand. To keep older versions editable, a later feature stores its result with kinds that already shipped plus a recipe, wherever its result can be drawn that way.
- **SVG and the other editors' formats remain interchange; `.capy` is the master file.**
  - Illustrator keeps editability in SVG only by embedding a private copy of the `.ai` file (`<i:pgf>`), and ignores the SVG markup when it reopens the file.
  - Inkscape's Plain SVG strips its own editing data.
  - Comic content never survives interchange between painting apps as editable data.
  - The [GA design](object-layer-ga-design.md#12-long-term-end-state-and-extension-stress-tests) already says "lossless reimport of native Capy editing through arbitrary SVG editors is not promised".

## 2. The format today

### 2.1 What already holds

| Need | Existing mechanism |
|---|---|
| Newer files in older readers | Unknown record types, fields, values, resource types and encodings open the whole document as Preserved: read-only, with its preview and a byte-exact Copy Original (`unknown_drawable_children_preserve_the_complete_package`) |
| Bulk geometry | Typed binary resources with a raw layout encoding, optional `capy.lz4-bytes/1` wrapping and decoded size bounded before allocation, as `capy.lut3d/1` does |
| References from binary data | Payloads carry local slot indices; the record's JSON lists the `{"ref"}` targets |
| Shared images | `capy.image/1` is immutable and may be used many times |
| Transforms | `Affine64` with validation already exists for image objects |
| Data other apps own | Ancillary, copy-safe records with opaque resources survive edits while their references resolve |
| Blending | 25 blend names, including all 16 CSS modes, plus pass-through groups |
| Color | F32 straight colors in sRGB, Display P3, Adobe RGB or ProPhoto, with values outside the gamut allowed; CSS Color 4 colors convert into them |
| Artboards later | Content outside the canvas is kept, and several outputs are already structurally valid (preserved today) |

Admission limits shape the rest of the design: 65,536 records, 262,144 evaluation
edges and resources, evaluation depth 128, 4,096 occurrences, and manifest JSON
of at most 64 MiB, 4,194,304 nodes and 127 nesting levels. Bulk geometry and deep
trees must therefore live in binary resources.

### 2.2 Reader classifications fixed

The [package contract](../reference/capy-package.md#object-records-and-defaults)
reserves Invalid for data that can never become valid. It calls a well-formed
combination that a later version may allow unsupported. The reader broke that
rule in eight places, so an older reader would have reported a newer file as
damaged:

| Case | Where | Now | Later use it blocked |
|---|---|---|---|
| Pass-through blend on content other than a stack | `package/artwork_records.rs` | Unsupported | Non-isolated vector layers, whose items blend with the layers below |
| Alpha lock or an offset on content they do not apply to | `package/artwork_records.rs` | Unsupported | Alpha lock or offsets on new content kinds |
| A known record type in a slot that does not accept it | `authored/topology.rs`, `package/artwork_records.rs` | Unsupported | A vector mask in the existing mask slot, an image used directly as an object |
| Outputs naming a known record that is not an output | `authored/topology.rs`, `package/artwork_records.rs` | Unsupported | Artboards and other output kinds |
| A known resource type in another binding | `package/resources.rs` | Unsupported | New uses of existing resource kinds |
| An attachment without an eligible target | `authored/scene.rs` rules, reached through `package/artwork_records.rs` | Unsupported | Clipping to or attaching effects to new content kinds |
| An ancillary record naming another ancillary record | `package/manifest.rs`, `authored/topology.rs` | Accepted; the referring record is never retained after an edit | Threads of notes, or annotations on another app's data |
| An unknown type marked copy-safe without being ancillary | `package/manifest.rs` | Unknown required artwork, so the package is preserved | A required record that may be cloned with its subject |

- **Tests:** `well_formed_combinations_a_later_version_may_allow_preserve_the_package` and the manifest, topology and extensions tests cover each case. They also check that malformed values stay invalid.
- **Edits** still cannot create these combinations.
- **The contract** now names them and adds four rules:
  - later versions extend the format with new names, never by giving an existing field a second meaning;
  - a known type's flags must match its registration;
  - copy-safe records may depend only on the identity of what they reference;
  - the envelope stays closed.

### 2.3 What the first vector design got wrong

| Gap | Why it mattered | Now in the [design](vector-layers-research.md#74-file-format) |
|---|---|---|
| A flat path list | Every SVG, Inkscape and Illustrator file is a tree. Adding groups later means two nesting mechanisms with different selection, undo and compositing rules | One tree. The vector object is the root group, and the record stream is in pre-order with End markers |
| No transform anywhere | Text, images and instances cannot absorb a transform. A stroke under a skew or uneven scale is stretched, which one width cannot describe | Paths still absorb transforms and scale strokes by √\|det\|. Groups, images, text and instances carry an F64 affine. Import keeps stretched strokes in a group |
| One optional fill and stroke | Illustrator, Affinity, Figma and Graphite use ordered paint lists; SVG's `paint-order` is the two-entry case | A style is an ordered list of fills and strokes, each with exactly one paint |
| A fixed binary item layout | Adding names, visibility or opacity later would force a second layout that readers decode forever | Records and properties are framed and tagged (section 2.4) |
| The style table in the manifest JSON | One style with a 10-stop gradient is about 100 JSON nodes, so about 40,000 such styles would fill the whole document's 4,194,304-node budget | Styles are inline in the binary records and interned in memory |
| Nowhere for editing intent | Live shapes, Inkscape path effects, Illustrator blends, Repeat and live booleans all need their settings kept | Results are ordinary records, and recipes are ignorable, not copy-safe properties |

The first audit proposed keeping recipes in separate ancillary records keyed by
item ID, with a fingerprint of the result. In-stream properties are better, as
stream 11 showed. Older readers cannot see item IDs inside JSON, so they would
keep such a record when its item changes or is deleted, and drop it on
duplicate. A property travels with its item.

### 2.4 What the second audit changed

| Gap | Evidence | Now in the design |
|---|---|---|
| A reader that did not know a container kind would misread its children and report an unbalanced End as damage | Xara writes explicit down and up records; Rive substitutes null objects; Excalidraw silently drops unknown element types | Every kind word has a parent bit, so End balance is checked from framing alone. Unknown parents are atomic |
| Unknown data could not name items or slots, so connectors, text on a path, embedded fonts and brush-drawn focus lines would all have been required data | TIFF MakerNotes break on rewrite because they hold offsets; Cap'n Proto separates pointers from data; the package already finds `{"ref"}` inside unknown JSON | Every tag word carries a value class: data, item references, slot references or nested records. Readers can keep, renumber and remap what they do not understand |
| Recipe inputs had nowhere to live | Affinity compounds and warp groups, Illustrator edit art, Clip Studio balloon unions and Photoshop's per-subpath booleans all keep source items beside the result | Recipe values may hold nested records with their own recipes |
| Recipes relied on every reader dropping them | PNG's specification says editors "usually discard all unrecognized information"; PDF tools ship stale appearance streams | A recipe carries a fingerprint of its dependency content, which the reader that knows it checks on load |
| "Changed" was undefined, so renaming an item could have dropped its recipe | Stream 11 | Dependency content is the required properties of the owner, its subtree and the items it names |
| One-sided widths would need a second profile layout | CorelDRAW variable outlines set each side; Illustrator has one-sided widths | Width knots carry a left and a right factor |
| A new anchor kind would make files read-only | Affinity's automatic Smart nodes; Clip Studio and SAI spline points | Readers edit unknown anchor kinds as Corner |
| No hidden, name or locked | Every editor surveyed; imports need them before an object list exists | Path tags from V0; hidden is required, name and locked are ignorable |
| Unknown-data rules that no shipped data uses rot | OOXML removed its unimplemented preserve rules; RFC 8701 and 9170 describe extension points that "rust shut" | Figure shapes store live-shape recipes from V1, so the first release uses all three handling classes |
| The space strokes are measured in under a group transform was undefined | Lottie strokes follow their style's transform; Rive lets each stroke choose; SVG transforms the outline | Fixed now as SVG's rule; a non-scaling stroke is the later opt-out |
| Encodings drift without a canonical form or a registry | Protobuf `reserved`; glTF's archived extensions | Canonical bytes, a checked-in append-only number registry and an experimental range that portable save refuses |
| A per-segment kind byte was reserved for exact arcs | Lottie and Rive are cubic-only; Office presets' 393 `arcTo` commands convert within tolerance | Exact arcs, circles and splines become recipes over cubic results, so older readers keep editing them |

### 2.5 Constraints kept by policy

- **No layer transform.** Occurrences keep integer offsets; `placement` stays removed. Transforms live inside object types.
- **Editable content is single-use.**
  - Document-wide symbols, swatches and styles need a new many-use editable record type with update propagation and cycle checks ([GA design §12.1](object-layer-ga-design.md#121-rules-that-keep-extensions-within-one-model)).
  - Until then, definitions are local to one vector object.
- **No Capy IDs in binary payloads.** Every image, font, definition, effect or other vector object an item uses is a slot in the record's JSON.
- **Preserved is whole-document.** A file that uses one unknown required feature is read-only in an older version. Open question 11 in the vector design asks whether items with stored results, such as text, could lock instead.

## 3. SVG 2 in 2026

**Status.**
- SVG 2 had a new Candidate Recommendation Snapshot on 2026-10-06. At risk: several `title`/`desc` elements, nested links, and `vector-effect` values other than `non-scaling-stroke`.
- **Removed:**
  - `miter-clip` and `arcs` joins (svgwg #1065, 2026-03);
  - text wrapping into shapes, `shape-inside` and `shape-subtract` (WG resolution 2026-04-23). `inline-size` stays, at risk;
  - multiple paints in `fill` and `stroke`, `solidcolor`, `tref` and SVG fonts.
- **Deferred:** mesh gradients and hatches.
- **Drafts with no implementation found:** Fill and Stroke 3 (stroke alignment, layered paints) and SVG Strokes. Compositing 2 adds `plus-lighter` to `mix-blend-mode` only.

**Rules that force structure, and how the design meets them**

| SVG rule | Consequence |
|---|---|
| Groups are non-isolated by default and isolate with opacity, filters, masks or blending; effects apply filter → clip → mask → opacity | Groups need explicit isolation and fixed effect slots in that order. Vector layers need pass-through occurrences for items to blend with layers below |
| Paint servers, markers, clips and masks inherit properties from their own ancestors, not from the referencing element | Import resolves inheritance; Capy stores specified values on each item, never a cascade |
| Gradients, patterns and mask and filter regions default to object-bounding-box units; a box with zero width or height disables the effect | Import converts bounding-box units to user space per use, as usvg does, and copies shared servers per element |
| Gradient and pattern geometry is part of the element's geometry | Paint geometry transforms with its path when paths absorb a transform |
| `clipPath` children may only be shapes, text or `use`; only geometry counts | A clip child holds paths and text; its paints are ignored |
| Masks use luminance coefficients 0.2125, 0.7154, 0.0721 in sRGB by default | The mask mode and its color space are part of the mask's evaluation contract |
| Filters work in the element's local space; `color-interpolation-filters` defaults to linearRGB | An SVG filter graph is its own effect type with its own color rule |
| Markers are instanced per vertex, oriented by tangent bisectors and scaled by stroke width | Markers are references from a stroke to definitions, resolved at render time |
| `non-scaling-stroke` computes the outline in screen space | A stroke property whose geometry depends on the view; cached outlines go stale on zoom |
| Strokes under non-uniform transforms are anisotropic | Paths absorbing transforms must scale strokes, or stay in a transformed group |
| Compositing happens in gamma-encoded sRGB | Faithful SVG needs a Perceptual sRGB document (section 6) |
| Gradient stops interpolate non-premultiplied (svgwg #180) | Gradient paints need an unpremultiplied interpolation option; Capy's gradients are premultiplied today (`gradient.rs`) |
| Text positioning lists index code points | Text storage needs a defined offset unit (section 4.10) |

usvg 0.48.1 is a good minimal render model but loses what editing needs:
- `use` and symbols become copies;
- units and bounding-box sharing disappear;
- colors are 8-bit sRGB;
- `non-scaling-stroke` is ignored.

Capy can use it as an importer and test oracle, not as its stored model.

## 4. Later additions by slot

Each row is new vocabulary inside `capy.vector-object/1` and
`capy.vector-items/1`, or a new record or layer field:
- vocabulary is a kind, tag, value or slot list;
- older readers preserve the file, or ignore the addition when it is flagged ignorable.

### 4.1 Item kinds

| Kind | Holds | Brings |
|---|---|---|
| Group | A parent record with properties, then children, then End | SVG `g`, groups and clip groups in every editor, Affinity nesting, CorelDRAW PowerClip, Xara ClipView |
| Image | An `images` slot to `capy.image/1`, an affine, interpolation, later a crop and projective or mesh placement | SVG `image`, Illustrator embedded raster, Photoshop smart-object placement, Clip Studio image materials. Linked files are out of scope; imports embed |
| Text | A required leaf kind with story, layout, font descriptors and a stored result (4.10) | SVG `text`, Illustrator text frames, Photoshop and Clip Studio text layers, Krita SVG 2 text |
| Instance | A definition, an affine, overrides keyed by paths of item IDs | SVG `use`/`symbol`, Inkscape clones, Illustrator and Affinity symbols, Figma and Sketch components |
| Definitions | A non-rendering parent of item subtrees that instances, patterns and markers use | SVG `defs`, `symbol`, `marker`, `pattern` content |
| Brush path | A path with a brush slot, seed and per-anchor input channels (raw pressure, tilt, velocity), rendered by the stamp engine | Textured vector strokes ([vector design §7.9](vector-layers-research.md#79-foundation-for-later-work)); Clip Studio vector lines, which keep raw pressure so a new brush can be applied later |
| Backdrop effect | An effect that reads the content below it inside a non-isolated group | CorelDRAW lenses, Affinity adjustment and live-filter children |
| Placed document | A document resource with a cached raster for older readers | Affinity embedded documents, Photoshop smart objects holding a layered file |

### 4.2 Group properties

- **Compositing:** opacity, blend mode, isolation (isolated or pass-through), fill opacity, Illustrator's knockout (off, on, inherit) and alpha-is-shape.
- **Clipping:** a clip child, a container of geometry whose paints are ignored.
  - It covers SVG `clipPath`, Illustrator clipping masks, Affinity clip-by-nesting, CorelDRAW PowerClip, Penpot masked groups and Figma vector masks.
  - Sibling masks in Figma, Sketch and Lunacy import by wrapping each masked run in a pass-through group that owns the mask.
- **Masks:** a mask child with a mode (luminance or alpha) and invert and clip flags. It covers SVG `mask`, Illustrator opacity masks, PDF soft masks and Lottie mattes.
- **Effects:**
  - an `effects` slot list naming `capy.effect/2` applications, so built-ins apply to items;
  - an SVG filter-graph effect type with the 17 primitives and its own color rule;
  - new built-ins: drop and inner shadow with spread, glows, outline, bevel, and backdrop, progressive and glass blurs. Capy has none of these, even as layer effects.
- **Group paints:** Illustrator's fills and strokes on a group, with a "contents" entry marking where the children render.
- **Clip to the content below** per item, as Graphite and Photoshop do.
- **Rules to fix when Group ships**, because changing them later changes how files look:
  - a clip child's region is the union of its items' fill geometry, as in SVG. Figma's vector masks, which include strokes, import by outlining the strokes;
  - clip and mask children accept any kind, Text and Group included, as Affinity and CorelDRAW require;
  - a mask's mode is always stored, never defaulted;
  - a pass-through group with a mask behaves as a pass-through layer group with a mask does today;
  - item effects need a defined space. Office rotates shadows with the shape and Illustrator does not, so the choice is made with the first item effect.

### 4.3 Paint kinds and paint-entry properties

| Addition | Detail |
|---|---|
| Linear and radial gradients | Geometry in path space (end points; center, radius, focal point and `fr`), spread (pad, reflect, repeat) and an affine. Stops reuse Capy's gradient values, extended with more than 32 stops, midpoints, an unpremultiplied interpolation option for SVG, and hue-direction and stepped interpolation |
| Other gradient kinds | Conic or angular, diamond, rectangular, elliptical, reflected, 3- and 4-color and diffusion gradients (Figma, Affinity, CorelDRAW, Photoshop, Xara) |
| Image paint | Fill, fit, crop or tile with an affine, as Figma, Sketch, DrawingML and Penpot fill shapes with images |
| Pattern | A definition plus an affine; square, brick and hex tilings and overlap go in the definition |
| Hatch | Families of lines with angle, spacing, offset and dashes (Inkscape hatches, DXF `HATCH`, Excalidraw hachure) |
| Mesh and freeform gradients | A patch mesh (PDF shading types 6 and 7; Inkscape, Illustrator, Affinity and CorelDRAW meshes) and triangle or point freeform gradients (PDF type 4, Illustrator) |
| Noise | Seeded noise on colors and stops (Affinity) |
| Swatch reference | A document-level swatch with an ordered modifier list (DrawingML `lumMod`, tints) and the resolved color stored as its result |
| Spot color | An ink name and tint, with the alternate color stored as its result |
| Context paint | SVG `context-fill` and `context-stroke`, for markers and instances |
| Per-entry opacity, blend and visibility | Illustrator, Affinity, Figma, Sketch, Lunacy, Photoshop strokes, After Effects and Rive paints |
| Per-entry alpha paint | A transparency gradient on one fill or stroke (Affinity, CorelDRAW, Xara) |
| Per-entry effects | Illustrator's effects on one fill or stroke |
| Overprint | An ignorable flag that matters only for print separations |

### 4.4 Stroke properties

- **Dashes:**
  - a dash array and offset in absolute units or multiples of the width, as Affinity and Office use;
  - adjustment that aligns dashes to corners and path ends (Illustrator, Affinity).
- **Alignment:** inside and outside, defined as Figma and Penpot do: twice the width, clipped to the fill. SVG export outlines it.
- **Markers:** separate start and end caps or markers with an open style list (arrows, circles, squares, crow's feet), scale and tip placement. SVG `marker` references with SVG's orientation rules.
- **Non-scaling stroke:** includes Inkscape's hairline and the 0.001-inch hairline that laser cutters read as a cut.
- **Pen transform or calligraphic nib:** a 2×2 pen matrix, or a nib shape with roundness and angle (CorelDRAW nib, Linearity brushes, Graphite's stroke transform). It describes imported anisotropic strokes without a group.
- **Opacity profile:** the same knots as the width profile, for Clip Studio's per-point opacity.
- **Brushes:** art, scatter and pattern brushes along a path (Illustrator, Figma stretch and scatter brushes, Affinity texture lines, CorelDRAW Artistic Media), as a brush reference with a result.
- **Gradient on a stroke:** within, along or across the stroke (Illustrator).
- **Scale with object:** an editing hint for strokes, effects and corners (Affinity, Clip Studio).

### 4.5 Path and item properties

- **Identity and metadata:** notes, tags, alt text, links and a free metadata bag (DrawingML `cNvPr`, PDF `Alt`, Excalidraw `link`/`customData`, tldraw `meta`). Also SVG `id` and `class`. All are ignorable and copy-safe.
- **Per-anchor arrays** in their own tags, indexed in geometry order:
  - corner radius (Figma, Sketch, Rive, Lunacy);
  - pressure and other raw inputs (Excalidraw, tldraw, Clip Studio, SAI);
  - DXF start and end widths.
- **Opacity and blend on a single item:** an item behaves like a group of its own paint entries.
- **Blend domain per object:** an object-local perceptual option, for SVG-faithful blending inside Linear documents.
- **Export and print visibility:** Affinity's `isVisibleInExport`, PDF optional content and Cricut guide layers.
- **Machine operation:** cut, score, engrave or draw, so cutting files need no color conventions (Cricut operations, laser color maps).
- **Text wrap:** an obstacle property `wrap {mode, offset}` (Illustrator, InDesign, Affinity, CorelDRAW), required because it moves text in other items.
- **Transform components:** anchor, position, rotation, scale and skew as the recipe for a group's affine (Lottie, After Effects, Rive). Animation would need these.

### 4.6 Live features stored as results with recipes

The result is ordinary records. The recipe is an ignorable, not copy-safe
property on the record that owns it. Inputs that must not render are nested
records inside the recipe. Recipes name other items and slots through the
Items and Slots value classes.

| Feature | Recipe on | Sources |
|---|---|---|
| Live shapes: rectangle with per-corner radii, ellipse, pie and arc, polygon, star, spiral, triangle, line with arrowheads | The path, with its frame | Illustrator Live Shapes, Photoshop `vogk` (whose `keyShapeInvalidated` is the same drop rule), Affinity's 25 shapes, Inkscape `sodipodi:type`, Krita, Figma `arcData`, Sketch, Penpot |
| Office and ODF presets with adjust handles, text rectangle and connection sites | A group of paths, with the preset name, adjust values and box | DrawingML's 187 presets, 63 of which draw several paths; ODF enhanced geometry |
| Live corners: radius, type (round, inverted, chamfer) per anchor | The path | Illustrator Live Corners, Affinity corners, Sketch corner styles, Inkscape Corners effect |
| Exact geometry: arcs, circles, ellipses, splines, bulge polylines | The path | DXF `ARC`, `CIRCLE`, `SPLINE`, `LWPOLYLINE`; Office `arcTo`; tldraw arc arrows |
| Path effects: taper, offset, roughen, simplify, bend, envelope, lattice, pattern along a path, dashes as geometry, trim, pucker and bloat, twist, zig zag | The path, holding the source geometry | Inkscape live path effects (`inkscape:original-d` plus `d`), Lottie and After Effects modifiers, Figma dynamic strokes |
| Sketchy rendering | The path, with generator version, seed and parameters | rough.js and Excalidraw |
| Freehand samples with pressure | The path, holding the raw samples; the result is the fitted path and profile | Excalidraw `freedraw`, tldraw draw segments |
| Live booleans, including Photoshop's per-subpath operations | A group holding the result, with the operands and modes in the recipe | Illustrator compound shapes, Affinity compounds (live by default in 3.3), Figma and Sketch boolean groups, Photoshop shape layers, Clip Studio balloon unions |
| Blends, contours, Repeat (linear, radial, grid, mirror), Objects on Path | A group of results, with key objects, spine and settings in the recipe | Illustrator and Affinity blends, CorelDRAW blends and contours, Figma Repeat, Inkscape Mirror and Rotate Copies, DXF `INSERT` arrays |
| Envelopes, warps and moulds over paths, text and images | A group, with the mesh or warp; image results are stored rasters | Illustrator Envelope Distort, Affinity Warp groups, Xara moulds, Photoshop warped text and smart objects |
| Live Paint and vector region fill | A group of face fills and edge strokes, with the source paths | Illustrator Live Paint, the vector region fill planned in the vector design |
| Connectors | The path, with its end items, anchors (a site key or a point normalized to the target's frame, never a segment parameter) and routing kind | DrawingML `cxnSp`, ODF, Inkscape, Excalidraw, tldraw, Figma, Google Slides |
| Speech balloons | A group of the outline and its Text, with the body shape, tails (kind, bend, base, tip) and text binding | Clip Studio balloon layers, Affinity callout shapes |
| Focus and speed lines | A group of strokes, with the reference and shape lines, center, spacing, disarray, seed and a baked extent | Clip Studio focus, speed and flash lines |
| Charts, tables, QR codes, dimension lines and callouts | A group of paths and text, with the data | Illustrator graphs, Office tables and charts, CorelDRAW QR codes, DXF `DIMENSION` (whose stored block AutoCAD requires) |
| Text converted to outlines | A group of paths, with the text in the recipe | Illustrator Create Outlines, Inkscape Object to Path |
| SVG text imported before Text exists | A group of paths, with the original `<text>` fragment as UTF-8 | Keeps the import from freezing an early text model |

Width profiles are native, not recipes: Inkscape's Power Stroke and Illustrator's
Width tool map onto the stroke's profile.

### 4.7 Instances and definitions

- **Local first:** definitions inside one vector object, used by instances, patterns and markers.
- **Overrides:**
  - Each override is `(path of item IDs, tag, value)`, per property rather than per item. The path runs from the instance's definition down through nested instances, as Figma's `guidPath` and Sketch's `uuid/uuid_property` do.
  - The outermost instance wins.
  - An override whose path no longer reaches a target is inert.
  - Krita turns `use` into copies and loses the link; Capy should not.
- **Later additions:**
  - instance swap;
  - component properties and variants;
  - slots that add children to one instance (Figma slots, Affinity's per-instance children);
  - 9-slice scaling and the registration point (Illustrator);
  - DXF per-copy attributes.
- **Materialized results:** an instance whose definition is in the same object can also store its resolved children as a result plus an instance recipe, as Penpot keeps full copies. Older readers then draw and edit it as a group.
- **Shared later:** document-wide symbols need the many-use editable definition type (section 2.5).

### 4.8 Layer and occurrence additions

| Addition | Slot | Why |
|---|---|---|
| Pass-through vector layers | `blend: pass_through` on object occurrences, now unsupported rather than invalid in older readers | Inkscape and Illustrator layers are non-isolated, so a Multiply path blends with the layers below |
| Comic panel frames | A new occurrence field such as `frame`, naming a vector object whose fills clip the group's children and whose strokes draw the border above them | Clip Studio frame border folders clip raster, vector, text and balloon layers. Photoshop puts vector masks and frames on groups. Neither fits the single raster `mask` |
| Vector masks | A new mask field, or a known type in `mask.source` (now unsupported in older readers) | Photoshop keeps a raster and a vector mask on one layer |
| Fill opacity | An occurrence field, once layer styles exist | Photoshop `iOpa`, Affinity |
| Layer styles | Built-in effects, especially stroke or edge, shadows and glows | Photoshop `lfx2`, Clip Studio's Border effect, Krita's ASL styles |
| Tones | New Halftone parameters (dot shape, lines per inch from the composition resolution, density source, opacity to dot size, offset) and a Screentone generator | Clip Studio's tone layer property, Krita's Screentone fill layer, MediBang halftones |
| Blend names | W3C Soft Light; Darker and Lighter Color; Plus Darker and Plus Lighter; Average, Negation, Reflect, Glow, Contrast Invert and Pigment; CorelDRAW's logical and channel modes | Capy's Soft Light uses Photoshop's formula in Perceptual documents; the rest come from Sketch, Lunacy, Graphite, Affinity and CorelDRAW |
| Compositing operators | Erase (destination-out) and Behind (destination-over), inside isolated content only | Clip Studio transparent vector lines, ibisPaint eraser shapes, CorelDRAW and Affinity |
| Visibility per output | A layer field for screen, print and export | PDF optional content, Cricut guide layers |
| Rulers attached to layers | An occurrence attachment for `capy.guides/1` rulers, and more ruler kinds | Clip Studio keeps perspective, symmetry and special rulers per layer |

### 4.9 Document-level additions

- **Artboards and pages:** additional outputs. Inkscape 1.5 moves pages to `svg:view`.
- **Comic page settings:** trim, bleed, safe area, binding, spreads, page numbers and default tone frequency (Clip Studio's canvas).
- **Gutter links** between panel frames (Clip Studio "Keep gutters aligned"), or baked on import.
- **Swatches and global colors:** a many-use record that items reference by slot.
- **Graphic styles:** named, linked paint lists, applied by reference.
- **Character and paragraph styles:** for text.
- **Layer comps:** per-layer visibility, position and style sets (Photoshop, Clip Studio).
- **Slices and export regions:** records that name a layer, an item or a rectangle, with export presets (Affinity).
- **Perspective grids:** non-rendering records; attaching an item to a plane is a recipe on that item.
- **CMYK, Lab, spot colors and overprint:** typed color contracts and an output-profile record ([GA design §12](object-layer-ga-design.md#12-long-term-end-state-and-extension-stress-tests)).
- **Physical size:** the composition's resolution already gives it; SVG export defaults to 96 pixels per inch.
- **Item references across layers:** a `links` slot list naming other vector objects, with item IDs inside them, plus cycle checks. Within one object, bindings need nothing new.
- **JSON naming items:** if a JSON record ever names an item, it uses `{"object":{"ref":…},"item":"<decimal u64>"}`, matching evaluation endpoints. The `ref` key itself stays exclusive.

### 4.10 Text

Text is a **required leaf kind**: it has no child records. Its result cannot be
drawn faithfully with earlier kinds without dropping the story, so older
versions preserve files that use it. Convert to Outlines and pre-text SVG import
use results with recipes instead (4.6). These rules freeze when the first text
data ships, and cannot change afterwards:

| Decision | Rule | Evidence |
|---|---|---|
| Characters | UTF-8, with run lengths in UTF-8 bytes on scalar boundaries. Never grapheme counts, which change between Unicode versions; no normalization on save | Rust, parley and harfrust use byte offsets. Imports convert from UTF-16 (PSD, Sketch, Figma) and code points (SVG positioning) |
| Separators | LF ends a paragraph and U+2028 forces a line break; CR, CRLF and U+2029 are normalized on input | |
| Styles | Coalescible style runs that cover the story, pointing into a style table | PSD style runs, Figma `styleOverrideTable`, parley's style runs |
| Spans | A separate list of ranges that never merge: ruby, tate-chu-yoko, warichu, links, inline objects | Two adjacent ruby groups or tate-chu-yoko units with equal styles must stay two units, as in Clip Studio and InDesign |
| Paragraphs | Settings stored per paragraph | Simpler to validate than PSD's paragraph run lengths |
| Writing mode | On the whole text item; text orientation and combining go on runs or spans | Every format checked puts it on the story, frame or layer |
| Story and frames | A Text item is one story. Frames and the text path are geometry it owns. Threading means several frames in one item | Illustrator and IDML thread stories through frames; owning the path avoids sibling references that copying breaks |
| Font identity | A descriptor in the item: family, style, PostScript name, version, axis values, face index and a content digest | Noto Sans CJK's variable font has Thin as its default instance, so "Regular" needs explicit axis values |
| Embedded fonts | Only embedded bytes use a `fonts` slot, to a new `capy.font/1` resource | Sharing a font resource never shares editing |
| Embedding policy | Follow OS/2 `fsType`, with the user's opt-in. Embedded fonts load only into Capy's own collection, never into the system | On the Fedora workstation, 35 of 251 font files allow preview and print only and 18 allow editing |
| Stored result | Glyph runs (font, size, axis location, paint) plus a deduplicated outline table, used on open and regenerated only on edit, with a layout version | Figma stores glyph outlines with a font digest; PDF stores glyph runs; PSD stores pixels; Office stores its computed autofit scale |
| Values | Only specified values, never a cascade. Named styles arrive later as a reference plus local overrides | |

- **Cost of the stored result:**
  - Outlines cost about 1.3 KB per CJK glyph raw and 0.4 KB after LZ4, as Capy-style F64 records.
  - That is 3–10 times the size of a font subset. The outline table deduplicates repeated glyphs.
- **Later text features** are run, span, paragraph and layout tags:
  - kinsoku and mojikumi sets, emphasis marks, hyphenation, tabs and lists;
  - OpenType features and variable axes;
  - composite fonts, columns, auto-size, text in shapes with subtract shapes, and path options.

  An older text-aware reader renders the stored result. It drops an ignorable feature only when it edits that text.
- **Rust stack:**
  - harfrust shapes vertical text.
  - parley 0.12 has no vertical mode, ruby, hyphenation or tab stops.
  - skrifa draws COLRv0 and v1 and reads bitmap glyphs, but not SVG-in-OpenType.
  - Vertical layout, ruby, tate-chu-yoko, kinsoku and mojikumi would be Capy's own work on harfrust and ICU4X.
  - rustybuzz is archived, and the hypher hyphenation patterns carry LPPL and MPL licenses that need review.
- **wasm cost:** parley with skrifa built to 1.23 MB (439 KB gzipped); complex-script segmentation raised it to 5.0 MB (2.9 MB gzipped).

## 5. Mappings onto the model

### 5.1 SVG

| SVG | Capy | Notes |
|---|---|---|
| Root `svg`, `viewBox` | Composition size and resolution | Units become pixels at 96 per inch |
| `g` | Group | Inkscape layer groups become group layers |
| `defs`, `symbol`, `use` | Definitions and Instance | `use` x/y become part of the affine |
| Nested `svg` | Group with a clip | |
| `switch`, `a` | The chosen branch; Group | Links become ignorable link properties |
| `title`, `desc`, `metadata` | Ignorable, copy-safe properties | |
| `path` and basic shapes | Path; shapes keep a live-shape recipe | Quadratics and arcs become cubics; an exact-geometry recipe can keep arcs |
| `transform` | Absorbed into paths; affine on other items | Stretched strokes stay in a transformed group |
| `fill`, `stroke`, `paint-order` | Paint list | `currentColor` resolves at import |
| CSS Color 4 colors | Portable colors | Converted into one of the four RGB spaces |
| Stroke properties | Stroke entry | Dashes, markers and `vector-effect` are later tags |
| Linear and radial gradients | Gradient paint | Bounding-box units resolve per use |
| `pattern` | Pattern paint plus definition | |
| Mesh, hatch | Mesh and hatch paints | Not in SVG 2; Inkscape only |
| `marker` | Marker definitions referenced by strokes | |
| `opacity`, `mix-blend-mode`, `isolation` | Group or item properties | |
| `clip-path`, `mask` | Clip and mask children of a group | |
| `filter` | Group effects, built-in where the meaning matches | Otherwise the SVG filter-graph type |
| `text`, `tspan`, `textPath` | Text | Until Text exists, import outlines it and keeps the original fragment as a recipe |
| `image` | Image item | Decoded pixels are stored; the original bytes are not kept |
| `foreignObject`, animation, script, CSS sheets | Not promised (section 7) | |

### 5.2 Inkscape

| Inkscape | Capy |
|---|---|
| Layers and sublayers (`inkscape:groupmode="layer"`) | Group layers and vector layers |
| Lock (`sodipodi:insensitive`), hide, labels | Item properties |
| Node types (`sodipodi:nodetypes` c, s, z, a) | Anchor kinds; auto-smooth becomes Smooth |
| `sodipodi:type` rect, arc, star, spiral | Paths with live-shape recipes |
| Live path effects | Recipes holding the source geometry; Power Stroke becomes a width profile |
| Clones, symbols, tiled clones | Instances; tiled clones are Repeat recipes |
| Connectors | Paths with connector recipes naming their end items |
| Shared gradient vectors, forked on edit | Gradient paints per item; a swatch reference when `inkscape:swatch` is set |
| Two-level patterns | Pattern definition plus per-use transform |
| Mesh and hatch with a JavaScript polyfill | Mesh and hatch paints |
| SVG 2 wrapped text with an SVG 1.1 fallback | Area text; the fallback is regenerated at export |
| Filters (213 presets) | Group effects |
| Pages, guides, grids | Outputs; straight rulers in `capy.guides/1`, flipping Inkscape's y-up guide positions; grids are dropped |
| Hairline | Non-scaling stroke |

Inkscape keeps unknown XML through a round trip and adds IDs to elements.
Capy keeps what maps onto its model. Anything else is dropped at import with a
notice, or kept as ignorable SVG extras (section 7).

### 5.3 Illustrator

| Illustrator | Capy |
|---|---|
| Layers and sublayers | Group layers and vector layers |
| Group, clip group | Group with a clip child |
| Path, compound path | Path with several subpaths |
| Appearance: ordered fills and strokes, each with opacity, blend and effects | Paint list with per-entry properties |
| Transparency: blend, opacity, isolate, knockout, opacity mask (clip, invert) | Group properties and mask child |
| Gradients with midpoints and per-use placement | Gradient paint with midpoints |
| Freeform gradient, gradient mesh | Freeform and mesh paints |
| Patterns with tile options | Pattern paint and definition |
| Variable and one-sided widths, arrowheads, dash alignment, stroke alignment | Two-sided width profile and later stroke tags |
| Brushes | Brush strokes or brush paths, later |
| Symbols (static, dynamic, 9-slice) | Instances and definitions |
| Live Shapes, Live Corners | Recipes |
| Plugin groups: Blend, Envelope, Live Paint, Repeat, compound shape, Intertwine, Objects on Path, Dimension | Groups with recipes |
| Text frames: point, area, on a path, threaded | Text |
| Embedded raster; linked files | Image item; linked files embed |
| Artboards | Outputs |
| Swatches: global, spot, tints; graphic styles | Document-level swatches and styles |
| Item uuid, name, hidden, locked, note | Item ID and item properties |

Illustrator's SVG export expands or rasterizes most of the right-hand column:
appearance stacks, live effects, brushes, meshes, blends and symbols with
overrides. An Illustrator user moving work to Capy needs a PDF or AI importer
(hayro reads PDF) more than SVG.

### 5.4 Affinity, CorelDRAW and Xara

| Construct | Capy |
|---|---|
| Affinity's single object tree with artboards, containers, curves and shapes | Group layers and vector layers; artboards as outputs |
| Affinity Appearance: ordered fills and strokes with blend, opacity and visibility | Paint list with per-entry properties |
| Affinity shapes (25 kinds) and Smart nodes | Live-shape recipes; Smart anchors are edited as Corner until known |
| Affinity compounds, warp groups and blend groups | Groups with recipes holding nested inputs |
| Affinity clip by nesting, mask layers, nested adjustments and live filters | Clip and mask children; item effects and backdrop effects |
| Affinity symbols with per-attribute sync and extra children | Instances with per-property overrides; slots later |
| Affinity layer effects and blend ranges | Built-in item effects; a blend-range field later |
| CorelDRAW PowerClip, lenses, contours, extrusions, envelopes and blends | Clip groups, backdrop effects, recipes; extrusions import as results |
| CorelDRAW fills: fountain, mesh, vector and bitmap patterns, textures, PostScript | Gradient, mesh, pattern and image paints; procedural textures as a recipe plus a cached tile; PostScript fills import as rasters |
| CorelDRAW variable outlines with two sides, calligraphic nibs, behind fill | Two-sided width profile, pen transform, a stroke entry before the fill |
| Xara live effects (stored bitmap plus parameters), moulds, ClipView, brushes | Image results with recipes, envelope recipes, clip groups, brush strokes |

All three keep editable sources beside generated results; Affinity stores
parameters only, so its importers must reimplement every generator.
CorelDRAW converged on stored fallbacks: variable outlines open in versions
before 24.3 as filled shapes, blends are saved with the file by default, and
saving to an older version offers "Keep appearance". Xara's record classes are
the closest precedent for the framing: an unknown ordinary record keeps its
children, an atomic one is skipped with its subtree, and an essential one stops
the import.

### 5.5 Figma, Sketch, Penpot and Graphite

| Construct | Capy |
|---|---|
| Frames, groups, sections | Groups; frame clipping is a clip child |
| Rectangle (per-corner radii, corner smoothing), ellipse with `arcData`, polygon, star, line | Paths with live-shape recipes |
| Vector networks | Each region becomes a fill-only path; the segment graph becomes one stroke-only path split at branch vertices. Shared-vertex editing is lost; a weld-table property could keep it later |
| Boolean groups (Figma, Sketch per-operand modes, Penpot with stored result) | Groups with boolean recipes |
| Paint lists with solid, gradient (linear, radial, angular, diamond), image, pattern and shader paints | Paint list; shader paints are out of scope |
| One stroke geometry shared by all stroke paints (Figma), or shared dash, cap and join (Sketch) | Independent stroke entries, a superset |
| Inside and outside strokes | Stroke alignment |
| Variable width points, stretch and scatter brushes, dynamic strokes | Width profile, brush strokes, path-effect recipes |
| Effects: shadows with spread, inner shadows, layer, background, progressive and glass blurs, noise, texture | Item effects; Figma's fixed order (inner shadows between fills and strokes) imports by splitting into groups |
| Sibling masks (`isMask`, `hasClippingMask`, `shouldBreakMaskChain`) | Pass-through groups owning clip or mask children |
| Components, instances with ID-path overrides, swaps, properties, variants, slots | Instances and their later additions |
| Graphite's strokes with their own transform, gradient midpoints, `opacity_fill`, `clipping_mask` | Pen transform, gradient midpoints, fill opacity, clip to content below |

Sketch opens a newer file in an older version and silently drops the newer data
on save; Penpot refuses files that declare features it lacks; Graphite errors on
unknown variants. Capy's Preserved outcome avoids both silent loss and refusal.

### 5.6 Painting and comic apps

| Construct | Capy |
|---|---|
| Photoshop shape layers: a fill layer plus a vector mask, with per-subpath combine, subtract, intersect and exclude | A vector layer with a boolean recipe. PSD stores no result, so importing subtract and intersect needs a Bézier boolean engine |
| Photoshop live shapes (`vogk`), vector masks on any layer, fill layers, live gradients | Live-shape recipes, vector masks, generator layers, gradient paints |
| Photoshop text (`TySh`), warped text, smart objects with perspective and warp, layer styles, fill opacity, layer comps | Text, warp recipes, image placement and placed documents, layer styles, fill opacity, layer comps |
| Clip Studio vector lines with per-point width, opacity and raw pen inputs, a brush and a seed | Brush paths with input channels and an opacity profile |
| Clip Studio text with ruby, tate-chu-yoko and edge | Text spans and run styles |
| Clip Studio balloons with tails and unions | Balloon recipes on groups |
| Clip Studio frame border folders | The occurrence `frame` field |
| Clip Studio focus and speed lines | Focus-line recipes |
| Clip Studio tones, gradient and fill layers, rulers | Halftone parameters and Screentone generator, generator layers, rulers attached to layers |
| Krita SVG vector layers, SVG 2 text, Screentone fill layer, comic panel tool | Vector layers, Text, Screentone generator, paths |
| Procreate text (vector until rasterized), QuickShape | Text; QuickShape stores nothing |
| Fresco vector brushes (filled outlines) | Filled paths |
| SAI linework with editable pressure | Brush paths |
| MediBang and ibisPaint panel objects, vector layers, eraser shapes | Frame field or paths, vector layers, the Erase operator |

No app stores per-point color. Every app's comic content becomes raster when it
leaves for another app; a `.clip` importer would be the only editable route.

### 5.7 Office, whiteboard, motion, CAD and PDF

| Construct | Capy |
|---|---|
| DrawingML preset and custom geometry, `chOff`/`chExt` group space | Preset recipes on groups; the child space folds into the group's affine |
| DrawingML connectors, text bodies, theme colors with modifiers, effects | Connector recipes, Text, swatch modifiers, item effects |
| SmartArt and charts with a cached drawing | Chart and table recipes; Office treats the recipe as authoritative where Capy keeps the result |
| Excalidraw and tldraw shapes, arrows with bindings, draw shapes with pressure, rough style | Live shapes, connector recipes, freehand-sample recipes, sketchy recipes |
| Lottie and After Effects shape layers, where a style applies to every preceding shape in its scope | Each (shape, style) pair becomes a path in the style's space; shared editing becomes a swatch later |
| Lottie and Rive modifiers, masks and mattes, transform components | Path-effect recipes, clip and mask children, transform components |
| Rive animation addressed by object ID and property key | Item IDs and tag numbers give the same addresses; style entries would need IDs |
| DXF lines, arcs, circles, splines, polylines with bulge and width, hatches, dimensions, blocks | Paths with exact-geometry recipes, hatch paints, dimension recipes, instances with arrays |
| Cutter and laser conventions: hairline cuts, color per operation | Non-scaling strokes and a machine-operation property |
| PDF knockout groups, soft masks, shading types 1–7, tiling patterns, spot colors, overprint, optional content, annotations | Knockout, mask modes, gradient, mesh and freeform paints, pattern paints, spot paints, overprint, output visibility; annotations out of scope |

## 6. Fidelity gaps outside the format

- **Document color setting:**
  - SVG composites in gamma-encoded sRGB. Faithful SVG needs an 8- or 16-bit Perceptual sRGB document, because float documents must blend in linear light.
  - A per-object perceptual option is a later addition (4.5).
- **Soft Light:** Capy uses Photoshop's formula in Perceptual documents ([rendering](../internals/rendering.md)), not the W3C formula SVG uses.
- **Isolated vector layers:** until pass-through object occurrences exist, a Multiply path inside a vector layer blends only with that layer's content.
- **Blend modes without equivalents:** Add, Linear Burn, Vivid Light, Linear Light, Pin Light, Hard Mix, Subtract and Divide have no SVG, CSS or PDF form, so SVG and PDF export must flatten them. Pass-through maps to SVG's non-isolated groups.
- **Raster-only masks** at the layer level.
- **Image re-encoding:** imported images keep decoded pixels only, so SVG export writes PNG even for JPEGs.
- **Layers panel:** it shows one row per layer and no child rows, which were removed in `3e809895b`. Complex imports need entering groups or an object list (open question 10 in the vector design). The format is unaffected.

## 7. Not promised

- **SVG extras:**
  - **Not imported:** SMIL and CSS animation, scripts, `foreignObject`, style-sheet rules and selectors, and authored number and unit spelling.
  - **Why:** Inkscape preserves these but does not edit them; Illustrator drops them.
  - **If wanted later:** an ignorable, copy-safe SVG-extras property can carry them unchanged.
- **Design-tool behavior:** auto layout, constraints, pins and sizing modes, prototyping, variables and tokens, collaboration fields and theme-relative colors. Artwork colors must be absolute.
- **Other content:**
  - 3D extrusion, cameras and embedded 3D models, which import as their results;
  - SmartArt layout engines and live-linked charts;
  - video, embeds and maps;
  - Type 3 fonts.
- **Round trips:** After Effects, Lottie and Rive animation, which would rely on the identity rules kept in the design.
- **Topology:** Figma vector-network topology, which imports as paths.
- **Kept out by policy:** linked files, which imports embed, and layer transforms, which are baked.

## 8. Not verified

- Most of Illustrator's SVG import behavior; Adobe documents little of it.
- How Photoshop saves unknown tagged blocks, whether its strokes follow the boolean result, and its polygon and star `vogk` keys.
- Everything about Inkscape 1.5, and Affinity details beyond its beta SDK.
- Figma's joins at branch vertices, a second mask in one parent, and its interpolation between width points.
- Clip Studio's behavior when focus lines meet a moved frame, and whether a balloon can fit its text.
- Safari's support for `mix-blend-mode` on SVG elements, `auto-start-reverse` and markers on every shape per engine, and `symbol` `refX`/`refY`.
- Behavior of 3D transforms inside SVG, and whether any engine composites SVG in linear light.
