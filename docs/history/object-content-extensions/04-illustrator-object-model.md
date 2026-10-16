# Illustrator object model and SVG: feature inventory (current to Illustrator 30.8, August 2026)

[Object content extensions](../object-content-extensions-research.md) · source report, 2026-10-15

## How I researched this

- **Version.** The latest documented release is **30.8 (August 2026)**, according to Adobe's [What's new](https://helpx.adobe.com/illustrator/using/whats-new.html) page. The 30.x line began with 30.0 in October 2025.
  - 30.7 added a Blend panel with Ease In/Ease Out (with a strength setting) and color acceleration that is separate from spacing ([announcement](https://community.adobe.com/announcements-651/illustrator-30-7-new-blend-panel-ease-in-out-blends-generate-artwork-with-firefly-bug-fixes-and-more-1635245)).
  - I found no 31.0 yet.
- **Adobe help pages.** helpx.adobe.com returns 403 to automated clients, so I read those pages through Wayback Machine snapshots from 2024 to 2026.
- **Object model.** This comes from the scripting reference and from Illustrator C++ SDK headers mirrored on GitHub:
  - [2019 SDK AIArt.h](https://github.com/Saiga1105/rhino.inside-revit/blob/2a644f9cd5ec2c709485bf12827ed9809b7478d7/Adobe/Illustrator/plugin/Illustrator2019SDK/illustratorapi/illustrator/AIArt.h)
  - [a newer SDK that includes AIRepeat.h](https://github.com/WestonThayer/Bloks/tree/309d67476219212de14c213078446ae9af87ac51/BloksAIPlugin/Vendor/illustratorapi/illustrator)
- **SVG output.** I checked claims against Adobe's documentation and against real SVG files exported by Illustrator on GitHub (found with code search).
- **SVG import.** Adobe documents almost none of it, so most import claims come from forum reports. Adobe's own advice is not to reopen exported SVG and to keep the .ai file as the master ([Adobe Learn](https://www.adobe.com/learn/illustrator/web/export-svg)).

**Flags used below:** **[O]** seen in real Illustrator-exported files; **[C]** community or forum report; **[I]** my inference from the data model; **[U]** unverified.

## 1. How Illustrator stores artwork

- **Item kinds.** The SDK's `AIArtType` lists:
  - `kGroupArt`, `kPathArt`, `kCompoundPathArt`, `kPlacedArt`, `kRasterArt`, `kPluginArt`, `kMeshArt`, `kTextFrameArt`, `kSymbolArt`
  - `kForeignArt`: a "black box containing drawing commands". This is the scripting `NonNativeItem`.
  - `kLegacyTextArt`: text from AI10 or earlier.
  - `kChartArt`: graphs, now deprecated.

  Scripting exposes the matching typenames PathItem, CompoundPathItem, GroupItem, TextFrame, PlacedItem, RasterItem, MeshItem, PluginItem, SymbolItem, GraphItem, NonNativeItem and LegacyTextItem. Scripts cannot create graph, mesh, plugin or legacy text items ([artwork tree](https://ai-scripting.docsforadobe.dev/objectmodel/theArtworkTree/)).
- **Live features fall into two mechanisms.**
  - **Plugin groups.** These hold *edit art* (the source children) plus a regenerated *result art* group (AIPluginGroup.h). Blends, envelopes, Live Paint, Image Trace, Repeat and compound shapes work this way. Scripting exposes `PluginItem.isTracing` and `.tracing` for traced items ([PluginItem](https://ai-scripting.docsforadobe.dev/jsobjref/PluginItem/)).
  - **Live effects inside the art style (Appearance).** The advanced Stroke panel features are stored this way: variable width, arrowheads, dash alignment, how a gradient is applied to a stroke, and brushes. They live in a "Beautiful Strokes" parameter dictionary inside a live-effect parameter dictionary (AIBeautifulStrokes.h).
- **Art style (Appearance)** is made of (AIArtStyleParser.h):
  - pre-effects;
  - ordered *paint fields*, each a fill or a stroke with its own effects, its own transparency, and a "color position" marking where in its effect chain the paint is applied;
  - post-effects;
  - an overall transparency entry;
  - for groups and text, a "Group Contents" position that sets where the children render relative to the group's own fills and strokes.
- **Transparency per object** (AIMask.h):
  - blend mode, opacity, isolated, knockout (off, on or inherit), and alpha-is-shape (the "Opacity & Mask Define Knockout Shape" option);
  - document-level isolated and knockout flags;
  - an opacity mask with mask art plus `linked`, `disabled`, `inverted` and `clipping` flags.

  Scripting has the same set as `blendingMode`, `opacity`, `isIsolated` and `artworkKnockout` (DISABLED, ENABLED or INHERITED) ([PageItem](https://ai-scripting.docsforadobe.dev/jsobjref/PageItem/), [constants](https://ai-scripting.docsforadobe.dev/jsobjref/scripting-constants/)).
- **Paths.**
  - Each segment is `{p, in, out, corner: bool}` (AIPath.h). Scripting's PathPoint has `anchor`, `leftDirection`, `rightDirection` and `pointType` (CORNER or SMOOTH) ([PathPoint](https://ai-scripting.docsforadobe.dev/jsobjref/PathPoint/)).
  - The path style holds fillPaint, strokePaint, fill, `evenodd` and resolution. The stroke holds color, overprint, width, a dash array of at most 6 values plus an offset, cap, join and miter limit (AIPathStyle.h).
- **Gradients** (AIColor.h).
  - Stops: rampPoint (0–100), midPoint (13–87, a percentage of the way to the next stop), color, and opacity (0–1).
  - Each use of a gradient on an object stores: origin, angle, length (the radius for radial), an accumulated matrix, hiliteAngle, and hiliteLength (a 0–1 fraction of the radius). A radial gradient's aspect ratio and rotation sit in that matrix [I].
- **Patterns.** Each use of a pattern stores the pattern reference plus shiftDist, shiftAngle, scale(x, y), rotate, reflect, reflectAngle, shearAngle, shearAxis and a matrix. Tiling starts from the ruler origin. "Transform patterns" is this per-use transform.
- **Colors.** Gray, CMYK, RGB, Lab (spot colors only), or a custom color: a swatch reference plus a tint, flagged as spot or global process, and possibly as registration.
- **Repeat** (AIRepeat.h):
  - **Radial:** center, radius, start angle, total angle, rotation, number of instances, reverse overlap, clockwise, and a merge mode (Overlap or Combine).
  - **Grid:** horizontal and vertical spacing, offsets, bounds, a shift type (none, horizontal, vertical or both; this is the brick layout), and separate row and column flips.
  - **Symmetry (Mirror):** axis center, axis angle and normal angle.
  - The SDK also has a **Concentric** type, which I haven't seen in the UI [U].
- **Symbols.** A symbol is static or dynamic; only dynamic symbols allow per-instance appearance overrides (AIDynamicSymbol.h). Each has a registration point (one of nine) and an optional 9-slice grid given by a center rectangle (AISymbol.h).
- **Live Shapes** are ordinary paths that carry live-shape properties. The properties only stay valid while the geometry still meets that shape's constraints (AIArt.h `kTransferLiveShapeProperties`).

## 2. Inventory: what to store, what Export As writes, what import does

| Feature | Data an importer must keep for it to stay editable | Export As SVG output | Illustrator SVG import |
|---|---|---|---|
| Layers and sublayers (any depth) | Tree; per layer: name, color, Template, Lock, Show, Print, Preview (outline view), Dim Images % ([layer options](https://helpx.adobe.com/illustrator/desktop/manage-layers/create-and-organize-layers/set-layer-and-sub-layer-options.html)) | Nested `<g id="Layer_1" data-name="Layer 1">` [O]. Save As keeps hidden layers as `display="none"` ([old SVG page](https://helpx.adobe.com/illustrator/using/svg.html)). Export As drops hidden content [C] ([thread](https://community.adobe.com/t5/illustrator-discussions/svg-export-only-visible-layers/m-p/8820329)). Lock, template, print and color are lost. | Groups come back; layer options are lost [U] |
| Objects listed in the Layers panel | Per item: name, hidden, locked, uuid (since 24.0), note, tags | Names go to `id` plus `data-name`. Since 28.4, ids are sanitized and `data-name` keeps the raw name [O] ([28.4 thread](https://community.adobe.com/t5/illustrator-discussions/svg-export-format-completely-changed-with-illustrator-28-4/m-p/14548587)) | `id` probably becomes the object name [U] |
| Groups | Ordered children; the group's own appearance, effects and opacity | `<g>` | `<g>` becomes a group |
| Compound path | Subpaths, one paint, nonzero or even-odd rule, subpath direction | One `<path>` with `fill-rule:evenodd` when set [O] | Likely becomes a compound path [U] |
| Clip group (clipping mask) | Group with a clipped flag; the topmost child is the clip path, its fill and stroke are removed, and it may be a compound path or text ([about clipping masks](https://helpx.adobe.com/illustrator/desktop/manage-objects/edit-objects/about-clipping-masks.html)) | `<clipPath id="clip-path">` plus `clip-path:url(#clip-path)` on the group [O] | Becomes a clip group and shows the warning "Clipping will be lost on roundtrip to Tiny" [C] ([thread](https://community.adobe.com/t5/illustrator-discussions/quot-clipping-will-be-lost-on-round-trip-to-tiny-quot/td-p/12169556)) |
| Opacity mask | Luminosity mask art; Clip, Invert, linked and disabled flags ([clip/invert](https://helpx.adobe.com/illustrator/desktop/manage-colors/apply-transparency-and-blending/invert-or-clip-opacity-masks.html)) | `<mask maskUnits="userSpaceOnUse">` around `<g filter="url(#luminosity-noclip)">` (feFlood white plus feBlend), or `luminosity-invert-noclip` (adds an inverting feColorMatrix) [O] (e.g. `akveo/nebular docs/assets/img/auth.svg`). A raster mask becomes an `<image>` inside the mask [O]. Linked and disabled are lost. | Masks are often flattened or applied wrongly [C] ([thread](https://community.adobe.com/t5/illustrator-discussions/illustrator-does-not-understand-svg-masks/m-p/12408862)) |
| Knockout Group, Isolate Blending | Knockout (off, on or inherit), isolated, alpha-is-shape, document-level flags ([knockout](https://helpx.adobe.com/illustrator/desktop/manage-colors/apply-transparency-and-blending/create-transparency-knockout-groups.html)) | `isolation:isolate`, seen on the root wrapper when blend modes are used [O]; per group [I]. Knockout has no SVG equivalent and I don't know what export does with it [U]. | [U] |
| Blend modes (16) | Normal, Darken, Multiply, Color Burn, Lighten, Screen, Color Dodge, Overlay, Soft Light, Hard Light, Difference, Exclusion, Hue, Saturation, Color, Luminosity | CSS `mix-blend-mode`; multiply and overlay seen [O] (`johnwalley/allotment assets/logo.svg`). All 16 have CSS names. | Lost on reopen; objects come back as Normal [C] ([thread](https://community.adobe.com/t5/illustrator-discussions/why-does-exporting-svgs-change-the-transparency-layer-s-to-normal/m-p/10438398)) |
| Appearance with several fills and strokes | Ordered paint fields, each with its own opacity, blend mode and effects; Group Contents position ([appearance](https://helpx.adobe.com/illustrator/using/appearance-attributes.html)) | Written as separate elements; per-paint opacity and blend go on those elements [I/U] | Separate objects; not rebuilt into one appearance [I] |
| Vector live effects (Distort & Transform, Warp, Offset Path, Outline Object/Stroke, Pathfinder effects, Convert to Shape, 3D Classic) | Ordered effect list (plugin id plus parameters) at object, fill or stroke level | Written as the expanded result [I] | Plain paths |
| Raster effects and Document Raster Effects Settings | Effect parameters plus document-wide resolution (ppi), color model, background, anti-aliasing, padding, preserve spot colors. Raster effects are SVG Filters, the bottom half of the Effect menu, and Drop Shadow, Inner Glow, Outer Glow and Feather ([effects](https://helpx.adobe.com/illustrator/using/effects.html)) | Generally an embedded PNG `<image>`. Recent Export As writes Drop Shadow and Outer Glow as filter chains (feOffset, feGaussianBlur, feFlood, …) with `data-name="drop-shadow-1"` / `"outer-glow-1"` [O]. Other raster effects: probably PNG [U]. | Image |
| SVG Filters effect | Filter XML; it must be the last effect | `<filter>`; if any effect follows it, the result is rasterized ([Adobe](https://helpx.adobe.com/illustrator/desktop/special-effects-styles/apply-filter-effects/apply-svg-filter-effects.html)) | Effect > SVG Filters > Import SVG Filter reads filter definitions (Adobe). Whether an opened file's `<filter>` becomes an SVG Filter effect is [U]. |
| Graphic Styles | Named art styles and the link from each object | Inlined into computed classes; names lost | None |
| Basic stroke | Width, cap, join, miter limit, up to 6 dash values plus offset | stroke, stroke-width, stroke-linecap/linejoin, stroke-miterlimit, stroke-dasharray [O] | Native stroke |
| Stroke align inside/outside | Alignment setting | Converted to an outlined, filled offset shape [C] ([Shaper](https://support.shapertools.com/hc/en-us/articles/115003079454)) | Filled shape |
| Dashes "Align to corners and path ends" | Dash-adjust flag ([dashes](https://helpx.adobe.com/illustrator/desktop/paint-and-fill/apply-and-edit-strokes/create-dotted-or-dashed-lines.html)) | Per-object computed dasharray such as `0 0 4.8328 4.8328` [O]; the adjustment appears to be baked in [I] | Plain dash |
| Arrowheads | Start and end arrow (symbol references), scale % for each, alignment (tip at end, attachment point, truncation point) | Expanded paths; no `<marker>` seen [I/U] | Paths |
| Variable width (Width tool) | Width source (constant, tablet data or profile); centered profile {t[], width fraction[]} or uncentered {t[], left[], right[]}; per-marker flags (corner, anchored, …) | Expanded into a filled shape [C] ([thread](https://community.adobe.com/t5/illustrator-discussions/variable-width-stroke-values-for-script/td-p/11127307)) | Filled shape |
| Brushes: Art, Scatter, Pattern, Calligraphic, Bristle | Brush definition (art, up to 5 tiles for a Pattern brush, colorization, scale, flip, fold, overlap, corner art) plus per-path options ([brushes](https://helpx.adobe.com/illustrator/using/brushes.html)) | Expanded art [I]. Adobe warns that brushes generate a lot of path data. | Paths |
| Anchor point type | Corner flag on each segment | Lost; only path commands remain | Has to be guessed [I] |
| Linear and radial gradients | Gradient definition plus per-object placement (origin, angle, length, matrix, highlight); stops with midpoint and opacity; aspect ratio ([gradients](https://helpx.adobe.com/illustrator/using/gradients.html)) | `<linearGradient>`/`<radialGradient gradientUnits="userSpaceOnUse">`; aspect ratio and angle go into `gradientTransform` and the highlight into `fx`/`fy` [O]; `stop-opacity`. A midpoint other than 50% is approximated with extra stops (5 stops became 9–11) [C] ([thread](https://community.adobe.com/t5/illustrator-discussions/how-to-get-individual-stop-color-offset-in-linear-gradient-using-extendscript/m-p/14817258)). Midpoints have no SVG equivalent ([Adobe engineer, 2003](https://lists.w3.org/Archives/Public/www-svg/2003Jun/0045.html)). | Native gradient, but midpoints come back as plain stops. Problems with `objectBoundingBox` gradients are reported but unconfirmed [C/U]. |
| Gradient on stroke: within, along, across | Application setting ([stroke gradients](https://helpx.adobe.com/illustrator/desktop/paint-and-fill/create-and-edit-gradients/apply-gradients-on-stroke.html)) | Within becomes a normal gradient stroke. Along and across are rasterized [C] ([thread](https://community.adobe.com/t5/illustrator-discussions/exporting-svgs-with-gradient-along-a-stroke/td-p/11004765)). | — |
| Freeform gradient (Points with spread; Lines) | Stop positions, colors, opacities, spread; polylines in Lines mode; fill only | Rasterized [C/U] | — |
| Gradient dither and perceptual interpolation (30.0) | Flags; where they are stored is [U] | None [I] | — |
| Gradient mesh | Grid of nodes, each with position, handles, color and opacity; patches ([mesh](https://helpx.adobe.com/illustrator/using/meshes.html)) | Rasterized (Adobe: "mesh objects are rasterized", [saving artwork](https://helpx.adobe.com/illustrator/using/saving-artwork.html)) | — |
| Patterns | Swatch with rectangular tile art plus Pattern Options: tile type (grid, brick by row/column, hex by row/column), brick offset, size, overlap, copies, Size Tile to Art, Move Tile with Art ([edit patterns](https://helpx.adobe.com/illustrator/desktop/paint-and-fill/create-and-edit-patterns/edit-patterns.html)); per-object transform | `<pattern id="New_Pattern_Swatch_3" data-name=… patternUnits="userSpaceOnUse" patternTransform>`; each object's own transform goes into a derived `<pattern xlink:href=… patternTransform>` [O]. Pattern Options are lost. | [U] |
| Swatches and color groups | Process, global process and spot colors; tints; registration; Lab; groups | Hex values only; names lost | Unnamed colors |
| Document color mode | CMYK or RGB, plus color profile | Converted to sRGB hex [I] | RGB |
| Live Shapes | Rectangle (per-corner radius), Rounded Rectangle, Ellipse (pie angles), Polygon (sides), Star, Line ([live shapes](https://helpx.adobe.com/illustrator/using/live-shapes.html)) | `<rect rx ry>`, `<circle>`, `<polygon>` [O]; `<ellipse>` and `<line>` [I] | [U] |
| Live Corners | Per anchor: radius, type (Round, Inverted Round, Chamfer), rounding (relative or absolute) ([Adobe Learn](https://adobe.com/learn/illustrator/web/live-corners-in-illustrator)) | Expanded path [I] | None |
| Shape Modes (live compound shapes) and Pathfinder | Children with a mode each (unite, minus front, intersect, exclude); can be nested ([compound shapes](https://helpx.adobe.com/illustrator/desktop/manage-objects/reshape-transform-objects/create-compound-shapes-with-pathfinder.html)) | Result geometry [I] | Paths |
| Offset Path, Outline Stroke, Simplify | The Object > Path commands are destructive; Offset Path and Outline Stroke also exist as live effects | Result | — |
| Symbols | Definition (art, registration point, static or dynamic, 9-slice rectangle); instance (matrix, overrides) ([symbols](https://helpx.adobe.com/illustrator/using/symbols.html), [9-slice](https://helpx.adobe.com/illustrator/desktop/manage-objects/traces-mockups-symbols/maintain-proportions-while-scaling-symbols.html)) | `<symbol>` and `<use>` (Adobe says symbol output improved); 9-slice instances and dynamic overrides are probably expanded [U] | [U]; one report of symbols missing after reopening [C] |
| Blends | Spine path; Smooth Color, Specified Steps or Specified Distance; orientation; 30.7 easing (with strength) and color acceleration ([blends](https://helpx.adobe.com/illustrator/using/blending-objects.html)) | Expanded in-between objects [I] | Groups |
| Envelope Distort | Warp (style, bend, distortion), mesh (rows, columns, points) or top object; options: anti-alias, raster shape kept by clip or transparency, fidelity, distort appearance, linear gradients and patterns ([options](https://helpx.adobe.com/illustrator/desktop/manage-objects/reshape-transform-objects/envelope-panel-options.html)) | Result art [I] | Paths |
| Live Paint | Source paths; fills on faces, strokes on edges; gap options ([Live Paint](https://helpx.adobe.com/illustrator/using/live-paint-groups.html)) | Expanded [I] | Paths |
| Repeat: radial, grid, mirror | The SDK configs in section 1 ([grid options](https://helpx.adobe.com/illustrator/desktop/paint-and-fill/create-and-edit-patterns/set-grid-repeat-options.html)) | Instances; whether it uses `<use>` is [U] | Paths |
| 3D and Materials | Object type (extrude, revolve, inflate, plane), depth, twist, taper, bevel; rotation; Substance materials and mapped graphics; lights; shadows; render settings ([3D](https://helpx.adobe.com/illustrator/desktop/special-effects-styles/create-3d-graphics.html)) | Raster by default; a vector render option produces vectors that can't be edited as 3D. 3D Classic gives vector output [C] ([thread](https://community.adobe.com/t5/illustrator-discussions/3d-can-t-convert-to-shapes-in-2023/m-p/13320397)) | Image or paths |
| Intertwine | Intertwine group plus front/back order per overlap region; can't be nested ([intertwine](https://helpx.adobe.com/illustrator/using/intertwine-objects.html)) | Result [I] | Paths |
| Puppet Warp | Probably destructive; pins don't appear to be saved [U] | Geometry | — |
| Type: point, area, on path, threaded | Frame kind (POINTTEXT, AREATEXT, PATHTEXT), story, frame-to-frame threads, character and paragraph attributes and styles, area text options (rows, columns, inset), type-on-path options | `<text transform="translate(x y)"><tspan x="0" y="0">` [O]. Area text becomes positioned lines [I]. Type on a path becomes `<textPath>` [O]. "Convert to Outlines" writes paths. font-family is PostScript name plus family name [O]. | Each line or tspan becomes separate point text [C] ([thread](https://community.adobe.com/t5/illustrator-discussions/svg-text-and-tspans-import/m-p/11419002)) |
| Variable fonts, OpenType features, vertical and Japanese text | Axis values per text run; feature toggles; mojikumi and kinsoku composition; vertical orientation | Variable axes are written as `font-variation-settings: 'wght' 800, 'wdth' 100` [O] (`arviz-devs/arviz …/key_feature_interoperability.svg`). OpenType features, vertical text and Japanese settings [U]. | [U] |
| Artboards | Up to 1,000 per document since CC 2018 ([Adobe blog](https://blog.adobe.com/en/publish/2017/11/10/artboard-enhancements-in-illustrator-cc)); name, rectangle, order, lock and color (30.0) | With "Use Artboards", one SVG file per artboard; without it, the viewBox fits the bounds of all art. Save As writes only the active artboard but includes content from all artboards. | One canvas |
| Pasteboard content | Off-artboard objects; canvas is 227 in, or 2,270 in for a large canvas ([large canvas](https://helpx.adobe.com/illustrator/kb/large-sized-canvas-troubleshooting.html)) | Included when not exporting per artboard | — |
| Guides | Paths flagged as guides; grids; perspective grid | Dropped | — |
| Linked vs embedded images | PlacedItem (file path, PDF page or crop, layer comps) vs RasterItem (pixels, color space) ([Links](https://helpx.adobe.com/illustrator/using/importing-artwork-files.html)) | Embed writes a base64 `<image>`; Save As uses PNG when there's alpha and JPEG otherwise. Link writes an external href. | Image [U] |
| Image Trace (live) | Source image plus trace settings (mode, palette, paths, corners, noise, method); 29.0 added gradients, shapes, transparency and auto grouping ([29.0](https://community.adobe.com/questions-652/illustrator-max-2025-v29-0-align-object-to-path-image-trace-text-to-vector-graphic-and-more-812615)) | Traced vectors [I] | — |
| Recolor and Generative Recolor | Nothing stored; they rewrite colors | — | — |
| Text to Vector Graphic, Generative Shape Fill, Generative Expand, Turntable | A "Generative Object" group with linked variations, lost when ungrouped ([linked variations](https://helpx.adobe.com/illustrator/desktop/use-generative-ai/scenarios-with-linked-variations.html)); the content is ordinary editable vectors. Generative Expand creates "Expanded Art" and "Original Art" subgroups ([Gen Expand](https://helpx.adobe.com/illustrator/desktop/use-generative-ai/expand-artwork-with-generative-expand.html)). Partner models generate a raster that is then auto-vectorized ([partner models](https://helpx.adobe.com/illustrator/desktop/use-generative-ai/partner-models-overview.html)). Since 30.7, generated text can stay editable. Grouping by color is [U]. | Plain paths and groups; Content Credentials metadata is embedded in SVG ([FAQ](https://helpx.adobe.com/illustrator/desktop/use-generative-ai/generative-ai-faq-illustrator.html)) | — |
| Retype | Turns outlined or raster text into live text in a matched font | — | — |
| Objects on Path | Group bound to a path; pivot (9 points), rotation, spacing; settings apply to the whole group only ([Objects on Path](https://helpx.adobe.com/illustrator/using/objects-on-path.html)) | Result [I] | — |
| Dimension tool | Live linear, angular or radial dimension objects (dimension line, extension lines, text, offsets) ([dimension objects](https://helpx.adobe.com/illustrator/desktop/measure-and-align/plot-and-measure/about-dimension-objects.html)); they become plain groups if edited with selection tools [C] | Expanded [I] | — |
| Mockup | Vector art mapped onto a photo, with editable content ([Mockup](https://helpx.adobe.com/illustrator/desktop/manage-objects/traces-mockups-symbols/create-mockups-for-images.html)) | [U] | — |
| Graphs, non-native art, legacy text | Graph data plus design; opaque foreign drawing; pre-CS text | Rendered result [I] | — |

## 3. SVG writer options

**Export As (also used by Export for Screens and Asset Export)** ([options](https://helpx.adobe.com/illustrator/using/exporting-artwork.html)):
- **Styling:**
  - Internal CSS: a `<style>` block with `.cls-N` classes.
  - Inline Style: `style="…"` on each element.
  - Presentation Attributes: `fill="…"` and similar attributes.
- **Font:** SVG (live text) or Convert to Outlines.
- **Images:** Embed or Link.
- **Object IDs:**
  - Layer Names: sanitized `id` plus `data-name`.
  - Minimal: single letters such as `id="b"`, with `data-name` still written [O].
  - Unique: `uuid-…` ids [O].
- **Decimal places:** the Save As dialog documents 1–7; the Export As range is [U].
- **Minify:** removes empty groups and whitespace and uses minimal IDs.
- **Responsive:** no fixed width or height is written.
- **Use Artboards:** one file per artboard.
- **28.4 changes:** switched to `cls-N` classes and `data-name`, and dropped the generator comment. Parts of this were reverted in 28.6 and 29.0 [C].

**Save As SVG (legacy writer)** ([saving artwork](https://helpx.adobe.com/illustrator/using/saving-artwork.html)):
- **Profile (DTD):** SVG 1.0, 1.1, Tiny 1.1, Tiny 1.1+, Tiny 1.2, Basic 1.1.
- **Fonts:** Adobe CEF, SVG, or Convert to outline; glyph subsetting options.
- **Images:** embed or link.
- **CSS:** Presentation Attributes, Style Attributes, Style Attributes (Entity References), or Style Elements.
- **Other options:** decimal places 1–7, encoding, Optimize for Adobe SVG Viewer, Adobe Graphics Server (variables) data, slicing data, XMP metadata, "Output fewer `<tspan>`", and "Use `<textPath>`" for type on a path (overflow text becomes visible).
- **Preserve Illustrator Editing Capabilities** embeds a complete AI file in `<i:pgf>`. When the file is reopened, Illustrator reads that copy and ignores the SVG markup, so changes made in other tools are lost. It roughly doubles file size ([Launchpad](https://bugs.launchpad.net/bugs/179679)).

## 4. Minimal object model for all of Illustrator's static artwork

**Document**
- Color mode (RGB or CMYK) and color profile.
- Ruler origin.
- Document Raster Effects Settings: ppi, color model, white or transparent background, anti-aliasing, clipping mask, padding, preserve spot colors.
- Document-level isolation and knockout.
- Artboards (up to 1,000): id, name, rectangle, order, locked, color.
- Canvas size.
- Guides.

**Resource tables**
- Swatches: process, global or spot; tint; CMYK, RGB, Lab or Gray values; registration; color groups.
- Gradients: linear, radial or freeform; stops {rampPoint, midPoint, color, opacity}; dither and perceptual flags.
- Patterns: tile art, tile rectangle, Pattern Options.
- Symbols: art, registration point, static or dynamic, optional 9-slice rectangle.
- Brushes: the five kinds with their parameters and art; Pattern brushes have up to 5 tiles.
- Graphic styles: named appearances.
- Width profiles, arrowhead definitions, SVG filter definitions.
- Character and paragraph styles; font references.

**Properties every item has**
- uuid, name, hidden, locked, note.
- Opacity, blend mode (16), isolated, knockout (three states), alpha-is-shape.
- Opacity mask: a mask group plus clip, invert, linked and disabled flags.
- Appearance: pre-effects, ordered paint fields (each with its own effects, color position and transparency), post-effects, and the Group Contents position.
- Graphic style reference.
- Text wrap: on/off, inside, offset.
- Pixel-aligned flag.

**Item kinds**
1. **Layer** (a container that can nest to any depth): color, template, print, preview, dim %.
2. **Group**: a clipped flag; when set, the topmost child is the clip.
3. **Path**: segments {anchor, in, out, corner}; closed; even-odd; guide flag; clip-mask flag. Optional live-shape parameters and per-anchor live-corner settings {radius, type, rounding}.
4. **CompoundPath**: child paths with a single paint.
5. **TextFrame** (point, area or on path): matrix; the frame or baseline path; thread links; a story made of character runs. Each run has font, size, leading, tracking, kerning, baseline shift, scaling, OpenType features, variable-font axes and Japanese attributes. Plus paragraph attributes, style references, area options and path options.
6. **Placed** (linked file): path, matrix, page or crop, layer comps.
7. **Raster** (embedded image): pixels, color space, matrix.
8. **Mesh**: a grid of nodes {position, handles, color, opacity}.
9. **SymbolInstance**: symbol reference, matrix, overrides.
10. **PluginGroup**: plugin kind plus a parameter dictionary, edit-art children, and cached result art. One kind covers Blend, Envelope, Live Paint, Trace, Compound Shape, Repeat, Intertwine, Objects on Path, Dimension, Mockup, Shaper Group, Symbol Set and Generative Object.
11. **Graph**: data plus design.
12. **Foreign (NonNative)**: opaque, render-only.
13. **LegacyText**: legacy text, kept until updated.

**Paint**
- None, a color, or a swatch reference plus tint, each with an overprint flag.
- A gradient reference plus placement {origin, angle, length, matrix, highlight angle and length}; freeform stops; and for strokes, within, along or across.
- A pattern reference plus its transform.

**Stroke**
- Width, cap, join, miter limit.
- Dash array (up to 6) plus offset plus the corner-alignment flag.
- Alignment: center, inside or outside.
- Start and end arrowheads with scale and alignment.
- Width profile: centered, or separate left and right.
- Brush reference plus per-path options.

**Effects**
- An ordered list of {effect id, parameters, enabled}, marked as vector or raster.

## 5. Illustrator features with no SVG representation

These are either expanded, rasterized or approximated on export, or dropped entirely:
- **Layers and document:** layer lock, template, print, outline preview, dim, layer color and targeting; Export As drops hidden layers. Multiple artboards in one file, artboard lock and artboard color. Guides, grids and the perspective grid. Notes, tags, variables and slices; Save As has options to write variables and slices.
- **Color:** swatches (global, spot, process), tints, color groups, Lab and CMYK values, overprint, CMYK document mode.
- **Appearance:** graphic styles; character and paragraph styles as named styles; the Appearance structure itself (several fills and strokes as properties of one object, effects on a single fill or stroke); every vector live effect; raster effects except SVG Filters and the observed drop-shadow and glow filters; the Document Raster Effects resolution.
- **Transparency:** knockout groups, alpha-is-shape, and opacity-mask linked and disabled states.
- **Gradients:** midpoints (approximated with extra stops), freeform gradients, gradients along or across a stroke, dither, perceptual interpolation, gradient mesh.
- **Strokes:** inside or outside alignment, dash alignment to corners (baked in), arrowheads, variable width, and all brushes.
- **Shapes:** anchor point corner/smooth type, Live Shape and Live Corner parameters, live compound shapes, Pathfinder and Offset effects.
- **Patterns and symbols:** Pattern Options (tile type, brick offset, overlap, copies); symbol 9-slice, dynamic overrides, registration point; symbol sets.
- **Live constructs:** Blends (including 30.7 easing), Envelopes, Live Paint, Repeat, Intertwine, Objects on Path, Dimension objects, Mockup, live Image Trace, 3D and Materials, Generative Object variations and prompts, graph data.
- **Type:** area-text reflow and frame geometry, threading, type-on-path effects and options, composer, hyphenation and justification, text wrap, lists and columns. OpenType features, vertical text and Japanese composition are [U].
- **Placed files:** metadata such as PDF page, crop and layer comps.

## Takeaways for Capy

1. **Illustrator does not round-trip through SVG.** Its own SVG Save As keeps editability only by embedding a private AI copy (`<i:pgf>`), and Adobe recommends never reopening an exported SVG. If our format must hold an Illustrator user's work, SVG cannot be the source of truth. It can be a lossy rendering target, optionally carrying our own private data in a separate namespace, as Illustrator does.
2. **Two general mechanisms cover nearly all live features.** One is a plugin-style group (source children, parameters, cached result). The other is an ordered effect stack on objects and on individual fills and strokes. Building those two well avoids designing dozens of bespoke item types.
3. **Our SVG exporter would need the same fallbacks as Illustrator's:** exact SVG constructs where they exist (clip, mask with a luminance filter, `mix-blend-mode`, gradient transforms, `<pattern>`, `<symbol>`/`<use>`, `<textPath>`, `font-variation-settings`); expanded geometry for strokes and live constructs; rasterized images for everything else.
