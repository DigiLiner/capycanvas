# Vector layers: research, code audit and design

[Technical documentation](../README.md) · [Design and validation history](README.md)

Research date: **2026-09-25**, source baseline **`796f7f5b`**. Code audit and
design: **2026-10-14**, source baseline **`15f01f80e`**. Extension audit and
model revision: **2026-10-16**, source baseline **`86a0fd0e4`**. Performance
revision: **2026-10-16**, source baseline **`7c71b731c`**.

This record holds the research behind vector layers and the design chosen after
auditing the code. It describes intended work, not implemented capabilities.

- **Evidence:** sections 2.1 and 3–5 are research from the first date.
- **Design:** sections 1, 2.2 and 6–9 are the audited design. Sections 7.2–7.4
  were revised after the [extension audit](object-content-extensions-research.md),
  which checked the model against SVG, Inkscape, Illustrator and other editors.
  Sections 7.2 and 7.5 were revised after the [performance research](vector-performance-research.md),
  which set scale targets and the caching, kernel and gesture rules.
  The design replaces the earlier recommendation, which proposed three things:
  - replaying recorded samples through the brush engine;
  - a `VectorIndex` beside `SelectionIndex`;
  - a raster cache persisted in the file.

The first releases cover vector lines and fills as an illustrator expects them.
Brush-textured paths come later and build on the same item model (section 7.9).

The research combines eight streams. Their full write-ups, with every source URL, are in [`vector-layers-research/`](vector-layers-research):

| # | Stream | File |
|---|---|---|
| 1 | Vector path editors (Illustrator, Inkscape, Affinity, Graphite, Figma, Linearity, CorelDRAW…) | [01-vector-editors.md](vector-layers-research/01-vector-editors.md) |
| 2 | Vector drawing apps (Clip Studio Paint, Fresco, Concepts, Affinity brushes, Illustrator drawing tools) | [02-vector-drawing-apps.md](vector-layers-research/02-vector-drawing-apps.md) |
| 3 | Animation, ink SDK and open-source stroke systems (Harmony, OpenToonz, Blender GP, PencilKit, Jetpack Ink, Rnote…) | [03-animation-and-ink-stroke-systems.md](vector-layers-research/03-animation-and-ink-stroke-systems.md) |
| 4 | File formats and the "replayable stroke" question (SVG, PDF, InkML, ISF, UIM, Jetpack Ink…) | [04-file-formats-and-replay.md](vector-layers-research/04-file-formats-and-replay.md) |
| 5 | Artist experience with vector drawing (tutorials, Reddit, CSP TIPS, pain points) | [05-artist-experience-drawing.md](vector-layers-research/05-artist-experience-drawing.md) |
| 6 | Artist experience with path editors (curricula, beginner struggles, makers/Cricut) | [06-artist-experience-path-editors.md](vector-layers-research/06-artist-experience-path-editors.md) |
| 7 | Rust ecosystem and algorithms (licenses, measured wasm sizes, complexity) | [07-rust-feasibility.md](vector-layers-research/07-rust-feasibility.md) |
| 8 | Capy Canvas codebase assessment (file:line references at `796f7f5b`; superseded by section 6) | [08-codebase-assessment.md](vector-layers-research/08-codebase-assessment.md) |

**Caveats on the evidence**
- **Web-search cap:** every stream ran out of web searches (200 per session) partway through, and later facts came from direct page fetches, crate sources and cloned repos.
- **Blocked Adobe and Inkscape manuals:** some behavior of Illustrator's and Affinity's tools, and some shortcuts, come from general product knowledge; the source files mark these **[K]** or "unverified".
- **Reddit coverage:** Reddit threads came from an archive API, so we saw post text and some top comments, not full discussions.
- **Counts and rankings:** the popularity figures and the wishlist are tallies across roughly 140 sources, not a survey.
- **Code audit:** the 2026-10-14 audit read the code at `15f01f80e`; it built and ran nothing. File:line references in section 6 apply to that baseline.

**Relationship to the other vector research record.** [Vector drawing and editing research](vector-drawing-research.md) is an independent study from the same day and source baseline. The two records disagreed on four decisions. The design settles them:

| Decision | Settled as | Agrees with |
|---|---|---|
| Brushes on vector layers | Solid round pens only at first (G-Pen and a new Monoline preset). Brush-textured paths are a later item kind; canvas-reading brushes never apply | [vector-drawing-research.md](vector-drawing-research.md) |
| Authoritative stroke data | A fitted cubic Bézier path plus a width profile. Recorded samples are not stored | [vector-drawing-research.md](vector-drawing-research.md) |
| Fallback for readers without vector support | Nothing extra in the file. Object-layer content keeps no saved raster cache; older readers preserve the package and show its preview | Neither; follows the [object-layer GA contract](object-layer-ga-design.md) |
| Text | Deferred; lines and fills do not need it | This record |

---

## 1. Executive summary

**Decision.** A Vector layer is an ordinary Object layer whose content is one
**vector object**, the root of a tree of **items**:
- The first item kind is the **path**: a compound cubic Bézier path with a fill rule and one **style**, an ordered list of fills and strokes. A stroke can carry a **width profile**.
- Shape tools, the Path tool and freehand pens all create paths. Groups, images, text and symbol instances are later kinds in the same tree.
- The file stores the items, never their outlines or rendered pixels.

**The design in brief**
- **Layer model:** no layer per path, no container record and no second layer hierarchy.
  - Rows, masks, effects, opacity, blend, clipping, clipboard and Rasterize Layer stay those of Object layers.
  - Path selection inside the layer is working state.
- **File:** one `capy.vector-object/1` record per layer references immutable binary `capy.vector-items/1` chunks. Together they hold the item tree in pre-order as tagged F64 data.
  - Readers without vector support preserve such files; the test `unknown_drawable_children_preserve_the_complete_package` already shows the mechanism.
  - Unknown item kinds and properties either preserve the file or, when flagged ignorable, are kept or dropped by fixed rules.
- **Rendering:** one new GPU coverage kernel with exact-area antialiasing and nonzero or even-odd fill rules, fed by CPU curve flattening and stroke expansion.
  - It also replaces the selection contour filler and the Figure distance-field shapes.
  - Layers evaluate at the display level the view needs, never finer than native at first; crisp zoom adds finer levels next.
- **Tools:** the Clip Studio families Capy already uses.
  - On vector layers, Figure (U) makes editable shapes and a new Path variant.
  - Move (O) selects paths, and a new Edit Points tool (A) edits anchors and handles.
  - G-Pen and Monoline draw vector lines, the Eraser cuts paths and Fill colors shapes.
  - Raster-only tools refuse with the existing notice.
- **Smoothing:** freehand vector lines use the preset's existing stabilizer, the global pressure curve, the pressure fall limiter and rulers unchanged. They are fitted to Béziers at pen-up.
- **Foundation:**
  - Brush paths, groups, images, text, instances, gradients and dashes are later item kinds, properties or paint kinds.
  - Live shapes, booleans, blends and other live features store their result as ordinary items plus a recipe.
  - Each addition is new vocabulary inside the same two types; older readers preserve the file, or safely ignore an addition flagged ignorable.

