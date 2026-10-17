# Vector layer performance: research and design

[Technical documentation](../README.md) · [Design and validation history](README.md)

Research and design: **2026-10-16**, source baseline **`7c71b731c`**.

This record covers how vector layers stay fast:
- the content each tier must handle;
- where time and memory go;
- what other renderers and editors learned;
- the data structures, caching, compositing and scheduling that keep the canvas
  and panels responsive.

It revises the rendering plan in [vector layers §7.5](vector-layers-research.md#75-rendering)
and adds in-memory structure to the object model of §7.2. The file format is
unchanged. It describes intended work, and every cost below is an estimate from
public figures and the tier hardware until the measurements in section 8
replace it.

The research combines six streams. Their write-ups, with every source URL, are in [`vector-performance-research/`](vector-performance-research):

| # | Stream | File |
|---|---|---|
| 1 | Vello and Graphite: renderer architecture, editor scale and caching | [01-vello-graphite.md](vector-performance-research/01-vello-graphite.md) |
| 2 | Inkscape, Illustrator, Affinity Designer, CorelDRAW and Krita | [02-inkscape-and-desktop-editors.md](vector-performance-research/02-inkscape-and-desktop-editors.md) |
| 3 | Chromium (Blink SVG, cc, Skia), Firefox WebRender and WebKit | [03-chromium-skia-webrender.md](vector-performance-research/03-chromium-skia-webrender.md) |
| 4 | Editor data structures, tool algorithms and panels: Figma, Penpot, tldraw, Excalidraw and geometry libraries | [04-editor-data-structures.md](vector-performance-research/04-editor-data-structures.md) |
| 5 | GPU path rendering on Mali, Adreno and WebGPU | [05-mobile-gpu-path-rendering.md](vector-performance-research/05-mobile-gpu-path-rendering.md) |
| 6 | Capy Canvas code audit at `7c71b731c` (file:line references) | [06-codebase-audit.md](vector-performance-research/06-codebase-audit.md) |

**Caveats on the evidence**
- **No device measurements.** Nothing here ran on the tier devices. Several costs are unknown:
  - Mali integer-atomic throughput;
  - flattening and stroking speed on Cortex-A75 and A76 cores;
  - per-dispatch overhead;
  - whether RGBA16F targets stay compressed on these GPUs.
- **Vendor sources.** Adobe and Corel community pages refused direct fetches, so those claims come from search extracts. The "up to 10×" speed-ups that Illustrator, Affinity and CorelDRAW announce have no published method.
- **Issue reports.** Most Inkscape, Affinity and Graphite timings come from single reporters on unstated hardware. They show which failure classes recur, not how fast Capy will be.
- **Hatching counts.** No source gave how many strokes a heavily inked comic page holds. The targets in section 2 need checking against real pages.
- **Code audit:** it read the code at `7c71b731c` and built and ran nothing. File:line references apply to that baseline.

---

## 1. Summary

**Targets.** The same content target applies to every tier, because files move between devices:
- A vector layer of **10,000 paths and 250,000 anchors** holds the tier rate for every motion.
- At **50,000 paths and 1,000,000 anchors** it stays correct and responsive. Still frames are slower, and long operations run in the background.
- One path of **32,000 anchors**, Illustrator's practical limit, stays editable at the tier rate.

**What makes vector content slow elsewhere**, most damaging first:
1. **Work per event that grows with the document.** Inkscape, Affinity, Graphite, tldraw and Excalidraw all hit this, Graphite at a few hundred objects and the others at a few thousand:
   - change notifications per object;
   - panels rebuilt on every change;
   - linear scans for picking and snapping;
   - O(n²) selection and ungroup.
2. **Redrawing everything every frame.** Vello and early WebRender re-render the whole scene each frame. Their own data shows CPU encoding and GPU fill dominating as scenes grow.
3. **Rasterizing more pixels than the screen shows.** Krita renders vector layers at image resolution. Zoomed-out Vello scenes pile up in a few tiles.
4. **GPU work sized in advance for scene-dependent data.** Vello's frames go blank when its buffers overflow.
5. **Strokes, dashes, clips and group effects multiplying cost.**
   - In Chrome, 2 px strokes are much slower than 1 px ones, and nested clips render as offscreen layers. Its dashing is capped at a million segments.
   - In Affinity, effects on a container re-render every child.
6. **Zoom.** Graphite keys its cache on the zoom level, so every zoom step discards it. Renderers that re-rasterize at every scale, rather than in steps, pay on every frame of a pinch.
7. **Mobile shader compilers and drivers.** Register pressure, workgroup memory and atomics behave very differently on Mali and Adreno than on desktop GPUs.
8. **Cache bookkeeping that itself becomes O(n),** and caches that conflict with threads (Inkscape).

**What this design does about each**:
1. **Every edit, frame and UI refresh costs O(what changed).** Immutable chunks are compared by identity, each transaction emits one change set, each layer keeps a derived index, and panels are virtualized.
2. **Retained, regenerable pixel caches and damage per item.** A frame redraws only what moved.
3. **Vector content is evaluated at the display level the view needs,** never at native density and then reduced. Its pixel caches are bounded by the screen, not the canvas.
4. **The CPU sizes all GPU work exactly before submission.** Nothing is allocated on the GPU or read back, and no frame is skipped. Work over a frame's cap continues in the next frame, visible tiles first.
5. **Stroke outlines are cached per path and style.** Dashes are capped and culled to windows. Clips (later) are coverage, not nested layers.
6. **Fixed power-of-two levels with hysteresis.** The presenter scales the current level while the new one renders, visible windows first.
7. **A coverage kernel shaped for Mali:** no atomics and no workgroup memory, 64-thread workgroups, and each pixel written once.
8. **A few coarse caches** with byte budgets and LRU eviction, keyed by identity they hold.

**Changes to the vector design** (section 7):
- display-level evaluation replaces native density;
- a per-tile gather kernel without atomics replaces atomic area accumulation;
- flattening within 1/4 instead of 1/8 output pixel;
- tile bins hold item IDs;
- records hold geometry by `Arc`;
- gestures keep their state out of the document, and drags split the layer by paint order and move a cached raster.

**Prerequisites in the current code** (section 5). Each must become proportional to what changed before vector layers ship:
- Each edit validates, inventories and charges history over the whole document.
- Each frame compares every layer's metadata structurally, several times.
- Autosave replays and re-encodes every history state, every 2 seconds.
- Any object layer turns off refinement, batching, the complete hierarchy and the Navigator's own composition for the whole document.
- The Web Layers panel keeps a DOM row per layer.

---

## 2. Scale targets

### 2.1 What other tools reach

| Tool | Evidence | Source |
|---|---|---|
| tldraw | 4,000 shapes per page by default. Benchmarks pages of 10,000 shapes, and splits freehand strokes at 600 points | [options.ts](https://github.com/tldraw/tldraw/blob/main/packages/editor/src/lib/options.ts), [PR 9093](https://github.com/tldraw/tldraw/pull/9093), [draw-shape docs](https://github.com/tldraw/tldraw/blob/main/apps/docs/content/sdk-features/draw-shape.mdx) |
| Excalidraw | Tests scenes of 2,000–10,000 freehand strokes. Before a fix, zooming 10,000 strokes took 1,850 ms and a pan frame 117 ms | [PR 12211](https://github.com/excalidraw/excalidraw/pull/12211) |
| Figma | Files commonly hold "tens of thousands of layers". Memory is limited to about 2 GB per tab | [Layers panel](https://www.figma.com/blog/improving-performance-in-the-layers-panel/), [memory](https://help.figma.com/hc/en-us/articles/360040528173-Reduce-memory-usage-in-files) |
| Illustrator | About 32,000 anchors per path | [Adobe forum](https://community.adobe.com/t5/illustrator-discussions/maximum-anchor-points-on-a-path/m-p/10163269/highlight/true) |
| Inkscape 1.0 | About 5,000 objects usable, 10,000 "pretty bad", 20,000 freezes | [#709](https://gitlab.com/inkscape/inkscape/-/issues/709) |
| Graphite 1.0 rc6 | A grid of about 160 squares becomes unusable. Maintainers advise fewer than 100 layers | [#4572](https://github.com/GraphiteEditor/Graphite/issues/4572), [#3123](https://github.com/GraphiteEditor/Graphite/issues/3123) |
| Affinity Designer 1.7 | Marquee-selecting 5,700 objects takes 22 s | [Forum](https://forum.affinity.serif.com/index.php?/topic/94234-version-171-very-slow-at-selecting-large-number-of-objects/) |
| Clip Studio Paint | Moves on dense hand-drawn vector layers freeze for seconds. Users simplify lines to recover | [Clip Studio Ask](https://ask.clip-studio.com/en-us/detail?id=30932) |

### 2.2 Capy's targets

| Per vector layer | Holds the tier rate | Stays responsive |
|---|---|---|
| Paths | 10,000 | 50,000 |
| Anchors | 250,000 | 1,000,000 |
| Anchors in one path being edited | 32,000 | 32,000 |
| Items moved, scaled or rotated together | All | All |
| Cost of a vector layer in a stack | No more per frame than a paint layer covering the same pixels | Same |

- **Holds the tier rate:** every motion in section 6.7 qualifies under the [performance targets](../PERFORMANCE_TARGETS.md).
- **Stays responsive:** navigation and drags of small selections still hold the rate. The following may take longer as still frames, but never delay the next motion by more than a frame:
  - the release of a large selection;
  - opening the document;
  - a change of display level.
- **Why one target for every tier:** display-level evaluation (6.5) makes most per-frame cost depend on the screen rather than on the canvas.
- **Inputs, not limits.** Counts are workload inputs, as in the vector design.
- **The long path holds the tier rate.** Report 4 proposed 5,000 anchors at the rate and 32,000 as responsive only. Runs (6.2) bound an anchor edit to one run of 256 anchors, so the full 32,000 is the target.
- **Basis:**
  - 10,000 matches the largest pages tldraw and Excalidraw benchmark, and the point where Inkscape becomes "pretty bad".
  - An average of 25 anchors per path allows for short hatching lines (2–6 anchors) and long contours (20–100).
  - Check both against inked pages before V1 qualifies.

### 2.3 Memory at the targets

| Data | 10,000 paths, 250,000 anchors | 50,000 paths, 1,000,000 anchors |
|---|---|---|
| Authored anchors (six F64 values and a kind byte, about 56 B each), records and styles | about 16 MB | about 64 MB |
| Derived outlines, flattened polylines and bins for one level | about 40 MB | about 160 MB |
| History for one step that moves every path | about 16 MB | about 64 MB |
| Pixel caches | Bounded by the screen (6.10) | Same |

These are estimates: the derived figures assume about six flattened points per segment and outlines on both sides of each stroke. The headless tests in 8.1 measure them.

---

## 3. Where time and memory go

### 3.1 Pixels and bytes per frame

| | Low | Mid | Top |
|---|---|---|---|
| Screen | 1200 × 1920, 2.3 MP | 1440 × 2200, 3.2 MP | 1800 × 2880, 5.2 MP |
| Tier canvas | 12.0 MP | 24.0 MP | 60.2 MP (the "61 MP" tier canvas) |
| Display level at Fit, and its pixels | 1 (2 × 2 footprint), 3.0 MP | 1, 6.0 MP | 1, 15.1 MP |
| GPU bytes per moving frame at peak bandwidth | 173 MB | 137 MB | 403 MB |
| One RGBA32F write of the whole canvas | 192 MB | 384 MB | 963 MB |
| One RGBA32F write of the Fit level | 48 MB | 96 MB | 241 MB |
| One RGBA16F write of the Fit level | 24 MB | 48 MB | 120 MB |

The display level follows [`display_mips::view_level`](../../crates/layer-render-wgpu/src/display_mips.rs): the largest power-of-two footprint no wider than a screen pixel. The table assumes the tablet in landscape with the canvas filling the screen, which puts all three tiers on level 1 at Fit. In portrait, or with panels narrowing the view, the mid and top tiers drop to level 2, with a quarter of the pixels. The Fit rows are therefore upper bounds.

What follows from the table:
- **No canvas-density redraw in a moving frame.** Redrawing a vector layer at canvas density cannot fit a moving frame on any tier, whatever the rasterizer. Its write alone exceeds the frame's bandwidth on all three.
- **Redrawing the view is a multi-frame event.** At Fit, rewriting the visible area at the display level uses 28%, 70% and 60% of a frame's bandwidth on the low, mid and top tiers. That is before composition and kernel work. So a full redraw of the view happens only on open or a level change, and is spread over frames. A motion never requires one.
- **Motions scale with what moves.** The damage rules, split caches and moving raster in section 6 arrange that.

### 3.2 Kernel work on the low tier

Report 5 estimates two scenes on the Mali-G52 MC2. It assumes about 30 G lane instructions per second sustained and about 15 instructions per pixel per path. These are not measurements.

| Scene | Paths | Segments | Covered path-pixels |
|---|---|---|---|
| A | 5,000 | 200,000 | about 20 M |
| B | 50,000 hatching strokes | 2,000,000 | about 20 M |

- **Per-path read-modify-write of the window in memory: about 640 MB per redraw,** roughly four frames of bandwidth even for scene A. Compositing must stay in registers.
- **Registers and one write per pixel:** a full canvas-density redraw of scene A takes about 16 ms of ALU and 13 ms of RGBA32F writes. Scene B cannot redraw the canvas in one frame under any structure.
- **Capacity per low-tier frame:** about 10–20 M tile-pixel visits and 3–5 MP of RGBA16F output.
- **A typical drag:** a 500-path group over 2 MP of windows in scene B costs 3–10 ms of ALU, 16 MB of output and about 5 MB of segments. It fits only with fine-grained damage and caches.

### 3.3 CPU

- **Budget.** The low tier allows 4 ms per moving frame for input, update and submission. Derivation for a motion (flattening, stroking and binning) should take at most half of that.
- **Unknown on these cores:** flattening and stroking throughput on the low tier's Cortex-A75. The low tablet scores 406 single-core in Geekbench 6, about a fifth of the top tier's ([tier hardware](../performance/hardware.md)).
- **Vello's sparse-strip renderers.** Coarse rasterization on the CPU, done every frame, "basically diminishes" the GPU's advantage ([#1759](https://github.com/linebender/vello/pull/1759)). Once that was fixed, CPU glyph rasterization dominated ([#1380](https://github.com/linebender/vello/issues/1380)). Capy does its CPU geometry work only for what changed, and caches it.
- **Stroking cost varies by implementation:**
  - kurbo's old stroker took the tiger scene from about 5 ms to more than 20 ms before its replacement ([vello#955](https://github.com/linebender/vello/issues/955)).
  - GPU stroke expansion is about 14 times slower on a Mali-G78 MP20 than on an M1 Max ([Levien and Uguray 2024](https://arxiv.org/abs/2405.00127)).
  - Capy strokes on the CPU, caches each outline per path and style, and measures this early (8.2).

---

## 4. Lessons from other renderers and editors

### 4.1 Renderers

| Problem | Evidence | Lesson |
|---|---|---|
| GPU buffers sized for scene-dependent data overflow | Vello skips the frame and reallocates, learning of the overflow a frame late ([#366](https://github.com/linebender/vello/issues/366)). More than about 130 animated tigers render black on an RTX 4080 ([#720](https://github.com/linebender/vello/issues/720)). Estimating sizes slowed encoding 1.75–2.3× ([#541](https://github.com/linebender/vello/issues/541)), and overestimated one scene's memory sevenfold ([Levien and Uguray](https://arxiv.org/abs/2405.00127)) | Count everything on the CPU before submission. Split work that does not fit; never skip a frame |
| Immediate mode re-encodes every frame | Rotating 10,000 children under a parent ran at 10–20 fps; patching transforms in place gave 40–60 ([Zulip](https://xi.zulipchat.com/#narrow/channel/197075-vello/topic/.28Showcase.29.20Vello.20Retained.2FFine.20Granular.20Mode.20WIP/near/459506834)). Vello's scene API gives paths no ID to cache by. WebRender's original redraw-everything approach was "too much" for most GPUs ([picture caching](https://mozillagfx.wordpress.com/2018/11/02/webrender-picture-caching/)) | Retain derived data and pixels under stable identity |
| Zoomed-out scenes collapse into a few tiles | Vello's fine stage took 567 ms with paris-30k shrunk to a small square ([#305](https://github.com/linebender/vello/issues/305)). Zoomed in, flattened lines were not culled and memory grew until machines hung ([#548](https://github.com/linebender/vello/issues/548)) | Evaluate at the display level with a level-dependent tolerance. Clip lines to windows, cap work per tile and split |
| CPU binning every frame | Removing coarse rasterization was needed because its CPU cost "basically diminishes" the GPU's advantage ([#1759](https://github.com/linebender/vello/pull/1759)) | Bin only changed paths, and cache bins per path and level |
| Complex antialiased paths | Chrome counts "slow paths": antialiased, non-convex, and neither hairlines nor small fills ([paint_op.cc](https://chromium.googlesource.com/chromium/src/+/main/cc/paint/paint_op.cc)). A layer with 6 or more is rastered with MSAA ([paint_op_buffer.h](https://chromium.googlesource.com/chromium/src/+/main/cc/paint/paint_op_buffer.h), [layer_tree_host_impl.cc](https://chromium.googlesource.com/chromium/src/+/main/cc/trees/layer_tree_host_impl.cc)), which turns off partial raster ([tile_manager.cc](https://chromium.googlesource.com/chromium/src/+/main/cc/tiles/tile_manager.cc)). Ganesh refuses paths over 16,384 verbs ([PathRenderer.h](https://skia.googlesource.com/skia/+/main/src/gpu/ganesh/PathRenderer.h)) | Exact-area coverage treats concave and convex paths alike; still cap work per window |
| Strokes and dashes | Paths animated smoothly at 1 px stroke but at about 12 fps at 2 px ([crbug 41155737](https://issues.chromium.org/issues/41155737)). One path produced 90 million dash segments, and Skia now caps dashing at a million ([SkDashPath.cpp](https://skia.googlesource.com/skia/+/main/src/utils/SkDashPath.cpp)) | Cache outlines; cap and cull dashes |
| Nested clips as offscreen layers | Canva's large SVGs took more than a second to render, especially while dragged or zoomed ([crbug 410960657](https://issues.chromium.org/issues/410960657)). Chrome replaced nested clip layers with path intersection ([CL 6487569](https://chromium-review.googlesource.com/c/chromium/src/+/6487569)) | Clips as coverage in the kernel, not as layers (later) |
| Caches keyed on zoom | Graphite's tile cache includes zoom in its key, so any zoom change discards it ([render_cache.rs](https://github.com/GraphiteEditor/Graphite/blob/master/node-graph/nodes/gstd/src/render_cache.rs)) | Fixed levels |
| Re-raster scale during pinch | Chrome keeps the old scale until the ratio passes 2, and snaps to an existing scale within 1.2× ([picture_layer_impl.cc](https://chromium.googlesource.com/chromium/src/+/main/cc/layers/picture_layer_impl.cc)). WebRender re-rasters at ratios of 0.5 or 2 and refines when the zoom ends ([picture.rs](https://github.com/mozilla-firefox/firefox/blob/main/gfx/wr/webrender/src/picture.rs)) | The same hysteresis (6.5) |
| Tile size and dirty tracking | Firefox's 512-px tiles regressed its SVG benchmark by 21–38% and were backed out ([bug 1789612](https://bugzilla.mozilla.org/show_bug.cgi?id=1789612)). WebRender splits or merges dirty-tracking cells by how often they change. A quantization mismatch between invalidation keys and the rasterizer left stale tiles ([bug 2052391](https://bugzilla.mozilla.org/show_bug.cgi?id=2052391)) | Keep 256-px windows with one dispatch per frame for all dirty tiles. Snap damage exactly as the kernel rounds |
| GPU memory | Chrome's whole Android tile budget is 96 MB on low-end devices and 256 MB otherwise ([layer_tree_settings.cc](https://chromium.googlesource.com/chromium/src/+/main/third_party/blink/renderer/platform/widget/compositing/layer_tree_settings.cc)). WebRender evicts tiles the previous frame did not use | Budget regenerable caches and evict what is not visible first |
| Cache bookkeeping | Inkscape hung on zoom with 17,000 paths, inside an O(n) insert into a `std::list` of cache candidates ([#6110](https://gitlab.com/inkscape/inkscape/-/issues/6110), [MR 7876](https://gitlab.com/inkscape/inkscape/-/merge_requests/7876)). Its render cache conflicts with multithreading, so the advice became to set it to zero ([MR 4986](https://gitlab.com/inkscape/inkscape/-/merge_requests/4986)) | A few coarse caches with O(1) bookkeeping, read from immutable snapshots without locks |
| Mobile compilers and drivers | On Adreno 640, Vello's fine stage took 38 ms instead of 6 because one buffer was both read and written ([#83](https://github.com/linebender/vello/issues/83)). A workgroup-memory read in a discarded `select` arm lost the device on Mali ([#1816](https://github.com/linebender/vello/issues/1816)). On Adreno 7xx in Chrome, 8 of 23 pipelines fail to build ([#1957](https://github.com/linebender/vello/issues/1957)). Zero-initializing workgroup memory stalls the first frame by 1.5–3 s in browsers ([#936](https://github.com/linebender/vello/issues/936)). Rive turns its atomic mode off on Android | Small kernels, no workgroup memory or atomics, separate read-only and read-write bindings, register checks and pipeline caches |
| CPU tessellation | On Android, 67% of Impeller's path time went to building vertex buffers, 45% of it in CPU tessellation alone. Stencil-then-cover cut a Rive frame from 43.7 to 10.6 ms on an iPhone 12 mini ([flutter#143077](https://github.com/flutter/flutter/issues/143077)) | Coverage from line segments, no triangulation |
| Low-tier GPUs | A Samsung A05s took about 35 ms for 200 rectangles that an M4 Max drew in 0.4 ms ([Zulip](https://xi.zulipchat.com/#narrow/channel/197075-vello/topic/Early.20z.20culling.20in.20hybrid.20via.20front.20to.20back.20rendering/near/582436628)) | Desktop numbers do not predict the tiers. Benchmark the kernel on the G52 first (8.2) |
| Occlusion | In a proof of concept, drawing opaque strips front to back made complex SVGs about 30% faster on a low-tier phone ([Zulip](https://xi.zulipchat.com/#narrow/channel/197075-vello/topic/Early.20z.20culling.20in.20hybrid.20via.20front.20to.20back.20rendering/near/582442270)). Removing overdraw under opaque image fills gave 30% on one scene ([Linebender](https://linebender.org/blog/tmil-24/)). Pathfinder culls occluded tiles | Drop commands under an opaque solid tile |

### 4.2 Editors

| Problem | Evidence | Lesson |
|---|---|---|
| Notifications per object | Shift-selecting 2,425 objects in Inkscape's layer list took more than eight minutes. Each add recomputed bounds, handles and centre ([#6391](https://gitlab.com/inkscape/inkscape/-/issues/6391)). Converting 1,600 rectangles made 2 billion calls in the Objects dialog ([MR 5183](https://gitlab.com/inkscape/inkscape/-/merge_requests/5183)). Ungrouping 30,000 objects took 85 s, then 35 s once batched ([MR 8161](https://gitlab.com/inkscape/inkscape/-/merge_requests/8161)) | One change set per transaction; panels refresh once per change set |
| Main-thread update proportional to objects | Inkscape 1.3 moved rendering off the main thread, but `update` "can't be moved off-main-thread" and is slow with many objects ([MR 4876](https://gitlab.com/inkscape/inkscape/-/merge_requests/4876)) | Immutable chunks give the render worker a snapshot without copying; derived data updates from the diff |
| Linear picking and snapping | Inkscape walks every item to render, pick and snap. Path-intersection snapping froze it for minutes ([#3171](https://gitlab.com/inkscape/inkscape/-/issues/3171)). Hover picking ran on every mouse event during pans ([inbox#13459](https://gitlab.com/inkscape/inbox/-/issues/13459)) | Tile bins, per-gesture candidate arrays, and no hover picking during navigation |
| Effects on containers | Affinity staff: effects on a top-level container recalculate and re-render every child ([forum](https://forum.affinity.serif.com/index.php?/topic/181174-designer-v2-suddenly-slow-laggy/)) | Group effects later, with bounded per-window intermediates |
| Undo by replay | One Inkscape user reported an undo of Inset on 8,000 objects taking about an hour ([#709](https://gitlab.com/inkscape/inkscape/-/issues/709)). In 1.3, a whole-document fix-up after every undo took 2–5 s ([#4720](https://gitlab.com/inkscape/inkscape/-/issues/4720)). Graphite stored the whole document per step ([#847](https://github.com/GraphiteEditor/Graphite/issues/847)) | Undo swaps chunk roots; damage and derived updates come from the chunk diff |
| Structures per glyph or per reference | Inkscape makes a display item per glyph, so a file of nearly a million glyphs opened in 25 s ([#5483](https://gitlab.com/inkscape/inkscape/-/issues/5483)). Clones of clones loaded in quadratic time because of a cycle check ([#4281](https://gitlab.com/inkscape/inkscape/-/issues/4281)). 37,440 rectangles in nested Affinity symbols dragged at 0.3 Hz | Text as glyph runs in one item; acyclic definitions; shared instance caches |
| Index maintenance | A structural dirty set cut a single-shape drag on a 10,000-shape tldraw page from 23.1 to 1.0 ms ([PR 9093](https://github.com/tldraw/tldraw/pull/9093)). The index had re-walked every shape on every diff ([issue 8882](https://github.com/tldraw/tldraw/issues/8882)) | Update only the changed items' bins and ancestors |
| Commits during gestures | Batching cut a resize of 2,000 tldraw shapes from about 65 to 14 ms per tick ([PR 9153](https://github.com/tldraw/tldraw/pull/9153)). An O(n²) ID-set rebuild made dragging 4,000 Excalidraw rectangles take 643 ms per frame ([PR 12183](https://github.com/excalidraw/excalidraw/pull/12183)). Penpot keeps in-progress transforms in a separate modifiers map ([modifiers.cljs](https://github.com/penpot/penpot/blob/develop/frontend/src/app/main/data/workspace/modifiers.cljs)) | Gesture state lives in tool modifiers; one edit at release |
| Panels | Figma's Layers panel computes data only for the 20–30 visible rows. Its memory went from O(n²) to O(n), and interactions got 30–50% faster ([Figma](https://www.figma.com/blog/improving-performance-in-the-layers-panel/)). Graphite's panel takes 8 s to render 10,000 layers ([#4627](https://github.com/GraphiteEditor/Graphite/issues/4627)). Adobe advises turning Layers thumbnails off for complex files | Virtualized projections, with thumbnails only for visible rows |
| Derived geometry on unrelated events | Inkscape recomputed live path effects on selection change and copy ([#1351](https://gitlab.com/inkscape/inkscape/-/issues/1351)) | Derived data recomputes only when its inputs change |
| Booleans and import | A tiny tolerance in livarot took 15 GB and a minute ([#5356](https://gitlab.com/inkscape/inkscape/-/issues/5356)). Graphite's SVG import spent 10 s on O(n²) layout ([#2515](https://github.com/GraphiteEditor/Graphite/issues/2515)). On nested squares, iOverlay took 60 ms and Clipper2 2.35 s ([vendor benchmark](https://ishape-rust.github.io/iShape-js/overlay/performance/performance.html)) | Off the frame, cancellable, with tolerance floors and output caps |
| Freehand fitting | Recomputing a whole stroke for each new point dropped frames on low-end phones ([Canva](https://www.canva.dev/blog/engineering/behind-the-draw/)). Inkscape's pencil fits only the tail while drawing ([pencil-tool.cpp](https://gitlab.com/inkscape/inkscape/-/blob/master/src/ui/tools/pencil-tool.cpp)) | Fit the tail incrementally and refit once at pen-up |

### 4.3 Mobile GPUs and WebGPU

**Arm's guidance** ([Mali Best Practices](https://documentation-service.arm.com/static/67a62b17091bfc3e0a947695), [Mali data sheet](https://documentation-service.arm.com/static/661e91e25d66282bc2cf9cd2)):
- **Workgroup memory is not on chip.** On Mali it is system RAM behind the load/store cache.
- **Atomics are fast only when one shader core owns the cache line.** Arm says: "If better solutions that use multiple passes are available, then do not use atomics."
- **Workgroup size:** use 64 threads.
- **Registers:** above 32 registers per thread, the number of resident threads halves (data sheet).
- **Compression:** compute writes to storage images turn off AFBC compression. Qualcomm says compute "almost certainly" turns off UBWC ([Adreno guide](https://docs.qualcomm.com/bundle/publicresource/80-78185-2/topics/mobile_best_practices.md)).
- **Float blending:** Bifrost has no accelerated floating-point blending; Valhall accelerates FP16.
- **MSAA is cheap on tilers,** because 4× resolves on chip. WebGPU cannot multisample rgba32float, and by report 5's arithmetic RGBA32F at 4× would not fit Mali's 256 bits of tile storage per pixel (data sheet).

**WebGPU limits** ([spec](https://www.w3.org/TR/webgpu/)):
- 16 KiB of workgroup storage and 256 invocations per workgroup;
- 128 MiB per storage binding and 8 storage buffers per stage;
- 32-bit atomics only.
- WGSL lets drivers reassociate and fuse f32 arithmetic, so results are not bit-identical across devices ([WGSL](https://www.w3.org/TR/WGSL/)).
- rgba32float storage is write-only or read-only in core; read-write needs an optional feature. rgba16float is blendable and filterable in core.

**Consequences for the kernel.** A compute rasterizer loses output compression anyway. It is competitive on these GPUs only if it writes each output pixel once and avoids atomics and workgroup memory. Plain storage buffers and 32-bit integer atomics are the least fragile features across drivers; fragment atomics, pixel local storage and raster ordering are where vendors diverge.

---

## 5. Capy today: what breaks at scale

Paths starting with `scene/` are relative to `crates/layer-render-wgpu/src`.

| Area | Finding | Needed before vector layers ship |
|---|---|---|
| Edit cost | On the general path, `Document::apply` enumerates every image of the old and new document and runs `validate_payloads`, which validates every object (`crates/layer-core/src/lib.rs:1456-1464`, `:1626`). `prepare_history_edit` applies forward and inverse and builds a full root inventory. `trim_history` seeds its accounting from every tile in the document (`lib.rs:2290-2297`, `:2370-2384`) | Validate per changed chunk, cached by identity. Vector edits take a fast path like `presents_same_images` (`lib.rs:1443`) |
| Autosave | Every 2 s with a changed stamp, `PreparedSession` replays each undo and redo state through a full `Document::apply`. It then JSON-encodes every record of every state (`crates/layer-core/src/package/session.rs:28-41`, `:170-208`) | Cache encoded bytes per chunk identity, so a checkpoint encodes only new chunks; replay costs O(changed) per state |
| Per-frame comparisons | `same_artwork` walks every layer and runs up to five times a frame. `same_objects` falls back to structural equality when the object store changed (`crates/layer-core/src/artwork_query.rs:161-173`). Metadata is rebuilt per layer, and the display graph is rebuilt every frame (`crates/layer-render-wgpu/src/scene/scale/graph.rs:186-196`) | Compare vector content by root identity and revision only. Keep per-item data out of `Metadata`, `ObjectKey` and `PartialEq` |
| Damage | `edited_damage` damages whole-object bounds, and returns nothing when order or composition changed, which recomposes the full extent (`scene/objects.rs:49-78`). `Damage::push` restarts its scan after every merge (`scene/scale.rs:49-62`) | Damage per item, collected as page sets (6.6) |
| Spatial index | One key per layer; `query_at` returns everything or nothing (`scene/object_spatial.rs:18-21`) | Tile bins in the derived layout (6.2) |
| Document-wide opt-outs | Any visible object layer turns off refinement, cuts native batches to one page, skips the complete hierarchy and the Navigator's own composition (`scene/scale/refinement.rs:25,96,105`, `scene/scale/hierarchy.rs:55`, `scene/scale/navigator.rs:42`) | Key them on pending image work (already a V0 prerequisite) |
| Object drags | Each pointer move applies a real `RecordChange` (`crates/layer-ui/src/object_motion.rs:28-70`). The moving layer re-renders every frame, with no separate drag composite ([rendering](../internals/rendering.md#incremental-composition)). On the low tier, moving one of four shared objects reaches 47.9–50.4 fresh poses/s, below 60 ([low tier](../performance/low-tier.md#image-objects)) | Gesture state in tool modifiers, and split caches (6.7). Image objects gain from both |
| Fan-out limits | The fused object preview takes at most 16 objects and 8 sources (`crates/layer-render-wgpu/src/object_sampling.rs:44-45`). The object cache is 64 MiB with 256 tasks (`scene/object_cache.rs:9-15`) | The vector leaf has its own path; it reuses admission and LRU eviction, not these limits |
| Memory | Pages and display images are RGBA32F everywhere. The display cache is 608 MiB (`scene/scale.rs:6`). A fully covered 61 MP layer would be 950 pages of 1 MiB | Vector pixels exist only at the display level and are regenerable (6.10) |
| Uniforms | Per-layer, per-page uniform records grow with layers × pages. Beyond `max_buffer_size`, evaluation falls back to native (`scene/scale.rs:403-407`) | Vector leaf data stays out of these records |
| Layer list | Each refresh rebuilds every row and calls `can_delete_layers` per row, which walks the order: O(L²) (`crates/layer-ui/src/session.rs:5865-5877`, `crates/layer-core/src/layers.rs:808-829`). The layer list is part of one coarse document notification. The Web panel keeps a DOM row per layer and stringifies each (`apps/layer-web/layers.js:365-378`). Native hosts diff the whole model as JSON (`crates/layer-host/src/model_update.rs`). GTK, Android, Apple and Windows lists are virtualized | Row-level change sets. Virtualize the Web list before any item browser exists |

---

## 6. Design

### 6.1 Principles

1. **Frames scale with what moves; edits scale with what changed.** No per-frame or per-edit work grows with items, anchors, canvas area or history. Work counters in the tests enforce this (8.1).
2. **One derivation, many readers.** Geometry becomes outline curves, then flattened polylines per level, then bins. Rendering, picking, the eraser, snapping, fill and export all read the same cached results.
3. **Derived data is regenerable and budgeted.** Every cache can be dropped and rebuilt. None is authored, saved or charged to history.
4. **The CPU sizes the GPU's work.** Counts are exact before submission. There is no GPU allocation, no readback and no skipped frame.
5. **Evaluate what the screen shows.** The view chooses the level and the windows. Native density is for exact queries only.
6. **Reuse the compositor.** Vector content is a leaf in the existing graph. Masks, effects, clipping and the stack above it are unchanged.
7. **Automatic level of detail instead of settings.** There is no outline mode or effects-off mode; better behaviour replaces them.

### 6.2 Data structures

```text
Authored: layer-core, immutable, shared by history and saves
  VectorObject   chunks: Arc<[Chunk]>
  Chunk          Arc<{ records: [Record; ≤64], summary: ChunkSummary }>
  Record         kind, id: u64, properties, geometry: Arc<PathGeometry>
  PathGeometry   subpaths; a path over 256 anchors keeps them in Arc runs of 256
  ChunkSummary   bounds with stroke reach, item count, flags, style keys; computed once per chunk

Derived per vector object root: shared Rust, no GPU types
  VectorLayout   items: ItemTable, ids: id → row, bins: TileBins, geometry: GeometryCache
  ItemTable      struct of arrays in paint order: id, parent, chunk, bounds, flags
  TileBins       256-pixel document tile → item IDs, plus one bin outside the canvas
  GeometryCache  (geometry, stroke style) → outline curves
                 (outline, level) → flattened polyline and its 16-pixel tile spans with backdrops

GPU: layer-render-wgpu, per frame
  Commands       per 16-pixel tile: ordered (path span, backdrop, paint, fill rule) or solid fill
  Segments       tile-relative fixed-point lines, 8 B each
  Pixel caches   display-level windows as graph branches; band and moving caches during gestures

Transient: engine and tool state, never authored
  Selection      sorted item IDs; selected anchors while editing points
  Modifiers      moving set and its affine, dragged anchors, erased IDs, provisional path
```

- **Shallow chunk copies.** Records hold geometry by `Arc`. Editing one item copies its chunk's records shallowly, plus the item's own geometry. History charges chunks and geometry by identity, as it charges selection contours today.
- **Long paths in runs.**
  - A path over 256 anchors keeps them in `Arc` runs of 256. An anchor edit then copies one run, about 14 KB, instead of the whole path, which is 1.8 MB at 32,000 anchors.
  - Derivation is per run too: only the edited run re-flattens. Its joins read the neighbouring run's end tangents.
  - Runs are an in-memory layout. The file stores the path unchanged.
- **Chunk summaries** make Select All, fitting the view to a selection, mixed property values and the layer's bounds cost O(chunks), not O(items).
- **Bins hold stable item IDs,** not positions:
  - Inserting, deleting or reordering items shifts no other entries. Paint order comes from the item table when a window's list is built.
  - An item enters a bin by the coverage of its flattened segments, inflated by its stroke reach, not by its bounding box. A long diagonal line then does not enter every tile.
- **The item table updates from the chunk diff.** Unchanged chunks keep their rows. A structural edit renumbers positions once per edit: about 50,000 integer writes at the responsive target, never per frame.
- **Caches hold the `Arc`,** or a `Weak` compared with `ptr_eq`, never a bare address that an allocator could reuse. Graphite reused stale renders when one memo hash served as both identity and equality ([#3646](https://github.com/GraphiteEditor/Graphite/issues/3646)).
- **No R-tree or BVH.** The canvas is bounded and its tiles are fixed:
  - 204 tiles at 12 MP and 950 at 61 MP.
  - 10,000 strokes over about three tiles each make 30,000 four-byte entries, about 120 KB.
  - Updating a path costs only the tiles it touches.
  - Marquee selection scans the item table's bounds. 50,000 box tests take well under a millisecond (estimate).
- **Ownership.** The engine owns one `VectorLayout` per vector layer and lends it to the renderer through `FramePacket`, as it lends `SceneView` today. Picking, the eraser and snapping use the same instance.

### 6.3 Derivation and caches

**Pipeline**
1. **Outline.**
   - Stroke expansion produces closed outline curves in layer pixels, in F64. It applies joins, caps, the miter limit and the width profile, and its result is cached per geometry and stroke style.
   - Fills use the geometry directly.
   - Exact-area coverage handles thin outlines correctly, so no separate hairline renderer is needed.
2. **Flatten.**
   - Curves flatten within 1/4 pixel of the evaluated level, cached per outline and level. At level *k*, that is 2^*k*/4 layer pixels.
   - Segment counts scale with tolerance^-½. Moving from 1/8 to 1/4 therefore removes about 29% of segments, uploads and kernel work.
   - Each coarser level has about 0.7× the segments of the finer one.
3. **Bin.**
   - Each path's segments are binned into the level's 16-pixel tiles. Each tile row gets a backdrop: the winding entering from the left edge.
   - Bins are cached with the flattened result. Translating a path re-bins it; changing its color does not.
4. **Commands.** Each frame, every dirty tile gets an ordered list of path spans from the bins, built on the CPU. Commands beneath an opaque solid fill are dropped.

**Budgets**
- Outline and flattened caches share a byte budget per document. Eviction is by least-recent frame of use, and items in visible windows stay.
- Only the current level and its neighbours are kept.

**Opening a document**
1. Decode the chunks.
2. Compute conservative bounds from control points, without flattening.
3. Fill the bins.
4. Flatten only the items in visible windows.
5. Derive the rest during idle frames, within budget.

### 6.4 Coverage kernel

This replaces the GPU side of [§7.5](vector-layers-research.md#75-rendering).

**Dispatch and threads**
- One compute dispatch per vector layer per frame covers its list of dirty 16 × 16 tiles at the evaluated level. A 2D grid is used once tiles exceed 65,535.
- One 64-thread workgroup handles each tile, with four pixels per thread.

**Coverage and compositing**
- **Coverage:**
  - Each pixel starts from the tile row's backdrop.
  - It adds the exact signed area contributed by each of the current path's segments in the tile, using the closed-form per-pixel area of Vello's fine stage.
  - There are no atomics, no workgroup memory and no barriers.
- **Fill and paint:**
  - The command's fill rule, nonzero or even-odd, maps winding to coverage.
  - The path's paint entries are composited source-over in registers, then the next command follows.
  - A solid command, for a tile entirely inside a path, skips the area loop.
- **Output:** after the last command, each pixel is stored once into the window texture. The window buffer is never read back between paths.

**Determinism**
- Commands and segments are always processed in the same order, so a device always produces the same pixels.
- Devices may differ in the last bits, because WGSL lets drivers reassociate f32 arithmetic.
- The F64 reference renderer, compared with a tolerance, remains the contract.

**Segments**
- Segments are tile-relative fixed point, 8 B each, quantized on the CPU.
- The CPU computes backdrops from the same quantized segments, so winding closes exactly.

**Mali constraints**
- Keep register use at or below 32 per thread, checked with Arm's offline compiler.
- Read-only and read-write data use separate bindings.
- Branches on a command's fill rule or paint are uniform across the workgroup.

**Bounded per frame**
- Commands and segments per frame are counted exactly and capped.
- Tiles beyond the cap render in the next frame, visible tiles first.
- A window whose dirty tiles cannot all finish in one frame renders into a spare texture from the cache pool. It replaces the window only when complete, so no frame shows it half-written.

**Dense tiles.** A pixel's cost is the number of segments of each path in its tile. If the microbenchmark (8.2) shows scribbled tiles dominating, add a variant in which each thread owns one row and accumulates cover in registers, still without atomics.

**Reuse.** The selection contour filler and the Figure distance fields move to this kernel, as planned.

**Known limits**
- **Seams.** Adjacent antialiased edges of different paths leave faint seams (conflation), as in Vello and Mozilla's tiled prototype ([vello#49](https://github.com/linebender/vello/issues/49), [bug 1888089](https://bugzilla.mozilla.org/show_bug.cgi?id=1888089)). Fixtures bound it.
- **Self-overlapping outlines.** Over-coverage at self-overlapping outlines remains, as §7.5 states.

**Alternatives not taken**

| Approach | Why not |
|---|---|
| Atomic area accumulation (the previous plan) | Arm's guidance on atomics and workgroup memory, Rive's and Vello's driver problems on Android, and a 512 KiB accumulator per window that no L1 cache holds |
| Stencil-then-cover or MSAA | WebGPU cannot multisample RGBA32F, and RGBA32F at 4× exceeds Mali's tile storage. It also needs many draw calls and render passes |
| CPU tessellation | It dominated Impeller's path cost on Android |
| Sparse strips with coverage computed on the CPU | Ruled out by [CPU and GPU responsibilities](../architecture.md#cpu-and-gpu-responsibilities): the CPU never computes canvas pixels |
| Vello's compute pipeline | Scene-sized GPU buffers, called "experimental" by its maintainers, and pinned to its own wgpu version (vector design §1) |

### 6.5 Levels, caches and zoom

- **Evaluation level.**
  - The display leaf (`Compositor::objects`) evaluates vector content at the level `display_mips::view_level` requests.
  - The native leaf (`object_tile`) evaluates level 0 only for exact work: exports, captures, color sampling and effects that need native input.
- **Native-resolution effects.** An effect above a vector layer that requires native input already forces native evaluation (`scene/scale.rs:403-407`). That layer loses the advantage until the effect supports display resolution.
- **Pixel caches.** Evaluated windows are graph branches, admitted and evicted within the display budget (`CACHE_BYTES`). They are regenerable, and nothing is kept at native density.
- **Zoom.** The current level stays while the view scale is within a factor of 2 of it; the presenter already samples the composed window and its neighbouring levels. When the level changes:
  - the new level's visible tiles render from the center outward, over several frames, within the GPU budget;
  - each window shows the old level, scaled, until its new result is ready.

  Chrome and WebRender use the same hysteresis, and Affinity the same progressive refinement.
- **Pan.** Newly exposed windows render on demand, and one window ahead in the direction of the pan is prefetched (as Chrome's "skewport" predicts motion).
- **Crisp zoom (V3)** adds levels finer than native to the same mechanism. The caching and tolerance rules do not change.
- **Thumbnails and the Navigator** render vector layers directly at their own size. At that level the tolerance leaves each path only a few segments.

### 6.6 Damage

1. **Chunk diff.** Compare old and new chunks with `Arc::ptr_eq`. Within changed chunks, compare records by geometry identity and properties.
2. **Area.** Each changed item damages its old and new bounds, inflated by stroke reach including miters and by one pixel of the level. Bounds are snapped exactly as the kernel rounds.
3. **Cheap cases:**
   - a paint-only change damages without re-deriving;
   - a reorder damages only the moved items' bounds;
   - hiding damages the item's bounds.
4. **Page sets.** Damage is collected as a set of pages per level, not as a list of rectangles. For vector edits this replaces the merging in `Damage::push`.
5. **Large changes.** When more than about half of a layer's items change, damage the layer's bounds instead of diffing.
6. **Undo and redo** produce the same diff as the edit.

### 6.7 Motions

**Rules for every gesture**
1. **No document writes per event.** Drags, anchor drags, erasing, scrubs and freehand strokes keep their state in tool modifiers. The engine renders with the modifiers and commits one edit at release. Image-object motion changes to match: today it applies a `RecordChange` on every pointer move.
2. **Split by paint order.** At gesture start, the layer splits into three ranges:
   - below the lowest affected item;
   - from the lowest to the highest affected item;
   - above the highest.

   Below and above are cached windows (graph branches), built on demand for the windows the gesture reaches, and ahead in its direction.
3. **The middle range.**
   - If it holds only affected items, it renders once into a moving cache, which the existing resampler places with the transient affine each frame.
   - If affected and unaffected items interleave, it re-renders each frame from cached derivation. The moving items' segments are transformed and re-binned on the CPU, at a cost proportional to their segments.
4. **Rescaling the moving cache.** It re-renders when its scale leaves the range of 0.5 to 2 times the cached scale, the same hysteresis as zoom.
5. **The rest of the stack.** Layers above and below stay cached as branches of the balanced Normal tree, so a frame recomposes only the branches on the moving layer's path. Other blend modes recompose as they do today.
6. **Release.**
   1. One edit commits.
   2. Moved items re-derive, because the edit rewrites their geometry, visible windows first.
   3. The moving cache stays on screen until each window's exact result is ready.

**Per motion**

| Motion | Per frame | Outside motion frames | Risk |
|---|---|---|---|
| Pan, zoom, rotate | Cached windows; newly exposed or new-level tiles within budget | — | Level changes on dense layers |
| Move or transform a selection | Place the moving cache; compose with the band caches | Bands and moving cache at start; re-derive moved items at release | Releasing large selections, and history memory |
| Edit Points | Re-flatten and re-expand the dragged anchors' run; re-render the middle range in damaged windows | — | Very long paths; runs bound the cost |
| Freehand vector line | Fit the tail; draw the provisional tail through the kernel in the layer's position | Refit at pen-up and replace in the same submission | The [instant stroke feedback](../reference/instant-stroke-feedback.md) budget at pen-up |
| Vector eraser | Find candidates from the bins under the swept capsule; hide hits; damage their bounds | Split at intersections at release | Candidate counts in dense hatching |
| Color or opacity scrub | Re-render the affected items' windows from caches | — | Large selections are fill-bound |
| Stroke width or profile scrub | Re-expand affected outlines within the frame budget, visible first | Finish the rest when the scrub pauses | In large selections, widths visibly lag during the scrub |
| Marquee and lasso | Scan item bounds; draw the marquee | — | Overlays for many selected outlines |

**Selection overlays**
- Outlines of selected paths are drawn at screen resolution.
- Below a cap of flattened segments per frame, they draw as `CursorSegment` lines.
- Above it, they render once through the kernel into an overlay cache, which moves with the transient affine.
- Anchors draw only for items in view, as one instanced batch.

### 6.8 Operations outside frames

- **Which operations:**
  - booleans and merge;
  - Offset, Convert Stroke to Shape and Simplify;
  - SVG import, and pasting large sets.
- **How they run:** as resumable jobs with a cancel token and a revision check.
  - Native hosts run them on workers. The Web host has one thread, so it runs them in time slices.
  - A result commits as one edit if the document revision still matches.
- **Booleans and offsets** get tolerance floors and output caps.
  - Both iOverlay and Clipper2 snap to integers and need curves refitted afterwards.
  - The library choice is left to V2.
- **Fill.** In V1, clicking outside a shape uses the existing GPU region fill on a Paint layer. A later vector fill region would build a planar arrangement from the flattened candidates near the click, inside a window that grows while the region touches its edge. That is geometry work, not pixel work.
- **Snapping (V2).** At gesture start, build sorted per-axis arrays of the snap points in view, then search them per move. Intersections are computed only near the pointer, never for all pairs.
- **Hover picking** pauses during pan and zoom.

### 6.9 Transforms stay baked into paths

Reports 2 and 4 recommend an affine on every path, so that moves keep caches. Reports 1 and 3 reach for the same benefit with flattened segments cached in item space. §7.2 bakes moves into the anchors instead. The comparison:

| | Bake into anchors (§7.2) | Affine on every path |
|---|---|---|
| Cost per drag frame | Same: the transient affine is tool state | Same |
| Release | Moved items re-derive, O(their anchors), visible windows first, with the moving cache shown meanwhile | Translation and rotation keep flattened caches, and so does a uniform scale with Scale Strokes on while it stays within the tolerance band (report 4). Scaling up past the band, or scaling with Scale Strokes off, re-derives anyway |
| History for moving every path | About 16 MB at the 10,000-path target | About 64 B per item |
| Meaning | One coordinate space for editing, picking and export; Scale Strokes is exact | A stroke under a skew or non-uniform scale is distorted, so those cases must bake anyway. Edit Points works in two spaces |
| Format | Unchanged | A new required property, frozen with the format |

**Decision: keep baking.**
- **When to revisit:** only if, on the low tier, releasing a move of every path at the 10,000-path target delays the next motion by more than a frame (8.3).
- **What to try first:** the engine knows the transform behind the new geometry. For rigid moves, the derived cache could adopt the old flattened segments, transformed, instead of re-flattening. That stays within the flattening tolerance and needs no change to the format.

### 6.10 Memory

**RAM**
- **Authored data** is chunks and geometry shared by identity. History charges them within its existing budget of 512 MiB and 256 entries (`crates/layer-core/src/history_budget.rs:6-7`). A move of every path charges about 16 MB at the 10,000-path target.
- **Derived data** has a byte budget with LRU eviction, and items outside visible windows go first. About 40 MB per level at the 10,000-path target.
- **Saves and autosave** cache encoded bytes per chunk identity, so they write new chunks only.
  - Packages store chunks as binary resources (§7.4), not JSON per item.
  - Copy, paste and duplicate share chunks.

**GPU memory**
- **No full-canvas vector textures.** Vector content gets none per layer. Its pixel caches are display-level windows inside `CACHE_BYTES`: regenerable, with windows that are not visible evicted first.
- **Fit view on the top tier.** One vector layer's visible windows take 241 MB at RGBA32F.
  - A gesture's split needs up to four such caches: below and above, both inside the layer and in the stack.
  - All four reach full size only when the gesture spans the whole view, as for Select All. Caches are built only for the windows the gesture reaches.
- **Command and segment buffers** are counted exactly each frame, capped, and reused from a ring.
- **No per-item GPU resources and no atlas.**

**RGBA16F display caches**
- **Gain:** they would halve this memory and its bandwidth. Arm advises against 32-bit float targets.
- **No contract at stake:** display pixels never become document data, so their precision is not part of the contract.
- **Scope:** this would be a renderer-wide change, not a vector-only one. Authored pages and exact composition stay RGBA32F.
- **Next step:** measure display composition with RGBA16F branches on the G52 and the top tier (open question 1).

### 6.11 Scheduling and responsiveness

- **Input never waits** for derivation, the GPU or saves.
- **Derivation in a frame** takes at most half the CPU budget, in this order:
  1. the gesture's items;
  2. visible windows;
  3. prefetch;
  4. everything else, at idle.

  Idle work yields to fresh input within a frame (the existing refinement rule).
- **Stale while refreshing.** Each window shows its cached or scaled result until the new one is ready. A frame never waits on derivation.
- **Long operations** run as resumable jobs (6.8).
- **Web.** The Web host runs on one thread, so derivation per frame is bounded there too. Pipeline compilation stays asynchronous, and kernels use no workgroup memory, which avoids zero-initialization stalls.
- **Open and save.** Opening renders visible windows first. Saving and autosave stay off the input path, and cost O(new chunks).

### 6.12 Panels and overlays

- **Change sets.** Each transaction reports which layer rows changed, plus a revision for each changed vector layer. Hosts refresh those rows, not the whole list. This removes the O(L²) row refresh (section 5).
- **During gestures** the layer list and Properties do not refresh on each event. Object motion already reports no changed regions while moving.
- **Properties for large selections.**
  - Mixed values come from chunk summaries when everything is selected.
  - Otherwise they come from the selected items, once per selection change, never per frame.
- **Thumbnails.**
  - Vector layer thumbnails render directly at thumbnail size through the kernel. At that level the tolerance is a few layer pixels.
  - They follow the existing throttle: at most eight requests, for visible rows only.
  - Each thumbnail is keyed by its layer's revision.
- **Item browsing** (open question 10 of the vector design) needs a virtualized projection:
  - rows computed only for the visible range;
  - subtree row counts in chunk summaries, so finding a row costs O(chunks);
  - range selection as one core operation.

  The Web list is virtualized first.

### 6.13 Later content

| Content | Rule |
|---|---|
| Groups | Isolate only groups with opacity below 1, a blend mode, a mask or effects. Fold opacity into a lone child. Intermediates are per window from a bounded pool, never a full texture per group |
| Clips | Coverage multiplied in the kernel, as an extra command, not nested layers |
| Dashes | Expanded on the CPU into the outline cache, culled to windows and capped per path |
| Gradients and patterns | Paint evaluated per pixel in the kernel from a paint table. Patterns sample tile images; no raster per instance |
| Text | Glyph runs inside one item, never one item per glyph. Glyph outlines cached per font, glyph and level. The stored glyph-run result means opening a file needs no shaping |
| Symbols and instances | One derivation per definition and level, shared by every instance. Definitions are acyclic by construction. A reverse index damages every instance when a definition changes |
| Images in vector items | Sampled from the existing image tiles, with no atlas repacking (Vello's 8192-px atlas limit is in [#1545](https://github.com/linebender/vello/issues/1545)) |
| Brush paths (V4) | Dabs generated from the cached flattened centreline; a cost class of their own |

---

## 7. Changes to the vector design

These are folded into the [vector layers record](vector-layers-research.md) with this one:

| Section there | Change |
|---|---|
| §1, Rendering | Vector layers evaluate at the display level; crisp zoom adds finer levels |
| §7.2, Undo | Records hold geometry by `Arc`; long paths keep anchors in runs; chunks carry summaries |
| §7.5, Coverage kernel | Flattening within 1/4 pixel of the evaluated level. A per-tile gather kernel without atomics, with CPU backdrops, writing each pixel once |
| §7.5, Caches and damage | Bins hold item IDs. Pixel caches are regenerable display-level windows. Damage is collected as page sets |
| §7.5, Drags and density | Gesture state lives in tool modifiers. The split by paint order and the moving cache are the plan, not a fallback. Native density only for exact work |
| §8, V0 | First, the kernel microbenchmark on the tier devices. Then the prerequisites of section 5, and the headless scaling tests |

---

## 8. Qualification

### 8.1 Headless scaling tests

These run in shared Rust, without a window.

**Generated fixtures**
- a hatching layer at 10,000 and at 50,000 strokes;
- a flats layer of 2,000 large fills;
- one path of 32,000 anchors;
- a stack of 50 vector layers.

**Work counters, not timers.** Count items visited, chunks validated, segments flattened, bins touched and records encoded for each of:
- hover;
- one frame of dragging one item, and of dragging 500;
- an anchor drag on the long path;
- a freehand point and an eraser sample;
- a marquee update;
- undo, redo and an autosave checkpoint;
- a color change on one item.

**Pass rule.** Each count must stay the same between the 10,000 and 50,000 fixtures, or grow only with what changed. These are the failure classes tldraw and Excalidraw found.

**Memory checks**
- history charges for moving every path;
- derived caches stay within budget;
- no full-canvas vector texture exists.

### 8.2 Kernel microbenchmark (first step of V0)

**Devices:** the TCL TAB 11 Gen 2 (Mali-G52), Wacom MovinkPad 11 (Mali-G57) and Wacom MovinkPad Pro 14 (Adreno 735).

**What to run:** the gather kernel over:
- scenes A and B from 3.2;
- 16-row tiles against 4-row strips;
- RGBA32F against RGBA16F output.

**Record:**
- GPU time;
- register use and spills from Arm's offline compiler;
- the CPU cost of flattening, stroking and binning both scenes.

**Pass conditions**
- At Fit, a full visible redraw of scene A finishes within four frames on each tier.
- A 500-path drag frame from 3.2 fits the GPU budget on the low tier.

If either fails, decide on the dense-tile variant and on RGBA16F before building on the kernel.

### 8.3 Device rows (V1 exit)

| Row | Pass |
|---|---|
| Pan and zoom over the 10,000-path layer, at Fit and at 100% | Tier rate |
| Drag one path, 500 paths and every path | Tier rate; after release, the next motion is delayed by at most one frame |
| Anchor drag on the 32,000-anchor path | Tier rate |
| Freehand inking above the 10,000-path layer | Tier rate and the instant-feedback latency |
| Color and width scrubs on 500 selected paths | Tier rate |
| Eraser across dense hatching | Tier rate |
| Opening the 10,000-path fixture | First frame and settle times within the [responsiveness](../performance/responsiveness.md) limits |
| A vector layer in the middle of a paint stack, while painting | No regression against a matched control without it |
| Peak memory while dragging every path | Recorded on each tier |

These extend the qualification in §8 of the vector design and the [GA workloads](object-layer-ga-design.md#143-performance-qualification).

### 8.4 Corpus

- **Fixtures** are generated by tests. No third-party art is committed.
- **Third-party SVGs** from the cited Inkscape issues serve only as local stress tests, after a licence check ([publication](../development/publication.md)). They include:
  - the 17,000-path city;
  - the 30,000-object clipped map;
  - the 2,425-label schematic.

---

## 9. Not verified

- **Mali hardware costs:** integer-atomic throughput, per-dispatch overhead, and whether rgba16float storage writes stay compressed.
- **Published renderer numbers:** none on the Mali-G52, Mali-G57 or Adreno 735 for Vello, Vello GPU, forma, Rive or Impeller.
- **CPU speeds:** flattening, stroking and binning on Cortex-A75, A76 and X4 cores.
- **Kernel arithmetic:** the scene parameters and instruction counts in 3.2.
- **Memory estimates:** the figures in 2.3 and 6.10.
- **Hatching counts** on real comic pages.
- **Browser features:** whether Chrome on Android exposes `float32-blendable`, `subgroups` and `timestamp-query` on the tier GPUs.
- **Illustrator's GPU path and Affinity's renderer:** their internals are undocumented. The vendors' speed-up claims have no published method.
- **Adobe and Corel:** claims from their community pages come from search extracts, not the full pages.

---

## 10. Open questions

1. **RGBA16F display caches.** Halving display memory and bandwidth is a renderer-wide change. Is the precision enough in Linear documents and HDR working spaces? Measure before V1.
2. **Moving cache during scale drags.** A resampled raster softens while scaling up, until release. Should small selections re-render exactly every frame instead, at the cost of re-binning?
3. **Width scrubs on large selections.** Is a visible lag in some items' widths acceptable while scrubbing, or should Properties scrub only the visible items and apply the rest at release?
4. **Dense tiles.** Does the gather kernel need the row-owned variant for scribbled hatching (8.2)?
5. **Tolerance.** Does 1/4 pixel facet visibly on large circles at 800% zoom before crisp zoom exists? Compare against the F64 reference and check by eye.
6. **Rigid-move reuse.** If the release of large moves misses its row, is adopting transformed segments (6.9) enough, or does it need parallel derivation on the big cores?
