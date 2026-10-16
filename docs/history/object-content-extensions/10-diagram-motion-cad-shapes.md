# Shapes and objects outside classic vector editors: inventory and gaps for the Capy vector object

[Object content extensions](../object-content-extensions-research.md) · source report, 2026-10-16

Checked against the vector design at `726cb6cdc`, or the draft before it in which recipes were separate records keyed by item ID. The design this audit produced is in [vector layers §7.2–7.4](../vector-layers-research.md#72-layer-model).

Research date 2026-10-16. Scope: office and diagram formats (DrawingML, ODF, Google Slides, Keynote), whiteboards (Excalidraw, tldraw), motion (Lottie, After Effects, Rive), CAD and maker work (DXF, SVG conventions for cutters and lasers, Figma arcs), PDF and print features, and other object kinds found in illustration documents. Each is checked against the planned Capy vector object: items in a pre-order stream, a cubic-only Path, an ordered style list per path, Group/Image/Text/Instance later, and stored result geometry plus a separate recipe record keyed by item ID.

How claims are marked: a URL after a claim is its source. **[K]** means general knowledge I did not re-check for this report. **(unverified)** means a source hinted at it but I could not confirm it.

Source code I read directly (shallow clones, treated as data):
- Excalidraw `4c37956` (2026-10-14): https://github.com/excalidraw/excalidraw/blob/4c37956ba5f04ec342a9d9d5891a38f22c5b7b4e/packages/element/src/types.ts and https://github.com/excalidraw/excalidraw/blob/4c37956ba5f04ec342a9d9d5891a38f22c5b7b4e/packages/excalidraw/data/restore.ts
- tldraw `dd11653` (2026-10-16): https://github.com/tldraw/tldraw/tree/dd116535b138520d560c9881ad095ab782dc74c7/packages/tlschema/src and https://github.com/tldraw/tldraw/blob/dd116535b138520d560c9881ad095ab782dc74c7/packages/store/src/lib/StoreSchema.ts
- Lottie spec `4b55957` (tags 1.0 and 1.0.1): https://github.com/lottie/lottie-spec/tree/4b55957472f8718e34e9c1298f0e8f021b6c597f, published at https://lottie.github.io/lottie-spec/
- Rive runtime `269ebca` (2026-10-16): https://github.com/rive-app/rive-runtime/blob/269ebcab3016e68340bb1427b720712213066eaf/src/file.cpp and https://github.com/rive-app/rive-runtime/blob/269ebcab3016e68340bb1427b720712213066eaf/include/rive/runtime_header.hpp
- ECMA-376 `presetShapeDefinitions.xml` (the annex file from Part 1, as mirrored by LibreOffice): https://github.com/LibreOffice/core/blob/master/oox/source/drawingml/customshapes/presetShapeDefinitions.xml. I parsed it myself; the counts below come from that parse.

---

## 0. Short answers

1. **Path segments.** Cubic-only is safe for the MVP. Lottie and Rive are cubic-only at runtime, and quadratics convert to cubics exactly. Exact arcs, circles and NURBS matter only for CAD round trips, and a recipe handles those better than a new segment kind would. Reserve a segment-kind field so the path body *can* grow, but plan to add exact curves as recipes. With a recipe, older readers keep rendering and editing. With a new segment kind, every older reader opens the file as Preserved.
2. **After Effects/Lottie paint operators and DrawingML child space.** Neither forces a structural change. Lottie's rule renders each (shape, style) pair separately. Capy can express that by baking each pair into its own Path, duplicating geometry when one style covers paths in several scopes. DrawingML `chOff/chExt` is a scale plus a translation, so it folds into a Group's F64 affine. PowerPoint does not render rotated children of non-uniformly scaled groups as a pure affine, so an importer bakes the result. One rule must be fixed before Group ships, because it cannot change afterwards: **which space stroke width, dashes and effect radii are measured in under a group affine.**
3. **Bindings.** Every format binds by an ID that is unique in some scope, plus an anchor (a site index or a normalized point). Every format except tldraw also stores the routed result. Layer-local u64 item IDs are enough *inside one vector object*. Across layers they are not: a reference needs both the object and the item. The package contract forbids embedding Capy object IDs inside binary resources, so the object reference has to go through a visible slot in the object record. Reserve that slot, or rule out cross-layer bindings, before freezing the format.

---

## 1. Office and diagram formats

### 1.1 DrawingML preset shapes (`prstGeom`)

- A file stores a preset shape only as a name plus adjust values, for example `<a:prstGeom prst="roundRect"><a:avLst><a:gd name="adj" fmla="val 16667"/></a:avLst></a:prstGeom>` [K]. The geometry lives in the reader's copy of `presetShapeDefinitions.xml`, so a reader without that table can't draw the shape. Google Slides uses the same model: its `Shape.shapeType` enum maps onto ECMA-376 `ST_ShapeType`, and 135 of its entries name the matching ECMA shape. https://developers.google.com/workspace/slides/api/reference/rest/v1/presentations.pages/shapes
- What the table contains (from my parse):

  | Item | Count |
  | --- | --- |
  | Preset shapes | 187 |
  | Shapes with adjust values (`avLst`) | 123 |
  | Shapes with handle lists (`ahLst`) | 120 (220 `ahXY` and 23 `ahPolar` handles) |
  | Shapes with connection sites (`cxnLst`) | 174 |
  | Shapes with a text rectangle (`rect`) | 182 |
  | **Shapes with more than one path** | **63** |

  - Path commands across all 320 paths: `moveTo` 446, `lnTo` 1695, `arcTo` 393, `close` 320, `quadBezTo` 33, `cubicBezTo` 28.
  - Path fill modes: `norm` 190, `none` 96, `darkenLess` 17, `darken` 12, `lighten` 3, `lightenLess` 2.
  - 86 paths set `stroke="false"`; 148 set `extrusionOk="false"`.
  - The guide formulas use 17 operators: `val`, `*/` (1088 uses), `+-` (1542), `+/`, `?:`, `abs`, `at2`, `cat2`, `cos`, `sin`, `tan`, `max`, `min`, `mod`, `pin`, `sat2` and `sqrt`.
- Guides are evaluated in order, and a guide can only use guides defined before it. https://learn.microsoft.com/en-us/dotnet/api/documentformat.openxml.drawing.shapeguide?view=openxml-3.0.1
- An `ahXY` handle writes its x/y position into named guides, within min/max bounds. A handle without min/max on an axis can't move along it. https://learn.microsoft.com/en-us/dotnet/api/documentformat.openxml.drawing.adjusthandlexy?view=openxml-3.0.1
- Preset geometry is computed from the shape's box (`w`, `h`, `ss` = min(w, h)), not scaled from a fixed outline. When a `roundRect` is resized, its corner radius follows the short side instead of stretching [K, visible in the formulas]. **For Capy:** Capy paths bake their transforms, so a preset recipe must store its own frame and box size, and the result path is derived from the recipe.
- **One preset shape is often several paints over several paths.** In shapes like "can" and "cube", one path uses a `darken`/`lighten` fill (a shade of the shape's single fill), another has `fill="none"` and is only stroked, and some set `stroke="false"`. https://learn.microsoft.com/en-us/dotnet/api/documentformat.openxml.drawing.pathfillmodevalues?view=openxml-3.0.1 A Capy Path has one style for all its subpaths, so a preset's result is a **Group of Paths**, with the recipe on the Group.

### 1.2 Custom geometry (`custGeom`)

- `custGeom` holds `avLst`, `gdLst`, `ahLst`, `cxnLst`, `rect` (the text box, in guide terms) and `pathLst`. https://learn.microsoft.com/en-us/dotnet/api/documentformat.openxml.drawing.customgeometry?view=openxml-3.0.1
- Each `path` has its own coordinate system (`w`, `h`) that is stretched to the shape box. It also has `fill`, `stroke` (true/false) and `extrusionOk`. Later paths draw on top of earlier ones. The commands are `moveTo`, `lnTo`, `arcTo`, `quadBezTo`, `cubicBezTo` and `close`. https://learn.microsoft.com/en-us/dotnet/api/documentformat.openxml.drawing.path?view=openxml-3.0.1
- `arcTo` has no endpoint. It takes `wR`, `hR`, `stAng` and `swAng` (in 60000ths of a degree), and "this start angle is locked to the last known pen position". https://learn.microsoft.com/en-us/dotnet/api/documentformat.openxml.drawing.arcto?view=openxml-3.0.1 These elliptical arcs can all be approximated by cubics within tolerance [K].

### 1.3 Group shapes and the child coordinate space (`grpSp`, `chOff/chExt`)

- A group's `xfrm` has `off`, `ext`, `chOff`, `chExt`, `rot` (in 1/60000°), `flipH` and `flipV`. Children are laid out in the child-extents rectangle, which is then mapped onto off/ext. https://learn.microsoft.com/en-us/dotnet/api/documentformat.openxml.drawing.transformgroup?view=openxml-3.0.1 and https://learn.microsoft.com/en-us/dotnet/api/documentformat.openxml.drawing.childextents?view=openxml-3.0.1
  - The mapping is a non-uniform scale plus a translation, followed by rotation and flips about the centre. That is an affine, so it folds into a Group's F64 affine.
- Microsoft's implementation note: "In Office, the child extents and offset are used to determine the center for scaling, rotation and flipping." https://learn.microsoft.com/en-us/openspecs/office_standards/ms-oi29500/bb0ea491-3131-4a6a-adc8-03f173655076
- PowerPoint does not draw a rotated child inside a non-uniformly scaled group with the pure combined affine, which would skew it. LibreOffice bug 141953 reports that children rotated 45–135° and 225–315° are treated as if their width and height were swapped. https://www.libreoffice.org/bugzilla/show_bug.cgi?id=141953 (unverified: the page was blocked for me; this comes from a search summary.)
  - **For Capy:** an importer computes each child's final geometry and bakes it. Capy's Group affine is more general than DrawingML's, so import loses nothing. Exporting back cannot be exact.
- Office line widths are absolute EMU values on each shape, and I found nothing saying a group transform scales them [K, unverified]. This bears on the stroke-width decision in §7.

### 1.4 Connectors (`cxnSp`)

- `stCxn` and `endCxn` have an `id` (the target shape's drawing-element ID, unique within the part) and an `idx` (an index into the target's connection-site table, `cxnLst`). https://learn.microsoft.com/en-us/dotnet/api/documentformat.openxml.drawing.startconnection?view=openxml-3.0.1 and https://learn.microsoft.com/en-us/dotnet/api/documentformat.openxml.presentation.connectionshape?view=openxml-3.0.1
- The connector's own geometry is stored too: a preset (`straightConnector1`, `bentConnector2..5` or `curvedConnector2..5`, with adjust values) plus an xfrm. That is a **result plus binding** pair, and PowerPoint reroutes the connector when its target moves [K].
- Each connection site has a position *and* an angle (`ang`), the direction the connector leaves in when routing (for example `cxn ang="cd4"` in the preset file).
- Google Slides uses the same model: `connectedObjectId` plus `connectionSiteIndex`, which "in most cases ... corresponds to the predefined connection site index from the ECMA-376 standard". Groups, tables and lines can't be connected to. Line categories are STRAIGHT, BENT and CURVED. https://developers.google.com/workspace/slides/api/reference/rest/v1/presentations.pages/lines

### 1.5 Pictures: cropping and fill modes

- `srcRect` crops with left/top/right/bottom percentages. "A positive percentage specifies an inset, while a negative percentage specifies an outset." https://learn.microsoft.com/en-us/dotnet/api/documentformat.openxml.drawing.sourcerectangle?view=openxml-3.0.1
- An image fill either stretches (`stretch/fillRect`) or tiles (`tile` with `tx`, `ty`, `sx`, `sy`, `flip`, `algn`). https://learn.microsoft.com/en-us/dotnet/api/documentformat.openxml.drawing.tile?view=openxml-3.0.1
- A picture is itself a shape (`p:pic` with `spPr`), so it can have geometry, an outline and effects. Ordinary shapes can also be filled with a picture. https://learn.microsoft.com/en-us/dotnet/api/documentformat.openxml.drawing.blipfill?view=openxml-3.0.1
- **For Capy:** an image used as a paint (tiled or stretched inside any path) is a different thing from an Image item with a crop.

### 1.6 Effects and 3D

- An `effectDag` can contain: `outerShdw`, `innerShdw`, `prstShdw`, `glow`, `softEdge`, `reflection`, `blur`, `fillOverlay`, `alphaMod`, `duotone`, `clrRepl`, `hsl`, `lum`, `tint`, `xfrm`, `relOff`, `blend`, `cont` and `effect` (a reference). https://learn.microsoft.com/en-us/dotnet/api/documentformat.openxml.drawing.effectdag?view=openxml-3.0.1
  - `effectDag` is a graph (containers, blends and references). The usual `effectLst` is a fixed set [K].
- Effects belong to **individual shapes** (`spPr/effectLst`). Shadow parameters (`dist`, `dir`, `blurRad`, `sx/sy/kx/ky`, `rotWithShape`) are in the shape's own space [K].
  - **For Capy:** effects exist only on Groups, so there is no shadow or glow on a single item. Importers wrap each shape that has effects in a Group. When item effects arrive, Capy must define their coordinate space, for example whether a shadow offset rotates with the group.
- 3D: `sp3d` gives top and bottom bevels, extrusion, contour and material. https://learn.microsoft.com/en-us/dotnet/api/documentformat.openxml.drawing.shape3dtype?view=openxml-3.0.1 `scene3d` adds a camera and a light rig [K]. Both are out of scope.

### 1.7 Text in shapes, theme colors and color modifiers

- Any shape can hold text (`txBody`, with `bodyPr` insets, wrapping, vertical text and auto-fit). The text box comes from the geometry's `rect` guides [K]. Excalidraw (`containerId`) and tldraw (`richText` on geo, arrow and note shapes) also put labels inside shapes; see §2.
- `schemeClr` is "a color bound to a user's theme", and any color can carry an ordered list of modifiers. https://learn.microsoft.com/en-us/dotnet/api/documentformat.openxml.drawing.schemecolor?view=openxml-3.0.1
  - Example: `lumMod`, where "A 50% luminance modulate reduces the luminance by half". https://learn.microsoft.com/en-us/dotnet/api/documentformat.openxml.drawing.luminancemodulation?view=openxml-3.0.1
  - Others include `lumOff`, `tint`, `shade`, `alpha`, `satMod` and `hueOff` [K].
- **For Capy:** a later `swatch` paint needs an ordered modifier list, plus the stored resolved color as its result.

### 1.8 SmartArt, charts and tables

- **SmartArt** stores a data model and a layout definition. Since Office 2010 it also stores a drawing part, which "specifies the last successful layout of a diagram". https://learn.microsoft.com/en-us/openspecs/office_standards/ms-odrawxml/ec96c9e0-757a-4f24-9725-c52d3c34e310
  - A Microsoft engineer describes the drawing part as "a cache ... overwritten on any recomputation"; edits made directly to it don't persist. https://learn.microsoft.com/en-us/answers/questions/912190/is-rot-attribute-in-txxfrm-autogenerated-in-a-smar
  - This is a direct precedent for Capy's result plus recipe, with the opposite rule about which one wins: Office treats the recipe as authoritative, while Capy keeps the result authoritative and drops a stale recipe.
- **Charts** keep formula references to spreadsheet ranges plus cached values (`numCache`, `strCache`) [K]. That is another stored result next to its source.
- **Tables** are a `graphicFrame` holding an `a:tbl`, with a grid, merged cells (`gridSpan`, `rowSpan`, `hMerge`, `vMerge`), text and borders per cell, and a table style [K].

### 1.9 Markup Compatibility (ECMA-376 Part 3) as a precedent

- `mc:AlternateContent` holds one or more `mc:Choice Requires="ns"` elements and an optional `mc:Fallback`. Choices are evaluated in order, "like a switch statement". https://www.ericwhite.com/blog/?p=2936 (same source for the attributes below)
  - Unknown namespaces listed in `mc:Ignorable` are dropped, by default together with their whole subtree.
  - `mc:ProcessContent` keeps the children of an ignored element.
  - Under `mc:MustUnderstand`, "the processing should stop immediately".
  - The processing order (Ignorable/ProcessContent, then AlternateContent, then MustUnderstand) is listed in the ISO/IEC 29500-3 contents. https://webstore.ansi.org/preview-pages/ISO/preview_ISO+IEC+29500-3-2015.pdf
- Office uses this to ship new shape kinds with an old fallback: Word shapes with a VML fallback, the newer chart types, and 3D models. A 3D model is an `am3d:model3d` element with glTF/GLB data in a media part, plus a fallback picture in `mc:Fallback`. https://bugs.libreoffice.org/show_bug.cgi?id=134141
- How this maps to Capy:
  - Ignorable is Capy's "ignorable" property flag.
  - MustUnderstand is the default for unknown, non-ignorable data. Capy's answer, Preserved, is gentler than "stop".
  - AlternateContent/Fallback is a result plus a recipe, with the *result as the fallback*.
  - One difference: an MCE fallback can be a whole alternative tree, while in Capy the result is required artwork and the recipe is optional extra data.
- `extLst/ext uri="{GUID}"` lets any element carry vendor extensions that unaware readers skip [K]. This is the same idea as tldraw's `meta` and Excalidraw's `customData` bags.

### 1.10 ODF / LibreOffice `draw:custom-shape`

- `draw:enhanced-geometry` stores the whole parametric recipe in the file: `draw:enhanced-path`, `draw:modifiers` (adjust values), `draw:equation` (formulas), `draw:handle`, text areas, glue points, mirroring and extrusion attributes. https://docs.oasis-open.org/office/OpenDocument/v1.3/os/part3-schema/OpenDocument-v1.3-os-part3-schema.html (§19.145; the section numbers below refer to this document)
  - A parameter can be `$n` (an adjust value by index) or `?name` (an equation result).
  - Path commands:
    - `A`/`B`: arc from a bounding box and two radial vectors, counter-clockwise
    - `V`/`W`: clockwise arcs
    - `T`/`U`: ellipse arc by angles
    - `X`/`Y`: quarter ellipses
    - `C`, `Q`, `L`, `M`, `Z`: cubic, quadratic, line, move, close
    - `N`: end the current set of subpaths (filled even-odd)
    - `F`: no fill; `S`: no stroke
- `draw:type` on the enhanced geometry is just a shape name for the rendering engine and "shall not influence the geometry of the shape" (§19.229.3). Unlike DrawingML presets, ODF stores the recipe itself rather than a table reference. Neither format stores the resulting geometry.
- `draw:connector` stores both the binding and the result (§10.3.10, §19.217, §19.229.2):
  - binding: `draw:start-shape`/`draw:end-shape` (IDREF), glue point IDs, and `draw:type` (standard, lines, line or curve);
  - result: `svg:d` and `svg:x1..y2`.
  - Glue points have a `draw:escape-direction` (§19.147).
- `draw:layer` is an *attribute* on each shape, not a container. ODF layers are tags on a single z-ordered page, so connectors can bind across "layers" without any special handling.

### 1.11 Keynote

Keynote's format is proprietary IWA (Snappy-compressed protobuf), known only through reverse engineering: https://github.com/obriensp/iWorkFileFormat. Its shapes appear to be parametric (rounded-rectangle radius, star points, arrows), like Office presets [K, unverified]. With no public spec, I skipped it.

---

## 2. Whiteboards

### 2.1 Excalidraw (element schema, current main)

- Base element (`types.ts` L22–90):
  - Geometry: `id`, `x`, `y`, `width`, `height` and `angle`. Rotation is the only transform; there is no scale or skew.
  - Style: `strokeColor`, `backgroundColor`, `fillStyle`, `strokeWidth`, `strokeStyle` (solid, dashed or dotted), `roundness {type, value?}`, `roughness`, `opacity` and `seed`. The seed is "Random integer used to seed shape generation so that the roughjs shape doesn't differ across renders" (L57).
  - Collaboration: `version`, `versionNonce` (to reconcile edits) and `index` (a fractional index).
  - Other: `isDeleted`, `groupIds` ("Ordered from deepest to shallowest", L73; groups are flat tags, not nodes), `frameId`, `boundElements` (back-references, L76), `link`, `locked` and `customData`.
  - https://github.com/excalidraw/excalidraw/blob/4c37956ba5f04ec342a9d9d5891a38f22c5b7b4e/packages/element/src/types.ts
- Fill styles are `hachure`, `cross-hatch`, `solid` and `zigzag` (L19).
- Element types:
  - shapes: `rectangle`, `diamond`, `ellipse` and the new `stickynote`;
  - `text`;
  - lines: `line` (with a `polygon` flag) and `arrow` (`elbowed`, `fixedSegments`);
  - `freedraw`;
  - `image` (`fileId`, `scale` for flips, and `crop {x,y,width,height,naturalWidth,naturalHeight}`);
  - containers and embeds: `frame`, `magicframe`, `iframe` and `embeddable`.
- `freedraw` stores `points`, `pressures`, `simulatePressure` and `strokeOptions {variability, streamline}` (L431+). The raw samples are the stored data, and the outline is generated when drawing.
- An arrow binding is a `FixedPointBinding {elementId, fixedPoint:[rx,ry], mode: inside|orbit|skip}` (L320), where the ratios are fractions of the target's width and height. The arrow's `points` are stored too, so this is a result plus a binding. Arrows can bind to text, images, frames and embeddables as well as shapes.
- Labels: text has a `containerId` (a shape or an arrow). On arrows it also has a `labelPosition`, "a normalized arc-length parameter (0–1) along the container's whole path. Independent of how the path is segmented" (L283–290).
- Arrowheads: `arrow`, `bar`, `circle(_outline)`, `triangle(_outline)`, `diamond(_outline)`, plus six crow's-foot heads for database cardinality.
- Unknown data: `restore.ts` keeps unknown *properties* ("spread the original element properties to not lose unknown ones for forward-compatibility", L521). It silently *drops unknown element types* (there is no default case, so they are filtered out, L757–761). https://github.com/excalidraw/excalidraw/blob/4c37956ba5f04ec342a9d9d5891a38f22c5b7b4e/packages/excalidraw/data/restore.ts
- The file wrapper is `{"type":"excalidraw","version":2,"source",...}`. https://docs.excalidraw.com/docs/codebase/json-schema
- **Silent dropping is exactly what Capy's Preserved state exists to prevent.**

### 2.2 tldraw (schema, bindings and migrations)

- Base shape: `id`, `type`, `x`, `y`, `rotation`, `index`, `parentId` (a page or another shape), `isLocked`, `opacity`, `props` and `meta` (a free JSON bag). The transform has no scale: resizing rewrites props instead (`w`, `h`, `scale`, and `scaleX/scaleY` on draw shapes). https://github.com/tldraw/tldraw/blob/dd116535b138520d560c9881ad095ab782dc74c7/packages/tlschema/src/shapes/TLBaseShape.ts
- Geo shapes:
  - Kinds (the `geo` enum): cloud, rectangle, ellipse, triangle, diamond, pentagon, hexagon, octagon, star, rhombus, oval, trapezoid, four arrows, x-box, check-box, heart, and others.
  - Props: `w`, `h`, `growY`, `scale`, `flipX/Y`, `dash`, `fill`, `color`, `size`, `font`, `align`, `verticalAlign`, `labelColor`, `richText` and `url`.
  - Colors and sizes are **style tokens** (`color:'black'`, `size:'m'`) that the theme resolves, not concrete values.
- Draw and highlight shapes store `segments[{type:'free'|'straight', path, dim?}]`. `path` is "Delta-encoded base64 path data. First point stored as Float32 ..., subsequent points as Float16 deltas". With `dim` 2 the points are (x, y) and pressure is dropped; with 3 they are (x, y, pressure). https://github.com/tldraw/tldraw/blob/dd116535b138520d560c9881ad095ab782dc74c7/packages/tlschema/src/shapes/TLDrawShape.ts
- An arrow stores `kind: 'arc'|'elbow'`, `start`, `end`, `bend` (one number, which makes a circular arc through three points), `elbowMidPoint`, start and end arrowheads (eight styles), `labelPosition` and `richText`. Only these parameters are stored; the drawn path is never saved.
- **Bindings are separate records**: `{id, type, fromId, toId, props, meta}`. An arrow binding's props are `terminal`, `normalizedAnchor` (0..1 within the target's bounding box), `isExact`, `isPrecise` and `snap`. https://github.com/tldraw/tldraw/blob/dd116535b138520d560c9881ad095ab782dc74c7/packages/tlschema/src/bindings/TLArrowBinding.ts
- Binding lifecycle rules (https://tldraw.dev/sdk-features/bindings):
  - "When shapes are copied, only bindings between copied shapes are duplicated."
  - "When shapes are moved to different pages, cross-page bindings are automatically removed."
  - When a target is deleted, an "isolation" hook lets the arrow "update its terminal position to match the current attachment point before the binding is removed". This is the same as Capy's "keep the result, drop the recipe".
- Versioning and unknown data:
  - Each shape type has a sequence of prop migrations; geo, for example, has 12 steps from `AddUrlProp` to `AddFlipProps`, with `down: 'retired'`.
  - The store rejects unknown record types ("Missing definition for record type", `StoreSchema.ts` L394).
  - Opening a file from a newer version fails with `TargetVersionTooNew` (`migrate.ts` L534–538). https://github.com/tldraw/tldraw/blob/dd116535b138520d560c9881ad095ab782dc74c7/packages/store/src/lib/migrate.ts
  - Object validators throw "Unexpected property" unless `allowUnknownProperties()` is set (`packages/validate/src/lib/validation.ts` L684, L772).
  - So tldraw refuses newer documents, where Capy opens them read-only.

### 2.3 The sketchy ("rough") style

- rough.js options: https://github.com/rough-stuff/rough/wiki
  - Shape: `roughness`, `bowing`, `seed`, `curveStepCount`, `curveFitting`, `simplification`, `preserveVertices`.
  - Fill: `fillStyle` (hachure, solid, zigzag, cross-hatch, dots, dashed, zigzag-line), `fillWeight`, `hachureAngle`, `hachureGap`.
  - Dashes: `strokeLineDash`, `fillLineDash`, `dashOffset`, `dashGap`, `zigzagOffset`.
  - Passes: `disableMultiStroke(Fill)`.
  - The seed "is useful for creating the exact shape when re-generating with the same parameters". Valid seeds are 1 to 2^31; 0 means no seed.
- A seed reproduces a drawing only with the same version of the generator, because the output depends on the library's code [K]. Excalidraw stores only the seed and parameters, so its drawings can change when rough.js changes.
- **For Capy:** a rough style fits as a *recipe* (generator ID, version, seed and parameters), with the result stored as geometry: the jittered outlines and hatch lines. A rough or hatch paint is useful beyond whiteboards, for comic hatching and for DXF hatch patterns (§4).

---

## 3. Motion graphics

### 3.1 Lottie (the Lottie Animation Community spec, 1.0 / 1.0.1)

- The spec splits content into shapes (geometry only), groups, styles and modifiers. https://lottie.github.io/lottie-spec/latest/specs/shapes/ Its shape list:
  - geometry: ellipse, rectangle (`p`, `s`, `r` for roundness), polystar (`pt`, `or`, `ir`, `os`, `is`, `r`, `sy`) and path;
  - structure: group and transform;
  - styles: fill, stroke, and gradient fill/stroke (`s`, `e`, `t`, plus `h` highlight length and `a` highlight angle, i.e. a focal radial);
  - modifiers: trim path (`s`, `e`, `o`, and `m` for parallel or sequential), pucker-bloat and rounded corners.
  - "Unknown shape types ... still allowed" (`schema/shapes/*.json` in the repo).
- The community docs list more modifiers that are used in practice: Repeater, Twist, Merge, Offset Path and Zig Zag. https://lottiefiles.github.io/lottie-docs/shapes/
- **Rendering model** (quoted from the spec's `docs/specs/shapes.md`):
  - "Styles and modifiers apply to all preceding shapes within the current scope, including subgroup-nested shapes."
  - "When multiple styles apply to the same shape, the shape is rendered repeatedly for each style, in reverse order."
  - "Group opacity ... applies atomically."
  - Formally: for each (shape, style) pair, apply the modifiers, then `Render(shape × T_shape, style × T_style)`.
  - Note 2: "Transforms can affect both shapes and styles (e.g. stroke width). For a given (shape, style), the shape and style transforms are not necessarily equal."
  - Note 1: a sequential trim "may require information about shapes from different groups."
  - "This rendering model is based on AfterEffects' Shape Layer semantics."
- **Geometry is cubic-only.** A path is a list of vertices with in and out tangents, and the parametric shapes are defined by how they convert to cubics. "Implementations MAY use different algorithms ... but the output shape MUST be visually indistinguishable".
- Transforms are stored as separate components: anchor `a`, position `p`, rotation `r`, scale `s`, skew `sk`, skew axis `sa` and opacity `o` (`helpers/transform.json`). Keyframes live inside each animated property.
- Masks in the spec have a mode (`n`/`a`/`s`/`i`: none, add, subtract, intersect), an opacity and a Bézier `pt` (`helpers/mask.json`). The community format adds lighten, darken and difference modes, plus invert and expand. https://lottiefiles.github.io/lottie-docs/layers/ Mask feather is unverified.
- Track mattes: `tt` is alpha, inverted alpha, luma or inverted luma, and `tp` points at the matte layer's `ind` (`docs/specs/layers.md`, `constants/matte-mode.json`).
- Blend modes 0–16 run from Normal to Luminosity, plus Add. The community docs add Hard Mix. https://lottiefiles.github.io/lottie-docs/constants/
- Slots (`sid`) substitute one value into every property that names the slot (`helpers/slot.json`). They work like swatches or theme parameters.
- Text layers have animated text documents, range selectors and animators. https://lottiefiles.github.io/lottie-docs/text/
- Versions follow semver. Players "SHOULD issue a warning if ... the animation specifies a newer minor version" (`docs/specs/composition.md`): they warn and keep playing.

### 3.2 After Effects shape layers (the source of the Lottie model)

Quotes from the After Effects help page (archived copy): https://web.archive.org/web/2023/https://helpx.adobe.com/after-effects/using/shape-attributes-paint-operations-path.html

- "By default, paint operations within a group are performed from the bottom to the top in the Timeline panel stacking order." A Composite option, "Above Previous In Same Group", reverses this.
- "Each stroke and fill has its own blending mode, which determines how it interacts with other paint operations in the same group." Rive's paints also have a blend mode (`blendModeValue`, §3.3).
- "Path operations apply to all paths above them in the same group."
- "If you place the Repeater after a path, above the fill and stroke property groups for a shape, then the set of virtual copies is filled or stroked as a compound path. If you leave the Repeater below the fill and stroke, then each copy is filled and stroked individually." So in After Effects, the paths one fill covers are treated as a single compound path.
- Merge Paths "takes all of the paths above it in the same group as input. ... The input paths are still visible in the Timeline panel, but ... removed from the rendering". Its modes are Merge, Add, Subtract, Intersect and Exclude Intersections. The inputs remain part of the document but are not drawn.
- The fill rule is non-zero (the default) or even-odd. The page also links to stroke tapering.

**What this means structurally.** In After Effects and Lottie, paints and geometry are siblings in a scope: one fill can cover several paths, and one path can be painted by fills from several scopes. Capy instead attaches an ordered style list to each Path. Any After Effects scene still flattens into Capy:

- For each scope's style, make one compound Path from the shapes it covers, transformed into the style's own scope space. (This matches Note 2: stroke width follows the style's transform.)
- Order the resulting Paths using the reverse-order rule.
- Duplicate geometry when shapes from several scopes share a style.

What gets lost is shared editing (editing one path updates every painting of it) and Merge Paths inputs. Both fit as recipes on a Group. A painting app doesn't need After Effects round trips, so none of this is structural (§7).

### 3.3 Rive (runtime format and object model)

- **Binary format.** A file starts with a "RIVE" fingerprint, a major and minor version and a file ID. Then comes a **table of contents that lists every property key with a 2-bit value type** (integer/boolean, string, double or color) (`runtime_header.hpp`). Each object is a type key, then (property key, value) pairs, ended by a 0.
- **Unknown data.** For an unknown object or property, the reader looks the key up in its own registry, then in the file's table of contents, and skips the value by its type (`src/file.cpp` L245–330 and L548–575). https://github.com/rive-app/rive-runtime/blob/269ebcab3016e68340bb1427b720712213066eaf/src/file.cpp
  - A key in neither place ("Unknown property key %d, missing from property ToC") marks the file as malformed.
  - Unknown objects become a null object (`importStack.readNullObject()`).
  - Every object still claims its slot in the artboard's index list, so "every id after it keeps pointing at the right object".
- **Format docs** (https://rive.app/docs/runtimes/advanced-topic/format):
  - The table of contents "allows the runtime to read past properties it wishes to skip or doesn't understand".
  - "Major versions are not cross-compatible."
  - Within a major version, "Unsupported features are treated as no-ops".
  - So Rive renders without whatever it doesn't understand. Capy prefers Preserved, plus explicit "ignorable" flags.
- **Hierarchy and animation.**
  - Each object's parent is a `parentId`, an index into the artboard's object list (format docs).
  - Animation is `KeyedObject.objectId` (key 51), then `KeyedProperty.propertyKey` (key 53), then keyframes. So **animation addresses an (object ID, stable property key) pair**, and everything that can be animated is its own object with an ID: vertices, fills, strokes, solid colors and gradient stops.
- **Shape model.** A Shape is a node with a transform. Its children are:
  - Paths, each with its own transform, `pathFlags` and `isHole`;
  - paints: Fill (`fillRule`) and Stroke (`thickness`, `cap`, `join`, **`transformAffectsStroke`**, `position`).

  This is After Effects' "paints apply to the shape's paths" model again.
- **Vertex kinds:**
  - StraightVertex, with a **per-vertex corner `radius`**;
  - CubicMirroredVertex;
  - CubicAsymmetricVertex;
  - CubicDetachedVertex, whose handles are stored as angle and distance (`inRotation`, `inDistance`, `outRotation`, `outDistance`).
- **Paint effects and other objects.**
  - Paint effects: TrimPath (`start`, `end`, `offset`, `mode`), DashPath and Feather.
  - ClippingShape has a `sourceId`, a reference to another node.
  - Bones, Skin and Tendon carry bind matrices; Mesh and MeshVertex carry UVs for deformation (generated headers under `include/rive/generated/shapes/` and `bones/`).
  - Editor docs: bones at https://rive.app/docs/editor/manipulating-shapes/bones, state machines at https://rive.app/docs/editor/state-machine/state-machine.

### 3.4 Capy choices that would block animation later

1. **Stable names for properties.** Capy's tagged properties already give items Rive-style (item ID, property tag) addresses. But *entries in the style list, gradient stops and paint parameters have no identity of their own*. Animating "the second stroke's width" needs either keyframes stored inside the property (the Lottie way, no addressing needed) or a stable entry ID (the Rive way). Reserving an optional `entry_id` tag on style entries now is cheap.
2. **Path morphing** animates the whole path value, with the same vertex structure at every keyframe (Lottie). Anchors don't need IDs, but anchor kinds should stay plain data rather than change the path's structure.
3. **Transforms.** A Group's F64 affine can't be interpolated sensibly through rotations. Lottie and After Effects use anchor/position/rotation/scale/skew; Rive uses x/y/rotation/scaleX/scaleY. A later `transform_components` property can hold the components and act as the recipe, with the affine as its result, without breaking older readers.
4. **"Paths bake transforms"** is compatible: Lottie paths have no transform of their own; their groups do.
5. **Recipes change per frame.** Trim, roundness and repeater count are animated recipe parameters. The stored result is the resting pose, which remains a valid fallback.
6. **Integer layer offsets and no layer transform.** Moving a layer by fractions of a pixel, or rotating it, would need a layer transform, or the content wrapped in a Group. The Group route already works.

---

## 4. CAD and maker workflows

### 4.1 DXF entities

- **LWPOLYLINE** vertices are `(x, y, start_width, end_width, bulge)`. https://ezdxf.readthedocs.io/en/stable/dxfentities/lwpolyline.html and https://ezdxf.readthedocs.io/en/stable/tutorials/lwpolyline.html
  - "The bulge value defines the ratio of the arc sagitta (versine) to half line segment length"; a positive bulge is counter-clockwise.
  - So one DXF polyline can contain **arcs and varying widths segment by segment**.
- **ARC, CIRCLE and ELLIPSE** are exact primitives. https://ezdxf.readthedocs.io/en/stable/dxfentities/ellipse.html [K for ARC and CIRCLE]
- **SPLINE** "is defined by control points, knot values and weights", with a `degree`, optional fit points, and weights for rational splines. https://ezdxf.readthedocs.io/en/stable/dxfentities/spline.html
- **HATCH** fills one or more boundaries. https://ezdxf.readthedocs.io/en/stable/dxfentities/hatch.html
  - A boundary is a PolylinePath, or an EdgePath built from LineEdge, ArcEdge, EllipseEdge and SplineEdge.
  - The fill is solid, a pattern (families of lines with angle, offset and dashes) or a gradient.
  - A hatch can be **associative**, i.e. linked to the objects that form its boundary.
- **DIMENSION** (https://ezdxf.readthedocs.io/en/stable/dxfentities/dimension.html): `dxf.geometry` is the "Name of the BLOCK that contains the entities that make up the dimension picture. For AutoCAD this graphical representation is mandatory, otherwise AutoCAD will not open the DXF document. BricsCAD will render the DIMENSION entity by itself, if the graphical representation is not present". Of all the formats here, this is the clearest case of **a mandatory stored result next to a parametric recipe**.
- **LEADER/MLEADER** have a path type (straight or spline), an arrowhead, a hook line and an annotation (text, tolerance or block). https://ezdxf.readthedocs.io/en/stable/dxfentities/leader.html
- **TEXT/MTEXT**: https://ezdxf.readthedocs.io/en/stable/dxfentities/mtext.html
- **BLOCK/INSERT.** An insert can carry: https://ezdxf.readthedocs.io/en/stable/blocks/insert.html
  - non-uniform scale ("Not all CAD applications support non-uniform scaling");
  - `row_count`, `column_count` and spacing, i.e. a built-in grid of copies;
  - ATTRIB entities: tagged text values that differ per copy.

  This matches Capy's Instance with overrides, plus an array repeat.
- **Layers, linetypes and lineweights.**
  - Lineweight is an "integer value in millimeters * 100 ... independently from the unit system used in the DXF document". Only a fixed set of values is allowed, and `-1` (BYLAYER), `-2` (BYBLOCK) and `-3` (DEFAULT) are special. https://ezdxf.readthedocs.io/en/stable/concepts/lineweights.html
  - Linetypes are named dash patterns; complex ones embed shapes and text [K].
  - Color, linetype and lineweight can each be BYLAYER or BYBLOCK: *the style is inherited from the container*.

### 4.2 Cutting machines and lasers (SVG conventions)

- **Epilog lasers.**
  - Red RGB(255,0,0) means vector cut, blue (0,0,255) means vector engrave, and black means raster engrave.
  - For vector work, the "fill should be off, and the stroke should be on". A **0.001 inch hairline** stroke reads as a cut; anything thicker reads as an engraving.
  - Sources: https://www.newpaltz.edu/media/dfl/2023%20File%20Preparation%20for%20the%20Epilog%20Laser%20Cutter.pdf, https://wikis.utexas.edu/x/BDuhE, https://makingcenter.parsons.edu/wp-content/uploads/2021/02/Epilog-Laser-Manual-2025.pdf
  - Some lab guides swap red and blue (https://websites.umass.edu/fablab/files/2014/01/General-Laser-cutter-file-workflow.pdf), so the mapping is a convention, not a standard.
- **LightBurn** puts each imported object on the layer with the nearest color, with at most 29 cut layers. https://forum.lightburnsoftware.com/t/color-mapping-svg-into-lightburn/173030 (forum post; details unverified)
- **Cricut Design Space** gives each layer an operation: Basic or Wavy cut, Perforate, Pen, Foil, Score, Deboss, Engrave, or Print Then Cut. Guide layers are not cut. https://help.cricut.com/hc/articles/26751604105879 (from a search summary; the page refuses scripted requests)
- **What Capy's export needs:**
  - Exact 8-bit sRGB stroke colors must survive export. Capy's portable color stores the encoded values, which is fine, as long as no export step converts through linear floating point.
  - A **hairline or non-scaling stroke** must exist.
  - Fill and stroke must be independently switchable.
  - Layer names and order must carry through to SVG groups.
  - An optional "machine operation" tag per layer or item would remove the need for color tricks.

### 4.3 Do exact arcs, circles or NURBS matter?

- **For makers, no.** Cutters and lasers convert everything to straight segments [K]. A tolerance of 1/64 px at 96 ppi is about 0.004 mm, far below laser kerf and blade tolerances [K arithmetic].
- **For CAD round trips, only for identity.** A DXF CIRCLE that comes back as a 4-segment spline is no longer recognizable as a drill hole to machining software, and its radius can't be read or edited exactly [K].
- **The tolerance needs care.**
  - A circle made of 4 cubics has a relative radius error of about 2.7×10⁻⁴ [K]. At r = 500 px that is 0.14 px, so at large radii a 1/64 px tolerance needs more segments per quarter circle.
  - Groups will later carry affines, and **scaling a group magnifies the approximation error**. Define the tolerance in the path's own units, and recompute from the exact recipe (if any) when transforming.
- **Which curves convert exactly** [K]:
  - Quadratics: exactly (degree elevation).
  - Ordinary (non-rational) B-splines up to degree 3: exactly, as piecewise cubics, by knot insertion.
  - Rational curves (exact conics in NURBS) and splines above degree 3: only approximately.
- **Conclusion:** keep exact primitives as a recipe, not as new segment kinds (§7, question 1).

### 4.4 Figma, Illustrator and Affinity: arcs and topology

- Figma ellipses carry `arcData {startingAngle, endingAngle, innerRadius}` for pies, arcs and donuts. https://developers.figma.com/docs/plugins/api/ArcData/ That is a primitive recipe.
- **Figma vector networks are a graph, not a list of subpaths.** https://developers.figma.com/docs/plugins/api/VectorNetwork/
  - Vertices carry a per-vertex `cornerRadius` and `handleMirroring`; segments reference vertices by index.
  - **Regions** each have a winding rule and loops of segment indices, with their own fills. Loops can't fork or have gaps, but a vertex can join more than two segments.
  - Capy's subpaths can't express that shared-vertex structure. Import becomes paths plus region fills, which is what Figma's own SVG export does [K].
- **Figma connectors.** https://developers.figma.com/docs/plugins/api/ConnectorEndpoint/ and https://developers.figma.com/docs/plugins/api/ConnectorNode/
  - `connectorStart`/`connectorEnd` have three forms: `{endpointNodeId, magnet: NONE|AUTO|TOP|LEFT|BOTTOM|RIGHT|CENTER}`, `{endpointNodeId, position}` (relative to the node), or `{position}` (on the canvas).
  - Line types are ELBOWED, STRAIGHT and CURVED.
- Illustrator stores and edits paths as cubic Béziers; its "live" ellipse and polygon shapes are parametric wrappers around cubic paths [K]. Affinity's shape tools are parametric objects that can be converted to curves [K].

---

## 5. PDF and print features the object model must leave room for

Section numbers refer to ISO 32000-1, in Adobe's free copy: https://opensource.adobe.com/dc-acrobat-sdk-docs/pdfstandards/PDF32000_2008.pdf

| Feature | What PDF defines | Capy status |
| --- | --- | --- |
| Isolated groups (§11.4.5) | The group is composited onto a transparent backdrop, so blend modes inside it see only the group's own content | Covered by the planned Group `isolation` |
| **Knockout groups** (§11.4.6) | "each individual element shall be composited with the group's initial backdrop rather than with the stack of preceding elements"; the topmost object wins | **Missing.** Group needs a `knockout` property |
| Soft masks (§11.6.5.2) | A mask made from a transparency group, using either **alpha or luminosity**, with a backdrop color and a transfer function | A Group mask child is planned, but **its mode (alpha or luminance) isn't specified** |
| Shadings (§8.7.4.5) | 1 function-based; 2 axial; 3 radial (two circles, so focal points and extend flags); 4 free-form Gouraud triangles; 5 lattice Gouraud; 6 Coons patch mesh; 7 tensor-product patch mesh | Paint keys: gradient (needs a two-circle radial with focal point and extend flags), mesh (types 6/7), free-form gradient (type 4). Type 1 is out of scope |
| Patterns (§8.7.3) | A tiling pattern repeats a cell of arbitrary content with a step and a matrix; a shading pattern uses a shading | A `pattern` paint key must be able to *reference items or a symbol*, so a paint can point at content |
| Overprint (§8.6.7) | OP/op flags in the graphics state, which matter only when printing separations | A fill/stroke key; **ignorable** for screen rendering |
| Spot colors (§8.6.6.4–5) | Separation and DeviceN color spaces: named inks, plus an alternate color space and a tint transform | A `spot` paint key: ink name and tint, with the alternate color stored as the rendered result |
| Optional content (§8.11) | Optional content groups with separate visibility for **View, Print and Export** | A layer field for visibility per output (screen, print, export) |
| Annotations (§12.5.5) | Annotation data plus appearance streams (form XObjects) that viewers render, and may regenerate | The same result-plus-source split as Capy recipes |
| Tagged PDF and alt text (§14.8, §14.9.3) | A structure tree, plus alternate descriptions that "could, for example, be vocalized by a text-to-speech engine" | An ignorable `alt_text` property; the structure tree is out of scope |
| Type 3 fonts (§9.6.5) | Each glyph is an arbitrary content stream (color and images allowed) | Out of scope; at most, Text could later accept glyph sources other than OpenType |
| Blend modes (§11.3.5, Tables 136–137) | Normal, Multiply, Screen, Overlay, Darken, Lighten, ColorDodge, ColorBurn, HardLight, SoftLight, Difference, Exclusion, Hue, Saturation, Color, Luminosity (Compatible = Normal) | Capy's `add`, `linear_burn`, `vivid_light`, `linear_light`, `pin_light`, `hard_mix`, `subtract` and `divide` have **no PDF equivalent**, so a PDF export has to flatten them |

---

## 6. Other object kinds found in illustration documents

| Kind | Examples elsewhere | How it fits Capy |
| --- | --- | --- |
| Charts and graphs | Illustrator graphs: a group plus graph data. Ungrouping one "means the underlying data in the grid is gone" (https://community.adobe.com/t5/illustrator-discussions/edit-data-in-graph-tool-after-creating-graph-pie-chart/td-p/9016771, https://www.smashingmagazine.com/2010/09/creating-graphs-with-adobe-illustrator). Office charts with cached values [K] | A recipe on a Group (data, chart type, style), with paths and text as the result. Dropping the recipe is Illustrator's "ungroup" |
| Tables | DrawingML `a:tbl` [K]; ODF tables in frames [K] | A recipe on a Group (grid, merges, cell text), with paths and Text items as the result |
| QR codes and barcodes | CorelDRAW live QR objects: payload type, pixel shape, corner roundness, and error correction "Low (7%) to High (30%)" (https://product.corel.com/help/CorelDRAW/540227992/Main/EN/Documentation/CorelDRAW-Inserting-QR-codes.html, https://product.corel.com/help/CorelDRAW/540227992/Main/EN/Documentation/CorelDRAW-Editing-QR-codes.html) | A recipe (payload, error correction, module style) on a Path or Group, with the module geometry as the result |
| Dimension lines | DXF DIMENSION with its mandatory picture block (§4.1); measuring tools in CorelDRAW and Inkscape [K] | A recipe with two points or item bindings, a text format and units, with a path and text as the result. Needs bindings (question 3) |
| Callouts | DrawingML `wedgeRectCallout` and `borderCallout1..3`, with handle-driven tails (§1.1); Excalidraw and tldraw labels | A preset or parametric recipe; the tail point can bind to an item |
| Connectors and flowcharts | DrawingML `cxnSp`, ODF `draw:connector`, Excalidraw, tldraw, Figma, Inkscape (`inkscape:connection-start="#id"` plus a generated `d` "to make sure it appears in a standard svg"; https://wiki.inkscape.org/wiki/Connectors, https://inkscape.org/forums/extensions/creating-diagram-connectors) | A Path whose recipe holds the bindings and a routing kind (straight, elbow, curve or arc) |
| Live path effects | Inkscape stores the result in `d`, the source path in `inkscape:original-d`, and the effect parameters in `inkscape:path-effect` definitions (https://wiki.inkscape.org/wiki/LivePathEffects, https://dl.fullcirclemagazine.org/issueIS07_en.pdf) | Exactly Capy's result plus recipe |
| Maps | None relevant | Out of scope |
| Embedded 3D | PowerPoint `am3d:model3d` (GLB) with a fallback picture (https://bugs.libreoffice.org/show_bug.cgi?id=134141); DrawingML `sp3d` | Out of scope; at most an Image item whose resource has a 3D source and a raster result |
| Video and embeds | Excalidraw `iframe`/`embeddable`; tldraw `video`, `embed` and `bookmark` shapes | Out of scope for a painting app |
| Linked files | DrawingML `r:link` relationships [K]; Illustrator linked images [K] | A new resource kind: an external reference plus an embedded cached copy |
| Hyperlinks | DrawingML `hlinkClick`/`hlinkHover` on `cNvPr` (https://learn.microsoft.com/en-us/dotnet/api/documentformat.openxml.drawing.nonvisualdrawingproperties?view=openxml-3.0.1); Excalidraw `link`; tldraw `url`; PDF link annotations | An ignorable `link` property on items and layers |
| Alt text | DrawingML `cNvPr descr`, `title` and `hidden` (same page); PDF `Alt`; SVG `<title>`/`<desc>` [K] | An ignorable `alt_text` property that exporters write out |
| Metadata (author, license) | Lottie `meta` (`g` generator, `a` author, `d` description, keywords; `docs/specs/composition.md`); XMP [K] | Document level; Capy already has an `xmp` binding |
| Guides, grids and perspective grids | Illustrator's perspective grid keeps objects in perspective when moved with its selection tool, but editing the grid doesn't update objects already drawn (https://www.macworld.com/article/1150414/aics5news.html, https://www.tutkit.com/en/text-tutorials/16640-creating-creative-designs-with-the-perspective-grid) | Document-level records that don't render; attaching an item to a perspective plane is a recipe on that item |
| Slices and export regions | Affinity has layer-based slices (export only that layer's content, sized to fit it) and custom slices (all visible layers within an area), each with its own formats and scales (https://affinity.help/designer2ipad/English.lproj/pages/ExportPersona/slicesPanel.html, https://affinity.help/photo2/en-US.lproj/pages/ExportPersona/exportPersona.html; whether slices are saved in the file is unverified) | Document-level records that point at a layer, an item or a document rectangle, with export presets |
| Sticky notes and frames | Excalidraw `stickynote` and `frame`; tldraw `note` and `frame` | A frame is a named Group that clips; a note is a preset plus text. Nothing new is needed |

---

## 7. Gaps vs the planned Capy model

Each gap is labelled:
- **(a) structural:** needs a redesign or a new layout. Several of these are structural *only if the MVP encoding doesn't reserve room for them now*.
- **(b) additive later:** can be added later; the table names the slot.
- **(c) out of scope.**

### (a) Structural, unless reserved before the MVP format is frozen

1. **Item framing that doesn't depend on the item's kind.** Excalidraw silently drops unknown types, and Rive inserts null placeholders. A reader that meets an unknown kind must still find where that item's properties, body *and children* end, so it can draw the rest and keep the bytes.
   - In a pre-order stream, that needs a common header on every item: kind, ID, flags, property-block length, body length, and a child count or subtree length.
   - If the child structure lives inside a kind-specific body (for example, only Group knows how many children it has), every new container kind breaks the stream.
   - *Check the MVP encoding for this. If it already does this, it isn't a gap.*
2. **Properties that carry their own length.** Rive needs a file-wide table of value types just to skip unknown keys, and it still can't keep them. Capy's "ignorable" flag and its Preserved mode both need every property to carry its length (or a type that implies it), so unknown properties can be kept byte for byte.
3. **A path body that can grow.** Two separate needs:
   - **A segment-kind field** (or a body version), so quadratic, arc or rational segments *could* be added later. I recommend never using it for exact primitives; see question 1.
   - **Optional data per anchor.** Examples elsewhere: pressure (Excalidraw `pressures`, tldraw `dim:3`), DXF start/end width per vertex, corner radius per vertex (Rive, Figma), and skin weights (Rive).
   - If anchors are fixed-size records and the width profile is a one-off special case, each new kind of per-anchor data needs a new body layout. Instead, make the body a set of tagged parallel arrays: positions, in/out handles and kinds required; everything else optional, with unknown arrays treated as Preserved or ignorable.
4. **Open encodings for style entries and paints.** Fill and Stroke must be tagged sets of keys, not fixed structs. Keys planned or found here:
   - dash, with units relative to the stroke width or absolute (Office dashes are relative, SVG's absolute [K]);
   - align; markers; non-scaling/hairline;
   - blend and opacity per entry (After Effects: "Each stroke and fill has its own blending mode"; Rive `blendModeValue`);
   - overprint; `entry_id`.

   Paint must be an open tagged union: color now; gradient, pattern, mesh, swatch, spot, image and hatch later.
5. **References to items in other layers.** Item IDs are local to a layer, and an Object layer holds one object. A connector, dimension or text-on-path in layer A that binds to an item in layer B needs both the object and the item ID.
   - The package contract says binary resources "use local slot keys with visible bindings in records; they do not embed Capy IDs".
   - So the vector object record needs a visible **link table** (slot key → `{"ref": object}`), and item properties refer to (slot key, item ID).
   - Without that, cross-layer bindings would mean redesigning how references leave the item stream. Reserving it costs one optional field.
   - Ruling cross-layer bindings out is also acceptable: tldraw removes bindings between pages, and Google Slides can't bind to groups.
6. **What a recipe covers, and what it depends on.** The plan ties each recipe to one item and fingerprints that item's result. But:
   - Office presets, charts, tables, SmartArt-style layouts, repeaters and Merge Paths produce a **whole subtree** (a Group of Paths and Text).
   - Connectors, dimensions, sequential trims and text-on-path read **other items**.

   Decide now:
   - The fingerprint covers the recipe owner's whole result subtree.
   - A recipe can list the items it reads (local IDs or link slots), with a fingerprint of those inputs. A newer reader can then tell when an older reader moved a bound target without touching the connector.
   - A recipe whose inputs are missing is dropped and its result kept (tldraw's "isolation").
7. **Rules that can't change after Group ships.** Group is not in the MVP; decide these together with it, because changing them later changes how existing files look:
   - **Stroke width, dash lengths and effect radii.** Either they are measured in the path's own space and then transformed by the parent groups' affines, as in SVG (a non-uniform group scale then gives an elliptical pen), or in the style's own space (Lottie, Note 2). Rive lets each stroke choose (`transformAffectsStroke`).
   - **The default mask mode.** Alpha or luminance; PDF and Lottie support both.
   - **The coordinate space of effects** (compare DrawingML's `rotWithShape`).

### (b) Additive later, with the slot

| Gap | Evidence | Slot |
| --- | --- | --- |
| Exact arcs, circles, ellipses, NURBS and bulge polylines | DXF ARC/CIRCLE/SPLINE/LWPOLYLINE; Office `arcTo` (393 uses in presets); ODF arc commands; Figma `arcData`; tldraw arc arrows | **Recipe** `exact_geometry` (per item or per run of segments) on top of the cubic result |
| Parametric primitives (rectangle corner radius, ellipse arc or donut, polygon, star) | Lottie rect/polystar, Figma arcData, tldraw geo shapes, Rive parametric paths | **Recipe** `primitive` with its own frame, since paths bake transforms |
| Office/ODF presets with adjust handles, a text rectangle and connection sites | §1.1–1.2, §1.10 | **Recipe** `preset` (name, adjust values, box) on a Group of Paths; connection sites derived from the recipe |
| Connectors and bindings within one object | DrawingML, ODF, Excalidraw, tldraw, Figma, Inkscape | **Recipe** `connector` on a Path. Each end holds a target item ID, an anchor (a site key, *or* a normalized point in the target's own frame) and a mode; plus a routing kind. The stored path is the result |
| Text labels inside shapes, text on a path | DrawingML `txBody`; Excalidraw `containerId`/`labelPosition`; tldraw `richText` | **Property tags** on Text: `frame_item` or `path_item` (a local ID), and `path_offset` as a normalized arc length |
| Rough or sketchy style | rough.js, Excalidraw | **Recipe** `rough` (generator version, seed, parameters); the stored outlines are the result |
| Hatch fills | Excalidraw hachure/cross-hatch; DXF HATCH patterns | **Paint key** `hatch` (families of lines: angle, spacing, offset, dashes) |
| Freehand samples with pressure | Excalidraw `freedraw`, tldraw draw segments | **Recipe** `freehand_samples` (raw points and pressure); the result is a fitted path with a width profile, or a filled outline |
| Lottie/After Effects modifiers: trim, offset, pucker/bloat, twist, zig zag, rounded corners, repeater, merge | §3.1–3.2 | **Recipes** on the Path or Group (merge keeps its hidden inputs inside the recipe) |
| One style shared across several paths (After Effects operators, Office theme styles) | §3.2, §1.7 | **Paint key** `swatch`, plus a **document-level** swatch/style record; no new structure needed |
| Theme colors with modifiers | DrawingML `schemeClr` with lumMod/tint/shade/alpha | A `swatch` paint with an ordered modifier list and the stored resolved color |
| Gradient extras: focal radial, extend modes, highlight angle | PDF type 3 (two circles); Lottie `h`/`a` | **Paint key** fields on the gradient |
| Mesh and free-form gradients | PDF shading types 4–7 | **Paint keys** `mesh` (Coons/tensor) and `freeform` (triangles) |
| Image as a paint (stretched or tiled), and Image crop or outset | DrawingML `blipFill`, `srcRect`, `tile`; Excalidraw `crop` | **Paint key** `image`; **property tag** `crop` on Image |
| A repeating pattern of drawn content | PDF tiling patterns | **Paint key** `pattern` that references a symbol (the same mechanism as Instance) |
| Knockout groups | PDF §11.4.6 | **Property tag** `knockout` on Group (not ignorable) |
| Luminance vs alpha masks, inverted masks | PDF soft-mask `/S`; Lottie luma and inverted mattes | **Property tag** `mask_mode` on Group; new layer **attachment** values such as `clip_luma` and `clip_inverted` |
| Vector masks on raster layers, and mask stacks with add/subtract/intersect | Lottie masks (`n`/`a`/`s`/`i`, plus lighten, darken, difference and expand) | **Layer field**: let the layer's `mask.source` point at a vector object; add a mode list if stacking is ever needed |
| Shadow, glow, soft edge and blur on single items | DrawingML puts `effectLst` on every shape | Group effects (planned) plus new built-in filters; importers wrap shapes in Groups |
| Arrowheads and markers | Excalidraw (including crow's feet), tldraw, DrawingML line ends [K], DXF leaders | **Stroke key** `markers` (a built-in kind or a symbol reference, sized relative to stroke width) |
| Hairline or non-scaling stroke | The 0.001 in laser convention; SVG `vector-effect: non-scaling-stroke`, which the SVG 2 spec marks at risk of being dropped (https://www.w3.org/TR/SVG2/coords.html); Rive `transformAffectsStroke` | **Stroke key** `non_scaling` and/or `hairline` |
| Widths and tapers that vary by segment | DXF LWPOLYLINE start/end widths; After Effects taper | The planned width profile, or a per-anchor `width` array |
| Blend and opacity per paint | After Effects, Rive | **Fill/stroke keys** `blend` and `opacity` |
| Spot colors and overprint | PDF Separation/DeviceN, OP/op | **Paint key** `spot`; **fill/stroke key** `overprint` (ignorable) |
| Visibility per output | PDF optional content (View/Print/Export); Cricut guide layers | **Layer field** `output_visibility` |
| Machine operation per layer or item | Cricut operations; laser color conventions | **Layer field or ignorable property** `machine_operation` |
| Alt text, hyperlinks and a free metadata bag | DrawingML `cNvPr`; PDF Alt; Excalidraw `link`/`customData`; tldraw `url`/`meta` | **Ignorable property tags** `alt_text`, `link` and `meta` |
| Separate transform components for animation | Lottie and Rive transforms | **Property tag** `transform_components` on Group, acting as the recipe for the affine |
| Stable IDs for style entries and gradient stops | In Rive every paint and stop is an object with an ID | **Style-entry key** `entry_id` (reserve now) |
| Overrides inside nested instances | Rive and Figma reach nested content by paths of IDs (Figma override paths [K]) | Override keys as **ID paths**, not single IDs |
| DXF INSERT arrays and per-copy attributes | §4.1 | **Recipe** `array` on Instance; overrides cover the ATTRIB values |
| Charts, tables, QR codes, dimensions and callouts | §6 | **Recipes** on a Group, with Paths and Text as the result |
| Guides, grids, perspective grids and slices | Illustrator, Affinity | **Document-level records** that don't render; a `perspective` recipe per item |
| Linked files | Office and Illustrator links | **Resource kind**: an external reference plus an embedded cached copy |
| Bindings across objects | ODF's layer attribute (§1.10); Excalidraw and tldraw use document-wide IDs | **Link slots in the object record** (reserve now; see (a) 5), plus **(slot, item ID)** property values |

### (c) Out of scope for a painting and illustration app

- 3D extrusion, bevels, cameras and lights (DrawingML `sp3d`/`scene3d`, ODF extrusion attributes) and embedded 3D models.
- SmartArt layout engines and charts linked live to spreadsheets (a static chart recipe stays possible). Also Office `effectDag` graphs, Type 3 fonts, and the Tagged PDF structure tree (alt text aside).
- Round trips of After Effects/Lottie animation (keyframes, expressions, text animators, precomps), and Rive's state machines, bones, skinning and meshes. Keep the identity rules in §3.4 so they stay possible.
- Collaboration fields (Excalidraw `version`, `versionNonce`, fractional `index` and `isDeleted`).
- Theme-relative colors that change with the UI's dark mode (tldraw's style tokens). Artwork colors must be absolute.
- Figma vector-network topology (import it as paths plus region fills); DXF complex linetypes, coordinate systems and 3D entities; video, iframes, embeds and maps.

### Specific answers

**(1) Must the set of path segment kinds be open to new kinds? Is cubic-only safe for the MVP?**

Cubic-only is safe:
- Lottie and Rive use only cubics at runtime, and Lottie explicitly allows any algorithm whose result is "visually indistinguishable".
- After Effects and Illustrator paths are cubic [K].
- Office presets mostly use straight lines and arcs, which convert within tolerance; quadratics convert exactly.
- Cutting machines and lasers convert everything to straight segments anyway.

What a cubic-only body loses is *identity and exactness for CAD*: a circle staying a circle, a bulge staying a bulge, a rational spline.
- **Leave room in the encoding:** a segment-kind field that must say "cubic" in version 1, or a body version.
- **Carry exact curves as an `exact_geometry` recipe** on top of the cubic result. Older readers then keep rendering *and editing*; their edits simply drop the exactness.
- A new segment kind, by contrast, would make every file that uses it Preserved for older readers.

Two things to fix now:
- Define the 1/64 px tolerance in the path's own units, and subdivide more as the radius grows. A circle of 4 cubics already misses 1/64 px from about r ≈ 58 px [K arithmetic: 2.7×10⁻⁴ × r].
- Require recomputing from the recipe when a later group scale magnifies the error.

**(2) Does the After Effects/Lottie "paint operators inside a group" model, or DrawingML's child coordinate space, force anything structural?**

No. Any Lottie scene flattens into Capy Paths with a style list per path:
- Work out each (shape, style) pair from the formal rendering model.
- Bake each shape into the style's scope space, so stroke width follows the style's transform (Note 2).
- Make one compound Path per style scope; After Effects fills already treat their paths as one compound path.
- Duplicate geometry when styles from several scopes cover the same shape.

What gets lost is *shared editing*. That is category (b): a shared swatch or style record, plus modifier and merge recipes on Groups. After Effects round trips are (c).

DrawingML child coordinates:
- `chOff/chExt` plus rotation and flips form an affine, which folds into a Group's F64 affine. Capy's affine is more general, so import loses nothing once results are baked.
- Two things force baking: PowerPoint's handling of rotated children (unverified bug report), and preset geometry that depends on the box size. The importer bakes results and keeps a `preset` recipe with its own box.

The one thing to decide before Groups ship, and then freeze, is which space measures stroke width, dashes and effect radii under a group affine.

**(3) Do connectors and bindings need references between items, and do u64 item IDs support them, including across layers?**

Yes, they need references. Every format binds by an ID that is unique within some scope:

| Format | Binding |
| --- | --- |
| DrawingML | `stCxn id` (drawing-element ID) + `idx` (site index) |
| ODF | `draw:start-shape` IDREF + glue point |
| Google Slides | `connectedObjectId` + site index |
| Excalidraw | `elementId` + `fixedPoint` ratios, with back-references in `boundElements` |
| tldraw | Binding records with `fromId`/`toId` + `normalizedAnchor` |
| Figma | `endpointNodeId` + magnet or position |
| Inkscape | `#id` |
| DXF | Handles for associative hatches and dimensions [K] |

Every format except tldraw stores the routed result next to the binding. Capy's planned design (a stored path plus a binding recipe) matches the majority, and degrades gracefully when a target disappears (tldraw's "isolation").

Layer-local u64 item IDs **are enough inside one vector object**. Copying a whole object even keeps its internal bindings intact, the same as tldraw's rule of copying only bindings between copied shapes.

They **are not enough across layers**. An item ID is only unique within its object, and the package contract forbids embedding Capy object IDs in binary resources.
- A binding across layers needs (a link slot in the visible object record → `{"ref": object}`, item ID).
- Copying or duplicating must then remap that binding or drop it.
- Anchors should be either a site key derived from the target's recipe, or a normalized point in the target's own frame, not (segment index, t), which path edits invalidate. tldraw and Excalidraw both chose normalized boxes.
- The recipe must record fingerprints of its inputs, so a newer reader can tell when an older reader moved the target.

For the MVP, the safe choice is to allow bindings only within the same vector object, and to reserve the link-slot field in the object record.
