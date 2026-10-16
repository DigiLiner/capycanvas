# Object content extensions: format audit and later additions

[Technical documentation](../README.md) · [Design and validation history](README.md)

Research date: **2026-10-15**, source baseline **`37fabf996`**. Reader fixes and
model revision: **2026-10-16**, source baseline **`86a0fd0e4`**.

This record checks whether the `.capy` format and the object-layer model can
grow into full SVG editing without a redesign. It also maps each later feature
onto the slot that would hold it. It describes intended work, not implemented
capabilities. The decisions it produced are in the
[vector-layer design](vector-layers-research.md#72-layer-model): one item tree,
tagged properties with rules for unknown data, ordered paint lists, transforms on
groups and non-path items, and results with recipes for live features.

The audit combined five research streams. Their write-ups, with every source URL, are in [`object-content-extensions/`](object-content-extensions):

| # | Stream | File |
|---|---|---|
| 1 | The `.capy` package and authored model, read in code | [01-capy-format-audit.md](object-content-extensions/01-capy-format-audit.md) |
| 2 | SVG 2 and its companion specs: features, model constraints, usvg | [02-svg-data-model.md](object-content-extensions/02-svg-data-model.md) |
| 3 | Inkscape 1.4.4 and unreleased 1.5: storage, live path effects, round trip | [03-inkscape-data-model.md](object-content-extensions/03-inkscape-data-model.md) |
| 4 | Illustrator 30.8: object model, Appearance, SVG writers | [04-illustrator-object-model.md](object-content-extensions/04-illustrator-object-model.md) |
| 5 | Krita, Affinity, Figma, Graphite, PDF and Clip Studio object models; Rust crates | [05-reference-object-models.md](object-content-extensions/05-reference-object-models.md) |

**Caveats on the evidence**
- **Adobe help pages** refuse automated clients, so Illustrator facts come from Wayback snapshots, SDK headers mirrored on GitHub and exported files found by code search. Adobe documents almost none of its SVG import; those claims are forum reports.
- **Inkscape 1.5 is unreleased.** Its facts come from master at `f093808f` (2026-10-15) and may change.
- **Browser support** comes from MDN browser-compat-data 8.1.6.
- **The code audit** read the code and ran its tests; it built no vector feature.

---

## 1. Verdict

- **The package can hold full SVG content.** Typed records, binary resources with local slots, references, the Preserved rule and ancillary records are all the machinery it needs. Nothing in the codec blocks groups, transforms, gradients, text or symbols.
- **The first vector design could not have grown that far.** It had:
  - a flat path list;
  - no transforms anywhere;
  - one optional fill and stroke per style;
  - a fixed binary layout;
  - a style table in the manifest's JSON;
  - nowhere to keep editing intent.

  All six were cheap to change before vector records ship and expensive afterwards. The design now avoids them (section 2.3).
- **The reader called some well-formed future files damaged.** That contradicted the package contract's own rule and is fixed (section 2.2).
- **SVG remains interchange and `.capy` the master file.**
  - Illustrator keeps editability in SVG only by embedding a private copy of the `.ai` file (`<i:pgf>`), and ignores the SVG markup when it reopens the file.
  - Inkscape's Plain SVG strips its own editing data.
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

### 2.2 Reader classifications fixed in this change

The [package contract](../reference/capy-package.md#object-records-and-defaults)
reserves Invalid for data that can never become valid, and calls a well-formed
combination a later version may allow unsupported. The reader broke that rule
in six places, so an older reader would have reported a newer file as damaged.
All six now return Unsupported:

| Case | Where | Later use it blocked |
|---|---|---|
| Pass-through blend on content other than a stack | `package/artwork_records.rs` | Non-isolated vector layers, whose items blend with the layers below |
| Alpha lock or an offset on content they do not apply to | `package/artwork_records.rs` | Alpha lock or offsets on new content kinds |
| A known record type in a slot that does not accept it | `authored/topology.rs`, `package/artwork_records.rs` | A vector mask in the existing mask slot, an image used directly as an object |
| Outputs naming a known record that is not an output | `authored/topology.rs`, `package/artwork_records.rs` | Artboards and other output kinds |
| A known resource type in another binding | `package/resources.rs` | New uses of existing resource kinds |
| An attachment without an eligible target | `authored/scene.rs` rules, reached through `package/artwork_records.rs` | Clipping to or attaching effects to new content kinds |

The regression test `well_formed_combinations_a_later_version_may_allow_preserve_the_package`
covers each case and checks that malformed values stay invalid. Edits still
cannot create these combinations. The contract now names them and adds the
rule they follow: later versions extend the format with new names, never by
giving an existing field a second meaning.

### 2.3 What the first vector design got wrong

| Gap | Why it mattered | Now in the [design](vector-layers-research.md#74-file-format) |
|---|---|---|
| A flat path list | Every SVG, Inkscape and Illustrator file is a tree. Adding groups later means two nesting mechanisms with different selection, undo and compositing rules | One tree. The vector object is the root group, and the item stream is in pre-order with End markers |
| No transform anywhere | Text, images and instances cannot absorb a transform. A stroke under a skew or uneven scale is stretched, which one width cannot describe | Paths still absorb transforms and scale strokes by √\|det\|. Groups, images, text and instances carry an F64 affine. Import keeps stretched strokes in a group |
| One optional fill and stroke | Illustrator, Affinity, Figma and Graphite use ordered paint lists; SVG's `paint-order` is the two-entry case | A style is an ordered list of fills and strokes, each with exactly one paint |
| A fixed binary item layout | Adding names, visibility or opacity later would force a second layout that readers decode forever | Items and properties are tagged and length-prefixed, with PNG-style ignorable and copy-safe bits |
| The style table in the manifest JSON | One style with a 10-stop gradient is about 100 JSON nodes, so about 40,000 such styles would fill the whole document's 4,194,304-node budget | Styles are inline in the binary items and interned in memory |
| Nowhere for editing intent | Live shapes, Inkscape path effects, Illustrator blends, Repeat and live booleans all need their settings kept | Results are ordinary items, and recipes are ignorable, not copy-safe properties |

The first audit proposed keeping recipes in separate ancillary records keyed by
item ID, with a fingerprint of the result to detect foreign edits. Item
properties with PNG's copy-safe rule are simpler. A reader that changes an item
drops that item's unknown unsafe properties itself, so nothing needs a
fingerprint, and the recipe moves with its item through duplicate, paste and
undo.

### 2.4 Constraints kept by policy

- **No layer transform.** Occurrences keep integer offsets; `placement` stays removed. Transforms live inside object types.
- **Editable content is single-use.** Document-wide symbols, swatches and styles need a new many-use editable record type with update propagation and cycle checks ([GA design §12.1](object-layer-ga-design.md#121-rules-that-keep-extensions-within-one-model)). Until then, definitions are local to one vector object.
- **No Capy IDs in binary payloads.** Every image, font, definition or effect an item uses is a slot in the record's JSON.
- **Preserved is whole-document.** A file that uses one unknown feature is read-only in an older version, unless the feature is ignorable.

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
| Text positioning lists index code points | Text storage needs a defined offset unit |

usvg 0.48.1 is a good minimal render model but loses what editing needs: `use`
and symbols become copies, units and bounding-box sharing disappear, colors are
8-bit sRGB and `non-scaling-stroke` is ignored. Capy can use it as an importer
and test oracle, not as its stored model.

## 4. Later additions by slot

Each row is new vocabulary inside `capy.vector-object/1` and
`capy.vector-items/1` (a kind, tag, value or slot list), or a new record type.
Older readers preserve the file, or ignore the addition when it is flagged
ignorable.

### 4.1 Item kinds

| Kind | Holds | Brings |
|---|---|---|
| Group | Properties, then children, then End | SVG `g`, Inkscape groups, Illustrator groups and clip groups, Affinity groups |
| Image | An `images` slot to `capy.image/1`, an affine, interpolation, later a crop | SVG `image`, Illustrator embedded raster, Affinity images. Linked files are out of scope; imports embed |
| Text | Characters, attribute runs, `fonts` slots, layout (point, area, on a path), an affine | SVG `text`, Illustrator text frames, Inkscape wrapped text |
| Instance | A definition, an affine, overrides keyed by item IDs | SVG `use`/`symbol`, Inkscape clones, Illustrator symbols |
| Definitions | A non-rendering container of item subtrees that instances, patterns and markers use | SVG `defs`, `symbol`, `marker`, `pattern` content |
| Brush path | A path with a brush slot and seed, rendered by the stamp engine | Textured vector strokes ([vector design §7.9](vector-layers-research.md#79-foundation-for-later-work)) |

### 4.2 Group properties

- **Compositing:** opacity, blend mode, and isolation (isolated or pass-through). Illustrator's knockout (off, on, inherit) and alpha-is-shape.
- **Clipping:** a clip child, a container of geometry whose paints are ignored. It covers SVG `clipPath` and Illustrator clipping masks, where the topmost child clips the group. A shared SVG clip path is copied for each use.
- **Masks:** a mask child with a mode (luminance or alpha) and invert and clip flags. It covers SVG `mask` and Illustrator opacity masks.
- **Effects:**
  - an `effects` slot list naming `capy.effect/2` applications, so built-ins apply to items;
  - an SVG filter-graph effect type with the 17 primitives and its own color rule;
  - a drop shadow, which Capy lacks even as a layer effect.
- **Group paints:** Illustrator's fills and strokes on a group, with a "contents" entry marking where the children render in the list.

### 4.3 Paint kinds and paint-entry properties

| Addition | Detail |
|---|---|
| Linear and radial gradients | Geometry in path space (end points; center, radius, focal point and `fr`), spread (pad, reflect, repeat) and a transform. Stops reuse Capy's gradient values, extended with more than 32 stops and an unpremultiplied interpolation option for SVG. Illustrator midpoints become extra stops, as its own SVG export does |
| Conic and diamond gradients | Figma and Affinity |
| Pattern | A definition plus a transform. Illustrator's brick and hex tilings and overlap go in the definition |
| Hatch | Inkscape's hatch: a pattern of strokes, with pitch and rotation |
| Mesh and freeform gradients | A patch mesh (PDF shading types 6 and 7; Inkscape and Illustrator meshes) and Illustrator's point and line freeform gradients |
| Swatch reference | A document-level swatch with a fallback color; global and spot colors use it |
| Context paint | SVG `context-fill` and `context-stroke`, for markers and instances |
| Per-entry opacity, blend and visibility | Illustrator, Affinity and Figma paint entries |
| Per-entry effects | Illustrator's effects on one fill or stroke |

### 4.4 Stroke properties

- **Dashes:** a dash array and offset, plus Illustrator's align to corners and path ends.
- **Alignment:** inside and outside (Illustrator, Affinity, Figma). It needs offsetting, and SVG export outlines it.
- **Markers:** marker references at start, middle and end, with SVG's orientation rules. Illustrator arrowheads add scale and tip alignment.
- **Non-scaling stroke:** includes Inkscape's hairline.
- **One-sided widths:** Illustrator's separate left and right width profiles.
- **Gradient on a stroke:** within, along or across the stroke (Illustrator).

### 4.5 Path and item properties

- **Item identity and metadata:** names, hidden, locked, notes, and SVG `id` and `class` as ignorable, copy-safe properties.
- **Segment kinds:** exact elliptical arcs for CAD, cutting machines and SVG `A` commands.
- **Opacity and blend on a single path:** a path behaves like a group of its own paint entries.
- **Blend domain per object:** an object-local perceptual option, for SVG-faithful blending inside Linear documents.

### 4.6 Live features stored as results with recipes

The result is ordinary items; the recipe is an ignorable, not copy-safe property
on the item or group that owns it.

| Feature | Recipe on | Sources |
|---|---|---|
| Live shapes: rectangle with per-corner radii, ellipse, pie and arc, polygon, star, spiral, line | The path | Illustrator Live Shapes; Inkscape `sodipodi:type` rect, arc, star, spiral; Krita parametric shapes |
| Live corners: radius, type (round, inverted, chamfer) per anchor | The path | Illustrator Live Corners; Inkscape Corners effect |
| Path effects: taper, offset, roughen (seeded), simplify, bend, envelope, lattice, pattern along a path, dashes as geometry | The path, holding the source geometry | Inkscape live path effects (`inkscape:original-d` plus `d`) |
| Live booleans | A group holding the result, with the operands and modes in the recipe | Illustrator compound shapes, Affinity compounds, Figma boolean groups, Inkscape Boolean Operation effect |
| Blends, Repeat (radial, grid, mirror), Objects on Path | A group of results, with key objects and settings in the recipe | Illustrator plugin groups; Figma Repeat; Inkscape Mirror and Rotate Copies |
| Envelopes and warps | A group, with the mesh or warp in the recipe | Illustrator Envelope Distort, Affinity Warp groups, Inkscape Envelope |
| Live Paint | A group of face fills and edge strokes, with the source paths in the recipe | Illustrator Live Paint; also the vector region fill planned in the vector design |
| Text converted to outlines | A group of paths, with the text in the recipe | Illustrator Create Outlines; Inkscape Object to Path |

Width profiles are native, not recipes: Inkscape's Power Stroke and Illustrator's
Width tool map onto the stroke's profile.

**Exceptions that cannot be recipes.** Connectors bound to other items (Inkscape),
text on a separate path, and live Image Trace of a placed image depend on other
items or slots. They need required kinds or properties, so older readers preserve
those files.

### 4.7 Instances and definitions

- **Local first:** definitions inside one vector object, used by instances, patterns and markers.
- **Overrides:** keyed by the definition's item IDs, and by paths of IDs through nested instances, as [GA design §12.1](object-layer-ga-design.md#121-rules-that-keep-extensions-within-one-model) requires. Figma and Affinity key them by stable node IDs. Krita turns `use` into copies and loses the link; Capy should not.
- **Shared later:** document-wide symbols need the many-use editable definition type (section 2.4).
- **Illustrator extras:** 9-slice scaling and the registration point are instance or definition properties.

### 4.8 Layer and occurrence additions

| Addition | Slot | Why |
|---|---|---|
| Pass-through vector layers | `blend: pass_through` on object occurrences, now unsupported rather than invalid in older readers | Inkscape and Illustrator layers are non-isolated, so a Multiply path blends with the layers below |
| Vector masks | A new mask field name or a known type in `mask.source` (now unsupported in older readers) | Illustrator and Affinity vector masks; comic panel frames |
| W3C Soft Light | A new blend name | Capy's Soft Light uses Photoshop's formula in Perceptual documents |

### 4.9 Document-level additions

- **Artboards and pages:** additional outputs. Inkscape 1.5 moves pages to `svg:view`.
- **Swatches and global colors:** a many-use record that items reference by slot.
- **Graphic styles:** named, linked paint lists, applied by reference.
- **Character and paragraph styles:** for text.
- **CMYK, spot colors and overprint:** typed color contracts ([GA design §12](object-layer-ga-design.md#12-long-term-end-state-and-extension-stress-tests)).
- **Physical size:** the composition's resolution already gives it; SVG export defaults to 96 pixels per inch.

## 5. Mapping of SVG, Inkscape and Illustrator constructs

### 5.1 SVG

| SVG | Capy | Notes |
|---|---|---|
| Root `svg`, `viewBox` | Composition size and resolution | Units become pixels at 96 per inch |
| `g` | Group | Inkscape layer groups become group layers |
| `defs`, `symbol`, `use` | Definitions and Instance | `use` x/y become part of the affine |
| Nested `svg` | Group with a clip | |
| `switch`, `a` | The chosen branch; Group | Links are dropped |
| `title`, `desc`, `metadata` | Ignorable, copy-safe properties | |
| `path` and basic shapes | Path; shapes keep a live-shape recipe | Quadratics and arcs become cubics until arc segments exist |
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
| `text`, `tspan`, `textPath` | Text | Until text exists, import outlines it and keeps the source text as a recipe |
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
| Connectors | Required connector kind, later |
| Shared gradient vectors, forked on edit | Gradient paints per item; a swatch reference when `inkscape:swatch` is set |
| Two-level patterns | Pattern definition plus per-use transform |
| Mesh and hatch with a JavaScript polyfill | Mesh and hatch paints |
| SVG 2 wrapped text with an SVG 1.1 fallback | Area text; the fallback is regenerated at export |
| Filters (213 presets) | Group effects |
| Pages, guides, grids | Outputs; straight rulers in `capy.guides/1`, flipping Inkscape's y-up guide positions; grids are dropped |
| Hairline | Non-scaling stroke |

Inkscape keeps unknown XML through a round trip and adds IDs to elements.
Capy keeps what maps onto its model; anything else is dropped at import with a
notice, or kept as ignorable SVG extras (section 7).

### 5.3 Illustrator

| Illustrator | Capy |
|---|---|
| Layers and sublayers | Group layers and vector layers |
| Group, clip group | Group with a clip child |
| Path, compound path | Path with several subpaths |
| Appearance: ordered fills and strokes, each with opacity, blend and effects | Paint list with per-entry properties |
| Transparency: blend, opacity, isolate, knockout, opacity mask (clip, invert) | Group properties and mask child |
| Gradients with midpoints and per-use placement | Gradient paint; midpoints as stops |
| Freeform gradient, gradient mesh | Freeform and mesh paints |
| Patterns with tile options | Pattern paint and definition |
| Variable width, arrowheads, dash alignment, stroke alignment | Width profile and later stroke tags |
| Brushes | Brush paths, later |
| Symbols (static, dynamic, 9-slice) | Instances and definitions |
| Live Shapes, Live Corners | Recipes |
| Plugin groups: Blend, Envelope, Live Paint, Repeat, compound shape, Intertwine, Objects on Path, Dimension | Groups with recipes |
| Text frames: point, area, on a path, threaded | Text |
| Embedded raster; linked files | Image item; linked files embed |
| Artboards | Outputs |
| Swatches: global, spot, tints; graphic styles | Document-level swatches and styles |
| Item uuid, name, hidden, locked, note | Item ID and ignorable properties |

Illustrator's SVG export expands or rasterizes most of the right-hand column:
appearance stacks, live effects, brushes, meshes, blends and symbols with
overrides. An Illustrator user moving work to Capy needs a PDF or AI importer
(hayro reads PDF) more than SVG.

## 6. Fidelity gaps outside the format

- **Document color setting:**
  - SVG composites in gamma-encoded sRGB. Faithful SVG needs an 8- or 16-bit Perceptual sRGB document, because float documents must blend in linear light.
  - A per-object perceptual option is a later addition (4.5).
- **Soft Light:** Capy uses Photoshop's formula in Perceptual documents ([rendering](../internals/rendering.md)), not the W3C formula SVG uses.
- **Isolated vector layers:** until pass-through object occurrences exist, a Multiply path inside a vector layer blends only with that layer's content.
- **Raster-only masks** at the layer level.
- **Image re-encoding:** imported images keep decoded pixels only, so SVG export writes PNG even for JPEGs.
- **Layers panel:** it shows one row per layer and no child rows, which were removed in `3e809895b`. Complex imports need entering groups or an object list (open question 10 in the vector design). The format is unaffected.

## 7. Not promised

- **Not imported:** SMIL and CSS animation, scripts, `foreignObject`, style-sheet rules and selectors, and authored number and unit spelling.
- **Why:** Inkscape preserves these but does not edit them; Illustrator drops them.
- **If wanted later:** an ignorable, copy-safe SVG-extras property can carry them unchanged on the items they came from.

## 8. Not verified

- Most of Illustrator's SVG import behavior; Adobe documents little of it.
- Everything about Inkscape 1.5.
- Safari's support for `mix-blend-mode` on SVG elements, `auto-start-reverse` and markers on every shape per engine, and `symbol` `refX`/`refY`.
- Behavior of 3D transforms inside SVG.
- Whether any engine composites SVG in linear light.