**What changed since the research**
1. **An Object layer holds exactly one object.** Commit `3e809895b` (2026-10-08) removed the object-layer collection record and child rows. The GA design's route of mixing vectors into "the existing ordered object list" no longer exists, so the vector object owns its items.
2. **`project_storage.rs` and `SelectionIndex` are gone** (`b3f6f8e51`). Binary data lives in typed resource records.
3. **Object-layer caches are derived renderer data,** never authored artwork. Persisting a raster cache for vector layers would contradict the GA contract.
4. **The CPU never processes canvas pixels** ([architecture](../architecture.md#cpu-and-gpu-responsibilities)). That rules out vello_cpu, tiny-skia and vello_hybrid, which computes strip coverage on the CPU. Full vello pins its own wgpu version and writes RGBA8, unlike Capy's RGBA32F premultiplied pipeline.
5. **No path filler fits.** Nothing fills paths with nonzero winding and good antialiasing, and the display never requests more than native density.
6. **Strokes are still not persisted, and two guides disagree with the code:**
   - [`docs/internals/brushes.md`](../internals/brushes.md) (lines 40 and 136) still describes stored strokes and replay.
   - [`docs/architecture.md`](../architecture.md) (lines 20–21) still describes object collections.
7. **The extension audit reshaped the model before it froze** (2026-10-16):
   - one item tree instead of a flat path list;
   - tagged properties with PNG-style rules for unknown data;
   - ordered paint lists instead of one optional fill and stroke;
   - transforms on groups and non-path items;
   - recipes for live features.

   It also made the package reader classify combinations a later version may allow as unsupported rather than damaged ([extension research](object-content-extensions-research.md)).
8. **A second audit fixed the record framing** (2026-10-16). It covered Affinity, CorelDRAW, Xara, Figma, Sketch, Penpot, painting apps, text engines, office and motion formats, CAD, and long-lived format designs. The changes:
   - a parent bit, so readers that do not know a container kind still find its end;
   - value classes, so unknown data can name items and slots safely;
   - recipes that hold nested inputs and carry a fingerprint;
   - a two-sided width profile;
   - a forward rule for anchor kinds;
   - item basics (hidden, name, locked) and live shapes from the first release.
9. **A performance study revised the renderer** (2026-10-16), against Vello, Graphite, Inkscape, Chromium, WebRender, other editors and Mali guidance ([performance research](vector-performance-research.md)):
   - vector content evaluates at the display level, not at native density;
   - a per-tile coverage kernel without atomics writes each pixel once;
   - gestures keep their state out of the document, and drags split the layer by paint order;
   - records share geometry by `Arc`, and chunks carry summaries.

---

## 2. Two approaches to vector, and the data model that bridges them

### 2.1 Data models found in the wild

| Model | Who | Stored | Strengths | Weaknesses |
|---|---|---|---|---|
| **A. Centerline stroke** | CSP, Concepts, OpenToonz, SAI linework, Blender GP v3, Harmony pencil lines, PencilKit, Jetpack Ink, Wacom UIM | Centerline (samples or spline) + per-point width/opacity + brush reference + seed | Swap brush, width or color later; intersection-aware erasing; reshape; textures replay through the brush engine | Export needs computed outlines; merge/subtract needs outlining first; fills need gap logic |
| **B. Filled outline** | Fresco vector, Illustrator Blob Brush, Affinity 3 Vector Blob Brush, Animate brush, perfect-freehand (Excalidraw, tldraw) | The outline polygon *is* the stroke | Exact SVG/PDF export; merge/subtract and same-color welding are natural | No re-brushing or width edits; thousands of anchors; "intersection" is ill-defined; solid color only |
| **Hybrid: path + width profile + applied brush** | Illustrator Paintbrush/Width tool, Affinity Vector Brush, Linearity, Inkscape Power Stroke, Synfig | Bézier path + width knots (+ art brush) | Designer precision; editable width | One profile per path; few inking cleanup tools |

Two further variants:
- **"Record input, re-render"** (Jetpack Ink, Excalidraw, tldraw, Wacom UIM sensor data): stores raw pen input and re-renders it through a brush. This is model A with raw samples as the truth.
- **Fitted geometry with baked pressure** (PencilKit, OpenToonz, Blender, Rnote, Xournal++): edits geometry more easily, but bakes pressure into width, so swapping the brush can't re-derive the look.

**The best designs keep both**:
- **Wacom UIM** stores sensor data plus a spline, with a mapping between them.
- **Jetpack Ink** stores its inputs plus a cached mesh.
- **Inkscape** stores `original-d` plus Live Path Effect (LPE) parameters, and writes the computed `d`.

### 2.2 What that means for us

Model A, the centerline, is what makes re-brushing, trimming and width editing
possible. Model B, the outline, is what makes export and merging exact. Capy
takes the hybrid row:

- **A path is the centerline.**
  - A pen path or shape authors its cubic Bézier geometry directly.
  - A freehand line is fitted to Béziers at pen-up.
- **Width is a profile along the path**, not baked into an outline. Thickening, tapering, restyling and a later brush swap therefore still work.
- **Outlines are derived** for rendering, hit-testing, SVG export of variable-width strokes, merging and Convert Stroke to Shape.
- **Shapes are paths with a fill.** Open and closed paths, compound paths with holes, merge results and imported SVG are the same item.
- **Recorded samples are not kept.**
  - Samples would let a later brush re-derive pressure dynamics other than width.
  - Against that, they multiply stored data and need a replay contract the [object-layer design](object-layer-ga-design.md) declined to freeze. They also stop describing the line after its first node edit.
  - Brush paths (section 7.9) start from the path and its width profile instead.

This follows the Inkscape and Wacom pattern of an editable source plus computed
geometry. Drawing and editing share one model instead of two subsystems.

---

## 3. Landscape: what each kind of app offers

### 3.1 Path editors: the shared core

Source: stream 1, section 8, has the full 30-row matrix.

| Tier | Features |
|---|---|
| **Table stakes (MVP)** | Selection with bounding-box transforms; **pen** (click = corner, drag = smooth, Alt breaks the handle, Shift constrains, Space moves the point being placed, click the start point to close); **node tool** (move anchors and handles, marquee, add/delete, corner↔smooth); cut/join/close/reverse; rect (corner radius), ellipse, polygon, star; one fill + one stroke (caps, joins, dashes); linear/radial gradients; **union/subtract/intersect/exclude**; compound paths; clipping; align/distribute; snapping; smoothed pencil; **outline stroke**; SVG in/out |
| **Expected by intermediate users** | **Shape Builder** (added by Inkscape in 2023 and Figma in 2025, so now standard); offset path; simplify; **variable width** (pressure → editable width points, Width tool, profiles); **curvature-style pen** (Illustrator Curvature, Affinity Smart, Inkscape BSpline/Spiro); **drag a segment to bend it** (Figma Bend, Graphite mold); live corners; smart guides; knife and vector eraser; text on path; **image trace**; live (non-destructive) merge; region paint bucket (Illustrator Live Paint, Figma); brushes along a path; multiple fills/strokes; repeat |
| **Advanced / niche** | Mesh gradients, blend/morph, envelope/warp, full effect stacks (Inkscape LPEs, Illustrator Appearance), procedural node graph (Graphite), vector networks (Figma), CMYK/spot, generative AI |

**The 15 most-used operations**, inferred from shared shortcuts, Adobe's contextual task bar, the fixes Figma shipped and UserVoice requests; no vendor publishes telemetry:
1. Select / move / scale / rotate / duplicate
2. Node editing
3. Pen drawing
4. Corner ↔ smooth conversion
5. Rectangles and ellipses
6. Fill/stroke color and weight, eyedropper
7. **Union/subtract, or Shape Builder**
8. Align and distribute
9. Group and arrange
10. Add or delete points
11. Join, close and cut
12. Outline stroke / text to outlines
13. Clipping mask
14. Offset path
15. Pencil or brush, then simplify

**Shortcut conventions to copy** (Graphite and Vectorpea show that a new editor can adopt them):
- V select, A node, P pen, N pencil, Shift+C convert, Shift+M Shape Builder, Shift+W width, Ctrl+J join, Ctrl+7/8 clip and compound.
- Inkscape's Ctrl+( and Ctrl+) for inset/outset, and Ctrl+L to simplify.

**The most relevant precedents for a painting app**:
- **Affinity** (free since Oct 2025): Vector, Pixel and Layout studios work in one document, and its vector brushes stretch raster textures along curves.
- **Graphite** (Rust; kurbo + vello + linesweeper; MIT/Apache): components we can borrow with attribution, including its vector domain model, pen and path tools, offset and intersection code, and brush sample channels.

### 3.2 Vector drawing apps: the tools that matter

| App | Data model | What artists praise | What artists complain about |
|---|---|---|---|
| **Clip Studio Paint** | Centerline + control points with per-point width/opacity factors; brush stored per stroke and **re-rendered with the raster brush engine** (textures replay) | Vector eraser (touched area / **up to intersection** / whole line); Object tool swaps brush, size or color of selected lines; Correct Line Width (thicken, thin, scale, fix); Simplify, Pinch, Connect, Redraw line and width; control points (move, add, delete, corner, width, opacity, split); vector magnet; fill "up to vector centerline" + close gap on a raster layer referencing the ink | Normal eraser leaves **invisible transparent strokes**; fill and gradient disabled on vector layers; "pixelated / fake vector" (renders at canvas resolution); SVG export drops width, color and taper; no mixing or watercolor brushes on vector; texture shifts after cuts; lag with heavy hatching |
| **Adobe Fresco** | **Filled outlines** (Blob-Brush style) | Clean, tapered pressure lines drawn "without wrestling with the pen tool"; **Vector Trimming** swipe gesture | Can't change brush, width or color after drawing; thousands of anchors in Illustrator; no SVG export (PDF → Affinity workaround); hairline gaps between fill and line |
| **Concepts** | Centerline + raw input; raster-on-vector textured brushes | Change brush, color, size **or smoothing** of existing strokes; **Nudge** (pull a line like string); **Slice**; infinite canvas; shape guides; SVG/DXF/PDF export | No fill bucket; exports are one line weight per stroke with textures stripped; Android/Windows lag behind iPad |
| **Affinity** | Path + raster texture + one pressure graph per curve; new Vector Blob Brush (3.x) | Textured vector brushes; editable pressure profile; nodes stay editable | One pressure profile per stroke is clumsy for local edits |
| **Illustrator (drawing tools)** | Path + appearance (width points, brushes) | Width tool and profiles; Pencil "edit selected paths" (redraw over a path to replace it); Blob Brush auto-merge; Live Paint with gap options | Freehand feels laggy and "CAD-like"; no erase-to-intersection |
| **Harmony / OpenToonz / Blender GP** | Centerline + thickness (Harmony pencil lines, Toonz thick quadratics, GP radius); Harmony textured strokes are vector outlines with a raster mask | **Fills computed from the stroke graph** with auto-close gap tolerance (Harmony 0–10 px, Toonz autoclose, Blender 5.2 Delaunay fill); invisible gap-closing strokes; fill placed *under* the line art | Animation-centric UIs |
| **Krita vector layers** | SVG shapes; the brush engine is **not** used | Nothing notable | "Useless for most line art"; no pressure; a wish-bug for about 10 years. The anti-pattern to avoid. |

**Killer features ranked by how often artists cite them** (streams 2 and 5):
1. Erase up to intersection / trim
2. Draw with the same brushes as raster, and keep strokes editable
3. Fill flats against ink with no gaps
4. Adjust line width after drawing
5. Stabilization plus hold-to-snap
6. Resize without quality loss (with a "scale line width" toggle)
7. Swap brush, size or color of existing strokes, one at a time or all at once
8. Grab to reshape (pinch/nudge/bend)
9. Real SVG/PDF export that keeps width and fills
10. Keep a hand-drawn, textured look
11. An eraser that behaves intuitively
12. Curve tool
13. Simplify
14. Performance with thousands of hatching strokes
15. Comic tooling (panels, balloons, speed lines)
16. Connect/magnet, off by default

---

## 4. How artists actually work (the journeys we need to support)

### 4.1 Canonical workflows

**Illustration or comic inking** (CSP; the most common vector workflow among artists):
1. Rough sketch on a raster layer at low opacity or tinted blue.
2. **Ink on a vector layer** with a G-pen and pressure on. Stabilization is typically 20–30, or up to 100 for shaky hands.
3. **Vector eraser, "up to intersection"**, to clean overlaps. Artists draw past crossings on purpose, then zap the overshoots.
4. Refine: correct line width, simplify, pinch, control points.
5. Optionally **swap the brush for the whole layer**. Tutorial comments call this out: *"GLOBALLY CHANGE BRUSH ON VECTOR LAYER OMG"*.
6. **Flats on a raster layer below**: mark the ink as a reference layer, then fill with close-gap and "fill up to the vector centerline".
7. Shade on raster (Multiply clipped to flats; highlights on an Add Glow layer). Color the lines with a clipped layer above the ink.
8. Comics add vector **panel borders**, vector **balloons**, and **speed/focus lines** from rulers. Blacks are spotted on raster.

**Scalable line art for merch, stickers and Cricut** (Fresco; the most common route for hobby and freelance work):
1. Sketch with a pixel brush.
2. Ink with a vector brush, pressure off for monoline.
3. **Trim** the overshoots.
4. Vector bucket fill.
5. Export PDF, then use Affinity or Illustrator to get SVG, because Fresco has no SVG export.

**Painter to vector, the multi-app workaround** (Procreate and ibisPaint users):
1. PNG.
2. Image Trace in Illustrator, Vectorizer.AI or Cricut's converter.
3. Clean up nodes and gaps.

Tutorials for this reach 180K–370K views. The complaints: *"522 nodes where a clean version… is about 59"*, *"jagged and awkward"*, double outlines around single lines, and gaps that block fills.

**Maker / cut files** (the largest hobby population; Cricut reported about 5.9M active users at the end of 2025):
- Closed, **welded** shapes with few nodes.
- Text converted to outlines.
- An **offset border** for stickers.
- No overlapping or double lines.
- Correct stroke-vs-fill semantics per machine: Cricut cuts a stroke-only path twice; LightBurn maps stroke color to cut layers.
- SVG/DXF/PDF output in physical units.

**Logo from a sketch** (freelancers):
1. Place the sketch.
2. Rebuild it from shapes plus Shape Builder, or trace with the pen/curvature tool.
3. Convert text to outlines.
4. SVG/PDF.

**Design sketching** (Concepts: industrial design, architecture):
1. Infinite canvas with grids.
2. Pressure or "wire" pens.
3. Nudge and Slice.
4. DXF/PDF.

### 4.2 The learning curriculum people actually follow

**Vector drawing** (from the 20+ most-viewed tutorials, including CSP's official 1.32M-view video, Loading Artist at 585K, and Piascik/Fresco at 274K):
1. Vector vs raster ("won't lose quality when resized").
2. Make a vector layer and ink over a sketch.
3. **Erase/trim to intersection.**
4. Thicken, thin and simplify.
5. Reshape by grabbing.
6. Swap brush or color of existing lines.
7. Fill on a layer that references the ink.
8. Export (taught late).

**Path editing** (Adobe *Classroom in a Book*, Domestika, Logos By Nick, Affinity Revolution):
1. Select and transform.
2. **Shapes, with a shapes-only mini project** (logo, mountain, badge).
3. Fill vs stroke.
4. **Merge / Shape Builder.**
5. Align and snapping.
6. **Curvature/pencil, then pen.**
7. Node editing and corners.
8. Width and outline stroke.
9. Groups, clipping, holes.
10. Text to outlines.
11. Image trace.
12. Export.

**The lesson from both curricula:** artists get a satisfying result *before* they touch a Bézier handle.

### 4.3 What breaks adoption (with representative quotes)

- **The feature is hard to find.** On CSP's official video, *"I've been using Clip Studio Paint like an idiot"* has 1.2K likes and *"…nearly a year without having this information?"* has 1.2K more. Vector hides in a layer-type menu under a jargon name.
- **The first erase or fill goes wrong.**
  - *"PSA: Do NOT use the normal eraser with your vector drawings"*: in CSP it draws invisible transparent vector lines.
  - Fill is disabled on CSP vector layers.
  - Fresco vector fills leave hairline gaps: *"needs an Expand by 1-5 pixels and call it 'Overfill'"*.
  - Many beginners give up on vector right there.
- **"Why are my vector lines pixelated?"** CSP renders at canvas resolution, which leads to *"fake vector?"* threads.
- **Lines look too clean.** *"it feels a bit sterile"*, *"lose the uniqueness of a brush tip"*.
- **Performance** with hatching, and file bloat from excess control points.
- **Control points are fiddly.** *"easier to redraw a line than vector it into place"*.
- **Path editors** have their own traps:
  - Bézier handles (*"literally doing my head in, HEEEEELP!"*).
  - Merge that fails silently on groups, text or line-only objects (*"One object is not a path"*).
  - Stroke vs fill vs "expand".
  - Strokes that don't scale with the object.
  - Open paths filling haphazardly.
  - Holes in letters like B and O.
  - Traces that produce hundreds of thousands of nodes.
  - Hidden "inside a group" states.

**What makes adoption stick** (the "aha" moments):
- An overlap disappears in one swipe: *"where has this been my whole life???"* (396 likes), *"so fun to zap those lines"*.
- Re-skinning finished ink.
- Thickening a line without redrawing it.
- Editable ink as an **accessibility aid** for shaky hands and carpal tunnel: correct instead of redraw.
- Drawing "like Procreate" and getting vector out: *"I can't tell you how many Google searches I've done looking for a program that allows me to draw like I can on Procreate, but in vector form"*.

### 4.4 Positioning

- **Linux:** CSP has no Linux build, and Krita's vector layers are weak for ink. "Pressure-sensitive, editable ink on Linux" fills a known gap. Krita-Artists threads show demand from artists who left CSP.
- **Everywhere else:** no app offers *editable pressure ink* together with *clean SVG/cut-file output* in one free, cross-platform tool.

---

## 5. File formats and the "replay" question

### 5.1 Is there a standard for replayable brush strokes?

**No.** The options:

| Format | Status | Can it describe a painting brush? | Who reads it |
|---|---|---|---|
| **W3C InkML** | Recommendation (2011) | No: ellipse/rectangle tip, color, width, transparency | MS Office ink, OneNote API, handwriting datasets |
| **Microsoft ISF** | Published spec (Open Specification Promise) | No (tip, size, fit-to-curve) | Windows Ink (as GIF with embedded ISF) |
| **Wacom Universal Ink Model 3.1** | Vendor spec (RIFF + protobuf); library Apache-2.0; no ISO/W3C status found | **Yes**: raster brushes with shape/fill textures, spacing, seed | Wacom ecosystem |
| **Jetpack Ink (`androidx.ink`) protos** | Open source (google/ink, Apache-2.0); BrushFamily serialization stable since 1.1.0-alpha02 | **Yes**: behavior graph, textures, `min_version` + fallback families | Android apps using Jetpack Ink |
| **PencilKit `PKDrawing`** | Opaque; readable only through Apple's API | Yes (Apple's inks) | Apple platforms |
| App formats (`.clip`, `.xopp`, `.rnote`, Excalidraw, tldraw) | Private | Engine-specific | Only their own app |

- **Replay needs the brush engine too.** Replaying samples reproduces a look only with identical engine behavior: spacing, dynamics curves, jitter RNG, blending.
- **Every "real" brush format is tied to one engine**: ABR, Krita `.kpp`, Procreate `.brush`, CSP `.sut`. MyPaint's `.myb` with libmypaint (ISC license) is the only portable engine-plus-format pair.
- **What "replay elsewhere" can realistically mean:**
  1. static geometry (SVG/PDF outlines);
  2. media (time-lapse video, animated SVG draw-on);
  3. ink data for ink tools (InkML);
  4. exact re-editing only in Capy Canvas, from our own data embedded in SVG/PDF.

### 5.2 What's safe in SVG

- **Interoperable SVG:**
  - Use SVG 1.1 syntax with presentation attributes.
  - Safe features: paths, groups with transforms, fill/stroke/opacity, caps/joins/dashes, linear and radial gradients, embedded PNG, and `viewBox` with physical units.
  - Avoid `<style>` blocks: Cricut and Silhouette ignore them.
  - Mesh gradients and hatches were removed from SVG 2.
  - Blend modes, filters and masks are patchy outside browsers.
- **No variable-width stroke** exists or is proposed (W3C ISSUE-2271, open since 2009).
- **Inkscape's pattern:** Live Path Effects keep `inkscape:original-d` plus `<inkscape:path-effect>` parameters, and write the computed outline into `d`. Unknown namespaces are preserved; Ink/Stitch depends on this.
- **The Inkscape pattern to avoid:** regenerating `d` on re-edit can overwrite changes other apps made.
- **Makers need explicit semantics:**
  - centerline strokes for plotting, scoring and running stitch;
  - clean filled outlines for vinyl and engraving;
  - no clip, mask or style;
  - physical units.

---

## 6. Code audit at `15f01f80e`

This audit replaces the earlier feasibility section, whose references described
`796f7f5b`. In this section, a path written without its crate is relative to
the crate named in the finding: `layer-core` for the model and format,
`layer-render-wgpu/src` for rendering.

### 6.1 Object layers and layer kinds

| Finding | Evidence | Consequence |
|---|---|---|
| An Object layer's content is one image object | `OccurrenceContent::Objects(ImageObjectHandle)` (`crates/layer-core/src/authored/artwork.rs:47-48`); `ImageObject { image, affine, interpolation }` (`authored/objects.rs:217-221`) | Vector content becomes a sibling object type behind the same content alternative |
| The collection design was removed | `34bc831bd` added `capy.object-layer/1` with `children`, and `ea680ff29` exposed child rows. `3e809895b` removed both (net −2,011 lines) so that layers reuse ordinary layer selection, properties, masks, clipboard and history | Do not restore a container record. The vector object owns its items internally, a type-specific hierarchy that [GA design §12](object-layer-ga-design.md#12-long-term-end-state-and-extension-stress-tests) allows |
| Kind checks are many and partly invisible to the compiler | `OccurrenceContent::Objects`: 37 non-test lines in 18 files. `LayerKind::Object`: 32 lines in 14 files. `matches!` lists such as `authored/occurrence_edits.rs:131,155,297` would silently miss a new kind | Keep `LayerKind::Object` and make the object handle an enum, so that image-only reads stop compiling |
| Picking is a rectangle test | `object_contains` does an inverse-affine point-in-rectangle test and ignores alpha (`authored/object_edits.rs:36`). Picking runs only under Move and Transform (`crates/layer-ui/src/object_editing.rs:112-117`) | Path hit-testing is new core code |
| Object bounds have five separate formulas | Core: `merge.rs:120-133`, `authored/object_edits.rs:7-25`. Renderer: `crates/layer-render-wgpu/src/scene/objects.rs:42`, `scene/object_spatial.rs:17`, `snapshot/bounds.rs:86` | One bounds function per object kind replaces all five |

### 6.2 File format

| Finding | Evidence | Consequence |
|---|---|---|
| Registry and wiring | 17 record kinds, each in both portable and private contexts (`package/registry.rs:30-48`). The image object touches 46 sites in 10 files: registry, manifest, topology, artwork, record adapters, `Edit`, roots and history accounting, session, session transfer, transfer and authored scene. Exhaustive destructuring finds most of them; the hard-coded type strings in `package/artwork_records.rs` and `_ =>` arms do not | Expect about the same footprint for a vector object, plus a resource kind |
| Unknown object types are preserved | An unknown referenced type under `objects` opens as Preserved, and Copy Original returns identical bytes (`unknown_drawable_children_preserve_the_complete_package`, `package/codec/admission.rs:206`) | Older readers keep vector files intact without new code |
| A list under `objects` is invalid | `package/manifest.rs:110` expects one object reference; an array becomes `DecodeError::Invalid`, which older readers report as damaged | Never widen `objects` into a list |
| Binary resource precedent | `capy.lut3d/1` stores a little-endian F32 block under raw encoding `capy.rgb-f32/1`, optionally wrapped in `capy.lz4-bytes/1`, and bounds the decoded size before allocation (`package/resources.rs:380-424`). Payloads carry local slot indices, never Capy IDs | Model path geometry on it. No existing resource kind fits paths |
| Selection contours are inline JSON | F32 points under `shape.contours` (`package/selection_records.rs:56-95`) | Fine for a few contours, not for artwork |
| Admission limits | 65,536 object records, 262,144 evaluation edges, 262,144 resources, 64 MiB and 4,194,304 JSON nodes of metadata, 4,096 occurrences. Resource bindings consume no evaluation edges | One record per path exhausts the record limit at about 6,000 strokes in each of 10 layers. Inline JSON points cap a document near 1.4 million points |
| History sharing | `Store` shares records by `Arc`, but `RecordChange<T>` holds `T` by value. Selection contours are charged per inner `Arc` (`history_budget.rs:137-151`) | Precedent for sharing unchanged paths between undo states |
| Session cost | Private sessions re-encode each changed record's complete JSON (`package/session_transfer.rs:38-43`). Only tile indexes are chunked, 64 entries each (`package/session.rs:86-99`) | Keep the vector record's JSON small and chunk its geometry |

### 6.3 Rendering

| Finding | Evidence | Consequence |
|---|---|---|
| Two object leaves | Native `object_tile` (`scene/objects.rs:264`) and display `Compositor::objects` (`scene/scale.rs:1403`). Masks, effects and clipping are applied above them by the generic `owner_image` (`scene/stack.rs:118`) | A new kind dispatches only at these leaves and at thumbnails |
| Any density can be evaluated, but none above native is requested | `ObjectWindow { origin, side: f64, size }` composes any density into the sampling map (`scene/objects.rs:94`). `display_mips::view_level` never goes below level 0 (`display_mips.rs:19`) | A path rasterizer can honor any density now. Crisp zoom needs above-native plan levels |
| No suitable filler | The selection contour filler counts edge crossings with `atomicXor` at 2×2 samples, even/odd only (`selection_clip_init.wgsl:7-69`). Figures are single-sample distance fields for line, rectangle and ellipse (`scene.wgsl:64-118`). There is no MSAA, no nonzero filler, no stroke-to-outline code and no geometry crate in `Cargo.lock` | Build one coverage kernel (7.5) |
| Damage is per object | `edited_damage` (`scene/objects.rs:49-78`) damages whole old and new object rectangles from `image.extent`. It falls back to compositing everything when order changes. The "spatial index" is a per-owner bounds map (`scene/object_spatial.rs:24-47`) | A vector object needs damage per item and a real index inside each layer |
| Object layers switch off document-wide optimizations | Any object layer turns off batching and idle refinement for the whole document (`scene/scale/refinement.rs:25,96,105`, `scene/scale/hierarchy.rs:55`) | Key these on pending image work first, or every vector document loses refinement |

### 6.4 Input, smoothing and brushes

| Finding | Evidence |
|---|---|
| Samples | `PenEvent` (`crates/layer-engine/src/input.rs:66`) becomes `StrokePoint` (`crates/layer-core/src/lib.rs:273`) in layer-local coordinates, after the global pressure curve and ruler projection. `StrokeBuilder` keeps predicted points apart, and `finish()` returns only real ones |
| Smoothing | `PositionSmoother` (`crates/layer-engine/src/brush/smoothing.rs:12`) integrates a causal kernel over the input. Its strength and adjust-by-speed come from the preset's `BrushStabilization` (`crates/layer-core/src/lib.rs:548`); the input scale is captured from zoom at pen-down. It runs before placement in `append_with_field` (`crates/layer-engine/src/brush.rs:228`). Pen-up extends the curve to the held final point instead of drawing a catch-up chord |
| Other input shaping | A pressure fall limiter (80 ms for contact pens, about 34 ms for G-Pen), prediction with late correction within 2 s, and Straight, Parallel and Radial rulers. There is no pull-string stabilizer, hold-to-snap or shape assist |
| Centerline already exists | For contact pens, `append_swept` (`crates/layer-engine/src/brush.rs:349`) emits a simplified, smoothed, variable-width centerline: the natural input for fitting. `PositionField` (`crates/layer-engine/src/brush/smoothing/preview.rs:4`) lets the dab generator follow any position field, which is the hook for brush paths |
| Strokes are transient | `Stroke` is "Never part of document persistence or undo history" (`crates/layer-core/src/lib.rs:1172`). The last stroke is kept for two seconds of corrections, then dropped |
| Brush classes | Solid: G-Pen, Calligraphy Pen, Marker. Textured dry and contact: the pencils, Charcoal, Crayon, Chalk and Pastel Block, the Rough, Antique, Realistic, Wet, Blotty and Brushed inks, the paintbrushes, Airbrush and Spray. Canvas-reading: Smudge, Natural Blender, the wet paints, watercolors, Liquify, Clone and the healing brushes |

### 6.5 Layers UI and tools

| Finding | Evidence | Consequence |
|---|---|---|
| Rows are flag-driven | `LayerState` (`crates/layer-ui/src/lib.rs:1284`) carries flags, not a row kind. A `content_icon` on a row that has a thumbnail draws a corner badge on every host. No host reads the `object` flag | A vector badge costs no host code |
| Creation | The shared New submenu (`crates/layer-ui/src/art_layers.rs:1527`) uses `LayerAction::New { group, clipped }`. The layer-panel footers are hand-written on all five hosts | Add New Vector Layer to the shared submenu, not to the footers |
| Tools | `LayerCanvasTool` (`art_layers.rs:59`) has no path or text tool. Figure is Line, Rectangle or Ellipse, painted destructively into paint, with no handles before or after commit (`crates/layer-core/src/figures.rs:8`, `crates/layer-ui/src/figures.rs:86-109`). Tool options use shared `ToolOption` kinds; hosts have no per-tool code | New tools that use existing option kinds cost almost nothing per host |
| Gestures | Non-paint tools go through `layer_pen` (`art_layers.rs:1601`). Fingers drag canvas objects only where `object_touch_hit` (`crates/layer-ui/src/session.rs:5618`) says so | Anchors and paths need touch hit targets there, or fingers navigate |
| Overlays | Shared `CursorSegment` primitives (`crates/layer-render/src/lib.rs:59`) offer lines, square handles and triangles. There is no round marker, and curves must be flattened | Anchor and handle display is a shared renderer change |
| Refusal | `LayerKind::Object => Err(DrawingRefusal::Object)` (`crates/layer-core/src/layers.rs:595`). The text in `assets/locales/en/objects.ftl:7-8` reads "holds placed images, not paint" | Vector layers need their own wording in all 15 locales |
| Colors and properties | Foreground, background and transparent color slots; Figure's Both mode already strokes with the foreground and fills with the background. The Properties drawer has Number, Toggle, Choice and Color kinds (`crates/layer-ui/src/effects.rs:575-582`) | Path style needs no new widget |
| Shortcuts | P cycles the ink pens, O is Move, U is Figure and Shift+U is Ruler. Single A, V, K, L, N, T, Y and X are free in the Capy keymap. The Photoshop and Affinity presets put Move on V (`crates/layer-ui/src/keymaps.rs`) | "Pen" and P are taken. The Bézier tool needs another name |
| Localization | 15 locales × 12 Fluent domains | Every new string lands in all 180 catalogs |

### 6.6 Dependencies

Measured sizes and licenses come from stream 7; decisions are from this design.

| Role | Crate | License | wasm size (gzip)¹ | Decision |
|---|---|---|---|---|
| Geometry: fitting, nearest point, arc length, flattening, offset | **kurbo 0.13** | MIT/Apache | 36 KB | Adopt in V0, after `cargo deny` and a wasm size check |
| Spatial index | rstar 0.13 | MIT/Apache | 14 KB | Not needed at first: bins per 256-pixel tile match the renderer's tiles |
| Robust polygon operations, integer mode | **i_overlay 9.0** | MIT/Apache | 61 KB | Adopt with merge in V2; refit results with kurbo |
| Merge on curves | linesweeper 0.4 | MIT/Apache | 74 KB | Not at first: early beta, and Graphite reported crashes |
| Fill rendering | vello_cpu, vello_hybrid | MIT/Apache | 126 KB | Rejected: both compute pixel coverage on the CPU |
| SVG import | **usvg 0.48** | MIT/Apache | 158 KB | V2 |
| PDF export | krilla 0.8 | MIT/Apache | 416 KB, lazy | Later |
| PDF/AI import; image trace | hayro 0.7; visioncortex 0.9 | MIT/Apache | 1.38 MB lazy; — | Later |
| Gap closing and regions to port | OpenToonz `tl2lautocloser`, `tcomputeregions` | BSD-3 | — | With vector region fill |

¹ Isolated wasm size-test builds (opt-level z, LTO, stripped), measured for stream 7.

**Don't link or copy:** potrace, autotrace, Rnote, Krita, Inkscape and Blender
(GPL); lib2geom (LGPL/MPL); mupdf (AGPL); clipper2-rust (BSL-1.0, not on the
allowlist).

---

## 7. Design

### 7.1 Principles

1. **Lines and fills first, as an illustrator expects.** Paths stay paths, strokes stay strokes, and shapes stay editable until Rasterize Layer.
2. **One item type for lines and fills.** Shapes, pen paths, freehand lines, merge results and imported shapes are all paths. Behavior differs by tool, not by parallel object kinds. Later kinds (groups, images, text, instances) join the same tree.
3. **Vector content is an object type, not a layer hierarchy** (6.1).
4. **Authored geometry, derived pixels.** The file stores neither outlines nor rendered pixels of vector content.
5. **Never worse than raster at what beginners try first.** Drawing, erasing, filling, resizing and undo all work on a vector layer, with no error and no invisible artifacts.
6. **Plain names.** Vector layer, Path, Edit Points, Make Compound Path, Convert Stroke to Shape.

### 7.2 Layer model

```text
Stack entry → Occurrence (name, visibility, opacity, blend, mask, effects, integer offset)
                └ content: objects → VectorObject: the root group of one item tree
                                       ├ items: [Chunk] → record stream, pre-order, front to back
                                       └ slots: images, fonts, definitions, effects (later)
Record   { kind word, id: u64, properties: [tag word → value] }
  Path     { geometry, fill rule, style, name, hidden, locked, live shape }   V0
  Group    a parent record: its children follow, then an End marker          later
  Text, Image, Instance, Definitions                                         later
Subpath  { anchors: [Anchor], closed: bool }
Anchor   { point, in_handle, out_handle, kind: Corner | Smooth | Symmetric }
Style    nested records: [Fill { paint } | Stroke { paint, width, cap, join, miter limit, width profile }]
```

- **One tree:**
  - The vector object is the root of a single item tree. SVG, PDF, Illustrator, Inkscape, Affinity, Figma and Krita all use one.
  - A flat path list would need a second nesting mechanism once groups, clipping groups and symbols arrive, with its own selection, undo and compositing rules.
  - V0 defines only Path. Group arrives in V2 as a parent record: a bit in its kind word says that children and an End marker follow. A reader that does not know a later container kind can still find where it ends.
  - Adding or removing a child changes only the chunk that holds it.
- **Handles:** `OccurrenceContent::Objects` holds an `ObjectHandle`, which is either `Image(ImageObjectHandle)` or `Vector(VectorObjectHandle)`.
  - `LayerKind::Object` remains, and a derived object kind drives tool routing and UI text.
  - Image-only reads (`.image`, interpolation, sampling preflight) stop compiling and are fixed one by one.
- **Transforms:**
  - **Paths keep none.** Their geometry is in the layer's local pixels.
    - Move, scale, rotate and flip rewrite the affected coordinates, which is exact for Béziers, as one undo step.
    - With Scale Strokes on, stroke widths, width profiles and later dash lengths scale by √|det|. Illustrator and Inkscape work the same way.
    - Later gradient and pattern geometry transforms with the path.
  - **Items that cannot absorb a transform carry an F64 affine:** groups, images, text and instances. Their content (shared definitions, glyphs, image pixels) cannot be rewritten in place.
  - **Under a group's affine,** stroke widths, dashes and effect radii are measured in the item's own space and transformed with it, as in SVG. A later non-scaling stroke is the opt-out. This is fixed now because changing it later would change how existing files look.
  - **Stretched strokes stay in a group.** One width cannot describe a stroke under a skew or uneven scale, so SVG import keeps such a path inside a group that holds the transform, as usvg does.
  - **No layer transform:** the root group has no affine, and the occurrence keeps its integer offset.
- **One vector object per layer, many items per object:**
  - The layer is the isolation and cache boundary, and it holds thousands of items.
  - Several vector layers are fine. Each one counts toward the 4,096-occurrence admission.
- **Item identity:**
  - Each item, groups included, has a layer-local `u64` ID. It is assigned at creation and kept through edits, undo and save.
  - **Allocation:** a new ID is a random nonzero value. It is checked against every ID the object contains or references: nested records, definitions and items the reader does not understand. IDs are never reused, never positions and never renumbered on save.
    - A counter recomputed from live items would reuse deleted IDs, as SQLite row IDs do.
    - A stored high-water mark would be rolled back by Undo.
  - **Duplicating a layer** keeps item IDs, because they are layer-local. Pasting renews colliding IDs and rewrites references within the pasted set in the same edit.
  - Like ruler IDs, the ID is owned by the record type, not a portable record ID.
  - It anchors working selection and history, and later references inside the layer, such as instance overrides and text on a path.
- **Working state:**
  - `WorkingState` gains the selected items of the current vector layer and, while editing points, the selected anchors. Later it also records the group being edited.
  - Selection is not an undo step, but Undo and Redo select the items they restore. Delete repairs it.
  - Portable saves omit it; private sessions keep it. Selecting an item makes its layer the current layer.
- **Undo:**
  - `Edit::RecordChange<VectorObject>` replaces the record.
  - The record stream lives in immutable chunks of at most 64 records (End markers included), and an edit copies only the chunks it touches.
  - Records hold path geometry by `Arc`, so copying a chunk is shallow. In memory, a path over 256 anchors keeps them in `Arc` runs of 256, so an anchor edit copies one run.
  - Each chunk carries a summary computed once: bounds, counts, flags and style keys. Select All, mixed property values and layer bounds then cost O(chunks).
  - Parents and subtree ranges are derived on load and after each edit, never stored.
  - History charges chunks and geometry by `Arc` identity, as it charges selection contours today.
- **Later content in the same layer:** images, groups, text and instances are further item kinds in this tree, not a second container (7.9 and the [extension research](object-content-extensions-research.md)).

### 7.3 Paths and appearance

**Geometry**

| Element | Representation | Reason |
|---|---|---|
| Anchor | Point plus in and out handle offsets, F64 in layer pixels. A zero offset is a retracted handle | Matches node editors; maps one-to-one to SVG and PDF `C`/`L` commands; exact under affine maps |
| Anchor kind | Corner (independent handles), Smooth (collinear handles, independent lengths), Symmetric (mirrored handles). A reader edits a kind it does not know as Corner, and writes Corner when it changes that anchor | Editing intent that handle positions cannot recover; Inkscape keeps `sodipodi:nodetypes` for the same reason. Rendering never needs it, so a later kind such as Affinity's automatic Smart node does not make files read-only |
| Segment | Cubic `(p0, p0+out0, p1+in1, p1)`; a line when both handles are retracted | Lottie, Rive, After Effects and Illustrator store cubics too. Exact arcs, circles and splines for CAD become recipes over a cubic result (7.4), so older readers still edit them. A new segment kind would be a new required geometry tag |
| Subpath | One or more anchors and a `closed` flag that adds the closing segment. A single-anchor subpath is a dot, drawn only with round or square caps | Covers lines, outlines and dots |
| Path | One or more subpaths with one fill rule (nonzero or even-odd) and one style. A compound path is a path with several subpaths; holes come from the rule | Merge results, letters with counters and imported SVG need no other type |
| Fill rule | On the path, not on each fill | It defines the shape's inside for every fill, hit-testing, merge and later clipping |
| Other curve types | Quadratics become cubics exactly. Arcs and ellipses become cubics split finely enough to stay within 1/64 pixel in the path's own units, so large radii get more segments | One segment kind for editing, rendering and export |
| Precision | F64 in memory and in the file | Image-object affines are F64 already; F32 would drift after repeated transforms on 61 MP canvases |

**Appearance**

- **A style is an ordered list of paints.**
  - Each entry is a Fill or a Stroke, painted first to last.
    - SVG's default is `[Fill, Stroke]`, and `paint-order: stroke` is `[Stroke, Fill]`.
    - Illustrator, Affinity, Figma, Sketch and Graphite allow any number of each.
  - The first tools and Properties edit at most one fill below one stroke. The format holds any list.
  - An empty list is valid: the path is selectable, shows as an outline while selected and acts as construction geometry.
- **Paint:** each entry has exactly one paint. V0 has a portable color. Gradients, patterns, meshes, swatch references and context paints are later paint kinds.
- **Fill:** a paint.
- **Stroke:**
  - A paint, a width in layer pixels, a cap (butt, round or square), a join (miter, round or bevel), a miter limit and an optional width profile.
  - The stroke is centered on the path. SVG 2's final join set is also miter, round and bevel; `miter-clip` and `arcs` were removed in 2026.
  - Frozen wire defaults follow SVG (butt, miter, 4), so export needs no attributes for them. Tool defaults are separate: pens use round caps and joins.
  - Dashes, inside and outside alignment, markers and non-scaling strokes are later stroke properties.
- **Width profile:**
  - It belongs to a stroke, as in Illustrator, Affinity and Graphite. A freehand line has one stroke with a profile.
  - It is a list of knots `(segment, t, left, right)` ordered along the path. Left and right are the factors on either side of the path; tools write equal sides at first.
    - Two sides are stored from the start because Illustrator's one-sided widths and CorelDRAW's two-sided outlines need them, and a second profile layout would be decoded forever.
  - Each side's width is half the stroke width times its factor, interpolated linearly between knots and held constant beyond the ends.
  - Knots are anchor-relative, so moving an anchor leaves widths in place.
  - Splitting a segment remaps its knots exactly. Deleting an anchor reparameterizes the merged segment.
  - Changing the stroke width scales the whole profile, as Illustrator does.
- **Styles are inline:**
  - Each path stores its own style. Readers intern equal styles in memory, and LZ4 absorbs the repetition in files.
  - A shared table would save no edits: changing the style of selected paths rewrites their chunks either way. Its indices would also be hidden references that compaction renumbers.
  - Linked styles (graphic styles, global swatches) come later as references to document-level definitions.
- **Opacity and blend:**
  - There is no per-path opacity, blend or effect at first. Layers provide them, and color alpha covers translucent paths.
  - The [GA scope rule](object-layer-ga-design.md#3-ga-state-and-deliberately-deferred-work) defers per-object opacity, blend and effects. They come later as properties of items, groups and paint entries.
- **Item basics from V0:**
  - **Hidden** is required data: a hidden item does not render.
  - **Name and locked** are ignorable and copy-safe.
  - Every editor surveyed has all three, and SVG, Inkscape and Illustrator imports need them before an object list exists.
- **Live shapes from V1:**
  - Rectangle, Ellipse, Polygon and Star keep their kind, frame (center, size, rotation) and parameters (corner radii, point count, inner radius) as a live-shape recipe (7.4).
  - Edit Points or a merge turns them into plain paths.
  - The first release thereby uses all three classes of unknown-data handling: required geometry, an ignorable copy-safe name and an ignorable, not copy-safe recipe. Rules no shipped data uses tend to break unnoticed.
- **Colors:**
  - Colors are portable colors, converted into the working space at evaluation as effect colors are.
  - Coverage blends in the composition's blend domain. In Linear documents, antialiased dark hairlines therefore render slightly lighter than in browsers (open question 7).
- **Derived data, never stored:** flattened segments, stroke outlines, bounds, tile bins, the tree index and hit-test data.

### 7.4 File format

The occurrence content `{"objects":{"ref":…}}` names either an image object or
a vector object.

```json
{"id":"…","type":"capy.vector-object/1","data":{
  "items":[{"ref":"…"},{"ref":"…"}]
}}
```

Each `items` entry references a `capy.vector-items/1` resource:
- **Data:** its record data gives the record count.
- **Payload:** a raw little-endian layout encoding, optionally wrapped in `capy.lz4-bytes/1` like LUTs.
- **Stream:** concatenated in order, the chunks form the record stream. A record never spans chunks; a chunk boundary may fall anywhere else, including inside a group.

**Framing.** The exact bytes belong in the
[package contract](../reference/capy-package.md) when implemented. The structure
is fixed now, because changing it later needs a new resource type:
- **Record:** a kind word, then, unless it is the End marker (kind 0), a `u64` ID and a length-prefixed property list.
- **Property:** a tag word, a byte length and the value. Every value is length-prefixed, including fixed-size ones, so any reader can step over any property.
- **Words are varuints with flags in the low bits:**
  - **kind word:** ignorable, copy-safe, parent (children and an End marker follow), one reserved bit; the rest is the kind number;
  - **tag word:** ignorable, copy-safe, a two-bit value class, one reserved bit; the rest is the tag number.
  - Flags are part of a number's identity. Changing them means a new number.
- **Canonical form:** minimal varuints; tags strictly ascending within a record; no repeated tag; a known value fills its length exactly; F64 values finite.
- **No growing values:** values never grow by appending fields. New fields are new tags, so chunks stay byte-stable and fingerprints stay meaningful.
- **V0 vocabulary:**
  - Kinds: Path.
  - Path tags: geometry, fill rule, style, hidden, name, locked and live shape.
  - Paint-entry kinds: Fill and Stroke. Stroke tags: width, cap, join, miter limit and width profile.
  - Paint kinds: color.
  - Geometry: per subpath, the anchor count and the closed flag. Per anchor, a `u8` kind and the point and both handle offsets as F64 pairs. Per width knot, a `u32` segment, an F64 `t` and two F64 factors.
- **Never:** Capy IDs inside the payload.

**Value classes** tell a reader that does not know a property what it refers to:

| Class | Value | A reader that does not know the property |
|---|---|---|
| Data | Opaque bytes with no item IDs, slot indices or offsets | Copies it |
| Items | A list of item IDs in this vector object | Rewrites them when IDs are renewed; treats a missing ID as a deleted item |
| Slots | A list of (slot list, index) pairs | Keeps those slots alive and renumbers them when it compacts slot lists |
| Nested | A record stream with the same framing | Applies these rules recursively |

- **Nested records:** styles and their paints, recipe inputs and later text tables are nested records.
- **Anonymous records:** a record inside a value may have ID 0 when its kind needs no identity, as paint entries do.

**Unknown kinds and tags.** PNG's chunk rules, made exact:

| Bits | Reader without support |
|---|---|
| Not ignorable, or a reserved bit set | Unsupported: the file opens as Preserved, never as Invalid |
| Ignorable and copy-safe | Ignores it when rendering and keeps it unchanged through edits. Clones it on duplicate, rewriting item references. Drops it when a reference would point outside the layer or at a deleted item |
| Ignorable, not copy-safe | Ignores it when rendering and keeps it while its dependency content is unchanged. Drops it when that changes, and on duplicate or paste |

- **Dependency content:** the owning record, its subtree, and the items its Items values name.
  - Each contributes its kind and its required properties, recursively, without IDs.
  - Renaming an item therefore keeps a recipe, while moving, restyling, reordering children or editing a referenced item drops it.
- **Unknown parent kinds are atomic:** their children are never interpreted, and an ignorable one hides its whole subtree from rendering. Containers whose children should still render (links, slices, tags) are therefore ignorable properties on Group, not new kinds.
- **Known tags with values the reader does not accept:**
  - an ignorable tag is handled as an unknown property with the same bits;
  - a required tag makes the file Unsupported;
  - a tag may define its own forward rule instead, as anchor kinds do.
- **Operations that make new items** (merge, outline stroke, split pieces) give them no unknown properties. The piece that keeps the original ID keeps them.

**Slots**
- **Why:** payloads cannot hold Capy IDs, so item kinds and properties that use package records or resources name them through slot lists in the record's JSON.
- **Lists:** `images`, `fonts`, `definitions`, `effects`, and later `links` to other vector objects. Each list arrives with the first feature that needs it.
- **References** use the Slots class wherever they appear, so writers can compact the lists without breaking data they do not understand.

**Live content: results with recipes**
- **Storage:** a live feature stores its result as ordinary records, and its editing intent as an ignorable, not copy-safe property:
  - a rounded rectangle is a Path plus a live-shape property;
  - a blend, a Repeat or a live boolean is a Group holding the results, with a recipe on the group.
- **Inputs that must not render,** such as a blend's key objects or a boolean's operands, are nested records inside the recipe, with their own recipes. Illustrator keeps its edit art apart from the result art in the same way.
- **References:** recipes may name other items (connectors, text on a path) and slots (a trace's source image, a brush) through the Items and Slots classes.
- **Fingerprint:** each recipe stores a fingerprint of its dependency content, such as a truncated SHA-256 over its canonical bytes; the recipe's own tag defines it.
  - The reader that knows the recipe checks it on load and detaches a mismatched recipe, leaving plain geometry.
  - Nothing is regenerated on open, so artwork never changes because the app was updated.
  - This guards against writers that ignore the drop rule, which PNG's history shows happens.
- **Readers that know the recipe** regenerate the result only when the recipe is edited. Transforms and recoloring update the recipe and its fingerprint.
- **Readers that do not know it** render the result and drop the recipe by the rules above. That is Illustrator's Expand, rather than Inkscape's silent overwrite of a foreign edit to `d`.
- **The rule for new features:**
  - If the result can be drawn with kinds that already shipped, store result plus recipe, so older versions keep the file editable. CorelDRAW does the same with its variable-outline fallback and "Keep appearance".
  - Only features whose result cannot be drawn that way, such as Text, are required kinds or tags.

**Instances (later)**
- **Overrides:** `(path of item IDs, tag, value)` from the instance's definition down through nested instances. The outermost instance wins, as in Figma.
- **Inert overrides:** an override whose path no longer reaches a target is inert, not Invalid. Shared definitions will be edited apart from their instances.
- **Identity:** local definitions share the vector object's ID space.

**Number registry**
- **Source of truth:** a checked-in list of every kind and tag number with its flags, value class, grammar and the kinds it applies to.
- **Append-only:** a test keeps it so. Retired numbers stay reserved.
- **Experimental range:** one range is for development builds; portable save refuses it.

**Rules**
- **Order:** items are front to back across `items` and within each chunk, matching `capy.stack/1`.
- **Validation:**
  - **Invalid for anything unknown is decided from framing alone:**
    - lengths and End balance, using parent bits;
    - canonical form;
    - out-of-range slot indices.
  - **Invalid for known data:**
    - non-finite values, `t` outside `[0,1]`, negative widths or factors, unordered knots;
    - duplicate IDs;
    - a count that disagrees with the payload.
  - **Unsupported, preserving the package as the existing rule requires:**
    - unknown required words, reserved bits and unknown slot lists;
    - known tags on other kinds;
    - admission bounds: items per layer, anchors per path, group depth and bytes per property, sized to admit a text story of about 1 MB.
  - **Accepted:** a missing ID in an Items value, which instance overrides may legitimately outlive, and an empty chunk.
  - **Allocation:** decoded size is bounded before allocation.
- **Writers:**
  - Chunk membership stays stable across edits, so unchanged chunks keep their resource IDs and encoded bytes when saved again.
  - A save that did not edit a layer reuses its chunks byte for byte.
- **Chunks of up to 64 records:**
  - One resource per layer would rewrite the whole layer in session history on every edit.
  - One resource per path costs about 200 bytes of resource JSON per path.
  - 64 matches the session's 64-entry tile-index chunks.
- **No record per path:** the limits in 6.2 rule it out, and every edit would walk every path record.
- **Older readers:**
  - A file with vector layers opens there as Preserved: read-only, showing its preview, with Copy Original.
  - That is the package contract's documented trade.
- **Evolution:**
  - `capy.vector-object/1` and `capy.vector-items/1` keep their names and meaning once released.
  - Later features are new kinds, tags, values and slot lists, never a new meaning for an existing one, so neither type needs a `/2`.
  - Each addition gets a fixed-file test.
- **Evaluation meaning:** the package contract's evaluation table gains these definitions, with an F64 reference renderer like `object_sampling_reference.rs`, so that renderer changes keep the authored meaning:
  - coverage as the exact area of the filled region in each pixel;
  - paint entries composited in list order;
  - stroke geometry: offset curves, SVG joins, caps and miter limit;
  - the flattening error bound;
  - two-sided width interpolation.
- **Fixtures and tests:**
  - **Fixture file:** a generator next to `crates/layer-color/examples/object_fixture.rs` and a fixed `vector-objects.capy`.
  - **Geometry coverage:**
    - open, closed and compound paths, both fill rules, every cap and join;
    - a symmetric and a one-sided width profile, a dot, a stroke below a fill;
    - a hidden path, an empty layer and a full chunk.
  - **Unknown data:**
    - An unknown required leaf kind, and a required parent kind with children: Preserved, not Invalid.
    - A reserved bit, or a required word marked copy-safe: Preserved.
    - An ignorable, copy-safe parent holding known paths: renders without its subtree; editing a sibling keeps its bytes.
    - Each value class in an ignorable property through move, recolor, rename, duplicate, deletion of a referenced item and slot compaction.
    - A live shape whose geometry a foreign writer changed without dropping the recipe: opens as plain geometry with that geometry unchanged.
  - **Stability:** a no-edit save keeps chunks byte-identical, and a seeded create, delete, undo and redo run never reissues an ID.
  - **Coverage of the rules:** the registry test, codec tests for round trip and Invalid for malformed framing, `session_tests` coverage and new render-contract rows in `authored-renders.tsv`.
  - **Fuzzing:** a fuzz test injects unknown properties with every bit combination at every nesting level and applies random edit scripts.

### 7.5 Rendering

The [performance research](vector-performance-research.md) sets the targets,
costs and caching rules behind this section.

**Coverage kernel**
- **CPU side (geometry only):**
  - Expands strokes into closed outlines with joins, caps, the miter limit and the width profile, cached per geometry and stroke style.
  - Flattens outlines into line segments within 1/4 pixel of the evaluated level, cached per outline and level.
  - Bins each path's segments into the level's 16-pixel tiles with a backdrop per tile row, in tile-relative fixed point.
  - Builds the ordered list of path spans for each dirty tile, and drops commands beneath an opaque solid fill.
- **GPU side:**
  - One dispatch per vector layer per frame, and one 64-thread workgroup per tile.
  - Each pixel starts from its backdrop and sums the exact signed area of each path's segments in the tile. It uses no atomics or workgroup memory. It then maps winding to coverage with the nonzero or even-odd rule.
  - Composites each path's paint entries source-over in registers, in item order inside the isolated layer, and writes each premultiplied RGBA32F pixel once.
  - A fixed command and segment order makes the result deterministic on a device.
  - Groups with opacity, blend, masks or effects (V2 and later) render into their own window first; groups without them add no pass.
- **Known limit:** area accumulation over-covers edge pixels where a stroke outline overlaps itself, such as at tight inner joins. Skia and vello accept the same limit; fixtures must bound it.
- **It replaces two existing renderers** (replace, don't layer):
  - **Selection contour filler:** keeps its even-odd meaning with better antialiasing.
  - **Figure distance fields:** on a Paint layer, a Figure becomes a path rasterized into the paint source through the same kernel.
  - Selection and figure render baselines are regenerated in the same change.

**Integration**
- **Leaves:** dispatch at `object_tile`, `Compositor::objects` and thumbnails. Masks, effects and clipping above the leaf are unchanged.
- **Evaluation level:**
  - The display leaf evaluates vector content at the level the view requests.
  - The native leaf evaluates level 0 only for exports, captures, color sampling and effects that need native input.
  - Pixel caches are regenerable display-level windows within the display budget.
- **Derived layout and damage:**
  - A `VectorLayout` per vector object holds the item table, the tile bins (256-pixel tile → item IDs) and the geometry caches.
  - It updates from the chunk diff and is shared by rendering, picking, the eraser and snapping.
  - Edits diff chunks by `Arc` identity, then records. Damage is the union of the changed items' old and new bounds: hull plus stroke reach, including miters, plus 1 pixel. It is collected as page sets.
  - Reordering damages only the reordered items' bounds. Vector edits never fall back to compositing everything.
- **Prerequisites:**
  - Key the document-wide refinement and batching opt-outs (6.3) on pending image work.
  - Make the edit, frame and autosave costs listed in the [performance research](vector-performance-research.md#5-capy-today-what-breaks-at-scale) proportional to what changed.
- **Live feedback:**
  - A path under construction, or a freehand line in progress, is a provisional item in the engine, not an authored record.
  - The same kernel renders it in the layer's stack position and damages only the changed tail. Prediction extends the tail as it does for brushes.
  - At pen-up or Finish, the fitted path replaces it in the same submission.
  - The [instant stroke feedback](../reference/instant-stroke-feedback.md) budget applies.
  - Drawing live through the G-Pen contact renderer instead was rejected: a vector layer has no paint target, and the edge would change at pen-up.
- **Gestures:**
  - Drags, anchor drags, erasing and scrubs keep their state in tool modifiers and commit one edit at release.
  - At gesture start, the layer splits by paint order into three ranges:
    - Below and above the affected items: cached.
    - The affected range: renders once into a moving cache that the transient affine places each frame. If unaffected items interleave with it, it re-renders each frame instead.
  - At release, the moved items re-derive, visible windows first, while the moving cache stays on screen.
- **Density:**
  - Until crisp zoom, no level is finer than native.
  - The editing overlay (outlines of selected paths, anchors and handles) is drawn at screen resolution, so editing precision does not depend on density.
  - Crisp zoom (V3) adds levels finer than native for object content in both evaluators. Paint and spatial effects keep native boundaries ([GA design §9.1](object-layer-ga-design.md#91-authored-source-versus-evaluated-result)), and image objects benefit too.
  - Until then, Capy claims "resize without quality loss", not "sharp at any zoom".
- **Overlay markers:**
  - `CursorSegment` gains a round marker for handle ends, and hollow and filled squares for unselected and selected anchors. This is a shared renderer change.
  - Outlines of large selections render once into an overlay cache.
- **Picking (CPU, F64):**
  - Tile bins give candidates, tested front to back.
  - A stroke hits within half its width plus a screen-pixel tolerance, using the view scale.
  - A fill hits where its winding is inside under the path's rule.
- **Memory:** all work is windowed and bounded, with no full-layer intermediates and no vector pixels kept at native density.

### 7.6 Tools and interaction

**Creating a vector layer**
- **Name:** keep "Vector layer". Clip Studio, Fresco, Krita, Affinity and Illustrator all use it, so it matches the tutorials people search for.
- **Creation:**
  - New Vector Layer joins the shared New submenu and command search, with no new footer button on the five hosts.
  - The row shows a vector badge on its thumbnail.
  - Rasterize Layer turns it into paint, as for image objects.
- **Figure tools follow the layer kind:**
  - On a Paint layer they paint pixels as today, through the path kernel. This includes the new Path variant, which rasterizes its stroke and fill at Finish, as Clip Studio's curve figure does.
  - On a Vector layer they keep editable paths.
  - On an image Object layer, the refusal notice also offers New Vector Layer.

**Tools on a vector layer**

| Tool (key) | Behavior | Notes |
|---|---|---|
| **Move** (O; V in the Photoshop and Affinity presets) | Click selects the frontmost path under the pointer in any visible, unlocked vector layer and makes that layer current. Shift toggles a path. Dragging on empty canvas marquee-selects paths in the current layer. Dragging moves; the existing Scale–Rotate frame scales and rotates. Alt duplicates, arrow keys nudge and Delete deletes. Double-click or Enter opens Edit Points; Escape leaves it | Extends object picking with path hit-testing and selecting individual paths. Marquee is new: image objects have none. Tool options gain Scale Strokes, on by default |
| **Edit Points** (A; a new variant in the Move slot) | Click or marquee anchors. Drag anchors, handles or a segment to bend it. Alt-drag a handle to break symmetry. Double-click an anchor to switch corner and smooth. Click a segment to add an anchor. Delete removes anchors and heals the curve. Shift constrains to 45° | On touch, on-screen modifier chips replace Alt and Shift. Anchors join `object_touch_hit` |
| **Figure** (U): Line, Rectangle, Ellipse, Polygon, Star, **Path** | Creates an editable path using the tool's paint mode (Outline, Fill or Both) and line width. The foreground color strokes and the background fills, as Figure's Both mode does today. Rectangle takes a corner radius and Polygon and Star a point count at creation. Rectangle, Ellipse, Polygon and Star keep these as a live-shape recipe (7.3); Edit Points turns the shape into a plain path | Path: click for a corner, drag for a smooth anchor. Alt breaks the handle, Shift constrains, clicking the first anchor closes and clicking an open end of a selected path continues it. Enter or Finish ends, Backspace removes the last anchor and Escape cancels. The polygon canvas bar (Remove Last Point · Cancel · Finish) is reused |
| **Ink pens** (P) | G-Pen and Monoline draw vector lines (7.7). Other presets refuse with the notice | |
| **Eraser** (E) | Touched Part (default) splits paths where the eraser covers the centerline. Whole Path deletes every path it touches. A cut closed path becomes open; its fill still closes implicitly, as in SVG and Illustrator's Path Eraser | Subtracting from fills comes with merge (V2); trimming to intersections with the ink tools (V4) |
| **Fill** (F) | Clicking inside a path's fill, or inside a closed path, sets its fill to the current color. Clicking elsewhere fills a new Paint layer below, using this layer as the reference with Close Gaps (the existing region fill) | Gives Clip Studio's flats workflow, with no error |
| **Lasso Fill** | Creates a filled closed path from the lasso | |
| Brushes, retouching, gradients, Smudge, Liquify, selection painting | Refuse with the object notice, reworded for vector layers, offering New Paint Layer and Rasterize Layer | |

**Commands**
- **Arrange within the layer:** Bring to Front, Bring Forward, Send Backward, Send to Back. Keys are chosen together with the keymap presets.
- **Path commands:** Make Compound Path, Release Compound Path, Join, Close Path, Split at Point and Reverse Path Direction.
- **Copy, Cut, Paste and Duplicate:**
  - They act on selected paths within and across vector layers.
  - Pasting paths onto a layer that is not a vector layer creates a vector layer above it.
  - External copies publish PNG as today, and SVG once export exists.
- **Properties for selected paths:**
  - Fill (color) and Stroke (color, width, cap, join, miter limit), plus the path's fill rule, using the existing Color, Number, Choice and Toggle kinds. A toggle per section removes the fill or the stroke.
  - A path whose style holds more than one fill or stroke, such as an import, shows its first fill and topmost stroke. Editing them keeps the other entries.
  - Mixed values show as mixed, and each gesture is one undo step.
- **Snapping:** Move snaps to rulers and the canvas as it does today. Snapping to anchors and paths comes in V2.
- **Groups (V2):** Group and Ungroup. Move selects a group as a unit; double-click enters it, as Illustrator's isolation mode does, and Edit Points reaches inside. The Layers panel keeps one row per layer, so nested content is browsed by entering groups.

### 7.7 Freehand lines, smoothing and brushes

**Pipeline**
1. **Input is unchanged:** `PenEvent` to `StrokePoint`, the global pressure curve and rulers.
2. **Smoothing is unchanged:**
   - The preset's stabilizer strength and adjust-by-speed, the pressure fall limiter, prediction and late correction.
   - No new smoother and no separate "fidelity" setting: the stabilizer slider is that control.
3. **Centerline:**
   - The engine records the vertices `append_swept` keeps. Each has its evaluated width (pressure mapping, taper and size bounds) and its arc length.
   - This is the same smoothed centerline the G-Pen draws today.
4. **Fitting at pen-up, and incrementally on the finalized prefix:**
   - Detect corners by angle and curvature, then fit cubic Béziers to each corner-free run with kurbo, within 0.25 screen pixels at the zoom captured at pen-down.
   - Fit width knots by simplifying in arc length and width, within 0.25 pixels. Drop the profile when width stays constant within that tolerance.
   - Anchors inside runs are Smooth; anchors at corners are Corner.
5. **Result:**
   - An open path whose stroke uses the foreground color, a width equal to the brush size, round caps and joins, and the fitted profile.
   - Taper becomes end knots. Brush opacity becomes stroke color alpha.
   - A late correction refits and amends the same edit.
6. **Exact input:** Path and shape construction use exact positions with Shift constraints, without smoothing.

**Which brushes draw vector lines**
- **Rule:** a preset draws vector lines when its mark is a solid round nib.
  - That means contact execution, a round tip, source-over deposit and no grain, roughness, jitter, scatter, color dynamics, wet mixing or smudge.
  - The predicate lives in shared Rust, so it also admits custom brushes, and it widens as brush paths support more.
- **First release:** G-Pen (pressure sets width) and a new Monoline preset (fixed width, round caps).
- **Later, with brush paths:**
  - Calligraphy Pen and Marker, whose width depends on direction and needs a nib model.
  - Textured pens, pencils and dry media, and Airbrush and Spray.
  - Until then they refuse on vector layers with the notice offering New Paint Layer.
- **Never:** Smudge, blenders, wet paints, watercolors, Liquify, Clone and the healing brushes. They read canvas pixels and depend on stroke order.
- **The pen's eraser end** uses the vector eraser.
- **Hold-to-snap shapes** are absent today. Adding them as a shared feature for both layer kinds would give vector layers editable shapes for free.

### 7.8 Export and import

- **Raster export** is unchanged: canonical document-grid composition.
- **SVG export (V2):**
  - Constant-width strokes export as SVG strokes, with exact caps, joins and miter limit.
  - Variable-width strokes export as filled outlines, and fills keep `fill-rule`.
  - Paint layers export as embedded PNG.
  - Layer opacity and blend export where SVG 1.1 expresses them and are flagged otherwise. Masks and effects use an explicit raster fallback with a notice.
  - Physical size comes from the composition resolution, otherwise 96 pixels per inch, with a `viewBox`.
  - Later additions:
    - A private `capy:` namespace for round trips, whose geometry fingerprint restores Capy data only when the visible geometry is unchanged.
    - Cut and plot presets with the maker semantics of section 5.2.
- **SVG import and paste (V2):**
  - usvg produces paths and styles.
  - Layer groups (Inkscape `groupmode="layer"`, Illustrator layers) become Capy group layers and vector layers; other groups become Groups.
  - A path whose stroke sits under a skew or uneven scale stays inside a group that holds the transform (7.2).
  - Content without a supported meaning (filters, text, or gradients before they exist) is rasterized or refused with a notice.
- **PDF export and import** come later (krilla, hayro).

### 7.9 Foundation for later work

| Later feature | Built on | New pieces |
|---|---|---|
| **Brush paths** (textured strokes on vector layers) | The path and its width profile drive `DabGenerator` through `PositionField`; width knots supply size | A new item kind with a seed and a brush slot naming a record that holds the `BrushSnapshot` and its tip and grain resources, and an arc-length texture phase so that cuts do not shift texture. Optional pressure channel for brushes whose other dynamics use pressure. Rendered by the stamp engine into the layer's windows |
| Trim to intersection, Width tool, Simplify, grab to reshape | Intersections of flattened centerlines via tile bins; width-profile edits | Ink cleanup tools (V4) |
| Merge (union, subtract, intersect, exclude), Shape Builder, Convert Stroke to Shape, Offset | i_overlay on flattened outlines, refit with kurbo; results are ordinary paths | V2 |
| Groups, images inside vector layers, text, instances | New item kinds in the same stream (7.4) | Group selection rules and isolation; text shaping and fonts; symbol definitions |
| Live shapes, live corners, booleans, blends, Repeat | Results plus recipes (7.4) | Recipe editors on canvas |
| Dashes, gradients, patterns, per-item opacity and blend | New stroke tags, paint kinds and item properties | Gradient handles on canvas |
| Vector region fill (Live Paint) | Faces from centerlines plus gap closing, stored as paths under the ink | OpenToonz-style region code |
| Crisp zoom | The density-independent `ObjectWindow` | Above-native plan levels (V3) |
| Image trace, PDF import, DXF export | Paths | Importers and exporters |

The [extension research](object-content-extensions-research.md) maps every SVG,
Inkscape and Illustrator construct to the kind, property or record that would
hold it.

### 7.10 Journeys to qualify

- **Badge (illustrator):**
  1. New Vector Layer, then a circle, a rounded rectangle and a star from Figure.
  2. Recolor them from Properties.
  3. Bend a segment and add an anchor in Edit Points.
  4. Make a compound path with a hole.
  5. Undo and redo throughout, then save, reopen and Rasterize Layer.
- **Logo trace:**
  1. Place a sketch as an image layer.
  2. Trace it with Path on a vector layer, closing and continuing paths.
  3. Join two ends and reverse a path.
- **Inking:**
  1. Sketch on paint.
  2. Ink with G-Pen on a vector layer at moderate stabilization.
  3. Cut overshoots with the eraser, select lines and change their color and width.
  4. Fill flats onto a new Paint layer below.
  5. Zoom in and edit anchors on a fitted line.
- **Refusals:** a textured brush, Smudge and a gradient on a vector layer each show the notice and its actions, never an invisible stroke.

Run each journey on GTK, Web, Android, Apple and Windows in light and dark
themes, after the GTK review, using the image-object journeys of
[Object layers](../ui/image-objects.md#ownership-and-verification) as templates.

---

## 8. Phasing

| Milestone | Scope | Exit |
|---|---|---|
| **V0. Foundation** (no UI) | First, the [kernel microbenchmark](vector-performance-research.md#82-kernel-microbenchmark-first-step-of-v0) on the tier devices. kurbo after `cargo deny` and a wasm size check. `VectorObject`, the object handle enum, records, resources, the record framing, value classes, unknown-data rules and number registry, validation and history. The coverage kernel replacing the selection filler and Figure distance fields. The derived layout, damage per item, hit-testing, the F64 reference renderer and render contracts. Refinement opt-outs keyed on images, and edit, frame and autosave costs proportional to what changed | Fixtures round-trip; both evaluators match the reference; selection and figure regressions pass on regenerated baselines; selection and figure motion rows show no regression against a matched control; [headless work counts](vector-performance-research.md#81-headless-scaling-tests) stay flat between 10,000 and 50,000 paths |
| **V1. Lines and fills** | New Vector Layer and its badge; Move path selection, marquee, transforms and arrange; Edit Points; Figure shapes with live-shape recipes and Path on both layer kinds; Properties style; G-Pen and Monoline vector lines; the vector eraser; Fill and Lasso Fill on vector layers; compound, join, close, split and reverse; copy and paste; refusal text; all hosts and catalogs | The 7.10 journeys pass on every host in both themes. The [device rows](vector-performance-research.md#83-device-rows-v1-exit) pass on all three tiers |
| **V2. Organize and exchange** | Groups with Group and Ungroup, merge, Convert Stroke to Shape, Offset, dashes, linear and radial gradients, align and distribute, anchor snapping, an eraser that subtracts from fills, SVG export, import and paste | SVG round trip through Inkscape and a browser; merge fuzzing under time and size limits |
| **V3. Crisp zoom** (can start after V0) | Above-native density for object content in both evaluators | Pan and zoom rows hold their tier rates with vector and image layers |
| **V4. Ink cleanup and brush paths** | Trim to intersection, Width tool, Simplify, reshape, brush paths | Brush-path replay equals live drawing, extending the existing determinism tests |

Qualification extends the [GA workloads](object-layer-ga-design.md#143-performance-qualification):
many small strokes in one layer, one path with 32,000 anchors, active anchor and
path drags, high zoom, newly exposed regions and mixed paint and vector stacks
([targets](vector-performance-research.md#2-scale-targets)).
Counts are workload inputs, not limits.

---

## 9. Open questions

1. **Figures on Paint layers.** Should they keep painting pixels (consistent with Clip Studio), or offer once to start a Vector layer?
2. **Select All.** With Move or Edit Points on a vector layer, should Select All select paths, as in Illustrator? Today it always acts on the pixel selection.
3. **Path key.** Path sits in the Figure slot (U), without a single key in the Capy keymap. Should the Photoshop and Affinity presets bind it to P?
4. **Width knots.** Anchor-relative knots are chosen. Prototype node editing on fitted lines to confirm them against arc-length knots.
5. **Miter limit.** Should new strokes default to 4 (SVG) or 10 (Illustrator)? Either is a tool default, not a wire default.
6. **Crisp zoom.** Should V3 land before V1 ships, given how much illustrators zoom while editing anchors?
7. **Hairlines in Linear documents.** Accept slightly lighter antialiased edges than browsers show, or add a per-layer perceptual option later?
8. **Maker scope.** How far should cut-file presets and the cut-ready check go once SVG export exists?
9. **Several fills and strokes.** The format holds any paint list. When should Properties show more than one fill and one stroke, as Illustrator's Appearance panel does?
10. **Browsing nested content.** With groups, is entering a group enough, or does a vector layer need an object list beside the Layers panel?
11. **Item-local read-only.** Preserved is whole-document, so one balloon using a newer required text feature makes a whole page read-only in an older version. Should items that carry a stored result, such as Text, lock and render that result instead? It would need a reader outcome between Editable and Preserved.
12. **Duplicates of live shapes in older versions.** Older readers drop recipes they do not know on duplicate and paste. Keeping them on an untransformed duplicate, with nested IDs renewed, is possible from framing alone. Is that worth the extra generic code?
