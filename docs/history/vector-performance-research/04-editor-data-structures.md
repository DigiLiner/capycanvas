# Vector-layer performance lessons: editor data structures, tool algorithms and UI responsiveness

[Vector layer performance](../vector-performance-research.md) · source report, 2026-10-16

Written against the rendering plan in [vector layers §7.5](../vector-layers-research.md#75-rendering) at `1f8fdd621`. The design this research produced is in the [performance record](../vector-performance-research.md).

Notation: **Source** means a cited page states it. **Inference** means it is my reasoning and has not been measured on Capy hardware.

## (a) Scale targets, with evidence

**What real tools hit or cap**

- **tldraw**
  - Default `maxShapesPerPage` is 4,000 ([options.ts](https://github.com/tldraw/tldraw/blob/main/packages/editor/src/lib/options.ts)).
  - Debounced zoom switches on above 500 shapes ([docs](https://tldraw.dev/sdk-features/performance)).
  - A freehand shape is split after 600 points "to prevent performance issues with very long strokes" ([draw-shape docs](https://github.com/tldraw/tldraw/blob/main/apps/docs/content/sdk-features/draw-shape.mdx)).
  - Their benchmarks use 10k-shape pages ([PR 9093](https://github.com/tldraw/tldraw/pull/9093)).
- **Excalidraw** tests handwriting scenes of 2,000, 5,000 and 10,000 freedraw strokes. Before a fix, a zoom change at 10k strokes took 1,850 ms and a pan frame 117 ms ([PR 12211](https://github.com/excalidraw/excalidraw/pull/12211)). Dragging 4,000 selected rectangles took 643 ms per frame because of O(n²) work ([PR 12183](https://github.com/excalidraw/excalidraw/pull/12183)).
- **Figma**
  - Says files commonly have "tens of thousands of layers" ([layers panel post](https://www.figma.com/blog/improving-performance-in-the-layers-panel/)).
  - Gives WASM memory a limit of about 2 GB per tab and locks the file at 100% ([help](https://help.figma.com/hc/en-us/articles/360040528173-Reduce-memory-usage-in-files)).
  - Lists "complex vectors with many points" among the heavy items (same page).
- **Illustrator**
  - Forum reports put the per-path limit at about 32,000 anchors ([Adobe forum](https://community.adobe.com/t5/illustrator-discussions/maximum-anchor-points-on-a-path/m-p/10163269/highlight/true)).
  - A Fresco vector drawing with more than 9,000 anchors made Illustrator hang. Fresco vector brushes produce outlined shapes, not center-line strokes ([thread](https://community.adobe.com/questions-646/problems-with-fresco-vector-drawings-and-simplify-tool-in-adobe-illustrator-306584)).
  - One user reported 1–2 s move lag on a file with 3,000 paths and 13,000 points. I saw this only in a search summary ([thread](https://community.adobe.com/t5/illustrator-discussions/who-can-tell-me-why-pdf-files-exported-from-cad-revit-are-slow/m-p/14846618/highlight/true)).
- **Clip Studio Paint.** Users report vector-layer move and transform freezing for seconds. Community answers blame control-point density from hand-drawn lines, and the "erase up to intersection" mode, which multiplies line records. The suggested fix is the vector-line simplification tool ([1](https://ask.clip-studio.com/en-us/detail?id=30932), [2](https://ask.clip-studio.com/en-us/detail?id=85312), [3](https://ask.clip-studio.com/en-us/detail?id=92930)). I found no counts.
- **Illustrator Live Paint (planar map)**
  - Responds in 0.4 s for 210 paths and 2,596 regions, and 1.1 s for 1,154 paths, on a 2007 G5.
  - The authors say response time is "proportional or worse to the total number of paths" ([Asente et al. 2007](https://www.lri.fr/~mbl/FundHCI/papers/Asente-SIGGRAPH07.pdf)).
- **Inkscape** has a long history of being slow with paths of thousands of nodes, in both zooming and node editing ([2004](https://lists.inkscape.org/hyperkitty/list/inkscape-user@lists.inkscape.org/message/THMIGMZ4HRNRC7OPYE74LQUBYUY5PQKV/), [2009](https://lists.inkscape.org/hyperkitty/list/inkscape-user@lists.inkscape.org/thread/DLHZFGC6FO242WAYBO3FUT6C2CCVUGID/)).

**Proposed target (inference)**

Files move between devices, so set one content target that every tier holds at its own frame rate. Don't scale the content target per tier. The faster top-tier CPU roughly offsets its smaller 2 ms budget. Note that the tier canvases differ: 12, 24 and 61 MP.

| Per vector layer | Must hold the tier rate | Must stay correct and responsive (async ops allowed) |
| --- | --- | --- |
| Paths | 10,000 | 50,000 |
| Anchors, total | 250,000 | 1,000,000 |
| Anchors in one path being point-edited | 5,000 | 32,000 (Illustrator's practical limit) |
| Selection moved, scaled or rotated | all items (transient matrix) | all items |
| Boolean, simplify, fill, import | not per frame; async, cancellable | not per frame; async, cancellable |

Basis for these numbers:
- 10k paths matches the top of what tldraw and Excalidraw benchmark.
- An average of about 25 anchors per fitted freehand stroke is a guess. Hatching lines need about 2–6 anchors and contours 20–100.
- At about 32 B per authored anchor, 250k anchors is about 8 MB. Flattening and outline caches might add 3–5× (inference).
- Hatching counts for a comic page are **unverified**. Validate against real inked pages from the team's artists before freezing these numbers.

## (b) Data structures and their costs

### Spatial index: grid vs R-tree vs BVH

What the projects use:
- **tldraw** uses an R-tree (rbush) for culling, brush and scribble selection, erasing and point queries ([PR 7676](https://github.com/tldraw/tldraw/pull/7676), [4.4 notes](https://tldraw.dev/blog/tldraw-sdk-4.4)).
  - An experiment replaced it with a 500×500 grid updated eagerly from side effects (+99/−356 lines). It was closed unmerged, with no stated reason ([PR 8245](https://github.com/tldraw/tldraw/pull/8245)).
  - The big win came from incremental maintenance, not from the tree. The index had re-walked every shape on every diff to catch derived bounds changes ([issue 8882](https://github.com/tldraw/tldraw/issues/8882)).
  - A structural dirty set (descendants of moved shapes, ancestors, binding partners) cut a single-shape drag on a 10k page from 23.1 ms to 1.0 ms. Dragging a group with 1k children only fell from 27.5 to 14.1 ms, because those bounds really change ([PR 9093](https://github.com/tldraw/tldraw/pull/9093)).
  - Skipping index mutation when bounds are unchanged removed per-pointer-move rebalancing while drawing ([PR 8799](https://github.com/tldraw/tldraw/pull/8799)).
- **Penpot** render-wasm uses a grid hash map: tile → set of shape IDs, plus a reverse index ([tiles.rs](https://github.com/penpot/penpot/blob/develop/render-wasm/src/tiles.rs)).
  - Tiles are 512 px in screen space, so a zoom change re-tiles every top-level shape, and only top-level shapes are indexed ([render.rs](https://github.com/penpot/penpot/blob/develop/render-wasm/src/render.rs)).
- **Excalidraw** has open proposals for a uniform-grid prefilter, because it scans the whole scene on every pan and zoom ([issue 10512](https://github.com/excalidraw/excalidraw/issues/10512), [PR 11463](https://github.com/excalidraw/excalidraw/pull/11463)).
- **Krita** uses an R-tree (`KoRTree`) with a lazy dirty set. Changed shapes collect in `aggregate4update` and are removed and reinserted in `updateTree()` just before any query ([KoShapeManager.cpp](https://invent.kde.org/graphics/krita/-/blob/master/libs/flake/KoShapeManager.cpp)).

Library costs:
- **rstar**: bulk load of 2,000 points in 230 µs vs 1.45 ms inserting one at a time, on an i7-8550U. A point lookup in a 100k tree takes about 180 ns ([README](https://github.com/georust/rstar/blob/master/rstar/README.md)). Default nodes hold 3–6 children, each node a heap `Vec` ([params.rs](https://github.com/georust/rstar/blob/master/rstar/src/params.rs)).
- **rbush**: bulk insert is 2–3× faster than one-by-one, and queries are 20–30% faster afterwards ([rbush](https://github.com/mourner/rbush)).
- **Flatbush**: indexes 1M rectangles in 109 ms with lower memory, but is static ([flatbush](https://github.com/mourner/flatbush)).
- **Box2D**: dynamic trees avoid reinsertion on small moves by fattening AABBs ([b2DynamicTree.h](https://box2d.org/doc_version_2_4/b2__dynamic__tree_8h_source.html)).

**Recommendation (inference): keep the planned 256-px document-space tile bins as the single index. Don't add an R-tree or BVH.**
- Capy's canvas is bounded, unlike the infinite canvases above. Document-space bins never re-tile on zoom, unlike Penpot's.
- The 12 MP canvas has 17×12 = 204 tiles; 61 MP has about 950.
- Ten thousand strokes covering about 3 tiles each is roughly 30k four-byte entries, about 120 KB.
- Updating one path costs O(tiles it touches).
- Bin by flattened-segment coverage, inflated by half the maximum stroke width plus anti-aliasing. Binning by bounding box would put a long diagonal stroke in every tile.
- Keep a clamp bin for geometry outside the canvas.
- For marquee selection, a flat struct-of-arrays bounds scan is fine at these sizes. Fifty thousand box tests is well under 0.1 ms in Rust (inference). tldraw's O(n) sweep hurt because of per-shape cache lookups and allocations in JS.

### Bounds and groups

- Use bottom-up cached bounds, and recompute only the ancestors of changed items. That is tldraw's structural dirty set ([PR 9093](https://github.com/tldraw/tldraw/pull/9093)).
- Penpot keeps both the pre-transform and post-transform extents of moving shapes and invalidates only their union ([render.rs](https://github.com/penpot/penpot/blob/develop/render-wasm/src/render.rs)).

### Chunked persistent storage

- Sixty-four entries per node matches proven structures:
  - im-rs `Vector` is an RRB tree with branching factor 64 ([docs](https://docs.rs/im/latest/im/vector/index.html)).
  - Zed's `SumTree` is an Arc-shared, copy-on-write B+ tree whose nodes carry summaries that add together. Snapshots are cheap and go to background threads, and Zed uses it in over 20 places ([Zed blog](https://zed.dev/blog/zed-decoded-rope-sumtree)).
- **Inference:** give each 64-record chunk a cached summary: bounds union, record count, and style-ID set. Compute it once per chunk `Arc`; it survives undo for free. "Select all", mixed-value properties and "fit selection" then cost O(chunks) instead of O(items).

## (c) Per-tool risks and techniques

### Select, move and transform

- **Transient transforms.**
  - Penpot puts in-progress transforms in a separate "modifiers" structure that the viewport merges in. Only shapes with modifiers redraw, and `apply-modifiers` folds them into the geometry on release ([modifiers.cljs](https://github.com/penpot/penpot/blob/develop/frontend/src/app/main/data/workspace/modifiers.cljs)).
  - Fabric reuses each object's cached bitmap during drag, rotate and scale ([Fabric caching](https://fabricjs.com/docs/fabric-object-caching/)).
  - Konva moves dragged shapes to their own layer and turns hit detection off during drags by default ([Konva](https://konvajs.org/docs/performance/All_Performance_Tips.html)).
- **Store a transform separately from local geometry.** Figma vertex positions are "relative to the node", and the node carries `relativeTransform` ([VectorNetwork](https://developers.figma.com/docs/plugins/api/VectorNetwork/), [relativeTransform](https://developers.figma.com/docs/plugins/api/properties/nodes-relativetransform/)).
  - Excalidraw's shape cache is keyed by element identity. It regenerated freedraw geometry on every remote move until they reused cached shapes when only x and y differ ([PR 11884](https://github.com/excalidraw/excalidraw/pull/11884)).
  - **Inference:** if a Capy move rewrites anchors, every moved path gets a new `Arc`, and every flatten and outline cache misses on release.
- **Commit in batches.**
  - tldraw: resizing 2,000 shapes went from about 65 to 14 ms per tick once per-shape store commits became one batch per nesting depth ([PR 9153](https://github.com/tldraw/tldraw/pull/9153)).
  - Excalidraw: rebuilding an ID set for every dragged element made drags O(n²) ([PR 12183](https://github.com/excalidraw/excalidraw/pull/12183)).
- **Skip hover hit-testing while the camera moves** ([tldraw PR 7826](https://github.com/tldraw/tldraw/pull/7826)).
- **Draw many selection indicators on canvas.** tldraw moved indicators from SVG to canvas, "up to 25x faster" in some cases ([4.4 notes](https://tldraw.dev/blog/tldraw-sdk-4.4)).

### Edit points

- Inkscape's node tool drew every node icon on entry and redrew whole selections; the proposed fix was redrawing only nodes whose state changed ([2009](https://lists.inkscape.org/hyperkitty/list/inkscape-user@lists.inkscape.org/thread/DLHZFGC6FO242WAYBO3FUT6C2CCVUGID/)).
- **Inference:** draw only anchors in view, as one instanced batch.
- **Inference:** re-flatten only the segments next to a dragged anchor. For paths beyond about 5k anchors, keep per-path segment bins for hit testing.

### Freehand vector lines (G-Pen, Monoline)

- **Don't recompute the whole stroke per point.**
  - Canva found that recomputing the entire stroke on each new point dropped frames on low-end mobile, so it splits strokes into segments ([Canva](https://www.canva.dev/blog/engineering/behind-the-draw/)).
  - perfect-freehand's `getStroke` runs its full pipeline on every call ([README](https://github.com/steveruizok/perfect-freehand)).
  - tldraw's rewrite into reusable flat arrays made the ink pipeline about 2.7× faster on strokes over 500 points ([PR 9154](https://github.com/tldraw/tldraw/pull/9154)).
- **Fit the tail incrementally.**
  - Inkscape's pencil tool fits only a red tail cubic while drawing. When one cubic no longer fits, it freezes the tail as green, continues tangent-matched, and refits the whole stroke at release ([pencil-tool.cpp](https://gitlab.com/inkscape/inkscape/-/blob/master/src/ui/tools/pencil-tool.cpp)).
  - Frisken warns that waiting for the full stroke produces a visible shape change when the final curve replaces the preview. I saw this only in a summary ([SIGGRAPH 2007](https://history.siggraph.org/?p=141606)).
- **Fitter caveats.**
  - kurbo's simplifier does not report corners, and computes tangents incorrectly for non-regular cubics ([simplify.rs](https://github.com/linebender/kurbo/blob/main/kurbo/src/simplify.rs)).
  - `fit_to_bezpath_opt` is "considerably slower" than `fit_to_bezpath` ([fit.rs](https://github.com/linebender/kurbo/blob/main/kurbo/src/fit.rs)); the optimal search is about 50× slower than adaptive subdivision ([Levien 2023](https://raphlinus.github.io/curves/2023/04/18/bezpath-simplify.html)).
  - Levien recommends low-pass filtering noisy input first (same post).
  - Schneider's iterative fit can get stuck in local minima ([Levien 2021](https://raphlinus.github.io/curves/2021/03/11/bezier-fitting.html)). Paper.js's Schneider fitter drops adjacent duplicate points first ([PathFitter.js](https://github.com/paperjs/paper.js/blob/develop/src/path/PathFitter.js)).
- **Inference:** reuse the raster stabilizer's smoothing in shared Rust. Detect corners, then fit the tail with `fit_to_bezpath`. Fit the width profile as a separate 1-D simplification.
- **Inference:** keep the in-progress stroke in tool state, not the document. Insert it once at release.

### Variable-width stroke outlines

- kurbo's stroker handles constant width only, and builds outlines from parallel curves plus joins rather than a rigorous sweep ([stroke.rs](https://github.com/linebender/kurbo/blob/main/kurbo/src/stroke.rs)).
- Levien names variable width as possible future work. Offset cusps appear where curvature equals 1/offset. His JS prototype took about 12 µs per generated segment ([parallel curves](https://raphlinus.github.io/curves/2022/09/09/parallel-beziers.html)).
- `ParamCurveFit` was designed for offset curves with unknown cusps ([fit.rs](https://github.com/linebender/kurbo/blob/main/kurbo/src/fit.rs)).
- From Levien and Uguray ([arXiv](https://arxiv.org/html/2405.00127v2)):
  - Skia's stroker was the fastest they measured but can exceed tolerance.
  - Wang's formula undershoots near cusps.
  - Evolutes must be emitted twice to keep winding consistent.
- **Inference:** render outlines with nonzero fill, which tolerates self-overlap. Run boolean cleanup only for "convert stroke to shape", never per frame. Cache outlines per (geometry, style). While drawing, re-expand only the tail.

### Booleans, offset and simplify

- **Speed.** iOverlay's own benchmark on a 6-core i5 ([benchmarks](https://ishape-rust.github.io/iShape-js/overlay/performance/performance.html)):
  - XOR of a 1,985-square checkerboard: about 5 ms for both iOverlay and Clipper2.
  - Nested squares at 4,096: Clipper2 2.35 s vs iOverlay 60 ms.
  - A 256-line net: about 19 ms.
  - So superlinear worst cases are real.
- **Robustness.**
  - Both libraries snap to integers: iOverlay defaults to i32 ([iOverlay](https://github.com/iShape-Rust/iOverlay)) and Clipper2 uses Int64 ([Clipper2](https://www.angusj.com/clipper2/Docs/Overview.htm)).
  - Both are polygon-only, so curves need flattening and refitting. iOverlay's variable-width stroke is integer-mode only.
  - Skia PathOps "gives up" on huge paths with unresolvable coincidence ([CL](https://codereview.chromium.org/1854333002/), [commit](https://skia.googlesource.com/skia/+/c06d9a7a7e7fdd7002e6f7e41e78d90cadfb6094%5E%21/)).
  - linesweeper handles Bézier booleans natively but is "early beta" and its GitHub repo is archived ([repo](https://github.com/jneem/linesweeper)).
  - For intersections: lyon_geom has cubic–cubic intersection ([docs](https://docs.rs/lyon_geom/latest/lyon_geom/struct.CubicBezierSegment.html)) and flo_curves has curve clipping ([docs](https://docs.rs/flo_curves/latest/flo_curves/)). kurbo offers only line intersection, nearest point and winding.
- **Threading.** Run these on a worker with a cancel token and a document-generation check. On the web, threads with shared memory need cross-origin isolation ([MDN](https://developer.mozilla.org/en-US/docs/Web/JavaScript/Reference/Global_Objects/SharedArrayBuffer)).

### Vector eraser

- Clip Studio users describe "erase up to intersection" as multiplying line data: two crossing lines become four records ([CSP](https://ask.clip-studio.com/en-us/detail?id=30932)).
- Excalidraw treats tiny freedraw dots as circles, so the eraser can skip path intersection for them ([PR 9946](https://github.com/excalidraw/excalidraw/pull/9946)).
- **Inference:**
  - Get candidates from the bins under the swept eraser capsule.
  - Test distance to the flattened center line against radius plus half-width.
  - Hide hits transiently and commit once at release.
  - Compute crossings only for touched strokes, never globally.

### Fill and fill region

- Live Paint rebuilds a whole snap-rounded Bentley–Ottmann planar map after every edit, at O(p log p + n) ([Asente](https://www.lri.fr/~mbl/FundHCI/papers/Asente-SIGGRAPH07.pdf)).
- Inkscape's paint bucket flood-fills a raster of the visible canvas and traces it with potrace, so the result depends on zoom. I saw this only in search summaries ([FLOSS manual](https://archive.flossmanuals.net/inkscape/ch018_bucket-fill-tool.html)).
- **Inference:** rasterize candidate outlines into a CPU mask at canvas resolution within a window around the click. Close gaps by dilation, flood-fill, trace, then fit. Grow the window while the fill touches its border. Cost is bounded by the window, not by the number of paths.

### Snapping

- tldraw takes snappable shapes from the viewport in a cached computed value ([SnapManager.ts](https://github.com/tldraw/tldraw/blob/main/packages/editor/src/lib/editor/managers/SnapManager/SnapManager.ts)).
- Penpot keeps per-axis range trees in a web worker ([snap.cljs](https://github.com/penpot/penpot/blob/develop/frontend/src/app/worker/snap.cljs)).
- **Inference:** build sorted per-axis arrays of visible snap points at gesture start, which is O(log n) per move. Build them during idle time if the build exceeds the frame budget.

### Hit testing strokes

- tldraw uses a 3 px margin, coarse 4 ([options.ts](https://github.com/tldraw/tldraw/blob/main/packages/editor/src/lib/options.ts)).
- Its geometry hot loops gained 1.7–19× by removing allocations and square roots ([PR 8210](https://github.com/tldraw/tldraw/pull/8210), [PR 8600](https://github.com/tldraw/tldraw/pull/8600)).
- Hit test only the candidates, in reverse z-order.

## (d) UI responsiveness

- **Layer or item panels.**
  - Figma's old panel gathered full data for every expanded row in one pass. The new one is two-pass: ordered row IDs first, then row data for only the windowed few dozen rows.
  - It uses lazy derived properties that recompute only on expansion or child changes, and a rope-like list that cut memory by up to 99%. Interactions got 30–50% faster in large files ([Figma](https://www.figma.com/blog/improving-performance-in-the-layers-panel/)).
  - Inkscape's rewrite builds subtrees only when they are expanded, and ignores some attribute changes ([MR 2466](https://gitlab.com/inkscape/inkscape/-/merge_requests/2466)).
  - Before the rewrite, an open Objects or History dialog slowed undo and grouping until restart ([issue 1536](https://gitlab.com/inkscape/inbox/-/issues/1536)).
  - GTK list views recycle a small set of row widgets, scale to millions of items, and batch changes with `items-changed` ([GTK](https://docs.gtk.org/gtk4/section-list-widget.html)).
- **Notifications.**
  - tldraw: passing many shapes to one call yields one observer notification ([docs](https://tldraw.dev/sdk-features/performance)).
  - Excalidraw records history only for committed actions, not drag frames ([PR 7348](https://github.com/excalidraw/excalidraw/pull/7348)).
- **Thumbnails and caches.**
  - Excalidraw time-boxes bitmap regeneration to 8 ms per frame and blits stale scaled bitmaps meanwhile ([PR 12211](https://github.com/excalidraw/excalidraw/pull/12211)).
  - Excalidraw caps each element's cache at 16.7 Mpx ([renderElement.ts](https://github.com/excalidraw/excalidraw/blob/master/packages/excalidraw/src/renderElement.ts)).
  - Per-object canvases crashed Lucidchart with hundreds of objects. I saw this only in a summary ([Lucid](https://lucid.co/techblog/2015/05/19/big-content-in-a-little-canvas)).
- **Large mixed selections.** Figma hides Selection Colors by default for very large selections because it gets slow ([Wichary](https://aresluna.org/designing-selection-colors)).
  - **Inference:** compute properties on selection change and at commit, never per pointer move. Feed them from chunk summaries.

## (e) Implications for the Capy design

**Sound**
- Immutable 64-record `Arc` chunks, matching im-rs and Zed.
- Document-space tile bins, the right fit for a bounded canvas.
- CPU F64 picking on candidates only.
- Derived data kept out of the file and out of undo.

**Risky, with suggested changes**

1. **Bins hold "path indices".** Any insert, delete or reorder shifts every later index, so every edit becomes a full rebuild.
   - Store stable item IDs in bins. Resolve z-order from the already-derived ID→order table.
2. **Caches keyed by `Arc` identity.**
   - **ABA hazard:** a freed `Arc`'s address can be reused, so a cache keyed by the pointer would wrongly hit. Hold a `Weak` and check `ptr_eq`, or give the geometry a generation ID.
   - **Moves invalidate everything:** a move that rewrites anchors misses every cache.
   - Give each record its own geometry `Arc` plus an affine transform, as Figma does. Translation, rotation and uniform scale then keep flatten caches; only a non-uniform scale or a non-scaling stroke forces re-expansion. Transform-only undo steps copy small records and share the geometry.
3. **Gesture state in the document.**
   - Put drags, rotations, erasing and in-progress strokes in tool state as transient modifiers rendered with a matrix, as Penpot does.
   - Exclude moved items from bins during the gesture and invalidate by the union of old and new bounds.
   - Commit one batched edit at release. Parents and subtree ranges are then derived once per gesture, not per event.
4. **Tolerance in the cache key.** If tolerance follows zoom, caches thrash during zoom.
   - Use a fixed document-space tolerance if vectors are composited at canvas resolution.
   - Otherwise quantize to power-of-two zoom steps, like tldraw's stepped scale ([docs](https://tldraw.dev/sdk-features/performance)).
5. **Large commits stall the next motion.** For example, re-expanding 5,000 outlines after a non-uniform scale.
   - Amortize under a per-frame budget, Excalidraw-style, and keep drawing transformed old outlines until the new ones are ready.
6. **Hidden O(n) per frame.** Add a headless 10k-path, 250k-anchor fixture in shared Rust.
   - Count per-event work for dragging one item, hovering, drawing, erasing and marquee.
   - Fail the test if the count scales with n. These are the bug classes tldraw and Excalidraw found ([issue 8882](https://github.com/tldraw/tldraw/issues/8882), [PR 12183](https://github.com/excalidraw/excalidraw/pull/12183)).
7. **Async operations.** Boolean, simplify, offset, convert stroke, fill region and SVG import run on workers with cancellation and stale-result rejection. Check the Web host's threading setup.
8. **Undo caps.** Cap undo by bytes of uniquely owned chunks as well as by step count. Clip Studio users report undo depth as a memory issue ([CSP](https://ask.clip-studio.com/en-us/detail?id=85312)).

**Keep it simple**
- One grid index and one flatten routine, shared by culling, picking, eraser, fill and snapping.
- No R-tree, no planar-map engine and no per-item bitmap caches.

## (f) What I could not verify

- Stroke counts on hatched comic pages, and Clip Studio's internals or thresholds.
- Figma's spatial index, and any Figma per-vertex memory figures.
- Why tldraw closed its grid experiment.
- Kurbo stroker timings: the paper gives a chart only.
- The iOverlay benchmarks are vendor-run.
- The Illustrator 3,000-path report, Lucidchart, Frisken and the Inkscape bucket description came from search summaries, not full pages.
- The low tablet's chip (reported by retailers as Helio G80, 6 GB RAM, [listing](https://www.pccomponentes.com/tablet-tcl-tab-11-gen-2-wifi-11-6gb-256gb-gris)) and how slow it is relative to the desktop CPUs in these benchmarks. Every cost in this report needs measuring on the reference tablets.
