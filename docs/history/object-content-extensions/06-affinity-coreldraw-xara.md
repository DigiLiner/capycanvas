# Affinity, CorelDRAW and Xara: object model inventory and gaps for Capy Object layers

[Object content extensions](../object-content-extensions-research.md) · source report, 2026-10-16

Checked against the vector design at `726cb6cdc`, or the draft before it in which recipes were separate records keyed by item ID. The design this audit produced is in [vector layers §7.2–7.4](../vector-layers-research.md#72-layer-model).

Research date 2026-10-16. Every claim has a source; `[K]` marks general knowledge that was not re-checked, and "unverified" marks things a source suggested but that were not confirmed.

## 0. Method and source quality

| App | Primary evidence used | Quality |
| --- | --- | --- |
| Affinity (V2 apps, unified Affinity 3.0–3.3.1) | The official **Affinity SDK 3.3.0** JavaScript reference and its bundled JSLib (BSD-3, © Canva), which enumerate node classes, enums and properties (https://sdk.affinity.studio/33000/, https://sdk.affinity.studio/33000/js/index.html, https://sdk.affinity.studio/33000/JSLib.zip); the Inkscape `.afdesign` importer, which reverse-engineers the container and record format (https://gitlab.com/inkscape/extras/extension-afdesign, commit cd5cf29 of 2026-03-09, based on https://github.com/VMDevCpp/afread); Designer 2 help (https://affinity.help/designer2/en-US.lproj/index.html) | High for the object model. The SDK is beta and "may contain errors and omissions". |
| CorelDRAW 2024–2026 | Current CorelDRAW Windows help (https://product.corel.com/help/CorelDRAW/Documentation-Windows/CorelDRAW-en/CorelDRAW-*.html); libcdr source (https://github.com/LibreOffice/libcdr, commit aaa57df of 2026-10-13) | Medium–high. The CorelDRAW VBA API pages (community.coreldraw.com/sdk/api) sit behind a bot wall. |
| Xara Designer Pro+ and `.xar` | Xara Xtreme LX 0.7r1785 GPL source (http://downloads.xara.com/opensource/XaraLX-0.7r1785.tar.bz2): Kernel/cxftags.h, cxfile.cpp, camfiltr.cpp, group.cpp, nodeliveeffect.cpp, nodemold.cpp, nodemldr.cpp, nodeblnd.cpp | High for the format mechanics, but the code dates from 2006. The spec PDF (http://site.xara.com/support/docs/webformat/spec/XARFormatDocument.pdf, referenced by https://pronom.nationalarchives.gov.uk/fmt/922) could not be fetched: its TLS certificate has expired and archive.org rate-limited us. |

## 1. Affinity

### 1.1 Timeline

- Designer, Photo and Publisher 2 (2.0–2.6) share one file family (.afdesign, .afphoto, .afpub) (https://affinity.help/designer2/en-US.lproj/pages/Introduction/keyFeatures.html).
- Canva released Affinity 3.0 in late 2025 as one free app with Pixel, Vector and Layout "Studios" and a new `.af` extension (https://www.cgchannel.com/?p=174669, https://newsgroup.xnview.com/viewtopic.php?p=209797).
- **3.1** (March 2026) added the Tone Brush, a Live Tone Blend Group and curves from pixel selections (https://www.cgchannel.com/?p=174669).
- **3.2** (April 2026) added the Texture filter, Vector Blob and Vector Erase brushes, Object Selection, Luminosity, Hue Range and Compound masks, and `.af` support in DaVinci Resolve 21 and Capture One (https://www.cgchannel.com/?p=175651).
- **3.3** (15 Sept 2026; 3.3.1.4940 on 8 Oct 2026 per a Wikipedia search result, unverified) added these:
  - live **Blend groups** with bias, easing, a spine path and a second path;
  - **Diffusion-curves** gradient fills;
  - **pigment blending** for mesh fills;
  - persistent **Measure** dimension objects;
  - G2 curve-energy display;
  - booleans that stay live by default;
  - a JavaScript Scripting Studio and SDK;
  - INDD and PUB import;
  - "Layer to 3D" placed 3D models.

  Sources: https://www.cgchannel.com/2026/09/canva-releases-affinity-3-3/, https://www.affinity.studio/blog/affinity-update-september-2026, https://www.affinity.studio/blog/blend-tool-mask-tool-3d-model-support, https://tidbits.com/?p=75714, https://note.com/platypus2000jp/n/ncea3211ecdf7?hl=en
- Compatibility is gated by major version. Affinity 3 opens V1 and V2 files; V2 cannot open V3 files (https://www.aldc.org/2026/05/affinity-3/; user reports agree).

### 1.2 Object kinds (SDK node classes, JSLib nodes.js)

The hierarchy has three levels:
- `Node` sits at the root.
- `LogicalNode` covers Document, Group and Container (a layer, with a tag colour).
- `PhysicalNode` covers Spread, Vector, Text, Raster, EmbeddedDocument and Develop.

The kinds:

- **Curve** (`PolyCurveNode`, file tag `PCrv`). A compound Bézier curve that **carries its own transform** (`Xfrm`), which the importer applies at read time (convert.py `_get_xfrm`). Any curve can also be an artboard (`artboardEnabled`).
- **Geometric shape** (`ShapeNode`/`ShpN`, with `Shpe` parameters, a `ShpB` box and `Xfrm`). There are 25 `ShapeType`s (https://sdk.affinity.studio/33000/js/enums/ShapeType.html): Rectangle, Ellipse, Triangle, Diamond, Trapezoid, Polygon, Star, DoubleStar, Arrow, Pie, Cog, Cloud, CalloutRectangle, CalloutEllipse, Crescent, Segment, SquareStar, Tear, Heart, Cat ×4, QRCode and Spiral.
  - The parameters are typed float, int, bool and enum (https://sdk.affinity.studio/33000/js/enums/ShapeFloatParam.html, …/ShapeIntParam.html, …/ShapeBoolParam.html, …/ShapeEnumParam.html). Examples: four corner radii and corner types (Round, Straight, RoundInverse, Cutout, None; …/ShapeCornerType.html); arrow end styles; cog tooth count and hole radius.
  - Spiral styles are SemiCircle, CounterSemiCircle, Fibonacci, SegmentedLogarithmic, SegmentedLinear and Plotted (…/ShapeSpiralStyle.html). The QR code shape carries typed payloads such as URL, vCard and WiFi.
  - **The file stores only the parameters and the box, never the outline.** The Inkscape importer re-implements each generator (svg/shape.py, 1,181 lines).
- **Image** (`ImageNode`; `ImgN`, `Rstr`). An embedded or linked file. It records the absolute and relative path, size, type and modification time, keeps its original colour profile, and has an extend type, a resampler (…/RasterResamplerType.html, which includes Lanczos and Mitchell) and stock metadata. Placement is Embedded or Linked (…/ImagePlacement.html).
- **Pixel layer** (`RasterNode`; `PatternRasterNode` is a tiled pattern layer with mirror). Formats are RGBA8/16, IA8/16, CMYKA8, LABA16, RGBAUF (float) and masks M8/M16/MF (…/RasterFormat.html). Pixel layers can be children of vector nodes (https://affinity.help/designer2/en-US.lproj/pages/Layers/layerClip.html).
- **Adjustment layers:** 21 or more classes, such as Levels, Curves, HSL, Recolour, Selective Colour, Split Toning and Tone Compression (https://affinity.help/designer2/en-US.lproj/pages/Layers/adjustmentLayers.html). Each can be attached to any object and has its own mask and blend mode.
- **Live filter layers:** about 35 classes, such as Gaussian, lens, motion and field blur, bloom, halftone, Voronoi, twirl and denoise.
- **Groups and layers** (`GroupNode`/`Grup`, `ContainerNode`/`Scop`). Pass-through is a flag set with the blend mode (blendmodeinterface.js `setBlendMode(mode, setPassthrough)`). There are **no knockout groups**; the only knockout is an Outer Shadow option, "fill knocks out".
- **Compound (live boolean).** Each child has a `CompoundOperation` of Union, Intersect, Subtract or Xor (https://affinity.help/designer2/en-US.lproj/pages/ObjectControl/compound.html, …/CompoundOperation.html). Live by default in 3.3.
- **Vector warp group.** An editable mesh with Mesh, Quad, Perspective, Arc, Bend, Fish Eye and Twist presets. Warps nest, can be muted and made into symbols, and the children (text and images included) stay editable (https://affinity.help/designer2/en-US.lproj/pages/ObjectControl/warp.html).
- **Blend group** (3.3). A live blend with spine, bias, easing and second path; the components stay editable. No SDK class yet.
- **Text** (`ArtTextNode`/TxtA, `FrameTextNode`/TxtF, `CurvePathTextNode`/CPTx or SPTN, `PolyCurveTextNode`/TxtC for text in a curve, `ShapePathTextNode`, `ShapeTextNode`/TxtS for text in a live shape, and `TableTextNode`). Tables are text nodes.
- **Measurement** (3.3, `MeasurementNode`, a VectorNode). Endpoints, scale factor, units, decimal places, annotation offset and end markers.
- **Embedded document** (`EmbeddedDocumentNode`/EmbN). An embedded Affinity or PDF document with a chosen spread or artboard, per-layer visibility overrides, PDF passthrough, raster DPI and a linked or embedded flag.
- **RAW develop** (`DevelopNode`): a live RAW development layer.
- **Symbols.** Each instance is a **full object copy** with per-property "master" fields and a sync state (`SLnk`/`ILOb`, `…Ma`/`…M` fields per the convert.py comments). Unsynced instances override any attribute, and instances can hold **extra non-symbol children**. Adjustments and warps can be made into symbols (https://affinity.help/designer2/en-US.lproj/pages/SymbolsAssets/symbols.html).
- **Assets** are library objects copied in on placement (https://affinity.help/designer2/en-US.lproj/pages/SymbolsAssets/assets.html).
- **Artboards** are not a kind but a flag on a curve, shape or text node.
- **3D objects** (3.3) can be lit, rotated and scaled (https://www.affinity.studio/blog/blend-tool-mask-tool-3d-model-support).

Child relationships are typed (https://sdk.affinity.studio/33000/js/enums/NodeChildType.html):
- `Main`: ordinary children. Under a non-container node they are clipped by the parent.
- `Enclosure`: masks, adjustments and filters attached to a node (file tag `AdCh`).
- `MasterSpread`.
- `WrapShape`: a text-wrap outline.
- `StoryPin`: an object anchored in text.

### 1.3 Per-object properties

- **Transform:** every node has an affine transform. Curves are not baked.
- **Visibility:** `isVisible` is separate from `isVisibleInExport` (…/VisibilityMode.html).
- **Opacity:** `globalOpacity` and **`fillOpacity`** are separate. Fill opacity "alters the opacity of the layer without affecting blending" (https://affinity.help/designer2/en-US.lproj/pages/Layers/layerBlendRanges.html).
- **Blend modes:** 33 values (…/BlendMode.html): Normal, **Pigment**, Darken, DarkerColour, Multiply, ColourBurn, LinearBurn, Lighten, LighterColour, Screen, ColourDodge, Add, Overlay, SoftLight, HardLight, VividLight, PinLight, LinearLight, HardMix, Difference, Exclusion, Subtract, Divide, Hue, Saturation, Luminosity, Colour, **Average, Negation, Reflect, Glow, ContrastInvert, Erase**. The file enum has grown in five versioned steps (styles.py V0–V4).
- **Blend options:** per-channel source and underlying blend ranges (splines), **blend gamma** 1.0–3.0 (text defaults to 1.45, other layers to 2.2), antialiasing Inherit/ForceOn/ForceOff and a custom coverage map.
- **Lock** with local and master editability for symbols; **name**; **tag colour**.
- **Tags:** key/value pairs, including alt text and extended description, plus "mark as decoration" (…/PredefinedTagKey.html).
- **Per-node export configuration** (slices).
- **Constraints:** fixed min/max, scale width/height, Min Fit, Max Fit, Stretch (…/ConstraintType.html; https://affinity.help/designer2/en-US.lproj/pages/DesignAids/constraints.html).
- **Layer states** (https://affinity.help/designer2/en-US.lproj/pages/ObjectControl/layerStates.html).
- **Fill mode:** alternate or winding, set **per polycurve, not per fill** (https://affinity.help/designer2/en-US.lproj/pages/CurvesShapes/fillMode.html).

### 1.4 Appearance

- **Appearance panel:** any number of fills and strokes in z-order, **each with its own blend mode and opacity** (https://affinity.help/designer2/en-US.lproj/pages/CurvesShapes/multiStrokesAndFills.html). The file stores them as BFFl/LIFl/LILn lists with a current index.
- **Fill descriptor:** a fill plus blend mode, affine, scale-with-object and **anchored-to-spread** (fills.js).
- **Fill kinds:** None, Solid, Gradient, Bitmap, Mesh, **Hatch** and **Diffusion** (…/FillType.html).
  - Gradients are Linear, Elliptical, Radial or Conical (…/GradientFillType.html). Stops have position, colour and opacity, with a **midpoint** between stops and optional **noise** (https://affinity.help/designer2/en-US.lproj/pages/Clr/gradientEditor.html). The gradient affine has 6 values.
  - Bitmap fills have extend Wrap/Mirror/Repeat/Zero, quality Nearest/Bilinear/Lanczos3 and scale-with-object (https://affinity.help/designer2/en-US.lproj/pages/Tools/tools_gradient.html).
  - Diffusion curves are Bézier, ellipse, arc or line curves with left and right colours, blur, strength, a pressure curve, and a background colour and strength (fills.js, …/DiffusionCurveKind.html).
  - Hatch is a list of hatch lines with origin, rotation, step and dash (hatch.js).
- **Colour attributes:** alpha, **noise**, intensity, tint and an **overprint** flag on every colour.
- **Transparency tool:** a separate gradient or bitmap transparency fill per object (transparencyinterface.js).
- **Strokes** (https://affinity.help/designer2/en-US.lproj/pages/Panels/strokePanel.html, …/LineStyleMask.html):
  - type None, Solid, Dash or **Texture** (a brush along the path);
  - width, cap, and join (MiterLegacy, Miter, Round, Bevel), miter limit;
  - **align** Centre/Inside/Outside, and **order** in front of or behind the fill;
  - **scale with object**;
  - dash of up to three dash/gap pairs **in multiples of the line width**, phase, and **balanced dashes**;
  - front and back arrowheads;
  - a symmetric **pressure** profile (https://affinity.help/designer2/en-US.lproj/pages/CurvesShapes/pressure.html).

  Arrowheads have 25 styles, separate X/Y scale and placement inside the line or at its end (…/ArrowHeadStyle.html). Path brushes have width, variance, controllers, head and tail offset, repeat, and corner strategy Pull/Fold/Overlap.
- **Corner tool:** per-node corner type (None, Round, Inverse, Straight, CutOut) and radius stored **on the curve** (geometry.js `getCorner`/`setCorner`, …/CurveCornerType.html). The SDK exposes both `polyCurve` and the derived `corneredPolyCurve`, and "the original shape's outline is retained" (https://affinity.help/designer2/en-US.lproj/pages/CurvesShapes/corneringShapes.html).
- **Contour tool:** a non-destructive offset with radius, join, miter, cap, and fill Force open / Force closed / Auto (https://affinity.help/designer2/en-US.lproj/pages/CurvesShapes/contouringShapes.html).
- **Node styles:** None, Cusp, Smooth and **Smart**, where Smart computes handles automatically (…/CurveNodeStyle.html).

### 1.5 Effects

- **Layer FX:** BevelEmboss, Outline, 3D (PhongBevel with multiple point lights), InnerShadow, InnerGlow, ColourOverlay, GradientOverlay, OuterGlow, OuterShadow and GaussianBlur (…/LayerEffectType.html).
  - Each effect has enabled, opacity, blend mode and scale-with-object.
  - Outline, inner shadow, the two overlays and outer shadow **can repeat and be reordered** (https://affinity.help/designer2/en-US.lproj/pages/LayerFX/create_layerFX.html).
  - The Outline effect has alignment and a fill of Solid, Gradient or **Contour** (https://affinity.help/designer2/en-US.lproj/pages/LayerFX/layerFX_outline.html).
- **Adjustments and live filters** attach as Enclosure children or act on everything below them within their parent.
- **Storage:** effects store parameters only, with no cached raster [K].

### 1.6 Masks and clipping

- **Clip by nesting:** "Any object can act as a parent or child" in clipping, for both vector and pixel content (https://affinity.help/designer2/en-US.lproj/pages/Layers/layerClip.html). Whether the parent's stroke draws above the clipped children is unverified.
- **Mask layers** (https://affinity.help/designer2/en-US.lproj/pages/Layers/LayerMasks.html):
  - Pixel masks use **greyscale or opacity**. Vector masks crop to a shape, which can be a group.
  - A mask can have its own blend mode.
  - A mask at sibling level "affects any object below them within the same parent layer".
  - Masks chain: a mask can carry masks of its own.
  - 3.2 adds live masks (https://www.affinity.studio/help/live-masks-live-layer-masks/).

### 1.7 Text

- **Kinds:** Art text, which scales when resized; Frame text with columns, gutter and vertical alignment; text in any curve or shape; path text; tables (https://affinity.help/designer2/en-US.lproj/pages/Text/text_general.html, …/frameText.html, …/shapeText.html).
- **Path text** has start and end handles, baseline offset and reverse, and **overflows onto a second path section running the opposite way, each section with its own baseline** (https://affinity.help/designer2/en-US.lproj/pages/Text/pathText.html). Converting an object to path text discards the original object.
- **Stories** flow across linked frames (`textFlowNodes`). Text wraps around WrapShape children, and objects can be pinned in text.
- **Per-run attributes** include **a full fill descriptor, stroke fill and stroke style, background fill, and underline and strikethrough fills** (svg/text.py); scale X/Y, shear and kerning (…/GlyphAttDoubleType.html); caps including petite caps and unicase (…/CapsType.html); OpenType features; **variable-font axes** (https://affinity.help/designer2/en-US.lproj/pages/Text/variableFonts.html).
- **Text styles** come in Paragraph, Character and Group types with a based-on hierarchy (https://affinity.help/designer2/en-US.lproj/pages/Text/textStyles_types.html).
- **Layout-only items:** fields, footnotes, endnotes and sidenotes, cross-references and data merge (…/GlyphType.html).
- **Direction:** the 3.3 SDK has no direction or vertical-writing attribute, so RTL and vertical support is unverified and probably absent.

### 1.8 Colour

- Colour spaces are Grey, RGB, CMYK, LAB and RGBU (unbounded) (…/ColourSpaceType.html). HSL and Pantone colours also appear in files.
- Documents carry RGB, CMYK, Lab and grey profiles.
- **Global colours** live in document palettes. Spot and overprint are flags on global colours, and tints exist (https://affinity.help/designer2/en-US.lproj/pages/Clr/overprint.html, …/spotClr.html).

### 1.9 File format

These facts come from the Inkscape importer's parser (extract.py, parse.py, consts.py).

- **The container is a private file system.**
  - Magic is 0x414BFF00, followed by a version, a class tag (Prsn, Objs and others) and an #Inf header with the FAT offset.
  - **A chain of timestamped FATs** maps named streams (doc.dat, previews.dat, session.dat, content.dat, …) to offsets. Streams use zlib or zstd, an optional delta transform and CRC32.
  - A reader takes the newest revision of each stream. This allows append-style saves and "Save History with Document".
- **doc.dat is self-describing.**
  - Each field is a type byte, a four-character tag and a value.
  - Each object lists its **class chain as tag plus a u16** (probably a version; inferred).
  - Objects can be shared (they carry an id) or links (they reference one).
- **Compatibility is all or nothing at the major-version level.** The Inkscape importer fails on classes it does not know, such as EmbN, ImgN, MRst and FilB (https://gitlab.com/inkscape/inbox/-/work_items/11551, https://www.inkscape.org/forums/other/affinity-design-importer-options-and-errors).

### 1.10 SVG and PDF export

- **SVG** (https://affinity.help/designer2/en-US.lproj/pages/ExportPersona/exportSettings.html):
  - Rasterize Nothing, Everything or **Unsupported** properties; when Nothing is chosen, unsupported elements are dropped.
  - Text as curves, flatten transforms, tile patterns, viewBox, relative coordinates.
- **Exporting** has separate Preserve accuracy / Preserve editability choices for gradients, adjustments, layer effects, lines and advanced blending.
- **Anecdotal user reports:** effects arrive as embedded bitmaps (https://lostmarble.com/forum/viewtopic.php?p=216413), and blend-mode art is rasterized (https://vectorstyler.com/forum/post/11955).
- **PDF:** without "Allow advanced features", stretched art text, stroked text, gradients and non-solid transparency are rasterized or outlined.
- **Import:** `.ai` files are read through their embedded PDF (https://affinity.help/designer2/en-US.lproj/pages/GetStarted/importAdobe.html). PDF import can favour editable text (https://affinity.help/designer2/en-US.lproj/pages/GetStarted/importPDF.html).

## 2. CorelDRAW 2024 / 2025 / 2026

### 2.1 Timeline

- **2024:**
  - **Painterly Brush**: pixel strokes driven by vector curves. The stroke updates when nodes are edited and also works on text.
  - The non-destructive bitmap-effects FX stack.
  - Remote fonts, Focus mode, and better PDF import.

  Source: https://learn.corel.com/wp-content/uploads/2024/02/CDGS-WhatsNew2024-WrittenTutorial.pdf
- **2025:** Painterly brush improvements, Advanced Print to PDF with separations, and CorelDRAW Web working with CDR, AI and PDF (https://www.coreldraw.com/en/blog/product/coreldraw-graphics-suite-2025/).
- **2026** (announced 3 March 2026): AI features and UI work. No vector-model changes were reported (https://www.coreldraw.com/en/blog/product/coreldraw-graphics-suite-2026/, https://www.corel.com/en/newsroom/news/press-releases/20260303-introducing-ai-powered-creativity-coreldraw-graphics-suite/).

### 2.2 Object kinds

- **Curves.** Node types are cusp, smooth and symmetrical (…/CorelDRAW-Node-types.html). Combined curves share one fill and outline (…/CorelDRAW-Combine-objects.html).
- **Rectangles** have per-corner **round, scallop or chamfer** corners and optional corner scaling relative to the object (…/CorelDRAW-Rectangles-and-squares.html).
- **Ellipses** can be arcs or pies (…/CorelDRAW-Ellipses-circles-arcs-and-pie-shapes.html).
- **Stars and polygons:** polygons, **perfect and complex (self-intersecting) stars** with points and sharpness (…/CorelDRAW-Polygons-and-stars.html).
- **Spirals** are symmetrical or logarithmic (…/CorelDRAW-Spirals.html).
- **Graph paper** draws a group of rectangles (…/CorelDRAW-Grids.html).
- **Common shapes** have glyph handles: basic, arrow, banner, callout and flowchart shapes (…/CorelDRAW-Predefined-shapes.html).
- **Text:** artistic and paragraph text.
- **Bitmaps** can be linked or embedded [K].
- **Tables** (…/CorelDRAW-Tables.html).
- **Dimension lines** come in parallel, horizontal/vertical, angular and segment forms. They are **dynamic**: snapped to nodes, their text updates when the measured object changes (…/CorelDRAW-Dimension-lines.html). Connectors [K].
- **Symbols** can be internal or **linked to external .csl libraries**, and can nest. **Instances override only size and position** (https://product.corel.com/help/CorelDRAW/540111147/CorelDRAW-en/CorelDRAW-Create-edit-and-delete-symbols.html, https://help.coreldraw.com/CorelDRAW/540111192/Documentation-Windows/CorelDRAW-en/CorelDRAW-Use-symbols-in-drawings.html).
- **PowerClip** frames can be any object, artistic text included. They can be empty, nested, or locked to their contents (…/CorelDRAW-PowerClip-objects.html).
- **Pages and layers:** multiple pages and **master layers**. Layers have separate **visible and print/export flags** (…/CorelDRAW-Change-layer-properties.html).
- **Generators:** the Impact and Pointillizer effects generate ordinary vectors (…/CorelDRAW-Impact-effects.html, …/CorelDRAW-Pointillizer.html).

### 2.3 Fills, outlines and transparency

- **Fill types:** uniform, **fountain** (linear, elliptical, conical, **rectangular**), vector pattern, bitmap pattern, **two-colour pattern**, **texture** (procedural, RGB only), **PostScript** and **mesh** (…/CorelDRAW-Fills.html).
  - Fountain fills have node transparency, midpoints, repeat / repeat and mirror, reverse, size as a percentage of the object, skew and rotate, smoothing, acceleration, **clockwise and counterclockwise hue blending**, and fountain steps (…/CorelDRAW-Fountain-fills.html, https://product.corel.com/help/CorelDRAW/540227992/Main/EN/Documentation/CorelDRAW-Applying-fountain-fills.html).
  - Mesh fills have per-node colour and **transparency**, mixing and smoothing (…/CorelDRAW-Mesh-fills.html).
- **Outlines** (…/CorelDRAW-Format-lines-and-outlines.html):
  - corners, caps (including extended square) and miter limit;
  - dashes in **Default, Align or Fixed** mode;
  - position outside, centred or inside;
  - **Behind fill**, **Scale with object** and **Overprint outline**;
  - a **calligraphic nib** with stretch 1–100 and tilt;
  - arrowheads;
  - cut-path colours for cutters.
- **Variable outline:** **Side 1 and Side 2 (asymmetric) widths** at percentage positions. "When a drawing containing a variable outline is opened in a version of CorelDRAW earlier than 24.3, the variable outline is treated like a filled object" (…/CorelDRAW-Variable-outlines.html). The writer stores a fallback result.
- **Transparency** is a second, fill-like layer (uniform, fountain, pattern, texture or hatch). It can be limited to **fill only or outline only** and has a merge mode (…/CorelDRAW-Object-transparency.html).
- **Merge modes** include **Logical AND/OR/XOR, Behind, Texturize, Invert, If lighter/If darker and R/G/B channel modes** (…/CorelDRAW-Merge-modes.html).
- **Overprint** is set per object (…/CorelDRAW-Color-trapping-and-overprinting.html).
- **Styles:** object styles and style sets are linked, so editing a style updates its users (…/CorelDRAW-Styles-and-style-sets.html).

### 2.4 Effects

Most effects support Copy effect and **Clone effect**, where the clone follows a master.

- **Blend:** steps or spacing; object and colour acceleration; direct, clockwise or counterclockwise colour; node mapping; along a path; compound and split blends (…/CorelDRAW-Blending-objects.html). **Intermediates are saved by default or optionally "rebuilt when opening"**; the same option applies to extrusions and texture fills (…/CorelDRAW-Save-drawings.html).
- **Contour:** inside, outside or to centre; steps; offset; corners; colour progression (…/CorelDRAW-Contour-objects.html).
- **Extrude:** vanishing point, rotation, bevel, up to 3 lights, draped fills (…/CorelDRAW-Create-extrusions.html).
- **Envelope:** four modes and presets. **Applies to bitmaps non-destructively** (…/CorelDRAW-Shape-objects-by-using-envelopes.html).
- **Perspective:** works on bitmaps and groups (…/CorelDRAW-Apply-perspective.html).
- **Distort:** push/pull, zipper or twister. **Several distortions stack** (…/CorelDRAW-Apply-distortion-effects.html).
- **Drop and inner shadows:** feather and merge mode (…/CorelDRAW-Add-drop-shadows-and-inner-shadows.html).
- **Block shadow:** produces cuttable curves (…/CorelDRAW-Add-block-shadows.html).
- **Bevel:** soft edge or emboss, with spotlight (…/CorelDRAW-Create-bevel-effects.html).
- **Lens:** 11 kinds, including Bitmap effect. A lens **changes the look of whatever lies beneath it**; it reads its backdrop (…/CorelDRAW-Apply-lenses.html).
- **Bitmap effects on vectors:** a stacked, reorderable and flattenable FX tab with auto-inflate (…/CorelDRAW-Applying-raster-effects.html).

### 2.5 Masks and clipping

PowerClip is the clipping mechanism. CorelDRAW has no separate luminance-mask object; transparency fills play that role.

### 2.6 Text

- **Paragraph frames** can be any closed object; text can auto-fit; frames link, including into paths and objects (…/CorelDRAW-Paragraph-text.html, …/CorelDRAW-Combine-and-link-paragraph-text-frames.html).
- **Fit text to path** with mirror and offset; text and path are one linked object (…/CorelDRAW-Fit-text-to-a-path.html).
- **Wrap:** contour or square wrap (…/CorelDRAW-Wrap-text.html).
- **Typography:** OpenType (…/CorelDRAW-OpenType-features.html); **vertical orientation per text object** (…/CorelDRAW-Format-Asian-text.html).

### 2.7 Colour

- **Models:** CMYK, RGB, HSB, HLS, Lab, greyscale and others (…/CorelDRAW-Color-models.html).
- **Spot palettes** with tints.
- **Colour styles** are linked document colours, grouped into **harmonies** and **gradients** (a master colour plus derived shades) (…/CorelDRAW-Create-and-apply-color-styles.html).

### 2.8 File format

- **RIFF/RIFX chunks** with the version in the form type. Chunks include `loda` (object data), `trfd` (**a transform per object**), `fild`/`fill` and `outl` (**shared fill and outline style tables referenced by ID**), `styd`, `txsm`, `bmp `, `lgob`/`grp ` and `vrsn` (https://github.com/LibreOffice/libcdr/blob/master/src/lib/CDRDocumentStructure.h).
- **Self-describing arguments:** a `loda` record has a table of argument offsets and type IDs, so unknown arguments can be skipped (CDRParser.cpp `readLoda`).
- **ZIP containers:** content/riffData.cdr, or later content/root.dat plus content/dataFileList.dat and content/data/*, plus color/profiles (https://github.com/LibreOffice/libcdr/blob/master/src/lib/CDRDocument.cpp).
- **Downgrade on save:** saving as an older version offers **Keep appearance vs Keep editable**, and an optional embedded CMX (flattened copy) (…/CorelDRAW-Save-drawings.html).

### 2.9 SVG and PDF export

- **SVG:** presentation attributes or internal/external CSS; text as text (with optional embedded fonts) or curves; object names and comments as XML tags. **Mesh fills and drop shadows become bitmaps** (https://product.corel.com/help/CorelDRAW/Documentation-Windows/CorelDRAW-en/CorelDRAW-Scalable-Vector-Graphics-SVG.html).
- **PDF round-trips (anecdotal):** users report text broken into fragments, gradient-with-transparency fills rasterized, and closed paths exported as open (https://community.coreldraw.com/talk/coreldraw-graphics-suite-2025/f/coreldraw-graphics-suite-2025-for-mac/68205/coreldraw-export-to-adobe-illustrator-is-badly-broken, https://community.coreldraw.com/talk/coreldraw-graphics-suite-2023/f/coreldraw-graphics-suite-for-windows/67444/pdf-export-converts-an-object-with-closed-curve-to-an-open-curve-path/319312).

## 3. Xara Designer Pro+ and XAR

### 3.1 Product state

The latest version is 25.0 (5 Aug 2025): live Shape Tool, corner controls, QR code tool and better tables (https://www.xaraxone.com/designerpro-plus/whats-new/). No 2026 release was found.

### 3.2 Object model

From the tag list in cxftags.h (2006 baseline):

- **Paths:** filled, stroked and relative variants. **PATHREF_IDENTICAL and PATHREF_TRANSFORM reuse an earlier path**, which deduplicates geometry.
- **QuickShapes** encode their variants in the tag number: simple or complex, reformed, stellated, rounded.
- **Groups and structure:** GROUP, GROUPTRANSP, CACHEDNODESGROUP.
- **Live compound nodes:**
  - BLEND and BLENDER (path, profiles, curve angles);
  - MOULD_ENVELOPE and MOULD_PERSPECTIVE (with group and path);
  - SHADOW;
  - BEVEL;
  - CONTOUR;
  - CLIPVIEW;
  - FEATHER;
  - LIVE_EFFECT, LOCKED_EFFECT and FEATHER_EFFECT.
- **Fills:** flat, linear (including 3-point), circular, elliptical, conical, **square**, **three-colour**, **four-colour**, bitmap, **contone bitmap**, **fractal**, **noise** and multistage. Fill effects include **rainbow and alt-rainbow** (HSV interpolation), and fills can repeat or repeat inverted.
- **Transparency attributes:** every fill kind has a transparency counterpart with a "type". Users report the types were renamed to W3C names (https://www.talkgraphics.com/showthread.php?p=431588, unverified).
- **Lines:** dashes (including scaled dashes), arrowheads and tails, VARIABLEWIDTHFUNC and TABLE, stroke shapes and airbrush, **brushes with recorded pressure and time samples**.
- **Text:** stories on paths, word wrap, **link info**, tabs and rulers.
- **Print:** overprint fill and line, print on all plates.
- **Colour:** complex colours (linked tints and shades [K]).
- **Attributes are tree nodes** (`NodeAttribute` derives from `NodeRenderable`, nodeattr.h).
- **Live copies and repeating objects** (https://www.talkgraphics.com/showthread.php?p=593594, https://www.techbyter.com/2011/xwd2/advanced.htm).

### 3.3 Compatibility mechanics

- **Record layout:** each record is a u32 tag, a u32 size and its data. Nesting is a pre-order stream with **TAG_DOWN (1) and TAG_UP (0)** markers. ATOMICTAGS (10), ESSENTIALTAGS (11) and TAGDESCRIPTION (12) come near the start of the file, and NONRENDERSECTION marks data a renderer may skip.
- **Unknown tags** (camfiltr.cpp `UnrecognisedTag`), quoting the code: "If the tag is an essential one, the import will be aborted. If the tag is atomic, the next subtree will be ignored. If the import is not aborted, the user will be given a suitable warning at the end of import, telling him … which tags were ignored."
  - A tag that is neither: only that record is skipped and its children still load.
- **Which tags are atomic:** "All compound nodes (e.g. bevels, contours, shadows, clipview) are now defined as being atomic. This is so that they can be backwards compatible with CX2." Live, locked and feather effects joined in X2 (2005). An old reader therefore drops the source objects inside rather than drawing them wrong. No writer of the essential list exists in this code.
- **Tag descriptions:** `AddTagDescription` names new tags so old readers can report what they skipped.
- **TAG_COMPOUNDRENDER** carries bounds and is "really just a hint to external XAR stream handlers" (group.cpp).
- **What is stored for live content:**
  - Mould results are **not saved**: "NodeMoulders do NOT save anything out" (nodemldr.cpp).
  - Contour and bevel nodes write empty markers, so results are rebuilt.
  - Blends are saved as parameters unless "convert blends to outlines" is on.
  - **A locked effect saves its rendered bitmap with a placement parallelogram, plus the effect ID and XML edit list, and can keep its source children** ("Flag set when NOT destructive").
  - A live effect saves only its ID, resolution and XML edits.
  - XPE bitmaps can be a placeholder that tells the loader to "regenerate the bitmap from the XPE info".
- **Version warnings:** users report a "made in a newer version" warning (https://www.talkgraphics.com/showthread.php?p=621050).

## 4. Cross-app summary: live content, source and result

- **Shapes:**
  - Affinity stores parameters only.
  - CorelDRAW stores parameters; whether it also stores the outline is unverified.
  - Xara encodes parameters in the tag.
  - Capy stores the result path plus a recipe.
- **Corners:**
  - Affinity keeps per-node corner data on the curve.
  - CorelDRAW keeps rectangle parameters; fillet on curves is destructive [K].
  - Capy uses a recipe.
- **Booleans:**
  - Affinity keeps live compound children.
  - CorelDRAW and Xara booleans are destructive [K].
  - Capy uses a recipe, which needs source items (§6).
- **Blend, contour, extrude:**
  - CorelDRAW stores sources plus parameters, with the result saved or rebuilt.
  - Xara stores sources plus parameters and rebuilds the result.
  - Affinity 3.3 has live blends; their format is unknown.
- **Envelope and warp:**
  - Affinity and CorelDRAW keep them live, including over images and text.
  - Xara saves the source plus geometry and rebuilds the result.
  - In Capy, warped images need a raster result.
- **Shadows and FX:** parametric everywhere, and atomic in Xara.
- **Plug-in effects:** parameters only; a Xara locked effect also stores the bitmap.
- **New stroke features:**
  - An older Affinity cannot open the file.
  - An older CorelDRAW gets a **fallback filled shape**.
  - An older Xara skips the record.
  - An older Capy reader goes Preserved.

## 5. Import expectations

From SVG, PDF and EPS of their own work, Affinity and CorelDRAW users expect the following to stay editable: compound paths and fill rule; solid and linear/radial gradients with stop opacity; strokes with caps, joins and dashes (inside and outside strokes usually arrive expanded); clipping and masks; group opacity; CSS blend modes; images; text where fonts allow; and layer names [K].

They expect to lose or see rasterized:
- effects and shadows;
- non-CSS blend modes;
- conical, rectangular and diamond gradients, and mesh;
- live-effect editability;
- symbols (they arrive as copies);
- spot colours and overprint (PDF only);
- text on path and frame flow.

Sources: §1.10, §2.9.

Two importer needs follow:
- **Pen transforms** for non-uniformly scaled strokes.
- **Text-shaped clip paths.**

## 6. Gaps vs the planned Capy model

Classes: **(a)** structural, so decide before the MVP format freezes; **(b)** additive later, with the slot named; **(c)** out of scope.

### 6.1 (a) Structural, or decide now

1. **Recipe sources must be item sub-streams, nested recipes included.**
   - Live booleans (Affinity compounds, live by default in 3.3), warp, envelope and mould groups, blends (start, end and spine objects) and contours all keep **editable source items** next to a generated result.
   - A recipe "keyed by item id" has nowhere to keep them. If they live in the main tree, older readers render them.
   - Xara (atomic subtrees: an old reader drops everything) loses content, and Affinity (no result stored) makes every reader implement every generator.
   - Decide now that a recipe can embed an item sub-stream, with its own ids, nested recipes and fingerprints, and can reference other items: a blend spine, a Corel clone master, a shared vanishing point.
2. **Every item header must allow skipping it with its subtree, and carry a compatibility class.**
   - Use Xara's split: ignorable kind (skip the record and keep its children), atomic (skip the whole subtree) and essential (cannot render). Capy's "ignorable" exists only for properties.
   - Put kind, flags and a subtree length or child count in every item header, so one unknown kind (a measurement, guide or annotation) doesn't make the whole object Preserved.
   - Add optional human-readable names for unknown kinds (a document record, after Xara TAG_TAGDESCRIPTION).
3. **The width-profile layout must be two-sided now.**
   - Corel variable outlines have Side 1 and Side 2 widths at percentage positions. Xara has width functions. Affinity is symmetric.
   - The profile ships in the MVP Path, so store (position, left, right) samples plus an interpolation kind.
4. **Anchor kind needs an open fallback.** Affinity has Smart (automatic) nodes. Either add `auto` now, or define that unknown anchor kinds degrade to corner instead of making the file Preserved.
5. **Clip and mask children accept any item kind, Text and Group included.**
   - Text clips content in Corel PowerClip and in Affinity.
   - Affinity masks can be groups and can carry masks.
   - Multiple masks multiply, so nesting is enough.
6. **Linked text frames:** decide now. Affinity `textFlowNodes`, Corel linked frames and Xara link info all let one story flow through several items. Either move the story into its own record referenced by an ordered list of frames, or declare this (c).
7. **Stable anchor ids are needed only for node-attached dimensions and connectors** (Corel dynamic dimensions). I recommend none, which makes those (c).

### 6.2 (b) Additive later, with the slot

- **Per-entry opacity and blend** (Affinity Appearance; Xara transparency): new keys on Fill and Stroke entries. This requires the entries to be keyed records.
- **Per-entry alpha paint** (Corel fill-only or outline-only transparency, Affinity Transparency tool, Xara): paint-entry key `alpha_paint`, or item property `transparency_paint`.
- **Fill opacity** (Affinity): Group property `fill_opacity`, and the same as a layer field once layer FX exist.
- **Pen transform / calligraphic nib** (Corel nib, non-uniform SVG and PDF strokes, Affinity and Corel object transforms): stroke key `pen` (a 2×2 matrix, or shape plus stretch plus angle). Otherwise expand the stroke via a recipe.
- **Dash and marker details:**
  - `dash_units` (absolute vs multiples of width; Affinity uses multiples);
  - `dash_adjust` (none / balanced / align / fixed);
  - arrowhead `{style, scale_x, scale_y, placement}` with an open style enum plus a custom path reference.
- **Stroke behind fill** needs no slot: put the Stroke entry before the Fill entry.
- **scale_with_object** as a stroke and paint key (an editing hint).
- **Brush or texture strokes along a path** (Affinity texture line style, Corel Painterly brush and Artistic Media, Xara brushes with pressure and time): stroke key `brush` (a brush resource plus pressure data), with a cached Image result after Xara's locked effect. A natural fit for Capy's GPU brush engine.
- **Gradients:** an open `kind` (conical, elliptical, rectangular/square/diamond, 3- and 4-colour, diffusion curves); stops `{offset, color, alpha, midpoint}`; `extend` pad/repeat/mirror; an `affine`; `interpolation` RGB, linear, or HSV clockwise/counterclockwise (Corel) or rainbow (Xara).
- **Mesh paint** (Affinity pigment blending, Corel per-node alpha): paint key `mesh`.
- **Pattern and hatch paints:** an image or item-tree tile with affine, extend, sampling and `anchor: object|layer` (Affinity anchored-to-spread); and a hatch line list.
- **Noise** on colours and stops (Affinity): paint key `noise` with a fixed seed.
- **Procedural textures** (Corel texture, Xara fractal and noise): recipe plus a cached tile resource (Corel's own "save texture fills with the file").
- **Global swatches, harmonies, linked tints:** paint key `swatch` plus a document swatch table.
- **CMYK, Lab, grey and spot colour values:** colour-space tags plus a document output-profile record. A spot colour is a name, a tint and an alternate RGB/Lab appearance.
- **Overprint:** paint-entry key; mostly (c).
- **Extra blend modes:**
  - from Affinity: Darker/Lighter Colour, Average, Negation, Reflect, Glow, Contrast Invert, Pigment, Erase;
  - from CorelDRAW: Logical AND/OR/XOR, Behind, Texturize, Invert, R/G/B channel modes.

  These go in as new enum values. Erase and Behind are Porter-Duff operators (destination-out, destination-over), so add them as a compositing-operator value rather than as blend formulas.
- **Blend gamma, antialias and coverage map** (Affinity): an item, group or layer field; mostly (c).
- **Blend ranges** (Affinity): a layer or group field.
- **Item-level layer FX** (shadows, glows, outline with contour gradient, bevel, 3D with lights, overlays; multiple and ordered): Group effects with new effect kinds. Each needs opacity, blend and scale_with_object. Leaf items with effects get wrapped in a Group on import.
- **Backdrop-reading items** (Corel lens; Affinity adjustment and live-filter children; sibling-scope masks):
  - inside one object, a new item kind `backdrop_effect`, scoped to the group backdrop and requiring a non-isolated parent;
  - across layers, Capy's layer-level unattached adjustments plus a new layer field `vector_mask` (occurrences have raster masks only);
  - Affinity positional masks and adjustments convert losslessly by wrapping earlier siblings in a group.
- **Live masks** (Affinity 3.2 luminosity range, hue range, object selection, compound): a mask-source generator kind.
- **Warp, envelope and perspective over images and text:** a recipe. Text results are outline paths; image results are a stored warped raster (Image item plus resource), or later a mesh-warped image kind.
- **Live-shape, corner and contour recipes** (25 Affinity shapes; per-node corners; Corel corners, stars, spirals, glyph shapes; Xara QuickShapes): recipe kinds. Per-anchor recipes rely on the fingerprint to invalidate.
- **Blend groups** (steps or spacing, easing, colour direction, node mapping, spine): a recipe that references sources (§6.1 item 1), with intermediates as paths. Corel clone links are a recipe field pointing at the master.
- **File size from many-step results:** dedup geometry with "same geometry as item X with affine" (Xara PATHREF_TRANSFORM), or a shared path resource.
- **Symbols:**
  - per-attribute overrides are covered by the planned overrides keyed by item id;
  - add an `insert` override (parent, position and an item sub-stream) for Affinity's extra children per instance;
  - nested symbols;
  - external library references (Corel .csl).
- **Text:**
  - a per-run paint list (Affinity fill, stroke, background and underline fills) that reuses Style entries;
  - per-run scale and shear;
  - OpenType features and variable axes;
  - columns and gutter, vertical alignment, overflow and auto-fit;
  - text in arbitrary shapes;
  - path start and end handles, offset and reverse;
  - `orientation: vertical` (Corel).
- **Text wrap** (Affinity WrapShape, Corel): item property `wrap {mode, offset}`.
- **Pinned objects in text** (Affinity): inline-object run glyph; probably (c).
- **Fill rule** needs nothing: Capy's per-fill rule is a superset.
- **Object transform metadata** (Affinity and CDR keep a transform per object): Path property `frame_affine`, a UI hint. Baking is otherwise lossless.
- **Export or print visibility** (Affinity isVisibleInExport, Corel layer flag): field `exportable`.
- **Metadata:** alt text, decoration flag, key/value tags, Corel object data. Item property `meta`.
- **Constraints** (Affinity): item property `constraints`.
- **Images:** linked vs embedded and the original ICC profile as resource fields; more `interpolation` values (Lanczos, bicubic, Mitchell).
- **Measurement items** (Affinity 3.3): new kind, ignorable.
- **Placed documents** (Affinity EmbeddedDocumentNode, Corel OLE): new kind referencing a resource, with a cached raster for older readers.
- **Editable pixels inside a vector group** (Affinity's pixel texture clipped in a curve): a paint layer clipped to the Object layer covers this at layer level. Inside the tree, use an Image item, or later an `embedded_source` kind referencing a paint stack, which adds an evaluation dependency.
- **Rule: ship new appearance features as recipe plus result.** CorelDRAW does this (variable outline → filled object, Keep appearance) and so does Xara (locked-effect bitmap). A new stroke or paint feature can be written as recipe plus result (an expanded fill path or an Image), so older readers render instead of going Preserved. Keep the base kinds minimal: a solid-filled Path and an Image are the universal fallbacks.

### 6.3 (c) Out of scope

- Pages, spreads, artboards, master layers, repeating objects, layer states and slices.
- Tables, fields, notes, index, cross-references, data merge and TOC.
- 3D objects (Affinity 3.3) and Corel extrude as live 3D; import the result.
- Third-party plug-in live effects; import the result.
- Separation semantics (overprint simulation, trapping, plates) beyond keeping the values.
- Saving undo history in the file.
- Node-attached dimensions and connectors.
- Linked frames, if §6.1 item 6 is decided against.
- Per-object blend gamma and coverage maps.
- PostScript fills (would execute code); import a pattern raster.
- Knockout groups. None of the three apps has them (Affinity only has the shadow "fill knocks out"), so they are low priority, though still useful for PDF and Illustrator import.

### 6.4 What the planned model already handles well

- **Result plus recipe beats both Affinity and Xara.** Affinity stores parameters only, so importers must re-implement generators and fail on unknown classes. Xara's atomic subtrees drop content. CorelDRAW converged on the same idea as Capy: variable-outline fallback, "save blends with file", "Keep appearance".
- **An ordered paint list** covers multiple fills and strokes and stroke-behind-fill.
- **A per-fill rule** is a superset of Affinity's per-curve fill mode.
- **Groups with isolate/pass-through, clip and mask children, and effects** cover Affinity nesting, Corel PowerClip and Xara ClipView, given §6.1 item 5.
- **Instance overrides keyed by item id** cover Affinity's per-attribute symbol overrides and exceed Corel's (transform only).
- **Baked path coordinates** match imports once a pen transform exists.
