# How other vector object models are built, and what to fix early (as of 2026-10-15)

[Object content extensions](../object-content-extensions-research.md) · source report, 2026-10-15

The "Proposal" row describes the first vector-layer design, which [section 2.3 of the main record](../object-content-extensions-research.md#23-what-the-first-vector-design-got-wrong) revised.

**Sources.** I read source code for Krita (master) and Graphite (commit `243a83ce`, 2026-10-15), plus the official Figma plugin and REST docs, the Affinity help, ISO 32000-1:2008 (the PDF spec) and the Clip Studio Paint (CSP) manual. Crate versions come from the crates.io API today. The repo's `docs/history/vector-layers-research/` already covers drawing features (CSP, Affinity, Fresco). This report covers only how each system structures its objects.

## 1. Krita (flake / KoShape)

**Storage and rendering.** Each vector layer is saved as `content.svg` inside the layer's `.shapelayer` entry in the .kra file ([kis_shape_layer.cc](https://github.com/KDE/krita/blob/master/libs/ui/flake/kis_shape_layer.cc), [kis_kra_save_visitor.cpp](https://github.com/KDE/krita/blob/master/plugins/impex/libkra/kis_kra_save_visitor.cpp)). Krita's own `SvgParser` turns it into a KoShape tree, and QPainter rasterizes the tree into the layer's pixels. Layer opacity, blend mode, layer styles and masks then apply to those pixels.

**Item kinds.**
- `KoPathShape`.
- Parametric `KoParameterShape` subclasses: rectangle, ellipse/arc/pie/chord, star and spiral ([pathshapes](https://github.com/KDE/krita/tree/master/plugins/flake/pathshapes)).
- `KoShapeGroup`.
- `KoSvgTextShape`, rewritten for 5.3/6.0 (Feb 2026). It adds on-canvas editing, SVG 2 inline-size wrapping, flow areas built by adding and subtracting shapes, textPath, and `paint-order` ([5.3 notes](https://krita.org/en/release-notes/krita-5-3-release-notes/)).
- An image shape.

**Per-shape properties** ([KoShape.h](https://github.com/KDE/krita/blob/master/libs/flake/KoShape.h)):
- A retained `QTransform`: a full affine, including skew.
- One background: color, gradient, raster pattern, vector pattern, mesh gradient or hatch.
- One stroke (`KoShapeStroke`): width, cap, join, miter limit, dashes, and a QBrush, so strokes can be gradients.
- Paint order, opacity, a `KoClipPath` and a luminance `KoClipMask`.
- SVG-style inherit flags for fill, stroke and paint order, and markers.
- No per-shape blend mode and no filters.

**Path points** ([KoPathPoint.h](https://github.com/KDE/krita/blob/master/libs/flake/KoPathPoint.h)). Each point stores an absolute position and absolute control points, plus `StartSubpath`/`StopSubpath`/`CloseSubpath`/`IsSmooth`/`IsSymmetric` flags. That is effectively your Corner/Smooth/Symmetric anchor.

**Parametric shapes keep an evaluated fallback.** `EllipseShape::saveSvg` writes `<circle>` or `<ellipse>` when it can. Otherwise it writes `<path sodipodi:type="arc" sodipodi:cx=… sodipodi:start=… d="…">`, which is Inkscape's convention of parameters plus an evaluated `d` ([EllipseShape.cpp](https://github.com/KDE/krita/blob/master/plugins/flake/pathshapes/ellipse/EllipseShape.cpp)). Editing nodes turns the shape into a plain path.

**SVG coverage** ([SvgParser.cpp](https://github.com/KDE/krita/blob/master/libs/flake/svg/SvgParser.cpp)).
- Supported: svg, g, a, symbol, switch, defs, linear and radial gradients, bicubic meshgradient, pattern, clipPath, mask, marker, CSS `style`, text/tspan/textPath, the basic shapes, path, image and use.
- `<filter>` falls into a `// not supported!` branch.
- `<use>` is resolved by parsing the referenced element again into a new group, so instances become copies and the link is lost on save. `use` width/height is still a TODO.
- No `mix-blend-mode` and no `isolation`.

The manual says "Only the SVG 1.1 features are supported, with exception of smaller features like the mesh gradients" ([manual](https://docs.krita.org/en/user_manual/vector_graphics.html)).

**Group opacity may not be isolated (uncertain, from reading [KoShapeManager.cpp](https://github.com/KDE/krita/blob/master/libs/flake/KoShapeManager.cpp)).** A shape's opacity goes through `QPainter::setOpacity` before its children paint. Group opacity therefore seems to apply to each child's draw, not to an isolated group buffer. Clip masks do render offscreen.

**Limits users hit.**
- No SVG filters. The 4.0 notes promised them for 4.1 ([4.0 notes](https://krita.org/en/posts/2018/krita-4-0-release-notes/)), and the feature request is still open ([bug 403280](https://bugs.kde.org/show_bug.cgi?id=403280)).
- Perspective, warp, cage and liquify transforms don't work on vector layers ([bug 397065](https://bugs.kde.org/show_bug.cgi?id=397065)).
- Booleans are destructive.
- The calligraphy tool produces filled outlines; there is no variable-width stroke.
- No per-shape blending.

Krita is GPL-3.0, so read its code but don't copy it.

## 2. Affinity (Designer 2 to the unified Affinity 3)

Canva's free unified app launched at the end of October 2025, and version 3.3 shipped in September 2026. It merges Designer, Photo and Publisher into Vector, Pixel and Layout studios and saves a proprietary binary `.af`. The help is still the Designer 2 help, and the object model looks unchanged. Version 3.3 added live Blend groups, a Diffusion gradient and pigment blending for mesh fills ([CG Channel](https://www.cgchannel.com/2026/09/canva-releases-affinity-3-3/)).

**Item kinds.** Everything is one tree whose "layers" are objects ([About layers](https://affinity.help/designer2/en-US.lproj/pages/Layers/aboutLayers.html)): Artboard, Vector and Pixel containers, parametric Shape, Curve, Compound, four text kinds, Group, Mask, Image, Linked/Embedded document, Adjustment and Symbol. Live filter layers ([Photo 2](https://affinity.help/photo2/en-US.lproj/pages/Layers/livefilters.html)), Warp groups and Blend groups add to these.

**Paint.** The Appearance panel holds an ordered stack of fills and strokes. Each entry has its own color or gradient, width (for strokes), blend mode, opacity and visibility. The Stroke panel's Order button places a stroke in front of or behind the fills ([Appearance](https://affinity.help/designer2/en-US.lproj/pages/Panels/appearancePanel.html), [multiple strokes/fills](https://affinity.help/designer2/en-US.lproj/pages/CurvesShapes/multiStrokesAndFills.html)).
- Strokes also have alignment, dashes, arrowheads and a saved pressure profile, which is a width curve along the stroke ([Pressure](https://affinity.help/designer2/en-US.lproj/pages/CurvesShapes/pressure.html)).
- Transparency exists at four levels: paint opacity, a separate transparency gradient on a fill or stroke, layer opacity, and effects ([Transparency](https://affinity.help/designer2/en-US.lproj/pages/Clr/transparency.html)).
- Each object also has a blend mode, blend ranges and Layer FX.

**Transforms are retained (inferred).** Shear can be undone, and the Transform panel can force or lock scaling of stroke widths, effect radii and corner radii ("Scale with object") ([Transform](https://affinity.help/designer2/en-US.lproj/pages/ObjectControl/transform.html)). SVG export has a "Flatten transforms" option; with it off, "objects remain dynamically transformed." The `.af` format is not public, so this is inferred from behaviour.

**Clips, masks and adjustments work by nesting.**
- Dropping one object onto another makes it a child clipped by the parent's outline. Vector or pixel objects can be parent or child, and "Lock Children" resizes the parent without scaling them ([Clipping](https://affinity.help/designer2/en-US.lproj/pages/Layers/layerClip.html)).
- Mask layers, pixel or vector with grey meaning partial, affect the siblings below them in the same parent, or a single object when nested into it. They have their own blend mode ([Masking](https://affinity.help/designer2/en-US.lproj/pages/Layers/LayerMasks.html)).
- An adjustment or live filter nested inside an object applies only to that object ([Adjustments](https://affinity.help/designer2/en-US.lproj/pages/Layers/adjustmentLayers.html)).

**Live content.**
- Compounds are non-destructive booleans. Each child has its own mode (add, subtract, intersect or xor), and the compound takes the lowest child's style ([Compounds](https://affinity.help/designer2/en-US.lproj/pages/ObjectControl/compound.html)).
- Shapes keep their parameters until Convert to Curves.
- Warp groups are nestable, non-destructive mesh warps ([Warp](https://affinity.help/designer2/en-US.lproj/pages/ObjectControl/warp.html)).
- Constraints anchor and scale children relative to their container ([Constraints](https://affinity.help/designer2/en-US.lproj/pages/DesignAids/constraints.html)).

**Symbols** are linked instances with a Sync toggle. When sync is off, an edited attribute (dimension, fill, effect, font) becomes local while the rest stays linked. Adjustments and warps can be symbols too ([Symbols](https://affinity.help/designer2/en-US.lproj/pages/SymbolsAssets/symbols.html)).

**SVG export** offers rasterize Nothing / Everything / Unsupported properties at a chosen DPI, plus text as curves, flatten transforms, viewBox and relative coordinates ([Export settings](https://affinity.help/designer2/en-US.lproj/pages/ExportPersona/exportSettings.html)).

## 3. Figma

**Item kinds.** A node tree: Frame, Group, Section, Component, ComponentSet, Instance, BooleanOperation, Vector, Rectangle, Ellipse, Polygon, Star, Line and Text, plus TextPath and TransformGroup in beta. There is no image node; images are a paint.

**Paints and strokes.** Nodes have `fills: Paint[]` and `strokes: Paint[]`. A Paint is Solid, Gradient (linear, radial, angular or diamond, with a `gradientTransform`), Image, Video, Pattern (beta) or Shader (beta), and each paint has visible, opacity and blendMode ([Paint](https://developers.figma.com/docs/plugins/api/Paint/)).
- **Stroke geometry is single per node:** one weight (or per-side weights), `strokeAlign` CENTER/INSIDE/OUTSIDE, cap, join, miter limit and dashes. All stroke paints share it.
- `variableWidthStrokeProperties` holds either a preset or custom points of {position 0–1 along the stroke, width as a fraction of the weight}. It "cannot be applied to… vector networks with branching paths" ([VW strokes](https://developers.figma.com/docs/plugins/api/properties/nodes-variablewidthstrokeproperties/)).
- `complexStrokeProperties` holds stretch and scatter brushes ([ComplexStrokeProperties](https://developers.figma.com/docs/plugins/api/ComplexStrokeProperties/)).
- `effects: Effect[]` covers shadows, layer, background and progressive blurs, noise, texture and glass ([Effect](https://developers.figma.com/docs/plugins/api/Effect/)).

**Compositing per node.** Opacity; blendMode, including `PASS_THROUGH` for groups; and `isMask` with `maskType` ALPHA, VECTOR or LUMINANCE. "A mask node masks its subsequent siblings," and the docs warn this can hide more than intended ([isMask](https://developers.figma.com/docs/plugins/api/properties/nodes-ismask/)).

**Transforms.** `relativeTransform` is retained, but its axes are always unit length. Scale is never stored in the matrix; resizing rewrites width and height or the geometry ([relativeTransform](https://developers.figma.com/docs/plugins/api/properties/nodes-relativetransform/)).

**Geometry.** A `vectorNetwork` has three parts ([VectorNetwork](https://developers.figma.com/docs/plugins/api/VectorNetwork/)):
- vertices, each with an optional per-vertex cap, join, corner radius and handle mirroring;
- segments, given as start and end indices with tangents;
- regions, each with a winding rule, loops of segment indices and optional per-region fills.

The REST API returns evaluated `fillGeometry` and `strokeGeometry`, plus a `fillOverrideTable` for per-region fills ([REST](https://developers.figma.com/docs/rest-api/file-node-types/)).

**Live content.** Shape nodes are parametric. BooleanOperationNode (UNION, INTERSECT, SUBTRACT, EXCLUDE) keeps its children live. TransformGroupNode carries `transformModifiers`, currently only linear and radial REPEAT ([TransformModifier](https://developers.figma.com/docs/plugins/api/TransformModifier/)). Flatten bakes all of it.

**Components.** Instances carry `mainComponent`, `componentProperties` and `overrides: [{id, overriddenFields}]`. `swapComponent` keeps overrides "using the same heuristics" as the editor ([InstanceNode](https://developers.figma.com/docs/plugins/api/InstanceNode/)).

**SVG export.** SVG has only center strokes. The old exporter drew inside strokes at double width under a mask; the current one moves the path so a center stroke matches ([Figma blog](https://www.figma.com/blog/with-figmas-new-svg-exports-less-more/)). How branching networks and per-vertex caps export isn't documented (unverified).

## 4. Graphite (Rust, Apache-2.0)

**Structure.** The document is a node graph. Layers are nodes whose outputs are `Graphic` values: Item or List of Vector, Raster (CPU or GPU), Color, Gradient, Text or a nested Graphic ([graphic.rs](https://github.com/GraphiteEditor/Graphite/blob/243a83ce1743809e124aca8d4c0b08a3b2ddb127/node-graph/libraries/graphic-types/src/graphic.rs)). The model has been reworked repeatedly in 2026:
- "Table" renamed to "List".
- Item and List made universal ([#4335](https://github.com/GraphiteEditor/Graphite/pull/4335)).
- PathStyle, Fill and Gradient deleted ([#4297](https://github.com/GraphiteEditor/Graphite/pull/4297)).
- Cascading appearance added ([#4433](https://github.com/GraphiteEditor/Graphite/pull/4433)).
- The region domain deleted ([#4463](https://github.com/GraphiteEditor/Graphite/pull/4463)).

**Vector** ([vector_types.rs](https://github.com/GraphiteEditor/Graphite/blob/243a83ce1743809e124aca8d4c0b08a3b2ddb127/node-graph/libraries/vector-types/src/vector/vector_types.rs), [vector_attributes.rs](https://github.com/GraphiteEditor/Graphite/blob/243a83ce1743809e124aca8d4c0b08a3b2ddb127/node-graph/libraries/vector-types/src/vector/vector_attributes.rs)):
- `point_domain`: stable PointIds and positions.
- `segment_domain`: SegmentIds, start and end point indices, and handles that are Linear, Quadratic or Cubic.
- `colinear_manipulators`: handle pairs kept in line (G1).

Segments may branch, and fills for branching shapes are computed from faces. The region domain and its FillId and StrokeId were deleted as dead code, and #4463 "regresses region determination for mesh geometry."

**Item attributes** ([list.rs](https://github.com/GraphiteEditor/Graphite/blob/243a83ce1743809e124aca8d4c0b08a3b2ddb127/node-graph/libraries/core-types/src/list.rs)): transform (composed through nested groups), blend_mode, opacity, `opacity_fill` ("does not affect content clipped to the item"), `clipping_mask` (inherits the alpha of the content beneath) and appearance.

**Styles.** `Appearance(List<Coverage>)` is an ordered list of paint passes ([appearance.rs](https://github.com/GraphiteEditor/Graphite/blob/243a83ce1743809e124aca8d4c0b08a3b2ddb127/node-graph/libraries/graphic-types/src/appearance.rs)). Each pass is Fill or Stroke with its own parameters (weight, dash, cap, join, miter limit, alignment and a stroke **transform**) plus a paint. #4433 makes a parent's appearance cascade to children that have none, which "opens the door to supporting multiple fills and strokes that was impossible until now." Issue [#2003](https://github.com/GraphiteEditor/Graphite/issues/2003) explains the stroke transform: nodes bake transforms into points, so the stroke keeps its own authoring space and isn't distorted.

**Live content.** Shapes and booleans are nodes. The Path node stores a `VectorModification`: inserts, removals, handle edits, deltas and G1 changes keyed by stable IDs, applied on top of the upstream geometry ([vector_modification.rs](https://github.com/GraphiteEditor/Graphite/blob/243a83ce1743809e124aca8d4c0b08a3b2ddb127/node-graph/libraries/vector-types/src/vector/vector_modification.rs)). Graphite stores only the source; the evaluated result is always recomputed.

## 5. PDF imaging model and Illustrator

In [ISO 32000-1](https://opensource.adobe.com/dc-acrobat-sdk-docs/pdfstandards/PDF32000_2008.pdf), content is a sequence of paint operations, not a retained tree.
- **Graphics state:** the current transform (CTM), the clip path, separate fill and stroke alpha (`ca`/`CA`), blend mode `BM` and the soft mask. Form XObjects with a matrix give reuse.
- **Transparency groups (§11.4):** `/Group /S /Transparency`. `/I` (isolated) composites the group against a transparent backdrop. `/K` (knockout) composites each element against the group's initial backdrop instead of the elements before it.
- **Soft masks (§11.5, §11.6.5.2):** `/SMask` with `/S /Alpha` or `/Luminosity`, a group `/G`, a backdrop `/BC` and a transfer function `/TR`.
- **Patterns (§8.7.3–8.7.4):** coloured or uncoloured tiling patterns, and shading patterns.
- **Shading types (§8.7.4.5):** 1 function-based, 2 axial, 3 radial, 4 free-form Gouraud triangle mesh, 5 lattice-form triangle mesh, 6 Coons patch mesh and 7 tensor-product patch mesh.
- **Optional content (§8.11):** optional content groups (OCGs) with membership dictionaries and configurations; this is how layers survive in PDF.

Illustrator `.ai` files "use both PDF and PGF formats" ([Adobe](https://helpx.adobe.com/illustrator/kb/optimize-native-pdf-file-sizes.html)). The native data holds the appearance model (multiple fills and strokes, live effects), and the PDF is the evaluated fallback that other apps read.

## 6. Clip Studio Paint (short)

**Data model.** A vector layer holds lines. Each line is a centerline of control points (straight, spline, quadratic or cubic) with per-point width and opacity, set by the Control Point tool's "Adjust line width" and "Adjust opacity" modes. Each line also stores its brush (tip, size, color, anti-aliasing, taper), which the raster brush engine re-renders ([manual](https://help.clip-studio.com/en-us/manual_en/180_layers/Vector_layers.htm)).
- **No fills:** the Fill, Gradient and Blend tools can't be used on vector layers.
- **Transforms are baked**, with a toggle for whether scaling also scales line thickness.
- **SVG round-trip keeps only shape:** "only the line shape is recorded, so the color, brush tip shape, and line thickness cannot be transferred." Pressure and taper are dropped ([tips 3714](https://tips.clip-studio.com/en-us/articles/3714)).

## 7. Rust crates (crates.io, today)

| Crate | Version | License | Role |
|---|---|---|---|
| kurbo | 0.13.1 | Apache-2.0 OR MIT | BezPath, Affine, arc length, stroke expansion, offsets |
| usvg | 0.48.1 | Apache-2.0 OR MIT | Turns SVG into a simplified tree. Groups carry transform, opacity, blend, isolate, clip, mask and filters. Paths have one fill, one stroke, paint-order and **no transform of their own**. Text is shaped with fontdb, harfrust and skrifa |
| resvg | 0.48.1 | Apache-2.0 OR MIT | CPU reference renderer, a test oracle (tiny-skia 0.12, BSD-3-Clause) |
| svgtypes / roxmltree | 0.16.1 / 0.21.1 | Apache-2.0 OR MIT / MIT OR Apache-2.0 | Attribute and path parsing; read-only XML |
| i_overlay | 9.1.0 | MIT OR Apache-2.0 | Polygon booleans on flattened input; curves need refitting |
| linesweeper / flo_curves | 0.5.0 / 0.8.1 | MIT OR Apache-2.0 / Apache-2.0 | Sweep-line booleans (Linebender) / booleans on Béziers |
| lyon | 1.0.19 | MIT OR Apache-2.0 | Fill and stroke tessellation |
| vello (+cpu, hybrid) | 0.11.0 (0.3.0, 0.2.0) | Apache-2.0 OR MIT | GPU compute renderer; paints come from peniko 0.6.1 |
| parley | 0.12.0 | Apache-2.0 OR MIT | Text layout, now built on harfrust, skrifa, fontique and ICU |
| harfrust | 0.14.0 | MIT | HarfBuzz port; rustybuzz 0.20.1 (MIT) has not been released since Nov 2024 |
| skrifa / swash | 0.48.0 / 0.2.10 | MIT OR Apache-2.0 / Apache-2.0 OR MIT | Font loading and outlines |
| krilla (+svg) | 0.8.2 (0.8.1) | MIT OR Apache-2.0 | PDF writer, and SVG to PDF |
| hayro (+svg) | 0.8.0 | Apache-2.0 OR MIT | PDF reader and renderer, and PDF to SVG |

None of these crates is GPL. Krita (GPL-3.0) and Inkscape (GPL) are reference only.

## Cross-system comparison

| | Tree | Paints | Transforms | Live content | Instancing | Clip/mask | Per-item compositing |
|---|---|---|---|---|---|---|---|
| Krita | Groups and shapes | 1 fill + 1 stroke, paint-order, inheritance | Retained affine | Parameters + `d`; destructive booleans | `<use>` turned into copies | Attached clipPath + luminance mask | Opacity only |
| Affinity | One object tree | Ordered fill/stroke list, each with blend and opacity | Retained (inferred) | Shapes, Compound modes, Warp/Blend groups | Symbols, sync per attribute | Child nested in parent is clipped; mask layers | Opacity, blend, FX, nested adjustments |
| Figma | Node tree | Paint lists, one shared stroke geometry | Rotation retained, scale baked | Shape and Boolean nodes, Repeat; REST returns evaluated geometry | Components, overrides by node ID | Mask masks following siblings | Opacity, blend (pass-through), effects[] |
| Graphite | Graph producing nested Lists | Ordered passes, cascading | Retained, plus stroke space | All nodes; path edits stored as ID-keyed changes | Graph nodes | Clip to content below | opacity, opacity_fill, blend |
| PDF/AI | Content stream + groups | Fill/stroke per operation; AI native data on top | CTM, XObject matrix | AI native data + PDF fallback | Form XObjects | Clip path + SMask | ca/CA, BM, isolated/knockout |
| CSP | Flat lines | Brush per line, per-point width/opacity | Baked + width toggle | None | None | Layer only | Layer only |
| Proposal | Layer → object → paths | Optional fill + stroke, layer table | Baked | — | — | Layer masks | Layer only |

## Structural lessons for your model

1. **Use one item tree with stable IDs, and avoid a second grouping concept.** A VectorObject that holds an ordered list of styled paths is already a group. Adding Group later leaves two nesting mechanisms with different selection, undo and compositing rules. Every system with clips, compounds, warps or instances uses a single tree. Make the layer hold a root group with Path as one item kind. Give items, and ideally anchors (as Graphite does), stable IDs so overrides, width points and edits have something to attach to.

2. **Keep a transform slot on items even if paths bake.** Text, images and instances can't bake into glyph or shared geometry. Figma retains rotation and bakes scale. Baking also needs rules for everything that isn't a point:
   - stroke width under scaling: a toggle in CSP, "Scale with object" in Affinity;
   - strokes under skew or non-uniform scale, which no single width can represent: Graphite keeps a per-stroke transform, otherwise you outline;
   - gradient and pattern coordinates, dashes and width profiles.

   SVG import hits all of these when it bakes usvg's `abs_transform`.

3. **Store styles as an ordered list of paint passes, not an optional fill plus an optional stroke.** Affinity, Figma, Illustrator and now Graphite all use lists. The SVG and Krita model is the special case with two passes. Give each stroke pass its own width, alignment, cap, join, miter limit, dashes and width profile, as Affinity and Graphite do, rather than Figma's shared stroke geometry. Decide now whether the fill rule lives on the path or on the fill pass.

4. **Make Paint a tagged enum with its own coordinate space.** Start with Solid, then add linear, radial and conic gradients, mesh (PDF types 6/7, SVG 2, Krita, Affinity), pattern and image. Non-solid paints need an object-space transform that updates when geometry bakes. SVG `objectBoundingBox` units should be resolved at import. Per-pass opacity and blend are cheap to reserve.

5. **Spell out what the style table means.** A shared entry can be a linked style (Figma style IDs), storage dedupe with copy-on-write, or inheritance (the SVG cascade, Krita's inherit flags, Graphite's cascading appearance). Each behaves differently for edits, undo and moving paths between layers. If styles are linked, scope the table to the document.

6. **Give containers PDF transparency-group semantics.** Reserve per-item opacity and blend; Krita's lack of per-shape blending is a known gap. Groups need an explicit isolated or pass-through mode (Figma PASS_THROUGH, usvg `isolate`, PDF `/I`); knockout is optional. Krita's opacity applied per draw (if that reading is right) shows why this must be defined as behaviour, not left to the renderer. On the GPU, isolation and masks cost offscreen layers, so default to pass-through.

7. **Pick one canonical clip/mask structure.** Four exist in the wild: an attached reference (SVG, Krita), a parent that clips nested children (Affinity), a mask that applies to the siblings after it (Figma), and clip-to-content-below (Graphite, Photoshop). A container with a mask slot of {mode: geometric clip | alpha | luminance, source items} round-trips to SVG clipPath/mask and PDF clip/SMask. Avoid semantics that depend on sibling position.

8. **Store live content as parameters, evaluate it deterministically, and treat any evaluated copy as disposable.** Krita (`sodipodi:arc` + `d`), Inkscape live path effects (`inkscape:original-d` + effect + `d`, [wiki](https://wiki.inkscape.org/wiki/LivePathEffects_Discussion)), Illustrator (native data + PDF) and the Figma REST API all keep both source and result. Your package contract allows only authored data, with a "disposable optional representation" for outputs. So store generator parameters (shape kind, boolean children, warp mesh) as versioned built-ins, like effects. Keep evaluated geometry as a cache or produce it only on export.

9. **Choose how width profiles are parameterized.** CSP stores per-point factors, which need resampling whenever points are simplified or inserted. Figma stores (position 0–1, fraction of the weight), and Affinity stores a curve along the stroke. Keep the profile on the stroke pass, per subpath, and independent of the anchor count. SVG and PDF export must outline it; CSP shows what is lost otherwise.

10. **Keep compound paths and skip vector networks.** Figma's networks rule out variable width on branching geometry and must be split up for SVG. Graphite deleted its region domain and lost fill accuracy on mesh geometry. Subpaths with SVG semantics match SVG, PDF, Krita, usvg and kurbo.

11. **Make room for item effects.** SVG filters, Figma `effects[]` and Affinity FX attach to items, while Krita and CSP only allow effects at layer level, and Krita's SVG filter gap has been open since 4.0. Either reserve an effects list on items and groups, or define SVG filter import as wrapping the item in a layer with effects. Deciding this now decides whether SVG import can be lossless.

12. **Instancing needs a definitions table, retained transforms and override addressing.** SVG use/symbol, PDF form XObjects, Affinity symbols and Figma components all have three parts: a document-level definition, an instance item with its own transform, and overrides keyed by stable IDs. Krita turns `<use>` into copies and loses the link on save, which is the failure to avoid.

## Uncertain or unverified

- Krita's group opacity being applied per draw comes from reading the source, not from a test.
- Affinity's retained transforms are inferred from behaviour; the `.af` format is not public.
- How Figma exports branching vector networks to SVG.
- Whether usvg supports mesh gradients (I believe not).
- How linesweeper handles curves.
- Whether CSP behaviour after 1.10 still matches the tips article.
- Graphite changes monthly; everything above is as of commit `243a83ce`.
