# Inkscape data model inventory, for a vector layer that has to round-trip Inkscape SVG

[Object content extensions](../object-content-extensions-research.md) · source report, 2026-10-15

## Version status (checked 2026-10-15)

- **Latest stable is 1.4.4.** It was tagged `INKSCAPE_1_4_4` on 2026-05-05 and announced around May 6–7 ([tags](https://gitlab.com/inkscape/inkscape/-/tags), [wiki release list](https://wiki.inkscape.org/wiki/ReleaseNotes)). 1.4.3 came out 2025-12-26.
- **1.5 has not been released.** There is no 1.5 tag or branch. The [1.5 notes](https://wiki.inkscape.org/wiki/Release_notes/1.5) are a draft. Its GitLab milestone was due April 2026 and slipped.
- **What is on master for 1.5:**
  - GTK 4 front end.
  - A new color system: RGB, linearRGB, DeviceCMYK, Gray, Lab/LCh/Luv, HSL/HSV/HSLuv, OkLab/OkLch/OkHsl, XYZ, and ICC profile spaces.
  - CapyPDF becomes the default PDF exporter.
  - Lasso selection.
  - A recolor feature in Fill & Stroke.
  - A preference to keep orphaned clones.
  - **A file format change:** pages become `svg:view` (details in the table).
- **1.4.3 and 1.4.4 are "bridge" releases.** They convert 1.5 pages back to the old format ([1.4.3 news](https://alternativeto.net/news/2025/12/inkscape-1-4-3-delivers-124-fixes-and-bridges-multipage-formats)).

**Sources.** I read the Inkscape source on master at commit f093808f (2026-10-15) through a sparse clone, and checked a few files against tag `INKSCAPE_1_4_4`. I also read the wiki release notes for [1.0](https://wiki.inkscape.org/wiki/Release_notes/1.0), [1.1](https://wiki.inkscape.org/wiki/Release_notes/1.1), [1.2](https://wiki.inkscape.org/wiki/Release_notes/1.2), [1.3](https://wiki.inkscape.org/wiki/Release_notes/1.3) and [1.4](https://wiki.inkscape.org/wiki/Release_notes/1.4). inkscape.org itself returned 403 to my fetches, so I used the wiki copies. Source links below are written `src/...` and resolve against `https://gitlab.com/inkscape/inkscape/-/blob/master/`.

**Corrections to common claims:**
- Stock markers switched to `context-stroke` in **1.1** (MR #2009), not 1.2. 1.2 added the marker editing UI.
- `osb:paint` was replaced by `inkscape:swatch` in **1.1** ([MR !2112](https://gitlab.com/inkscape/inkscape/-/merge_requests/2112)). Older files are converted on load.

## Inventory

"Editable" means what a foreign editor must store so Inkscape still treats the object as live after a round trip. "Fallback" means whether a plain SVG renderer shows the right result.

| Feature | SVG storage | Keep to stay EDITABLE | Rendered fallback |
|---|---|---|---|
| Layers / sublayers | `<g inkscape:groupmode="layer" inkscape:label="…">`; nested layer groups are sublayers; `inkscape:current-layer` sits in namedview ([sp-item-group.cpp](https://gitlab.com/inkscape/inkscape/-/blob/master/src/object/sp-item-group.cpp)) | groupmode, label, document order | Yes, as plain `<g>`. Names are lost in Plain SVG |
| Lock | `sodipodi:insensitive="true"` on any item ([sp-item.cpp](https://gitlab.com/inkscape/inkscape/-/blob/master/src/object/sp-item.cpp)) | the attribute | UI-only |
| Hide | `style="display:none"`; shown layers are written as `display:inline` | the standard property | Yes |
| Layers and Objects metadata | `inkscape:label` on any item, `inkscape:highlight-color`, `inkscape:expanded`; selection sets as `<inkscape:tag>/<inkscape:tagref>` | keep verbatim | UI-only |
| Node types | `sodipodi:nodetypes`: one character per node (c cusp, s smooth, z symmetric, a auto-smooth). For closed subpaths the first node's character is repeated. Path parameters inside an LPE use `<param>-nodetypes` ([node.cpp](https://gitlab.com/inkscape/inkscape/-/blob/master/src/ui/tool/node.cpp), [path-manipulator.cpp](https://gitlab.com/inkscape/inkscape/-/blob/master/src/ui/tool/path-manipulator.cpp)) | the string, kept aligned with the node order of `d` or `original-d`. Recompute it when you edit nodes | Yes: `d` already holds the evaluated auto-smooth handles |
| Rect | Native `<rect x y width height rx ry>`. Separate corner radii only through the Corners LPE (1.4). With an LPE it becomes `<path sodipodi:type="rect" x y width height [rx ry] d>` ([sp-rect.cpp](https://gitlab.com/inkscape/inkscape/-/blob/master/src/object/sp-rect.cpp)) | the rect attributes | Yes |
| Circle / ellipse / arc | Full shapes are native `<circle>` / `<ellipse>`. A partial arc, or any ellipse with an LPE, is `<path sodipodi:type="arc" sodipodi:cx cy rx ry start end sodipodi:arc-type="slice\|chord\|arc" sodipodi:open d>` ([sp-ellipse.cpp](https://gitlab.com/inkscape/inkscape/-/blob/master/src/object/sp-ellipse.cpp)) | sodipodi:* parameters | Yes (`d`) |
| Star / polygon | `<path sodipodi:type="star" sodipodi:sides cx cy r1 r2 arg1 arg2 inkscape:flatsided inkscape:rounded inkscape:randomized d>` ([sp-star.cpp](https://gitlab.com/inkscape/inkscape/-/blob/master/src/object/sp-star.cpp)) | all parameters. Inkscape rebuilds `d` from them, so a foreign edit to `d` alone is overwritten ([wiki](https://wiki.inkscape.org/wiki/Inkscape_SVG_vs._plain_SVG)) | Yes (`d`) |
| Spiral | `<path sodipodi:type="spiral" sodipodi:cx cy expansion revolution radius argument t0 d>` ([sp-spiral.cpp](https://gitlab.com/inkscape/inkscape/-/blob/master/src/object/sp-spiral.cpp)) | same as star | Yes |
| 3D box | `<g sodipodi:type="inkscape:box3d" inkscape:perspectiveID="#p" inkscape:corner0 inkscape:corner7>` with six `<path sodipodi:type="inkscape:box3dside" inkscape:box3dsidetype d>`. The perspective is `<inkscape:perspective sodipodi:type="inkscape:persp3d" inkscape:vp_x vp_y vp_z persp3d-origin>` in defs ([box3d.cpp](https://gitlab.com/inkscape/inkscape/-/blob/master/src/object/box3d.cpp), [persp3d.cpp](https://gitlab.com/inkscape/inkscape/-/blob/master/src/object/persp3d.cpp)) | box, sides, perspective element | Yes (side paths) |
| Dynamic / linked offset | `<path sodipodi:type="inkscape:offset" inkscape:radius inkscape:original="<source d>" [inkscape:href="#src"] d>`. A linked offset has `href` ([sp-offset.cpp](https://gitlab.com/inkscape/inkscape/-/blob/master/src/object/sp-offset.cpp)) | radius, original, href | Yes |
| Connectors | `<path inkscape:connector-type="polyline\|orthogonal" inkscape:connector-curvature inkscape:connection-start="#a" [-start-point] inkscape:connection-end … d>`. Obstacles carry `inkscape:connector-avoid` ([sp-conn-end-pair.cpp](https://gitlab.com/inkscape/inkscape/-/blob/master/src/object/sp-conn-end-pair.cpp)) | endpoint references, type; ids of the connected objects must stay stable | Yes (the routed `d`) |
| Live Path Effects (general) | `<inkscape:path-effect id effect="<key>" is_visible lpeversion …params>` in defs. The item has `inkscape:path-effect="#e1;#e2"`, an ordered stack. A path stores its input in `inkscape:original-d`; a shape's input is its own parameters; for a group, each descendant path keeps `original-d`. The output goes in `d` ([sp-lpe-item.cpp](https://gitlab.com/inkscape/inkscape/-/blob/master/src/object/sp-lpe-item.cpp), [lpeobject.cpp](https://gitlab.com/inkscape/inkscape/-/blob/master/src/live_effects/lpeobject.cpp), [effect.cpp](https://gitlab.com/inkscape/inkscape/-/blob/master/src/live_effects/effect.cpp)) | the effect elements (all parameters as strings), the stack, `original-d`. Inkscape re-evaluates and overwrites `d`. "Flatten" or Object to Path deletes the source | Yes: `d` is the evaluated output |
| Clones | `<use xlink:href="#id" x y width height transform>`. Inkscape reads `href` or `xlink:href` and writes `xlink:href`. Preferences cover clone compensation, orphan handling (1.5 adds "become orphans") and relinking duplicates. Unlink replaces the `use` with a deep copy plus its transform | the reference and transform | Yes |
| Tiled clones | `<use inkscape:tiled-clone-of="#orig">`; the original carries `inkscape:tile-cx/cy/w/h/x0/y0` ([clonetiler.cpp](https://gitlab.com/inkscape/inkscape/-/blob/master/src/ui/dialog/clonetiler.cpp)) | those attributes; dialog settings live in preferences | Yes |
| Symbols | `<symbol>` in defs (SVG 2 `x/y/width/height` supported since 1.2) plus `<use>`. Library symbols carry `inkscape:symbol-style`. The dialog reads `share/symbols`, and `.vss`/`.vssx` since 1.3 | symbol and `use` | Yes |
| Markers | `<marker refX refY orient markerUnits markerWidth markerHeight viewBox style="overflow:visible" inkscape:stockid inkscape:isstock inkscape:collect>`. Stock markers use `orient="auto-start-reverse"` and `stroke:context-stroke` ([share/markers/markers.svg](https://gitlab.com/inkscape/inkscape/-/blob/master/share/markers/markers.svg)). Custom markers come from Objects to Marker | standard attributes, stockid | SVG 2 renderers: yes. The SVG 1.1 export options (off by default) duplicate reversed markers and replace context paint with real colors |
| Gradients | A "vector" `<linearGradient|radialGradient inkscape:collect="always">` holds only stops. Each object gets a private gradient with `xlink:href="#vector" gradientUnits="userSpaceOnUse"`, its coordinates and gradientTransform. If a shared vector is edited, Inkscape forks it (preference `forkgradientvectors`, on by default). Unreferenced `collect="always"` defs are removed automatically ([gradient-chemistry.cpp](https://gitlab.com/inkscape/inkscape/-/blob/master/src/gradient-chemistry.cpp)) | the href chain and ids. Flattening it still renders, but loses the sharing | Yes |
| Swatches | `<linearGradient inkscape:swatch="solid\|gradient">` (one stop for solid); objects use `fill:url(#swatch)`. Pre-1.1 `osb:paint` is converted on load ([sp-gradient.cpp](https://gitlab.com/inkscape/inkscape/-/blob/master/src/object/sp-gradient.cpp)) | `inkscape:swatch` | Yes |
| Mesh gradients | `<meshgradient x y type="bicubic\|coons">` > `<meshrow>` > `<meshpatch>` > `<stop path="c…" style>`. On save Inkscape appends `<script id="mesh_polyfill">` to the root (preference, on by default; [actions-svg-processing.cpp](https://gitlab.com/inkscape/inkscape/-/blob/master/src/actions/actions-svg-processing.cpp)) | the whole element tree | **JavaScript polyfill only, no static fallback.** Meshes were cut from SVG 2 ([Tav Bah](http://tavmjong.free.fr/svg2_status.html)) |
| Hatches | `<hatch hatchUnits pitch rotate x y transform style>` > `<hatchpath d offset>`; a polyfill script is added on save. Stock hatches are in [share/paint/Hatches.svg](https://gitlab.com/inkscape/inkscape/-/blob/master/share/paint/Hatches.svg) | element tree | Polyfill only; also cut from SVG 2 |
| Patterns | Two levels: the object refers to a link `<pattern xlink:href="#root" patternTransform [x y width height]>`, which points at a root `<pattern patternUnits="userSpaceOnUse" width height style="fill:…">` holding the content. On-canvas handles (1.3) edit the link's patternTransform. The Pattern Editor (1.3) stores gap as the link's width/height differing from the root's, and color as `fill` on the root, inherited by content with unset fill ([pattern-manipulation.cpp](https://gitlab.com/inkscape/inkscape/-/blob/master/src/pattern-manipulation.cpp)) | the link/root split | Yes |
| SVG 1.1 multi-line text | `<text xml:space="preserve">` with `<tspan sodipodi:role="line" x y>` per line. Inkscape re-lays lines out from `line-height` | `sodipodi:role` | Yes |
| SVG 2 wrapped text | Frame text: Inkscape creates a `<rect>` **in defs**, and the text gets `style="white-space:pre;shape-inside:url(#rect)"`, optionally `shape-padding` / `shape-subtract` / `shape-margin`. Column text: `inline-size`. At save time, `insert_text_fallback` writes one tspan per line with x/y, and dx to fake justification. On load, Inkscape ignores x/y/dx/dy/rotate inside wrapped text ([sp-text.cpp](https://gitlab.com/inkscape/inkscape/-/blob/master/src/object/sp-text.cpp)) | the wrapping properties, the frame shape, the unpositioned text. Regenerate the fallback on save | Yes: positioned SVG 1.1 tspans (preference, on by default) |
| Legacy flowed text | `<flowRoot><flowRegion><rect|use/></flowRegion><flowPara>/<flowSpan>/<flowDiv>` (SVG 1.2 draft). Still read; only created when "Use SVG2 auto-flowed text" is turned off | element tree | **None: browsers render nothing** |
| Text on path | `<textPath xlink:href="#path" startOffset side>` | the path element must stay | Yes (`side` is SVG 2) |
| Typography | `font-*` properties plus `-inkscape-font-specification` (a Pango description). This is a CSS property, so it survives Plain SVG. Also `font-variation-settings`, `font-feature-settings`, `font-variant-*`; vertical text via `writing-mode` and `text-orientation`; manual kerning via `dx/dy/rotate` lists on tspans plus `letter-spacing` / `word-spacing` ([attributes.cpp](https://gitlab.com/inkscape/inkscape/-/blob/master/src/attributes.cpp)) | keep all of it, including the `-inkscape-` property | Yes, if the font is available |
| Filters | Standard `<filter>` graphs in defs with `style="color-interpolation-filters:sRGB"`, a region and `inkscape:auto-region`. The 213 presets in [share/filters/filters.svg](https://gitlab.com/inkscape/inkscape/-/blob/master/share/filters/filters.svg) carry `inkscape:label`, `inkscape:menu`, `inkscape:menu-tooltip`. The blur slider makes a feGaussianBlur filter marked `inkscape:collect`. The 1.4 Filter Gallery only browses these presets | the graph | Yes (renderer quality varies) |
| Opacity / blend | `opacity`, `fill-opacity`, `stroke-opacity`, `mix-blend-mode`, `isolation` in style (CSS blend since 1.0). Legacy 0.92 blends are `feBlend in2="BackgroundImage"` plus root `enable-background`, still rendered by Inkscape | properties | CSS: yes. Legacy BackgroundImage: no browser support |
| Clips / masks | `clip-path="url()"` / `mask="url()"` pointing at `<clipPath>` / `<mask>` in defs. Inverse clips and masks come from the PowerClip / PowerMask LPEs, which add an inverse path or background rect plus a filter | standard | Yes |
| Pages, 1.2–1.4 | `<inkscape:page x y width height inkscape:label margin bleed page-size>` inside `<sodipodi:namedview>`. The root viewBox is the first page | page elements | Browsers show only the root viewBox |
| Pages, 1.5-dev | `<view viewBox inkscape:label inkscape:margin inkscape:bleed inkscape:page-size>` in `<defs>`. Older files migrate on load ([MR !7525](https://gitlab.com/inkscape/inkscape/-/merge_requests/7525), merged 2025-11-21; [sp-page.cpp](https://gitlab.com/inkscape/inkscape/-/blob/master/src/object/sp-page.cpp), [document-update.cpp](https://gitlab.com/inkscape/inkscape/-/blob/master/src/document-update.cpp)) | view elements | Pages can be addressed as `#id` |
| Guides | `<sodipodi:guide position orientation inkscape:label inkscape:color inkscape:locked>` in namedview. **Position is in user units with y measured up from the bottom**, a legacy convention ([sp-guide.cpp](https://gitlab.com/inkscape/inkscape/-/blob/master/src/object/sp-guide.cpp)) | element | No |
| Grids | `<inkscape:grid type="xygrid\|axonomgrid\|modular" originx originy spacingx spacingy units color opacity empcolor empopacity empspacing dotted visible enabled gridanglex gridanglez marginx marginy gapx gapy>`; the modular type is from 1.4 | element | No |
| Units / scale | Root `width`/`height` with units plus `viewBox` define the user unit (210mm with `0 0 210 297` gives 1 user unit = 1 mm). `inkscape:document-units` is the display unit only. 96 px/in since 0.92; 1.4 adds a scale lock | keep all three | Yes |
| Export hints | `inkscape:export-filename` (relative since 1.2), `inkscape:export-xdpi/-ydpi`, `inkscape:export-bgcolor`, `inkscape:export-batch-path/-name`, on the root or on items | keep verbatim | n/a |
| Color, 1.4 | `#rrggbb icc-color(name, v…)` plus `<color-profile name local xlink:href|data:…base64 rendering-intent>` in defs. 1.3 PDF import keeps CMYK this way; 1.4 adds only CMYK palettes ([color-profile.cpp](https://gitlab.com/inkscape/inkscape/-/blob/master/src/object/color-profile.cpp)) | profile plus the icc values | Yes: the hex comes first |
| Color, 1.5-dev | Parses and prints `oklch()`, `oklab()`, `lab()`, `lch()`, `hsl()`, `hwb()`, `color(srgb-linear|xyz…)`, `device-cmyk()`, and `#hex icc-color()` for ICC spaces ([src/colors/](https://gitlab.com/inkscape/inkscape/-/tree/master/src/colors)) | the original space and values | `device-cmyk()`: none. oklch/lab: modern browsers only |
| Rotation center | `inkscape:transform-center-x/-y`: offset from the bounding-box center in user units; **y is stored up-positive** ([sp-item.cpp](https://gitlab.com/inkscape/inkscape/-/blob/master/src/object/sp-item.cpp)) | keep | n/a |
| Hairline (1.1) | `-inkscape-stroke:hairline` plus `vector-effect:non-scaling-stroke` and `stroke-width:1px` | keep | Approximate |
| Images | `<image xlink:href="data:…|path" preserveAspectRatio>`, `inkscape:svg-dpi` for embedded SVG, optional `sodipodi:absref` | keep | Yes |
| View state | namedview: pagecolor, `inkscape:pageopacity`, `inkscape:pagecheckerboard`, `inkscape:deskcolor`, zoom/cx/cy/window-*, `inkscape:snap-*`, `inkscape:lockguides`, `inkscape:clip-to-page` | keep verbatim | n/a |

### Notable LPEs

Each LPE is listed as its `effect=` key, then its parameters, then any extra objects it creates. Parameter formats ([parameter/](https://gitlab.com/inkscape/inkscape/-/tree/master/src/live_effects/parameter)):
- A path parameter is either inline path data or `"#id"`.
- Arrays are separated by `" | "`; per-node satellites by `" @ "`.
- Random parameters are `"value;seed"`, so output is reproducible.

- **Power Stroke** (`powerstroke`): `offset_points="t,w | …"`, interpolator_type/beta, scale_width, linecaps, linejoin, miter_limit. On apply it turns the stroke into a fill; any existing fill moves to a linked path referenced by `inkscape:linked-fill` ([fill-conversion.cpp](https://gitlab.com/inkscape/inkscape/-/blob/master/src/live_effects/fill-conversion.cpp)). The Pencil tool with pressure creates it from pressure samples ([pencil-tool.cpp](https://gitlab.com/inkscape/inkscape/-/blob/master/src/ui/tools/pencil-tool.cpp)).
- **Taper Stroke** (`taper_stroke`): stroke_width, attach_start, end_offset, start/end_smoothing, plus shape and join enums. Same stroke-to-fill conversion.
- **Pattern Along Path** (`skeletal`): pattern, prop_scale, scale_y_rel, spacing, normal_offset, tang_offset, copy type.
- **Bend** (`bend_path`): bendpath, prop_scale, vertical.
- **Envelope** (`envelope`): bendpath1–4.
- **Lattice** (`lattice2`): gridpoint* parameters.
- **Perspective/Envelope** (`perspective-envelope`): four corner points.
- **Spiro** (`spiro`): no parameters. Corner or smooth point types are inferred from the handles in `original-d`.
- **BSpline** (`bspline`): weight, steps, uniform. The control polygon is in `original-d`; weights are implied by handle positions. The Pen and Pencil Spiro/BSpline modes apply these two LPEs ([freehand-base.cpp](https://gitlab.com/inkscape/inkscape/-/blob/master/src/ui/tools/freehand-base.cpp)).
- **Offset** (`offset`): offset, unit, linejoin, miter_limit.
- **Corners (Fillet/Chamfer)** (`fillet_chamfer`): `nodesatellites_param` per node, plus mode, radius, flexible.
- **Simplify** (`simplify`): threshold, steps. The dense input survives in `original-d`.
- **Roughen** (`roughen`): displace_x/y and segments, all seeded.
- **Dashed Stroke** (`dashed_stroke`): numberdashes, holefactor, splitsegments. The dashes are real geometry in `d`.
- **Clone Original** (`clone_original`): `linkeditem="#id"`, allow_transforms, plus lists of attributes and CSS properties to copy.
- **Fill Between Many** (`fill_between_many`): `linkedpaths="#a,0,1 | …"`.
- **Boolean Operation** (`bool_op`): `operand-path="#id"`, operation, swap-operands. It hides the operand with `filter:url(#selectable_hidder_filter)` ([lpe-bool.cpp](https://gitlab.com/inkscape/inkscape/-/blob/master/src/live_effects/lpe-bool.cpp)).
- **Mirror** (`mirror_symmetry`), **Rotate Copies** (`copy_rotate`), **Tiling** (`tiling`, 1.2), **Slice** (`slice`, 1.1): with `split_items`, they create **real sibling elements** listed in `lpesatellites="#id | …"`. Without it, the copies are fused into `d`.
- **Measure Segments** (`measure_segments`): creates text, path and marker satellites.
- **PowerClip / PowerMask** (`powerclip`, `powermask`): change the clip or mask contents.

### Transforms (`sp-item.cpp` `doWriteTransform`)

- **Default is "optimized"** (`/options/preservetransform/value=0`). Inkscape writes transforms into the geometry:
  - Paths: `d` is rewritten.
  - Rect, ellipse and image: x/y/w/h (and rx/ry) are scaled; any rotation or skew left over stays in `transform`.
  - Star and spiral: only a *uniform* scale is absorbed; anything else stays in `transform`.
  - Text: font size and x/y are scaled; rotation is kept.
  - Groups always keep `transform`.
- **Optimization is skipped** when the item has a clip, a mask, or a filter (unless the move is a pure translation), and when the item's LPE opts out.
- **Stroke scaling.** With "Scale stroke width" on (the default), stroke width and dash lengths are multiplied by `descrim = sqrt(|det|)`.
  - **After a non-uniform scale, the stroke therefore stays isotropic, at the geometric mean of the two scales.** That differs from what SVG would draw with the `transform` kept (an elliptically stretched stroke).
  - With the preference off, the stroke is compensated by 1/descrim.
  - Rounded corners, patterns, hatches and gradients are scaled or compensated by their own preferences (all "transform with object" by default).
  - Gradients are converted to `userSpaceOnUse` first.
- **"Preserved"** keeps the `transform` attribute. Stroke compensation still applies.

### What Inkscape keeps and drops on load and save

- **The whole XML tree is kept** ([repr-io.cpp](https://gitlab.com/inkscape/inkscape/-/blob/master/src/xml/repr-io.cpp)):
  - Unknown elements, attributes and namespaces, comments, processing instructions and CDATA all survive.
  - Unknown element types get no object, so they are kept but not rendered. `foreignObject` is one of them.
  - Known namespaces are renamed to Inkscape's prefixes; unknown ones keep theirs.
- **It adds ids to every svg:/sodipodi:/inkscape: element that lacks one**, when the element is built (`SPObject::invoke_build`). This is from reading the code: expect `path1`-style ids after a round trip.
- **Style.** Each property remembers where it came from: the `style` attribute, a presentation attribute, or a stylesheet, and is written back to the same place. `class` and `<style>` survive; CSS is parsed with libcroco. Edits go into `style`.
- **Garbage collection.** Defs marked `inkscape:collect="always"` are deleted automatically once nothing references them. Other unused defs stay until Clean Up Document.
- **Migrations on load, keyed on `inkscape:version`:** pre-0.92 DPI and baseline, pre-1.0 empty lines, pre-1.1 `osb:paint`, pre-1.2 feComposite, pre-1.5 hotspot, and pages to `svg:view` ([document.cpp](https://gitlab.com/inkscape/inkscape/-/blob/master/src/document.cpp)).
- **Save works on a copy** ([output.cpp](https://gitlab.com/inkscape/inkscape/-/blob/master/src/extension/output.cpp)). It removes Adobe `i:pgf`, sets the SVG version, inserts the text fallback and the mesh/hatch polyfills, and optionally applies the SVG 1.1 marker rewrites.
- **Plain SVG** additionally strips every `inkscape:` / `sodipodi:` attribute and element ([svg.cpp](https://gitlab.com/inkscape/inkscape/-/blob/master/src/extension/internal/svg.cpp)). That removes LPE definitions, shape parameters, layer names, namedview, pages before 1.5, and guides. The evaluated `d`, the `-inkscape-*` CSS properties, and the 1.5 `<view>` elements remain. The [manual](https://inkscape-manuals.readthedocs.io/en/latest/saving.html) describes Plain SVG as being for non-Inkscape use.

### Operations that leave no editable source

- **Shape Builder** (1.3): new paths. In 1.4, an image selected with paths is split into clipped clones.
- **Boolean operations**, **Combine** and **Break Apart**: plain paths. Clones are unlinked first (1.0).
- **Stroke to Path**: a group holding the fill, the stroke outline and a markers group (1.0).
- **Object to Path**: flattens the LPE stack. Since 1.3 a whole text becomes a single path, and clones are converted or only unlinked depending on a preference.
- **Calligraphy tool**: a plain filled path with pressure and tilt baked into the outline ([calligraphic-tool.cpp](https://gitlab.com/inkscape/inkscape/-/blob/master/src/ui/tools/calligraphic-tool.cpp)).
- By contrast, Pencil with pressure, and the Pen/Pencil Spiro, BSpline and shape modes (which add Power Stroke, Pattern Along Path or Bend), all stay editable as LPEs.

## Inkscape design patterns that matter for a new format

1. **Parametric source plus evaluated output.** Every generated object also writes standard geometry (`d`, or native rect/circle). Inkscape treats the parameters as the truth and regenerates on load or edit. Any other reader still draws correctly, but a foreign edit to the output is silently overwritten. An importer must either keep both and re-evaluate, or drop the parameters when it edits the geometry.
2. **Effects as referenced objects.** An ordered list of ids points to effect nodes in defs. All parameters are strings, and each effect carries a version (`lpeversion`). Linked inputs are id references (`#id`), so ids must stay stable.
3. **Keep unknown XML.** The editor's model is a view over a full DOM, which is why round-trips don't lose data. Inkscape still adds ids and renames known prefixes.
4. **Style provenance.** Each property is written back to where it came from.
5. **Shared resources through href chains, forked when edited and collected when unused.** This covers gradient vectors and pattern link/root pairs.
6. **Fallbacks are made at save time, not stored in the model.** Text fallback and polyfills are written to the copy being saved and ignored on load.
7. **Standard SVG first; metadata in namespaces.** Plain SVG is just Inkscape SVG with the namespaces stripped, and 1.5 is moving pages into standard `svg:view`.
8. **Derived objects are real elements with a list pointing back** (split satellites, `linked-fill`). There are also hidden helpers: text frames in defs, Boolean operands hidden by a filter.
9. **Migrations are gated on the stored version number**, with no general schema machinery.
10. **Baking transforms loses information** (stroke anisotropy, original parameters). A new format should keep transform, geometry and stroke-scaling behavior explicit and separate.
11. **Coordinate quirks to normalize:** guides and rotation centers store y up-positive.

## Uncertain items

- Everything about 1.5 comes from unreleased master and could change before release.
- Whether the 1.5 color picker writes non-sRGB functions such as `oklch()` into files by default. The code suggests the chosen space is kept; I did not run it.
- Whether a wrapped-text fallback keeps hard line breaks distinct from soft wraps on reload.
- Auto-smooth nodes date from 0.92. That is from memory; no source was checked.
- Whether LPEs are re-evaluated on open or only at the next update. Either way, a foreign `d` edit is lost.
- The id-on-load behavior is from reading the code, not from a test run.
