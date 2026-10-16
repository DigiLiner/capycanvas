# SVG data-model inventory (status as of 2026-10-15)

[Object content extensions](../object-content-extensions-research.md) · source report, 2026-10-15

## 0. Where the specs stand

- **SVG 2 has a new Candidate Recommendation Snapshot**, published 06 Oct 2026 (https://www.w3.org/TR/2026/CR-SVG2-20261006/). It cannot become a Recommendation before 01 Dec 2026. Three features are marked at risk:
  - more than one `title`/`desc` for localisation;
  - nested links;
  - `vector-effect` values other than `non-scaling-stroke`.
- **Removed or deferred in SVG 2** (Changes appendix, https://www.w3.org/TR/SVG2/changes.html):
  - Mesh gradients and `hatch`/`hatchpath` are deferred "to a future version". `solidcolor` is removed.
  - `stroke-linejoin: miter-clip | arcs` were removed in March 2026 (svgwg #1065, closed 2026-03-18, https://github.com/w3c/svgwg/issues/1065). The property is now only `miter | round | bevel`.
  - `shape-inside`/`shape-subtract` are removed. The WG resolved on 2026-04-23 to "Drop text wrapping in CSS shapes for SVG2" (https://www.w3.org/2026/04/23-svg-minutes.html).
  - `inline-size` is kept, but issue #1087 is still open (https://github.com/w3c/svgwg/issues/1087). All the `text-inline-size` WPT reftests fail in every browser. An editor said it is "still at risk".
  - Multiple paints in `fill`/`stroke` were rolled back. `child()` paint and `z-index` were removed.
  - Also gone: `tref`, `altGlyph*`, SVG fonts, `kerning`, the `cursor` element, `requiredFeatures`, `xml:base`, `baseProfile`/`version`, the `clip` property and `color-rendering`.
- **Kept or new in SVG 2:**
  - painting: `paint-order`, `context-fill`/`context-stroke`, `url() [none|<color>]` fallback, `fr`, `auto-start-reverse`, markers on all shapes;
  - structure: `href` without xlink, `transform-origin` (UA default `0 0`), `symbol` with `x/y/width/height/refX/refY`;
  - geometry: `d` as a CSS property, a new `path-length` property (in the 2026 CR), `pathLength` on all shapes, `rx/ry:auto` on `ellipse`;
  - text: `textPath` `path` and `side`, `white-space`.
- **Companion specs:**
  - CSS Masking 1 is a Candidate Recommendation Draft (CRD) of 2021-08-05 (https://www.w3.org/TR/css-masking-1/).
  - Compositing 1 is a CRD of 2024-03-21 (https://www.w3.org/TR/compositing-1/).
  - Compositing 2 is an Editor's Draft (ED) of 2025-12-13 (https://drafts.csswg.org/compositing-2/). `plus-lighter` is added to the `mix-blend-mode` grammar only, not to `<blend-mode>`. `plus-darker` exists only for canvas.
  - Filter Effects 1 is a Working Draft of 2018-12-18 (https://www.w3.org/TR/filter-effects-1/).
  - Fill and Stroke 3 is an ED of 2026-03-04 (https://drafts.csswg.org/fill-stroke-3/). It adds `stroke-align`, layered fills and strokes, and `stroke-dash-corner`/`-justify`. Much of it is unresolved, and I found no implementations.
  - SVG Strokes is an ED of 2025-09-14 (https://svgwg.org/specs/strokes/). It calls itself "an early draft that largely contains features that were dropped during the development of SVG 2". The SVG 2 stroke definition stays normative.
- **Browser data source.** Version numbers below come from MDN browser-compat-data (BCD) v8.1.6, timestamp 2026-10-15 (https://github.com/mdn/browser-compat-data). Format is C = Chrome, F = Firefox, S = Safari.

## A. Feature inventory

Key: **Render** is the data needed to draw it faithfully. **Edit extras** is what you need beyond that to re-export it unchanged and editable. "all" means long supported by every engine.

| Feature | Render | Edit extras | Browsers (SVG 2 items) |
|---|---|---|---|
| Root `svg` | width/height, viewBox, preserveAspectRatio (default `xMidYMid meet`) | authored units (mm/in), namespaces, unknown attributes | all |
| `g` / layers | child order, transform, opacity, blend, isolation, clip, mask, filter | id, class, labels (`inkscape:label`/`groupmode`), title/desc, lock/hide state | all |
| `defs`/`symbol`/`use` | resolved instance tree. `use` x/y act as a translate; `symbol` behaves as a nested viewport | the `use`→target link, use x/y/w/h, symbol viewBox, pAR, refX/refY. Styles set on `use` reach the instance by inheritance | all. symbol refX/refY: no BCD entry, probably unimplemented (unverified). `use` with a `data:` URL was removed (C 120, F 122, S never) |
| Nested `svg` | viewport rect, viewBox, pAR, overflow clip | keep it as an element; % lengths resolve against it | all |
| `switch` + `systemLanguage` | the chosen branch | all branches | all |
| `a` | treat as `g` | href, target | all; nested links at risk |
| `title`/`desc`/`metadata` | none | text, lang, RDF | all |
| `path d` | M/L/C/Q/Z; arcs (A) must become exact geometry | command types (A, relative, H/V, S/T), number text, `pathLength` | all. `d` as CSS property: C 52, F 97, S no |
| rect/circle/ellipse/line/polyline/polygon | the spec's "equivalent path", which fixes where dashes start and where marker vertices fall | the element and its parameters (rx/ry, `auto`) | all |
| `transform` | 2D affine | the authored function list (`rotate(a cx cy)`, order), `transform-origin`, `transform-box`. Attribute and CSS syntax differ | transform-origin on SVG: C 19, F 43, S 6. transform-box: C 64, F 55, S 11. 3D: see B10 |
| fill/stroke `<paint>` | `none`, color, paint-server reference with fallback, `context-fill`/`context-stroke`, `currentColor` | the keyword (`currentColor`, `context-*`), `var()`, the fallback, how the color was written | `context-*`: C 124, F 111, S no |
| Colors (CSS Color 4/5) | float components plus color space (display-p3, lab/lch, oklab/oklch, `color()`, `color-mix()`, `light-dark()`) | the authored form | C 111, F 113, S 15–16.2. That data is for CSS in general. Parsing in SVG attributes and wide-gamut SVG output are unverified |
| Stroke properties | width, linecap, linejoin (miter/round/bevel), miterlimit (any non-negative value), dasharray/dashoffset, opacity; `pathLength` rescales dashes | % values, authored strings | all |
| `paint-order` | per-element order of fill, stroke and markers | authored value | C 35, F 60, S 8. HTML text only from C 123 and S 11 |
| `vector-effect` | `non-scaling-stroke` | the other values (at risk) | `non-scaling-stroke`: C 6, F 15, S 5.1. Other values: none |
| `linearGradient` | x1/y1/x2/y2, units, gradientTransform, spreadMethod, stops (offset, color, opacity), color-interpolation | the `href` template chain, sharing by id, % vs number, Inkscape split gradients (stops in one, geometry in another) | `href` without xlink: C 50, F 51, S 12.1. `linearRGB` interpolation: **Firefox 123 only** (https://developer.mozilla.org/docs/Mozilla/Firefox/Releases/123) |
| `radialGradient` | cx, cy, r, fx, fy, fr. A focal circle outside the end circle draws a cone (SVG 2, aligned with Canvas) | same as above | fr: C 24, F 55, S 7 |
| `pattern` | x/y/w/h, patternUnits (default objectBoundingBox), patternContentUnits (default userSpaceOnUse), viewBox/pAR, patternTransform, content, overflow hidden | `href` inheritance of attributes and children | all |
| mesh / hatch / solidcolor | — | opaque XML, or convert | no browser support |
| `marker` | markerWidth/markerHeight (3), refX/refY, orient (auto, auto-start-reverse, angle), markerUnits (strokeWidth), viewBox, overflow, content; marker-start/mid/end | keep the marker definition and the references, not the placed instances | all. auto-start-reverse and markers on every shape: per-engine support not verified |
| opacity, fill-opacity, stroke-opacity | values | — | all |
| `mix-blend-mode` (16 modes plus `plus-lighter`), `isolation` | per-element blend mode and isolation | **no presentation attribute exists**; they must be written in style or CSS (https://www.w3.org/TR/SVG2/styling.html) | on SVG elements: C 41, F 32. **S: BCD says not supported** (check this). plus-lighter in general: C 100, F 99, S 9.1. isolation: C 41, F 36, S 8 |
| `clip-path` | `url(#clipPath)` (units, children, clip-rule, per-child transform), or a basic shape with a geometry box | the clipPath definition and sharing | `url()`: all. Basic shapes on SVG: C 23, F 52, S 7. Geometry boxes: C 119, F 51–54, S 13 |
| `mask` | maskUnits (objectBoundingBox), region −10%/−10%/120%/120%, maskContentUnits, mask-type (luminance/alpha), content | CSS mask-image layers, mask-mode, mask-composite | mask-type: C 24, F 35, S 7. Unprefixed CSS `mask`: C 120, F 53, S 15.4 |
| `filter` | region (filterUnits objectBoundingBox), primitiveUnits (userSpaceOnUse), the primitive graph, color-interpolation-filters (default linearRGB); CSS filter functions (always sRGB); filter chains | primitive `result` names, labels | element: all. feDropShadow: C 13, F 30, S 6. CSS filter functions on SVG elements: C 89, F 35, S no. feGaussianBlur edgeMode: Safari only |
| `text`/`tspan` | characters, font properties, per-code-point x/y/dx/dy/rotate lists, textLength/lengthAdjust, text-anchor per chunk, baseline properties, letter/word-spacing, writing-mode, text-orientation, direction, white-space/xml:space, decoration | the fonts (embedded or referenced), span structure, whitespace as written | SVG 1.1 subset: all. `tspan textLength`: F no. text-orientation: C 48, F 41, S 14 |
| `textPath` | href to a path or basic shape, startOffset, method, spacing, side, `path` | the link to the path | `path` and `side`: **Firefox 61 only** |
| Text wrapping (`inline-size`, `shape-inside`) | line layout | Inkscape and Krita write these | no browser |
| `image` | href (`data:` or external), x/y/w/h (auto), pAR, image-rendering. An SVG used as an image runs in secure mode (no script, no external loads) | the original encoded bytes or the URL | all. `pixelated`: C 41, F 93, S 10 |
| `foreignObject` | needs an HTML engine | opaque XML | browsers yes; resvg and Inkscape do not render it (Inkscape unverified) |
| CSS styling | the computed style | the `<style>` text, class lists, which origin set each value, `!important`, `var()`, `@media`, `@font-face` | all |

## B. Model constraints (rules that force structure)

1. **Groups, isolation and effect order.**
   - A `g` is non-isolated by default, and so is `use`.
   - A group becomes isolated with opacity, filters, masking, blending or 3D transforms. Clipping no longer isolates; the Compositing 1 change log removed it. `svg` always isolates. Graphical-effect content (clipPath, mask, pattern, marker) renders as its own isolated fragment (https://www.w3.org/TR/SVG2/render.html).
   - Isolated group steps: render the children onto transparent black, apply effects, then the group transform, then blend and composite.
   - The CSS Masking order is filter → clip → mask → opacity. Each node therefore needs fixed effect slots. Any other order needs nested groups.
   - In a non-isolated group, children with `mix-blend-mode` blend with everything back to the nearest isolated ancestor. The GPU compositor needs that backdrop.
2. **Inheritance.**
   - Properties inherit into a paint server, marker, clipPath or mask from **its own** ancestors, not from the element that references it (https://www.w3.org/TR/SVG2/pservers.html).
   - `use` instances inherit from the `use` element. Style sheets are re-scoped into the shadow tree, so selectors can match differently.
   - `context-fill`/`context-stroke` let a marker or `use` instance take paint from its context element.
   - Presentation attributes sit at the start of the author origin with specificity 0. Inline style and sheets override them.
   - To edit, store **specified** values and where they came from. Storing only computed values loses the file's structure.
3. **objectBoundingBox (OBB) units.**
   - Gradients and patterns default to OBB, and so do the mask and filter regions. Clip paths, mask content and filter primitives default to userSpaceOnUse.
   - The bounding box is the fill geometry without stroke. For a group it is the union of its children. A `tspan` uses the box of its `text`. Text boxes depend on font metrics.
   - One shared OBB gradient renders differently on every element that uses it, and changing geometry changes the paint. Resolving it to user space at load forces one copy per element.
   - Non-square boxes skew gradient normals, so a radial gradient becomes an ellipse.
   - If the box has zero width or height (for example a horizontal line), "the given effect (e.g., a gradient or a filter) will be ignored" (https://www.w3.org/TR/SVG2/coords.html).
4. **Transforms on paint servers.** The gradient vector is "treated as part of its own geometry". `gradientTransform` and `patternTransform` apply before the element's transform. Pattern tiles live in pattern space. A `viewBox` on a pattern overrides patternContentUnits.
5. **Clip-path content rules.**
   - Children may only be shapes, `text` and `use`. A `use` must point directly at a shape or text; anything indirect makes the clipPath invalid. `g` is not allowed.
   - Only geometry counts ("exclusive of… fill, stroke, stroke-width"). The clip is the union of the children.
   - Each child has its own transform, `clip-rule` and optional `clip-path`. `clip-path` on the clipPath element itself intersects. Children hidden by `display` or `visibility` contribute nothing (https://www.w3.org/TR/css-masking-1/).
6. **Masks.**
   - Luminance is the default for the `mask` element, using the feColorMatrix luminanceToAlpha coefficients (0.2125, 0.7154, 0.0721) multiplied by alpha.
   - It is computed in sRGB unless the mask has `color-interpolation: linearRGB` (initial value is sRGB).
   - Content can be anything, including images, filters and nested masks.
7. **Filters.**
   - The default region is −10%/−10%/120%/120% of the bounding box and it clips the result, so shadows can be cut off.
   - primitiveUnits defaults to userSpaceOnUse, so stdDeviation, offsets, turbulence frequency and light positions all scale with the transform.
   - Filter Effects says filters apply "in the element's local coordinate system" and the WD recommends an "operating coordinate space". With rotation or skew, a GPU must filter in local space, or approximate as resvg does: it scales parameters by the transform's scale only (`scale_coordinates` in resvg 0.48.1 `filter/mod.rs`).
   - `color-interpolation-filters` (default linearRGB) applies per primitive, which means a color conversion at every primitive boundary. CSS filter functions are always sRGB.
   - Chains apply in order, and one invalid `url()` cancels the whole chain.
   - `BackgroundImage`, `BackgroundAlpha`, `FillPaint` and `StrokePaint` are effectively unimplemented. usvg maps them to SourceGraphic.
   - `kernelUnitLength` (convolve and lighting) depends on resolution.
8. **Markers.**
   - Markers are instanced per vertex, including vertices of zero-length segments and consecutive movetos (https://www.w3.org/TR/SVG2/paths.html).
   - `orient=auto` uses the bisector of the incoming and outgoing tangents.
   - `markerUnits=strokeWidth` scales by stroke width. viewBox and refX/refY map the content. The UA style sheet sets overflow hidden.
   - Markers paint after the stroke unless `paint-order` says otherwise.
   - Store marker references and resolve them at render time. Shapes converted to paths must use the spec's equivalent path so vertices and dash start points match.
9. **`non-scaling-stroke`.** The stroke outline is computed in the host space (the viewport of the outermost `svg`) and transformed back. Stroke geometry then depends on every ancestor transform, including canvas zoom, so cached tessellation goes stale when the view zooms. Markers become non-scaling too.
10. **Strokes under transforms.** "Normally stroking involves calculating stroke outline of the shape's path in current user coordinate system" (SVG 2 painting chapter), and the outline is then transformed.
    - Non-uniform scale or skew therefore gives an anisotropic stroke, and dash lengths are in user units. A device-space stroker with a single scalar width is wrong except under similarity transforms.
    - 3D transforms isolate the group and switch `vector-effect` off. I found no authoritative per-browser behaviour for 3D on elements inside an `svg`, so I recommend not modelling 3D.
11. **Color spaces.**
    - `color-interpolation` (initial sRGB) covers gradient interpolation and "compositing and blending". Browsers composite in gamma-encoded sRGB. I know of no engine that composites in linearRGB (unverified).
    - Only Firefox 123+ interpolates gradients in linearRGB. resvg ignores `color-interpolation`.
    - **Stops interpolate non-premultiplied**: the formula uses `C0.rgba`, and svgwg #180 confirms this (https://github.com/w3c/svgwg/issues/180). CSS gradients, by contrast, are premultiplied. So `stop-color: transparent` is transparent black and fades through gray. Your renderer needs a non-premultiplied interpolation path for SVG gradients.
    - Filter defaults are linearRGB, as above.
12. **Defaults.**
    - Fill and stroke: fill black, fill-rule nonzero, stroke none, stroke-width 1px, linecap butt, linejoin miter, miterlimit 4, dasharray none, dashoffset 0. All opacities 1.
    - Compositing: paint-order normal, mix-blend-mode normal, isolation auto, mask-type luminance, clip-rule nonzero.
    - Markers: markerUnits strokeWidth, markerWidth/markerHeight 3, orient 0.
    - Gradients: gradientUnits objectBoundingBox, x1/y1/x2/y2 = 0%, 0%, 100%, 0%. Radial cx/cy/r = 50%, fx/fy = cx/cy, fr = 0%. spreadMethod pad. Stops: black, opacity 1.
    - Patterns: patternUnits objectBoundingBox, patternContentUnits userSpaceOnUse.
    - Clip, mask, filter: clipPathUnits userSpaceOnUse; maskUnits objectBoundingBox, maskContentUnits userSpaceOnUse; filterUnits objectBoundingBox, primitiveUnits userSpaceOnUse.
    - Overflow is visible, but the UA style sheet hides it for non-root `svg`, `image`, `marker`, `pattern` and `symbol`. transform-origin is `0 0`, except on the root `svg`.
    - Color interpolation: sRGB for painting, linearRGB for filters.
    - Text and misc: font-size medium (16px), text-anchor start, flood-color black, lighting-color white.
13. **Units.**
    - A user unit is a px; 1in = 96px, and mm, pt and pc follow from that.
    - % lengths use the nearest viewport: width for x, height for y, and √((w²+h²)/2) for other lengths. em uses the element's font-size.
    - Converting to px at load loses the authored units, so store a length as value plus unit.
14. **Text.**
    - Positioning lists index Unicode **code points**, while the DOM indexes UTF-16 code units.
    - An absolute x/y starts a new text chunk, and text-anchor applies per chunk.
    - On a closed path, `textPath` renders one loop. Layout depends on the fonts, so an editable model must keep text plus font references and cannot pre-flatten to outlines.

## C. usvg 0.48.1 (latest on crates.io, 2026-08-02)

The crate docs say: "All supported attributes are resolved", "CSS will be applied", and "Only simple paths"; the full list is at https://docs.rs/usvg/0.48.1/usvg/. I read the field lists below from the 0.48.1 source, `src/tree/{mod,filter,text}.rs`. All fields are `pub(crate)` with accessor methods.

**Tree:** `size`, `root: Group`, `linear_gradients`, `radial_gradients`, `patterns`, `clip_paths`, `masks`, `filters` (each a `Vec<Arc<_>>`), `fontdb`.

**Node:** `Group | Path | Image | Text`.

**Group:**
- `id`, `transform`, `abs_transform`, `opacity`, `blend_mode`, `isolate`, `clip_path: Option<Arc<ClipPath>>`, `is_context_element`, `mask: Option<Arc<Mask>>`, `filters: Vec<Arc<Filter>>`, `children`.
- Bounding boxes: `bounding_box`, `abs_bounding_box`, `stroke_bounding_box`, `abs_stroke_bounding_box`, `layer_bounding_box`, `abs_layer_bounding_box`.
- `should_isolate()` is true for `isolate`, opacity ≠ 1, clip, mask, filters, or a non-normal blend mode.

**Path:** `id`, `visible`, `fill: Option<Fill>`, `stroke: Option<Stroke>`, `paint_order` (`FillAndStroke | StrokeAndFill`, no markers), `rendering_mode` (shape-rendering), `data: Arc<tiny_skia_path::Path>`, `abs_transform`, the four bounding boxes.
- A Path has no transform and no opacity of its own. Effects exist only on Groups, and usvg wraps a path in a group when it needs them.

**Fill:** `paint`, `opacity`, `rule`, `context_element`.

**Stroke:** `paint`, `dasharray: Option<Vec<f32>>`, `dashoffset`, `miterlimit` (≥1), `opacity`, `width`, `linecap`, `linejoin` (`Miter | MiterClip | Round | Bevel`), `context_element`.

**Paint:** `Color(Color{red, green, blue: u8})`, `LinearGradient(Arc)`, `RadialGradient(Arc)`, `Pattern(Arc)`. Color is 8-bit sRGB only; alpha is moved into the opacity fields.

**Paint servers:**
- `BaseGradient`: `id`, `units` (used only during parsing), `transform`, `spread_method`, `stops: Vec<Stop{offset, color, opacity}>`.
- `LinearGradient`: `base`, `x1, y1, x2, y2`.
- `RadialGradient`: `base`, `cx, cy, r, fx, fy, fr`.
- `Pattern`: `id`, `units` and `content_units` (parse only), `transform`, `rect`, `view_box: Option<ViewBox>`, `root: Group`.

**ClipPath:** `id`, `transform`, `clip_path`, `root`.

**Mask:** `id`, `rect`, `kind` (`Luminance | Alpha`), `mask`, `root`.

**Filter:**
- `Filter`: `id`, `rect`, `primitives`.
- `Primitive`: `rect`, `color_interpolation` (`SRGB | LinearRGB`), `result`, `kind`.
- `kind` has 17 variants: Blend, ColorMatrix, ComponentTransfer, Composite (including arithmetic k1–k4), ConvolveMatrix, DiffuseLighting, DisplacementMap, DropShadow, Flood, GaussianBlur, Image (`root: Group`), Merge, Morphology, Offset, SpecularLighting, Tile, Turbulence.
- `Input` is only `SourceGraphic | SourceAlpha | Reference(String)`.

**Image:** `id`, `visible`, `size` (the image's intrinsic size), `rendering_mode`, `kind` (`JPEG | PNG | GIF | WEBP(Arc<Vec<u8>>)` or `SVG(Tree)`), `abs_transform`, `abs_bounding_box`. preserveAspectRatio is turned into a transform, plus a clip for `slice`.

**Text:**
- `Text`: `id`, `rendering_mode`, `dx`, `dy`, `rotate` (one per code point), `writing_mode` (`LeftToRight | TopToBottom`), `chunks`, `abs_transform`, the bounding boxes, `flattened: Box<Group>` (glyphs as paths), `layouted: Vec<Span>`.
- `TextChunk`: `x`/`y: Option`, `anchor`, `spans`, `text_flow` (`Linear | Path(TextPath{id, start_offset, path})`), `text`.
- `TextSpan`: `start`, `end`, `fill`, `stroke`, `paint_order`, `font{families, style, stretch, weight, variations}`, `font_size`, `small_caps`, `apply_kerning`, `font_optical_sizing`, `decoration{underline, overline, line_through}`, `dominant_baseline`, `alignment_baseline`, `baseline_shift`, `visible`, `letter_spacing`, `word_spacing`, `text_length`, `length_adjust`.

**BlendMode:** 16 modes. `PlusLighter` is on resvg `main` but **not in 0.48.1**.

**What usvg resolves away:**
- CSS and the style attribute, `inherit`.
- `use` and `symbol` become copied subtrees, so the links are lost.
- Nested `svg` becomes a group plus clip. `switch` is resolved and `a` becomes `g`.
- Markers become ordinary groups of paths.
- Shapes become paths and arcs and relative commands become absolute M/L/Q/C/Z, so `rect rx` and arcs are lost.
- Units become px.
- **OBB becomes userSpaceOnUse**, which copies paint servers per element.
- CSS filter functions become Filter objects with computed regions.
- Text is laid out and flattened.
- Groups with no effect collapse, but `g`/`use` are kept so their ids survive.
- There is also a writer, `Tree::to_string(WriteOptions{preserve_text…})`.

**What it does not support:**
- `vector-effect` (the name is parsed but has no effect).
- `color-interpolation` for gradients and masks.
- CSS Color 4: svgtypes 0.16.1 parses only hex, `rgb()`/`hsl()` and named colors, so no lab, oklch, `color()` or `color-mix()`.
- `var()`; pseudo-classes other than `:first-child`.
- Mesh and hatch.
- BackgroundImage, BackgroundAlpha, FillPaint, StrokePaint.
- SVG fonts.
- Text: `inline-size` and shape wrapping, textPath `side`/`path`, `text-orientation`, `white-space`.
- symbol refX/refY, `transform-box`, `use` of external files.
- CSS mask-image/mask-composite and basic-shape clip-path.
- blur edgeMode.
- foreignObject, animation, script, events, `view`, `cursor`.

Sources: https://github.com/linebender/resvg/blob/main/docs/unsupported.md and https://github.com/linebender/resvg/blob/main/docs/svg2-changelog.md. That changelog is partly stale: it marks `fr` as unsupported, yet 0.48.1 parses it.

**As a render model:** it is a good minimal static tree, but it is lossy for editing. It has no instancing, no units, no shared OBB servers, no float or wide-gamut color, and no non-scaling-stroke.

## D. Non-static and rarely edited features

| Feature | Status and browsers | How editors treat it |
|---|---|---|
| SMIL (`animate`, `set`, `animateTransform`, `animateMotion`, `mpath`) | moved out of SVG 2 into SVG Animations; all browsers | Inkscape neither plays nor edits it. Because Inkscape works on the XML tree it probably preserves it (unverified). Illustrator: probably dropped (unverified) |
| CSS `@keyframes` / transitions | all browsers | kept only as `<style>` text |
| script, `on*` attributes | browsers; disabled when SVG is used as an image | Inkscape's Object Properties has an Interactivity section for `on*` attributes. Illustrator has a legacy SVG Interactivity panel (unverified) |
| `foreignObject` | all browsers | needs an HTML engine; resvg skips it |
| `view`, fragment identifiers, `cursor` | `view` is supported; the `cursor` element is removed | editors ignore them |
| `discard` | moved to SVG Animations; no BCD entry | — |
| Mesh gradients | not in SVG 2 | Inkscape edits them and writes `meshgradient` plus a JavaScript polyfill (Inkscape 1.0). The polyfill does not run when the SVG is shown in `<img>` (https://wiki.inkscape.org/wiki/Mesh_Gradients). Illustrator reportedly rasterizes gradient meshes on SVG export (unverified) |
| Wrapped text | — | Inkscape writes SVG 2 `shape-inside`/`inline-size` with an SVG 1.1 fallback, plus legacy `flowRoot` (https://wiki.inkscape.org/wiki/SVG2_Text_Bugs). **Krita 5.3** implements SVG 2 wrapping, including subtractive flow shapes, text on path and paint-order. It is a painting app, so it is a direct precedent for you (https://krita.org/en/release-notes/krita-5-3-release-notes/) |
| Live path effects | — | Inkscape keeps the source in `inkscape:original-d` and the effects in `inkscape:path-effect` (definitions in `defs`), and writes the computed result to `d`. That is a useful pattern: authored parameters plus a baked fallback |
| Illustrator private data | — | "Preserve Illustrator Editing Capabilities" embeds AI data in `<i:pgf>`. On reopen Illustrator reads that block instead of the SVG, so edits made in other apps are lost (forum reports; Adobe help pages blocked my fetches). An effect placed after an SVG filter makes Illustrator export a raster (Adobe help, quoted in a search result) |

## Bottom line

To round-trip full SVG, the model must be a DOM-like authored tree:
- element types and shapes, use/symbol links, shared paint servers with units and `href` chains;
- specified values with their origin, plus the style sheets;
- lengths as value plus unit, float colors with a color space;
- marker and filter definitions as references;
- text as characters plus fonts;
- opaque preservation for anything not rendered.

A usvg-style normalized tree can serve as the derived render cache. It cannot be the stored format.

## Not verified

- Safari `mix-blend-mode` on SVG elements: BCD says no.
- `auto-start-reverse` and markers on every shape, per engine.
- symbol refX/refY support.
- Behaviour of 3D transforms in SVG.
- Wide-gamut color in SVG attributes.
- That no engine composites in linearRGB.
- Mask luminance with `linearRGB` in browsers.
- Inkscape and Illustrator handling of SMIL and script.
- Illustrator's mesh export.
