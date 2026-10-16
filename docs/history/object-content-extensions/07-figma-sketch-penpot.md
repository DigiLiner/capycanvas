# Vector and shape data models in Figma, Sketch, Penpot, Lunacy, Linearity Curve and Graphite, checked against the planned Capy item model

[Object content extensions](../object-content-extensions-research.md) · source report, 2026-10-16

Checked against the vector design at `726cb6cdc`, or the draft before it in which recipes were separate records keyed by item ID. The design this audit produced is in [vector layers §7.2–7.4](../vector-layers-research.md#72-layer-model).

Date: 2026-10-16. Markers: **[K]** is general knowledge with no source checked for this report. **unverified** is a claim found in a single or secondary source, or inferred from one.

## Sources read

Repositories were cloned at these commits and read directly.

| Source | Commit, date | What was read |
|---|---|---|
| Figma plugin typings, github.com/figma/plugin-typings | `dff4874`, 2026-10-15 | `plugin-api.d.ts` |
| Figma REST types, github.com/figma/rest-api-spec | `1c15f00`, 2026-10-07 | `dist/api_types.ts` |
| Sketch file format, github.com/sketch-hq/sketch-file-format | `4493900`, 2023-04-03 (package 6.5.0) | `packages/file-format/schema/**` |
| Penpot, github.com/penpot/penpot | `472ba8d`, 2026-10-16 | `common/src/app/common/types/**`, `files/migrations.cljc`, `features.cljc`, `frontend/src/app/main/ui/shapes/*`, `CHANGES.md`, `docs/user-guide/export-import/*` |
| Graphite, github.com/GraphiteEditor/Graphite | `46f48a5`, 2026-10-15 | `node-graph/rfcs/document-format.md`, `node-graph/libraries/{vector-types,graphic-types,core-types,no-std-types,rendering}` |

URL prefixes used below:

- `FT` = https://github.com/figma/plugin-typings/blob/dff4874ca083e18cd74835fd5afc5079d46866b7/plugin-api.d.ts
- `FR` = https://github.com/figma/rest-api-spec/blob/1c15f007cc81b4409efb2f24fec89608e8a7548d/dist/api_types.ts
- `SK` = https://github.com/sketch-hq/sketch-file-format/blob/4493900abbfa49ae82fbcb8ad85cccf2cc2256b0/packages/file-format/schema/
- `PP` = https://github.com/penpot/penpot/blob/472ba8dde58b66651adef116f7d0095a948654d9/
- `GR` = https://github.com/GraphiteEditor/Graphite/blob/46f48a54106dc67d91feda8b316788394c493b8e/

The Sketch schema has had no commits since April 2023, so it does not show the 2025 Athens document model (§2.4).

## 1. Verdicts

- **Vector networks: no structural change** (§3.1).
- **Sibling masks: no structural change** (§3.2).
- **The real risks are four:**
  - the MVP envelope rules (§5, A1–A4);
  - whether paint entries are open records;
  - how instances are keyed and rendered;
  - Figma's fixed effect order, which puts an inner shadow between fills and strokes.

## 2. Per-tool inventory

### 2.1 Figma

**Design node kinds:** DOCUMENT, PAGE, FRAME, GROUP, TRANSFORM_GROUP, SLICE, RECTANGLE, LINE, ELLIPSE, POLYGON, STAR, VECTOR, TEXT, TEXT_PATH, COMPONENT_SET, COMPONENT, INSTANCE, SLOT, BOOLEAN_OPERATION and SECTION.

**FigJam and Slides kinds:** STICKY, STAMP, TABLE, TABLE_CELL, HIGHLIGHT, WASHI_TAPE, SHAPE_WITH_TEXT, CODE_BLOCK, CONNECTOR, WIDGET, EMBED, LINK_UNFURL, MEDIA, SLIDE, SLIDE_ROW, SLIDE_GRID and INTERACTIVE_SLIDE_ELEMENT (FT#L10466–L12413; FR#L839).

Node fields:

- **Rectangle:** four corner radii, `cornerSmoothing` (0 to 1; 0.6 is the iOS squircle) and per-side stroke weights (FT#L8775, FT#L8996–L9040; REST `rectangleCornerRadii`, FR#L612).
- **Ellipse:** `arcData {startingAngle, endingAngle, innerRadius}` (FT#L4343).
- **Polygon:** `pointCount`.
- **Star:** `pointCount` and `innerRadius`.
- **BOOLEAN_OPERATION:** union, intersect, subtract or exclude, with live children (FT#L11342).
- **TRANSFORM_GROUP:** `transformModifiers`, currently `REPEAT` only, either linear (horizontal or vertical) or radial, with `count`, `offset` and `unitType` (FT#L10850, FT#L12878–L12912).
- **TEXT_PATH:** owns a vector network plus `textPathStartData {segment, position}`. Per the docs, "The base vector network cannot currently be modified after creating the TextPathNode" (FT#L1336–L1355, FT#L5696, FT#L11074).
- **SLOT (2026):** content an instance supplies, with min and max child counts (FT#L11136–L11160, FT#L11317).

Plugin update 123 (2026-01-26) opened text-on-path, transform groups, brushes, dynamic strokes and width profiles to the API: https://developers.figma.com/docs/plugins/updates/2026/01/26/version-1-update-123

**Vector networks** (FT#L5387–L5470):

- **Vertices:** `x` and `y`, plus optional per-vertex `strokeCap`, `strokeJoin`, `cornerRadius` and `handleMirroring` (none, angle, or angle and length).
- **Segments:** start and end vertex indices, and `tangentStart` and `tangentEnd`.
- **Regions:** `windingRule`, `loops` (lists of segment indices), and their own `fills`. Vertices can have any number of segments.
- **Caps:** `strokeCap` includes arrowheads (FT#L7680). REST adds LINE_ARROW, TRIANGLE_ARROW and WASHI_TAPE caps (FR#L504–L548).
- **REST:** has no networks, only baked `fillGeometry` and `strokeGeometry` (FR#L504–L560).
- **`.fig` storage:** the network is a binary blob. Coordinates are in `normalizedSize` space, and vertices, segments and regions carry `styleID`s into a `styleOverrideTable`. Each node also stores baked `fillGeometry` and `strokeGeometry`, where the stroke is the "expanded stroke outline, not the center line" (https://grida.co/docs/wg/feat-fig/glossary/fig.kiwi; community source, unverified).

**Paint** (FT#L4787–L4941). The union is SOLID, GRADIENT_LINEAR, GRADIENT_RADIAL, GRADIENT_ANGULAR, GRADIENT_DIAMOND, IMAGE, VIDEO, PATTERN and SHADER. Every paint has its own `visible`, `opacity` and `blendMode`.

- **Gradients:** a 2×3 `gradientTransform` and stops. REST uses `gradientHandlePositions` instead (FR#L1591).
- **Image:** scale modes FILL, FIT, CROP or TILE; an image transform; a scaling factor; rotation in 90° steps; and filters for exposure, contrast, saturation, temperature, tint, highlights and shadows.
- **PATTERN:** `sourceNodeId` (any node), rectangular or hexagonal tiles, scaling, spacing and alignment (FT#L4879).
- **SHADER (Config 2026):** a reference to a shader resource with typed properties (FT#L1629–L1658, FT#L4913–L5050; https://www.figma.com/blog/config-2026-recap/).

**Color.** RGB(A) in the document profile, which is LEGACY, SRGB or DISPLAY_P3 (FT#L10474). Since update 141 (2026-10-15), an optional authored `sourceColor` can be SRGB, P3, HSL, HSB, OKLAB or OKLCH (FT#L3971–L4013; https://developers.figma.com/docs/plugins/updates/). Paint fields and gradient stops can bind to variables.

**Strokes.** All stroke paints share one geometry (FT#L8706–L8770):

- weight, with per-side weights on rectangles and frames;
- `strokeAlign` center, inside or outside. Per the docs, "inside and outside stroke are actually implemented by doubling the stroke weight and masking the stroke by the fill" (FT#L8757);
- `dashPattern`;
- join and cap, which can vary per vertex;
- `strokeMiterLimit`. REST stores `strokeMiterAngle` in degrees instead (FR#L548).

**Complex strokes (Draw)** (FT#L8823–L8992):

- **`variableWidthStrokeProperties`:** a preset (UNIFORM, WEDGE, TAPER, QUARTER_TAPER, EYE or MIRRORED_TAPER) or CUSTOM. CUSTOM holds `variableWidthPoints`, each a `position` from 0 to 1 along the stroke and a `width` as a fraction of the weight. Variable width "cannot be applied to complex vector networks (i.e. vector networks with branching paths)" and does not combine with dynamic strokes.
- **`complexStrokeProperties`:** BASIC, DYNAMIC (`frequency`, `wiggle`, `smoothen`), or BRUSH.
  - STRETCH brushes are built-in or CUSTOM, with a `direction`.
  - SCATTER brushes have `gap`, `wiggle`, `sizeJitter`, `angularJitter` and `rotation`.
- **REST** exposes neither: both traits are typed as bare `object` (FR#L608).
- Draw's launch post called the width tool "coming soon" (https://www.figma.com/blog/introducing-figma-draw/).

**Effects** (FT#L4351–L4686):

- **DROP_SHADOW:** color, offset, radius, `spread`, `blendMode` and `showShadowBehindNode`.
- **INNER_SHADOW.**
- **LAYER_BLUR and BACKGROUND_BLUR:** each normal or progressive. Progressive blur adds a start radius and start and end offsets.
- **NOISE:** monotone, duotone or multitone, with size and density.
- **TEXTURE:** size, radius and `clipToShape`.
- **GLASS:** light intensity and angle, refraction, depth, dispersion and frost.
- **SHADER.**

Limits per layer: 8 drop shadows, 8 inner shadows, 1 layer blur, 2 noise effects, 1 texture, 1 background blur and 1 glass.

**Render order is fixed by type:**

1. Background blur or glass at the bottom (only one of the two renders).
2. Drop shadows.
3. Fill paints.
4. Inner shadows.
5. Stroke paints.
6. Layer blur, noise and texture on top, in their listed order.

Source: https://help.figma.com/hc/en-us/articles/360041488473

**Masks.** `isMask` "masks its subsequent siblings" until the end of the parent (FT#L7597–L7650). `maskType` is ALPHA, VECTOR or LUMINANCE. For VECTOR, the fill and stroke regions are "fully visible" (FR#L641–L675). How a second mask in the same parent behaves is unverified.

**Instancing.**

- **Definitions:** a COMPONENT node; variants live in a COMPONENT_SET.
- **Properties:** BOOLEAN, TEXT, INSTANCE_SWAP, VARIANT or SLOT. Descendants link to them through `componentPropertyReferences` (FT#L11136–L11200).
- **Instances** (FT#L11244–L11316; FR#L1110) have:
  - `mainComponent`;
  - `componentProperties`;
  - `scaleFactor`;
  - exposed nested instances;
  - `overrides {id, overriddenFields}`;
  - `swapComponent` (keeps overrides heuristically) and `detachInstance` (detaches ancestors too).
- **What can be overridden:** nearly every field, including `vectorNetwork`, `characters`, `styledTextSegments`, `fills`, `relativeTransform` and `effects` (FT#L3763–L3895).
- **IDs inside instances** are paths, for example `I360:21745;1269:159559` (community report: https://forum.figma.com/ask-the-community-7/can-t-get-node-with-prefix-i-9560).
- **In `.fig`:** `symbolData {symbolID, symbolOverrides: NodeChange[] keyed by guidPath (overrideKeys from the instance root to the target), uniformScaleFactor}`, plus `derivedSymbolData`, which holds the resolved child properties (grida glossary; unverified).

**Layout.**

- `relativeTransform` (2×3) plus width and height (FT#L7382).
- Constraints MIN, CENTER, MAX, STRETCH or SCALE.
- Auto layout: padding, spacing, wrap, alignment, and grid spans (FR#L400–L470; FT#L8640–L8690).

**Text.** `characters`, `characterStyleOverrides[]` indexing a `styleOverrideTable`, `lineTypes[]` (ordered or unordered lists) and `lineIndentations[]` (FR#L677). Segments carry (FT#L5569–L5690):

- font size, and `fontName {family, style, variationSettings}`;
- decoration style, offset, thickness, color and skip-ink;
- case, line height and letter spacing;
- per-run `fills: Paint[]`;
- lists, indentation, and paragraph indent and spacing;
- `textWrapStyle` AUTO, BALANCE or PRETTY (2026-08);
- hyperlinks;
- `openTypeFeatures`.

At node level (FT#L11013–L11070): `textAutoResize` NONE, HEIGHT or WIDTH_AND_HEIGHT; truncation and `maxLines`; horizontal and vertical alignment; `leadingTrim`; hanging punctuation and lists. Fonts are referenced by family and style and never embedded. Variable axes arrived in update 138 (2026-09-03).

**Out of scope here:** Motion (timeline and keyframes: `animationStyles`, `manualKeyframeTracks`) and code layers (https://www.figma.com/blog/config-2026-recap/).

**Storage.**

- A `.fig` file has an 8-byte prelude (`fig-kiwi`), a u32 version, a compressed Kiwi schema, then compressed data (https://grida.co/docs/wg/feat-fig/; community source, unverified).
- In Kiwi, "New fields can be added to any message while maintaining backwards compatibility". Old versions "can optionally read new data if a copy of the new schema is bundled", which "lets the decoder skip over unknown fields". Structs are closed (https://github.com/evanw/kiwi).
- Child order uses fractional-index strings (`parentIndex.position`). REST has `schemaVersion` (FR#L5700).

### 2.2 Penpot

**Types.** `:frame :group :bool :rect :path :text :circle :svg-raw :image` (PP common/src/app/common/types/shape.cljc).

- **`:image` is deprecated.** Migration 0005 turns images into a rectangle with an image fill (PP common/src/app/common/files/migrations.cljc#L1369).
- **`:svg-raw`** keeps imported SVG elements verbatim, children included.

**Geometry.**

- **Transform:** `selrect`, four corner `points`, `transform` and `transform-inverse`, and x, y, width, height, rotation and flips. Objects form a flat UUID map with `parent-id` and `shapes`.
- **Paths:** binary, 28 bytes per segment: a command plus six f32 values (PP common/src/app/common/types/path/impl.cljc#L32). A fill rule exists only on imported SVG.
- **Corners:** radii `r1`–`r4`.
- **Booleans:** keep their children plus a stored result `:content` (`schema:bool-attrs`).

**Fills** (PP common/src/app/common/types/fills.cljc, color.cljc#L75–L140):

- a vector of exactly one of: 8-bit hex color, gradient (linear or radial with start, end and width) or image;
- `fill-opacity` and a library-color reference;
- no blend mode per fill.

**Strokes** use a closed schema (shape.cljc `schema:stroke-attrs`, stroke.cljc):

- their own paint, width, and per-side widths (2.19);
- `stroke-style` solid, dotted, dashed or mixed;
- dash and gap;
- alignment center, inner or outer;
- `stroke-cap-start` and `stroke-cap-end`, each one of round, square, line-arrow, triangle-arrow, square-marker, circle-marker or diamond-marker.

Inner and outer strokes render by clip or mask with doubled width (PP frontend/src/app/main/ui/shapes/custom_stroke.cljs#L30–L60).

**Effects.** A vector of drop or inner shadows, each with spread. One layer `blur`. `background-blur` has been separate since migration 0029. Blend modes are the 16 CSS modes only.

**Masks.** In a `:masked-group`, the **first child is the mask**, rendered as both a clipPath and a mask (PP frontend/src/app/main/ui/shapes/group.cljs#L20–L60).

**Components.** An instance is a full copy of the main component's tree. Each copy has:

- `shape-ref`, pointing to the main shape;
- `touched`, the set of synced attribute groups the user overrode (fill, geometry, text content and so on);
- `swap-slot-<uuid>` groups for swapped children;
- `component-id`, `component-file`, `component-root` and `main-instance`.

Source: PP common/src/app/common/types/component.cljc#L48–L130, L263. Variants are behind `variants/v1`.

**Text.** A tree of root, paragraph-set, paragraph and leaf, with fonts, fills (a vector), direction, decoration and typography references. `position-data` stores the laid-out runs (PP common/src/app/common/types/shape/text.cljc). SVG export writes native `<text>`, and "Some SVG's may require fonts to be installed at system level" (CHANGES.md L2854).

**Storage.**

- **Format:** `.penpot` v3 is a ZIP of JSON with a manifest (format version 1, plus features). A data version tracks the model (PP docs/user-guide/export-import/penpot-file-format.njk).
- **Migrations:** the file stores the set of migration names applied to it (migrations.cljc#L50–L80, L2335–L2375).
- **Features:** the file declares required features. A client missing one raises `:feature-not-supported` or `:file-feature-mismatch` (PP common/src/app/common/features.cljc#L45–L260).
- **Closed schemas:** they reject extra keys. Migration 0030 exists because old files "fail validation with an `:malli.core/extra-key` error".

### 2.3 Lunacy (`.free`)

**Container.** A ZIP with `pages/*.json`, `images/`, `fonts/` (embedded fonts), `shared/`, `document.json`, `meta.json` and `preview.webp`. Floats are 32-bit, defaults are omitted, and NULL, NaN or Infinity make a file broken (https://lunacy.docs.icons8.com/free-format).

**Layer types.** FRAME, COMPONENT, SLOT, STATES, SECTION, GROUP, SHAPE (a group of paths), PATH, LINE, OVAL, POLYGON, RECT, STAR, TRIANGLE, CONNECTOR, INSTANCE, SLICE and TEXT.

**Layer fields:**

- `Transform` (6 floats), `Pos` and `Size`;
- `BoolOp`;
- `Mask`, `BreakMask` and `MaskType` (Vector, Alpha or Luminance);
- `Fills` and `Borders`, with opacity and blend per fill;
- blend modes 0 to 17, ending with PlusLighter;
- `CornerRadius[]` and `SmoothCorners`.

**Paths.** Vertices are `[x, y, mode, radius, fromX, fromY, toX, toY]`, with the radius at half precision. An optional `PathData` SVG string overrides the points. Paths also have `StartMarker` and `EndMarker`.

**Effects and text.** Shadows and inner shadows have spread and blend. Blur is Gaussian or background. Text has `DrawOnPath`, `Warp`, range `Inlines`, `Truncate` and `MaxLines`.

**Overrides** are described as "unbounded". The documentation's example keys them by a `target` array of layer IDs, which the spec does not list, so it is inconsistent.

**Versions.** v1 to v8 shipped from 2024-03 to 2026-03-15, with "many renames" in v5 and v7. No compatibility rules are published.

### 2.4 Sketch

**Layer classes.** shapePath, rectangle, oval, polygon, star, triangle, shapeGroup, group, artboard, symbolMaster, symbolInstance, text, bitmap, slice, hotspot and page.

**Every layer has** `frame`, `rotation`, both flips, `booleanOperation`, `resizingConstraint`, `hasClippingMask`, `clippingMaskMode`, `shouldBreakMaskChain` and `style` (SK layers/abstract-layer.schema.yaml).

**Shapes.** Points are `curvePoint` records with `point`, `curveFrom` and `curveTo` in 0..1 frame space [K]. Each point has (SK objects/curve-point.schema.yaml):

- `curveMode` straight, mirrored, asymmetric or disconnected;
- its own `cornerRadius`;
- `cornerStyle` rounded, rounded-inverted, angled or squared.

The layer also has `pointRadiusBehaviour` (rounded or smooth) and an `edited` flag. Parametric shapes keep both their points and their parameters. A `shapeGroup` has a winding rule, and each child's boolean op combines with the result of the children before it [K].

**Style** (SK objects/style, fill, border, border-options, shadow, blur, gradient, color; enums/blend-mode):

- `fills[]` and `borders[]`, each with `isEnabled`, blend mode and opacity. Borders also have their own thickness and position.
- Dash, cap and join are shared across borders.
- Start and end markers: open and filled arrows, line, and open or filled circles and squares.
- `shadows[]` with spread, and `innerShadows[]`.
- One `blur`: Gaussian, motion, zoom or background.
- `colorControls`.
- Blend modes: the 16 CSS modes plus **Plus Darker and Plus Lighter**.
- Gradients use `from`, `to` and `elipseLength`, with no affine.
- Colors have an optional `swatchID`. The color space is set per document.

**Masks** "affect all layers that lie above them". "Ignore Underlying Mask" is stored as `shouldBreakMaskChain`. A mask is outline or alpha (https://go.sketch.com/docs/designing/shapes/masking-shapes.md). The docs advise toggling off the mask's fill while positioning it, which implies the mask layer's own style is drawn (inferred, unverified).

**Symbols.** An instance has `symbolID`, `scale` and `overrideValues`. Each override name is `<uuid>/<uuid>…_<property>`: a **path of IDs** plus one of `stringValue`, `symbolID`, `image` or `layerStyle` (SK layers/symbol-instance.schema.yaml, objects/override-name.schema.yaml).

**Text.** An attributed string whose runs carry a font (PostScript name, size, `variation` keyed by numeric axis), color, kerning and paragraph style. The layer adds `textBehaviour`, `glyphBounds` and `automaticallyDrawOnUnderlyingPath`. Fonts can be embedded through `fontReferences`.

**Athens (2025.1) changes** (https://developer.sketch.com/plugins/updates/new-in-sketch-20251; https://www.sketch.com/changelog/athens/):

- Frames and Graphics, distinguished by `groupBehavior`;
- Stacks (auto layout);
- sizing and pins;
- `blurs[]`;
- `isInnerShadow`;
- tints.

A moderator describes it as "a document model update that fundamentally changes some of the things in the way documents are handled" (https://forum.sketch.com/t/havent-upgraded-to-sketch-2025-1-3-can-i-go-back-to-101-8-later/5081).

**Versioning.** A file carries a document `version` and a `compatibilityVersion`. An older app can open a newer file, but "document data only supported by the newer version of Sketch is not retained" (https://developer.sketch.com/file-format/versioning).

### 2.5 Linearity Curve

The file format is not public. `.curve` and `.vectornator` share one format (https://fileinfo.com/extension/curve).

- **Strokes:** width, regular or brush mode, dash and gap, cap, join, position, and **separate start and end arrows** (https://linearity.io/academy/curve/ipad/user-guide/colors/strokes).
- **Brush:** paths with variable width, editable "by adjusting the nodes or changing its profile". The nib has **roundness** (100% is a circle, 1% a flat line), an **angle**, a contour, optional pressure, smoothing, and custom presets (https://www.linearity.io/features/brush-tool/). Roundness and angle make it a calligraphic nib, not a width profile.
- **Layers:** top-level layers contain elements and nested groups, and layer opacity applies to every child. Mask/Unmask works on a selection (https://www.linearity.io/academy/curve/mac/user-guide/workspace/organize-layers; https://www.linearity.io/academy/curve/ipad/user-guide/vector-editing/shaping-tools/).
- **Recent releases** (https://www.linearity.io/whats-new/):
  - 6.8 (2026-03): Glass Effect;
  - 6.10 (2026-05): corner smoothing and path bending;
  - 6.11 (2026-06): noise effect and SVG export improvements.
- **Not documented:** blend modes, gradients and symbols. An old forum thread title says SVG export "deactivates blend modes and clipping masks" (https://forum.vectornator.io/t/exporting-to-svg-deactivates-blend-modes-and-clipping-masks/1800; title only, unverified).

### 2.6 Graphite

A Graphite document *is* a node graph: a recipe with no stored result.

**Vector data.** A point domain (ids and f64 positions), a segment domain (start and end indices plus handles) and `colinear_manipulators` (GR node-graph/libraries/vector-types/src/vector/vector_types.rs#L17, vector_attributes.rs#L84–L220). Fills come from the faces of the segment graph, minus faces whose boundary winding marks them as "deliberate negative space" (vector_attributes.rs#L935).

**The newest commit** (#4732) adds a point-pool `Path`: an ordered list of indices into a shared pool, with linear, quadratic or cubic curves. Per the code, "Welding is two paths, or two ends of one path, referencing the same anchor" (GR libraries/vector-types/src/vector/path.rs#L1–L60).

**Appearance** is an ordered list of Fill or Stroke passes. Each pass's paint can be any graphic (GR libraries/core-types/src/list.rs#L80–L100; graphic-types/src/appearance.rs#L13–L50).

**Strokes** have weight, dashes with an offset, cap, join, miter, align, and **their own `transform: DAffine2`** (GR vector-types/src/vector/style.rs#L210–L225).

**Gradients** have:

- spread pad, reflect, repeat or clear;
- form linear or radial;
- an interpolation color space, with hue direction for polar spaces;
- stepped, linear or smooth interpolation;
- cyclic stops;
- **per-stop midpoints**.

**Item attributes:**

- `transform`, `blend_mode` and `opacity`;
- **`opacity_fill`**, which "does not affect content clipped to the item";
- **`clipping_mask`**, which "inherits the alpha of the content beneath it" (list.rs#L14–L80).

Blend modes include Darker Color, Lighter Color, Erase, Restore and Multiply Alpha (GR libraries/no-std-types/src/blending.rs#L11–L45).

**Storage (`.gdd` RFC).** A flat node registry plus CRDT deltas. Values are type-erased so migrations need no old struct shapes, and proto-node declarations are content-addressed. There is one `format_version`. Unknown `DataSource` variants are "a hard error on load", and an `Other(Value)` delta serves as an escape hatch (GR node-graph/rfcs/document-format.md).

## 3. Cross-cutting analysis

### 3.1 Vector networks: how they map onto compound paths

**Input.**

- The plugin API gives `vectorNetwork`.
- `.fig` gives the network blob plus baked geometry.
- REST gives baked geometry only, so no topology at all.

**Mapping.**

1. **Regions.** Each region becomes a fill-only Path, in region order. Its loops become closed subpaths, its fills (or the node's) become the style, and its winding rule becomes the fill rule. This is exact.
2. **Segments.** The segment graph becomes one stroke-only Path above the fills. Trails run through vertices with exactly two segments and break at any other vertex. Closed cycles become closed subpaths. At branch vertices, pair segments by the smallest turn.
3. **Per-vertex caps** become per-end markers once those exist, or a split into separate Paths.
4. **Per-vertex corner radius.** Bake the rounding into the result and keep the sharp network as a live-corners recipe.
5. **Node opacity, blend and effects** wrap the result in a Group.

**Losses.**

- The shared-vertex editing topology. This affects editing only.
- Joins at branch vertices. How Figma joins three or more segments is unverified.
- Dash phase across trails.
- Inside or outside alignment needs the stroke Path clipped by the fill region.

Exact rendering is available by importing the outlined stroke. Variable width is not at risk, because Figma forbids it on branching networks (FT#L8971–L8980).

**Verdict: no new geometry model.** If network editing must round trip, add an optional weld table on Path, following Graphite (path.rs#L41–L45). It can be ignorable if A4 holds.

### 3.2 Sibling masks and how they map to group-owned masks

| Tool | Semantics | Source |
|---|---|---|
| Figma | masks every later sibling to the end of the parent; ALPHA, VECTOR (fill ∪ stroke) or LUMINANCE | FT#L7597; FR#L641 |
| Sketch | outline or alpha; masks the layers above; a layer escapes with `shouldBreakMaskChain` | SK abstract-layer; masking doc above |
| Lunacy | `Mask`, `BreakMask`, `MaskType` Vector, Alpha or Luminance | free-format doc |
| Penpot | the first child of a masked group is the mask | PP group.cljs#L20 |
| Graphite | per-item `clipping_mask`; SVG mask and clip output | list.rs#L25; renderer.rs#L186, L1558 |

**Import.**

1. Scan each parent's children from bottom to top.
2. At a mask node, open a pass-through Group with the mask node as its clip child (VECTOR or outline) or mask child (ALPHA or LUMINANCE).
3. Add the following siblings until the end of the parent (Figma), or until a sibling that breaks the chain or the next mask (Sketch, Lunacy).
4. If Sketch draws the mask's style, also add a visible copy of the mask shape (unverified).

If a run is broken in the middle and resumes, duplicate the mask into a second group. Rendering stays exact; only the editing link between the copies is lost.

**Export.** The mask child becomes the first child with `isMask` or `hasClippingMask`.

**Verdict: no structural change.** The Group design needs three rules:

- the clip or mask child can be any kind;
- pass-through plus mask behaves as it does for layers (crates/layer-core/src/layers.rs#L185–L190);
- the clip covers fill ∪ stroke.

### 3.3 Paint and effects compared

**Every tool has:**

- multiple ordered fills and strokes;
- per-paint visibility and opacity (Penpot has opacity only);
- per-paint blend (not Penpot or Graphite);
- multiple shadows with spread;
- inner shadows;
- layer and background blur.

**Figma adds:** progressive blur, noise, texture, glass and shaders.

**Capy lacks:** per-entry opacity, blend and enabled; drop and inner shadow; and backdrop effects.

Capy's ordered stack is a superset of the fixed pipelines, with one exception: Figma puts inner shadows between fills and strokes. Import handles that by splitting into a Group of (fills plus inner shadow, strokes).

Figma's stroke paints share one geometry and Sketch's borders share dash, cap and join. Capy's independent stroke entries are a superset of both.

### 3.4 Instancing: what "overrides keyed by item id" misses

1. **Nested instances need ID paths.** Figma uses `guidPath`, Sketch `uuid/uuid_prop`, and Figma's runtime IDs are `I…;…`. A u64 key cannot reach into nested instances.
2. **Overrides are per property.** Figma lets nearly every field be overridden (FT#L3763), and Penpot records touched attribute groups. Replacing whole items turns every definition edit into a conflict.
3. **Instance swap.**
4. **Component properties**, which drive several descendants through references.
5. **Variants.**
6. **Slots**, which add children. Overrides cannot add items.
7. **A stored result** that readers can draw: Figma `derivedSymbolData`, Penpot full copies.
8. **A uniform scale factor and detach.**
9. **Definitions must be document-level records.** Item IDs are local to a layer, and an Object layer holds one object. Content patterns need the same record.

**Recommendation.** Model an instance as a Group of the materialized result plus an instance recipe. The recipe holds the definition reference, ID-path keys, property-level patches and property values. Each child carries a back-reference to its definition item, as Penpot's `shape-ref` does.

### 3.5 Layout and transforms

| Tool | Transform storage |
|---|---|
| Figma | `relativeTransform` plus size; network coordinates in normalized space |
| Sketch | frame, rotation and flips; points in unit space [K] |
| Penpot | selrect, four points and a matrix with its inverse |
| Lunacy | a 6-float matrix |
| Graphite | per-item affine, **and a separate transform per stroke** |

Baking transforms into coordinates is exact for geometry. Two things must be defined:

- whether a group's affine transforms strokes, as SVG does (an elliptical pen), or not, as Figma does;
- the frame (center, size, rotation) that parametric recipes store.

Auto layout, constraints, grids and sizing are out of scope. Frame clipping maps to a group clip. Corner-radius scaling is a recipe policy; Figma scales radii only with `scaleFactor` (FT#L11290).

### 3.6 Text

Fonts are referenced by name in every tool: family and style plus axes (Figma), PostScript name plus numeric axes (Sketch), or font id (Penpot). Sketch and Lunacy can also embed font data; Figma never does.

**Shared run and paragraph features:**

- per-run paint lists;
- line height in pixels, percent or auto; letter spacing; case;
- decoration details;
- paragraph spacing and indent;
- lists with levels;
- horizontal and vertical alignment;
- auto-resize;
- truncation with a maximum line count;
- OpenType features and variable axes;
- baseline shift and direction;
- text on path;
- warp (Lunacy).

Penpot (`position-data`) and Sketch (`glyphBounds`) store layout results.

**For Capy's planned Text:**

- runs should be open records carrying a full Style;
- paragraphs should be records of their own;
- a font resource can be name-only or embedded, with a fallback;
- path layout embeds its own path; a sibling reference, as Sketch uses, breaks under copying;
- optionally, glyph outlines as a recipe result.

### 3.7 Storage and forward compatibility

| Tool | Scheme | Behaviour with unknown data |
|---|---|---|
| Sketch | `version` plus `compatibilityVersion` | opens a compatible newer file, then **silently drops unknown data on save** |
| Figma `.fig` | Kiwi schema embedded in each file | open messages, so unknown fields can be skipped; one server-side version in practice |
| Penpot | data version, named migration set and required features | refuses missing features; closed schemas reject extra keys |
| Lunacy | integer v1–v8, with renames | unspecified |
| Graphite | `format_version` plus type-erased values | unknown variants are a hard error; `Other(Value)` escape hatch |
| Capy (planned) | — | Preserved (read-only) unless ignorable; stale recipes dropped |

**Lessons.**

- Read-only beats silent loss.
- Store results that readers can draw: Figma's baked geometry and `derivedSymbolData`, Penpot's boolean content and text positions, Sketch's points. This is the precedent for A2.
- Penpot's named migration set lets parallel branches add migrations without colliding.
- Lunacy shows how much an early format churns.

### 3.8 What breaks on SVG export

- **Figma:**
  - Angular and diamond gradients were exported as radial, later through `foreignObject`, which breaks `<use>`. Mid-2025 support advice still says to rasterize angular gradients.
  - Background blur fails historically, and blend modes are lost on noise filters.
  - By construction, inside and outside strokes need masks, and brushes, variable widths, noise, glass and shaders can only become outlines, filters or rasters [K].
  - Sources: https://forum.figma.com/report-a-problem-6/gradients-not-exporting-properly-in-svg-36592 ; https://forum.figma.com/ask-the-community-7/svg-exports-using-foreignobject-38163 ; https://webdesign.tutsplus.com/using-figma-for-svg-design--CRS-200909c/what-exports-well-to-svg ; https://forum.figma.com/report-a-problem-6/filter-blendmode-gets-lost-when-exporting-as-svg-51354
- **Sketch:**
  - inner shadows come out as a black fill;
  - translucent inner borders are wrong;
  - angled gradients distort on non-square shapes;
  - blur filters are ignored when an SVG is imported back.
  - Sources: https://forum.sketch.com/t/layer-with-shadow-inner-shadow-svg-export-issue/633 ; https://forum.sketch.com/t/exporting-svg-with-inner-borders-is-incorrect/5581 ; https://forum.sketch.com/t/gradients-in-svg-inconsistent/6026 ; https://forum.sketch.com/t/blur-effects-in-svg-files-are-ignored-by-sketch-when-it-opens-them/5368
- **Penpot:** native `<text>` needs installed fonts; linear gradients in text were once exported as radial (since fixed); layout and components are lost (CHANGES.md L2854, L325).
- **Graphite:** blend modes outside the 16 CSS modes trigger an "Unsupported blend mode" warning; raster content becomes `data:image/png`; masks become `<mask>` or `<clipPath>` (GR libraries/no-std-types/src/blending.rs#L106–L140; libraries/rendering/src/renderer.rs#L186, L2327).
- **Capy will lose the same classes:** 9 of its 25 blend modes, most of its filters, variable width and stroke alignment (which need outlining), conic and diamond gradients, and backdrop effects. This is inherent to SVG.

## 4. Item kinds mapped to the planned Capy kinds

| External kind | Capy representation |
|---|---|
| Vector or path kinds | Path. Networks become a Group of Paths |
| Rectangle, ellipse with arc, polygon, star, line | Path result plus a parametric recipe with its frame |
| Boolean op, shapeGroup, `:bool`, Lunacy SHAPE | Path result plus a boolean recipe holding the operand subtree (Sketch sets the op per operand) |
| Group, frame (background plus clip), section | Group |
| TRANSFORM_GROUP | Group of the copies plus a repeat recipe |
| Component, component set, slot, symbol master | document-level definition (not in the plan yet) |
| Instance, symbolInstance | Group of the result plus an instance recipe |
| Text, TEXT_PATH | Text |
| Bitmap, image fill | Image item, or an image or pattern paint |
| `svg-raw`, slices, connectors, FigJam and Slides nodes | out of scope |

## 5. Gaps vs the planned Capy model

### (a) Structural: decide these in the MVP envelope

- **A1. Subtree extent independent of kind** in the pre-order stream (a child count or byte length per record). Without it, an MVP reader cannot skip an unknown container and keep its known siblings.
- **A2. Recipes attach to any item, and a recipe's result is ordinary items** (a run of siblings or a Group). Later kinds then ship as known kinds plus a recipe, and older readers render the result instead of falling back to Preserved. Decide which kinds an MVP result may use.
- **A3. Fill and stroke entries and their paint are tagged open records** that follow the item property rules for unknown and ignorable keys. Every tool adds keys here. This is satisfied already if the plan encodes them with tagged keys.
- **A4. Ignorable properties that depend on geometry carry a fingerprint like recipes**, or are dropped when the body is edited. Examples are weld tables and an authored source color.

### (b) Additive later: each gap and its slot

| Gap | Seen in | Slot |
|---|---|---|
| Item `hidden`, `name`, `locked` | all tools | item tags. `hidden` cannot be ignorable. **Worth adding in MVP** |
| Paint opacity, blend, enabled | Figma, Sketch, Lunacy | paint-entry keys, non-ignorable |
| Gradients: linear, radial, conic, diamond; 2×3 affine; spread; interpolation space with hue direction; stepped or smooth interpolation; midpoints | Figma, Sketch, Graphite | paint kind `gradient` with open stop records |
| Image paint: fill, fit, crop or tile; transform; rotation; adjustments | Figma, Sketch, Lunacy, Penpot | paint kind `image` (adjustments reuse Capy filter IDs) |
| Content pattern: definition reference, square or hex tiles, spacing | Figma PATTERN, Graphite | paint kind `pattern` plus a document-level definition record |
| Mesh gradient, noise fill | Sketch; Figma via shaders | paint kinds |
| Swatch binding on a color (gradient stops, shadows) | Sketch, Figma, Penpot | color-value key plus a document swatch record |
| Authored color model (OKLCH and others) | Figma update 141 | ignorable color key with a fingerprint |
| Darker Color, Lighter Color, Plus Darker, Plus Lighter | Graphite and Photoshop; Sketch, Lunacy, Apple | blend enum values. Import can approximate Plus Lighter as Add and Plus Darker as Linear Burn |
| Stroke align, defined as doubled width clipped to the fill | Figma, Penpot, Graphite | stroke key `align` |
| Dash plus offset | all tools | stroke keys |
| Separate start and end caps and markers | Figma, Sketch, Penpot, Lunacy, Linearity | stroke keys, one per end |
| Smooth and asymmetric width profiles; import of Figma's position-along-length points | Figma, Illustrator [K] | width-profile keys; arc-length inversion to (segment, t) on import (smooth interpolation in Figma unverified) |
| Art brushes (stretch, scatter) and calligraphic nibs | Figma Draw, Linearity | stroke key `brush` referencing a definition. Distinct from raster-stamp brush paths |
| Dynamic strokes, offset path | Figma | Path recipe |
| Non-scaling or transformed pen | Graphite, SVG | define whether a group affine transforms strokes; otherwise a `pen_transform` stroke key |
| Live corners: radius, style (round, inverted, chamfer, square), smoothing | Figma, Sketch, Lunacy, Linearity | per-anchor recipe |
| Parametric shapes | Figma, Sketch, Penpot | recipe that stores the frame affine |
| Live boolean ops | Figma, Sketch (op per operand), Penpot | recipe holding an item subtree (needs A2) |
| Repeat (linear or radial) | Figma | Group recipe |
| Text-on-path offset and side; warp | Figma, Sketch, Lunacy | Text keys and a warp recipe |
| Path opacity and blend without a wrapping Group | all tools | item tags (wrapping works today) |
| Clip to the content below, per item | Graphite, Photoshop | item tag |
| Fill opacity separate from opacity | Graphite, Photoshop | layer field and item tag, once shadows exist |
| Drop shadow (spread, blend, knock-out), inner shadow, glow | all tools | new built-in filters |
| Backdrop blur, glass, progressive blur | Figma, Sketch, Penpot, Lunacy, Linearity | an effect that reads the backdrop; a variable-radius parameter |
| Noise, texture | Figma, Linearity | filters (check against Capy's 54) |
| Group clip or mask child of any kind; pass-through plus mask; clip covers fill ∪ stroke | Figma, Penpot | Group rules |
| Instance: ID-path keys, property patches, swap, properties, variants, slots, scale, detach, materialized result | Figma, Sketch, Penpot, Lunacy | Group plus an instance recipe; document-level definitions |
| Text: Style per run, paragraph records, lists, auto-resize, vertical align, truncation, wrap style, decorations, OpenType features, axes, direction, baseline | Figma, Sketch, Penpot, Lunacy | run and paragraph keys; font resources name-only or embedded, with a fallback; glyph result |
| References across layers | Figma, Sketch | route through document-level definitions |
| Weld table for network editing | Figma, Graphite | ignorable Path property with a fingerprint |

### (c) Out of scope for a painting and illustration app

- **Layout:** auto layout, constraints, pins, sizing modes, grids and guides, and per-side stroke weights on frames.
- **Document structure:** sections, pages, multiple artboards, Sketch Graphics containers, slices and export settings.
- **Interaction and data:** prototyping, variables and expressions, tokens and themes, annotations and dev status.
- **Time and code:** Motion and Linearity Move animation, code layers, and custom shaders.
- **Video:** video paint.
- **Other products' nodes:** FigJam and Slides nodes.
- **Pass-through of foreign content:** raw SVG pass-through, and per-vertex joins on networks.
- **Procedural documents:** Graphite node graphs, which an importer bakes.
- **Collaboration ordering:** CRDT ordering.
- **Plugin data:** a single namespaced ignorable bag would do, if ever needed.

### Not verified in this pass

- I did not read Capy's current encoding for items and style entries, so A1 and A3 may already be met.
- Three Figma behaviours are unverified: joins at branch vertices, a second mask in one parent, and interpolation between width points.
- Whether Sketch draws the mask layer's own style is inferred from its docs.
- Linearity's internals are not public.
