# Vector rendering performance lessons from Chromium (Blink, cc, Skia), Firefox WebRender and WebKit, for Capy vector layers

[Vector layer performance](../vector-performance-research.md) · source report, 2026-10-16

Written against the rendering plan in [vector layers §7.5](../vector-layers-research.md#75-rendering) at `1f8fdd621`. The design this research produced is in the [performance record](../vector-performance-research.md).

Scope: I read the current source on `main` (October 2026), design docs, bug threads and engineering blogs. Each claim cites the page it came from. Text marked **(inference)** is my own reasoning, not something a source says.

---

## (a) Top bottlenecks, with evidence

### 1. Antialiased complex (concave) paths are the main raster cost, so every engine routes them away from the general GPU path
- **Chrome's stated reason.** A Chrome engineer (2016): "Complex path rendering on the GPU is expensive, so Chrome has a heuristic to disable GPU path rendering when there are more than a certain number of paths and/or the paths have particular characteristics" ([chromium-discuss](https://groups.google.com/a/chromium.org/g/chromium-discuss/c/ya16IBMgsgE/m/EypvzAahBQAJ)).
- **The original veto rule.** Skia refused GPU raster when there were 5 or more antialiased concave non-hairline paths, or more than a tolerated number of non-dash path effects ([SkPictureContentInfo.cpp](https://skia.googlesource.com/skia/+/7ad2f08/src/core/SkPictureContentInfo.cpp)). Chrome's docs say pages with "many SVGs with non-convex paths" lose GPU raster ([how to get GPU rasterization](https://www.chromium.org/developers/design-documents/chromium-graphics/how-to-get-gpu-rasterization/)).
- **The current rule.** cc counts "slow paths" per layer:
  - A slow path is an antialiased, non-convex path. A hairline does not count. A fill under 64×64 px on a non-volatile path does not count either ([paint_op.cc](https://chromium.googlesource.com/chromium/src/+/main/cc/paint/paint_op.cc)).
  - At 6 or more slow paths, the layer is rastered with MSAA ([paint_op_buffer.h](https://chromium.googlesource.com/chromium/src/+/main/cc/paint/paint_op_buffer.h), [layer_tree_host_impl.cc](https://chromium.googlesource.com/chromium/src/+/main/cc/trees/layer_tree_host_impl.cc)).
  - MSAA turns off partial raster: "Partial raster doesn't support MSAA, as the MSAA resolve is unaware of clip rects" ([tile_manager.cc](https://chromium.googlesource.com/chromium/src/+/main/cc/tiles/tile_manager.cc)).
- **Ganesh fallback order.** Path renderers are tried in this order: dash-line, AA convex, AA hairline, AA linearizing convex, atlas, small path, triangulating, tessellation, then default ([PathRendererChain.cpp](https://skia.googlesource.com/skia/+/main/src/gpu/ganesh/PathRendererChain.cpp)).
  - The default and tessellation renderers only antialias with MSAA ([DefaultPathRenderer.cpp](https://skia.googlesource.com/skia/+/main/src/gpu/ganesh/ops/DefaultPathRenderer.cpp), [TessellationPathRenderer.cpp](https://skia.googlesource.com/skia/+/main/src/gpu/ganesh/ops/TessellationPathRenderer.cpp)).
  - Paths over 16,384 verbs are refused "to guard … from excessively complex paths, which may cause OOM or other stalls" ([PathRenderer.h](https://skia.googlesource.com/skia/+/main/src/gpu/ganesh/PathRenderer.h)).
  - Without MSAA, a large concave AA path falls through to the CPU software mask renderer ([SurfaceDrawContext.cpp](https://skia.googlesource.com/skia/+/main/src/gpu/ganesh/SurfaceDrawContext.cpp)).
  - The older coverage-counting renderer sent paths with more than 1000 verbs and "more vertices than pixels" to the CPU ([GrCoverageCountingPathRenderer.cpp](https://skia.googlesource.com/skia.git/+/a1ea0a96f4/src/gpu/ccpr/GrCoverageCountingPathRenderer.cpp)).
- **Graphite's view of MSAA.** Graphite uses MSAA where it can. It avoids it because of "poor performance on older integrated GPUs" and "high memory overhead on non-tiling GPUs", and then falls back to CPU path rasterization with an atlas cache ([Graphite blog](https://blog.google/chromium/introducing-skia-graphite-chromes/)).
- **Real-world case: nested clips (2025).** Canva reported large SVGs taking "> 1s" to render, "particularly when dragged or zoomed" ([crbug 410960657](https://issues.chromium.org/issues/410960657)). The fix replaced nested clip-path `saveLayer`s with geometric path intersection: "Clipping the paths will be more performant when rasterizing than using nested saveLayer()s" ([CL 6487569](https://chromium-review.googlesource.com/c/chromium/src/+/6487569)). The flag `ClipPathNestedRasterOptimization` is now `stable` ([runtime_enabled_features.json5](https://chromium.googlesource.com/chromium/src/+/main/third_party/blink/renderer/platform/runtime_enabled_features.json5)).

### 2. Strokes and dashes cost far more than fills
- **1 px vs 2 px.** Animating paths ran smoothly at 1 px stroke width but "dropped to about 12 fps with the 2px stroke width" (Chrome 39) ([crbug 41155737](https://issues.chromium.org/issues/41155737)).
- **Why 1 px is cheap.** Skia draws an antialiased stroke 1 device pixel wide or less as a hairline with scaled-down coverage ([SkDraw.cpp](https://skia.googlesource.com/skia/+/main/src/core/SkDraw.cpp)). Wider strokes are expanded into fill paths on the CPU when no GPU renderer accepts them ([SurfaceDrawContext.cpp](https://skia.googlesource.com/skia/+/main/src/gpu/ganesh/SurfaceDrawContext.cpp)).
- **Extreme widths.** The tessellator refuses strokes wider than 10,000 device px because of "an exponential explosion [of] chops and memory usage" (crbug 1266446, cited in [TessellationPathRenderer.cpp](https://skia.googlesource.com/skia/+/main/src/gpu/ganesh/ops/TessellationPathRenderer.cpp)).
- **Dashing.** One path once produced "90 million dash segments", crashing the allocator. Skia now caps dashing at 1 million segments, about 17 MB per path ([SkDashPath.cpp](https://skia.googlesource.com/skia/+/main/src/utils/SkDashPath.cpp)).

### 3. Per-element overhead in retained trees: layout, bounds, layers, hit testing
- **Layout and bounds in Blink.** `SVGContentContainer::Layout` walks every child. If any child's bounds change, it recomputes the union over all children ([svg_content_container.cc](https://chromium.googlesource.com/chromium/src/+/main/third_party/blink/renderer/core/layout/svg/svg_content_container.cc)).
- **Hit testing in Blink.**
  - It is a reverse linear walk of the children with a bounding-box prefilter ([svg_content_container.cc](https://chromium.googlesource.com/chromium/src/+/main/third_party/blink/renderer/core/layout/svg/svg_content_container.cc)).
  - Stroke hit tests build and cache a full stroked outline path ([layout_svg_shape.cc](https://chromium.googlesource.com/chromium/src/+/main/third_party/blink/renderer/core/layout/svg/layout_svg_shape.cc)).
  - Skipping the stroke bounding-box prefilter regressed `SvgHitTesting.html` by 200–225% ([codereview 1048043002](https://codereview.chromium.org/1048043002)).
- **Legacy WebKit.**
  - The legacy engine "always needs a re-layout if transform changes".
  - Animated SVG took 20–25 ms per frame on an embedded target, versus a few ms on desktop ([WPE: new SVG engine](https://wpewebkit.org/blog/05-new-svg-engine.html)).
  - The new layer-based engine (LBSE) first gave every renderer a layer. "Each layer costs memory, and worse, each layer takes part in a lot of per-frame bookkeeping." It now creates layers only when needed ([LBSE conditional layers](https://blogs.igalia.com/nzimmermann/posts/2026-07-14-lbse-conditional-layers/)).
- **Chrome before M89.** Chrome repainted SVG on every frame of an animation until Chromium 89 composited SVG animations ([Chrome blog](https://developer.chrome.com/blog/hardware-accelerated-animations)).

### 4. One change fans out to many dependents
- **Shared resources.** When a Blink SVG resource (gradient, clip, mask, pattern) changes, it calls `NotifyContentChanged` on every client. Invalidation masks dedupe repeats, and it runs cycle detection ([layout_svg_resource_container.cc](https://chromium.googlesource.com/chromium/src/+/main/third_party/blink/renderer/core/layout/svg/layout_svg_resource_container.cc)).
- **`<use>` instances.** Each instance deep-clones the referenced subtree (`cloneNode(deep=true)`) ([svg_use_element.cc](https://chromium.googlesource.com/chromium/src/+/main/third_party/blink/renderer/core/svg/svg_use_element.cc)). Changing the referenced element rebuilds every instance's shadow tree ([svg_element.cc](https://chromium.googlesource.com/chromium/src/+/main/third_party/blink/renderer/core/svg/svg_element.cc)).

### 5. Tile granularity, upload cost and memory
- **Firefox SVG goes through CPU "blobs".** Firefox rasterizes arbitrary SVG paths through the blob software fallback, "one of our most common source of performance issues" ([bug 1888089](https://bugzilla.mozilla.org/show_bug.cgi?id=1888089)).
- **256² tiles carry overhead.** "We get a lot of overhead from rasterizing blobs in many small tiles, and not that much parallelism to make up for it" ([bug 1628105](https://bugzilla.mozilla.org/show_bug.cgi?id=1628105)).
- **512² tiles did not settle it.**
  - The larger tiles improved raster and upload, but "invalidation gets coarser" ([bug 1787706](https://bugzilla.mozilla.org/show_bug.cgi?id=1787706)).
  - They regressed the SVG benchmark (tsvgx) by 21–38% and were backed out. The conclusion was that larger tiles should apply only for large screens or visible areas ([bug 1789612](https://bugzilla.mozilla.org/show_bug.cgi?id=1789612)).
  - Firefox measured 8–10 ms per blob raster call for a roughly 1000×500 SVG ([bug 1772249](https://bugzilla.mozilla.org/show_bug.cgi?id=1772249)).
- **Upload and paint latency (older Chrome).** Uploading one 256×256 texture took "3-5ms" on low-end devices. Paints of "300ms +" caused checkerboarding ([impl-side painting](https://www.chromium.org/developers/design-documents/impl-side-painting/)).
- **Chrome tile memory budgets.**
  - Android: 96 MB on low-end devices or under 2 GB RAM, otherwise 256 MB.
  - Desktop: 512 MB, scaled up to about 1152 MB on large displays and capped at a quarter of RAM ([layer_tree_settings.cc](https://chromium.googlesource.com/chromium/src/+/main/third_party/blink/renderer/platform/widget/compositing/layer_tree_settings.cc)).
- **Redrawing everything has a price.** WebRender's original "re-render everything each frame" was "too much" on most GPUs for complex pages ([picture caching post](https://mozillagfx.wordpress.com/2018/11/02/webrender-picture-caching/)). One Mac report measured 3 W with WebRender off versus 16–17 W on during some scrolls; the investigation focused on blur and blend content being redrawn every frame ([bug 1452489](https://bugzilla.mozilla.org/show_bug.cgi?id=1452489)).

---

## (b) Techniques adopted and their measured effect

### Display lists, partial raster and paint caching (Chromium)
- **Spatial index.** `DisplayItemList` builds an R-tree so that rastering a rectangle "only [replays] the ops required" ([display_item_list.h](https://chromium.googlesource.com/chromium/src/+/main/cc/paint/display_item_list.h)).
- **Partial raster.** It is on by default and rasters only the invalidated rect into the tile's previous resource. MSAA layers are excluded ([layer_tree_settings.cc](https://chromium.googlesource.com/chromium/src/+/main/third_party/blink/renderer/platform/widget/compositing/layer_tree_settings.cc), [tile_manager.cc](https://chromium.googlesource.com/chromium/src/+/main/cc/tiles/tile_manager.cc)).
- **Raster invalidation rules** ([platform paint README](https://chromium.googlesource.com/chromium/src/+/main/third_party/blink/renderer/platform/graphics/paint/README.md)):
  - Appearing or disappearing chunks invalidate their bounds.
  - A chunk moved backward in order invalidates both old and new bounds.
  - A clip-only change invalidates only the difference.
  - Changed display items get full or incremental invalidation.
- **Avoiding re-layerization.** "Repaint-only updates" and `DirectlyUpdateTransform` skip the expensive layerization step when layers are known not to change ([same README](https://chromium.googlesource.com/chromium/src/+/main/third_party/blink/renderer/platform/graphics/paint/README.md)).
- **Offscreen-layer folding.** cc folds a `SaveLayerAlpha` that wraps a single draw into that draw's alpha, and drops empty saveLayer/restore pairs ([paint_op_buffer_iterator.cc](https://chromium.googlesource.com/chromium/src/+/main/cc/paint/paint_op_buffer_iterator.cc)).
- **Retained display lists (Firefox).** Rebuilding only the changed parts and merging them cut median paint time by more than 33%, and missed 16 ms deadlines from 7.8% to 4.7%. When most of the page changes, the analysis work is wasted and the list is rebuilt from scratch ([Mozilla Hacks](https://hacks.mozilla.org/2018/06/retained-display-lists/)).

### Tiling, priorities and memory (cc)
- **Tile sizes.**
  - Software raster: about 256×256.
  - GPU raster: about "viewport width x one quarter viewport height" ([How cc Works](https://chromium.googlesource.com/chromium/src/+/main/docs/how_cc_works.md), [tile_size_calculator.cc](https://chromium.googlesource.com/chromium/src/+/main/cc/layers/tile_size_calculator.cc)).
  - Android default tiles grow to 384 or 512 on large screens ([layer_tree_settings.cc](https://chromium.googlesource.com/chromium/src/+/main/third_party/blink/renderer/platform/widget/compositing/layer_tree_settings.cc)).
- **Prefetch area.**
  - Priority bins are NOW, SOON and EVENTUALLY ([tile_priority.h](https://chromium.googlesource.com/chromium/src/+/main/cc/tiles/tile_priority.h)).
  - The interest area is 3000 px ([features.h](https://chromium.googlesource.com/chromium/src/+/main/cc/base/features.h)).
  - The "skewport" predicts 0.2 s of motion for GPU raster, up to 2000 px ([layer_tree_settings.h](https://chromium.googlesource.com/chromium/src/+/main/cc/trees/layer_tree_settings.h)).
- **Memory limits and eviction.**
  - Tiles needed now use a hard memory limit; prepaint tiles use a soft limit.
  - Tiles beyond SOON are evicted first.
  - If the hard limit is exceeded, "some content may not draw" ([tile_manager.cc](https://chromium.googlesource.com/chromium/src/+/main/cc/tiles/tile_manager.cc)).
  - The original design: "The total GPU Memory budget is then assigned in decreasing priority order" ([impl-side painting](https://www.chromium.org/developers/design-documents/impl-side-painting/)).
- **Checkerboarding versus frame rate.** Pinch gestures give smoothness priority. A default-on feature now switches to new-content priority when a scroll would otherwise checkerboard ([proxy_impl.cc](https://chromium.googlesource.com/chromium/src/+/main/cc/trees/proxy_impl.cc), [features.h](https://chromium.googlesource.com/chromium/src/+/main/cc/base/features.h)).
- **Effect of GPU raster.** Some paint workloads went "from 100ms/frame to 4-5ms/frame" ([how to get GPU rasterization](https://www.chromium.org/developers/design-documents/chromium-graphics/how-to-get-gpu-rasterization/)).
- **GPU raster is serial.** It is "limited to one worker thread at a time", and raster tasks "cannot be cancelled once they have started running" ([How cc Works](https://chromium.googlesource.com/chromium/src/+/main/docs/how_cc_works.md)).

### Raster scale during zoom: both engines use hysteresis
- **Chrome during pinch.**
  - It keeps the old raster scale and re-rasters only when the ideal/raster ratio exceeds 2, or when zooming out below the raster scale.
  - It steps by factors of 2 and snaps to an existing tiling within 1.2×.
  - It returns to the ideal scale at gesture end.
  - For `will-change: transform` content it never shrinks the raster scale, and only raises it when the content would otherwise fall below 0.25× of ideal/native ([picture_layer_impl.cc](https://chromium.googlesource.com/chromium/src/+/main/cc/layers/picture_layer_impl.cc)).
- **WebRender's low-quality pinch zoom.** It keeps the current scale until the zoom ratio reaches ≤0.5 or ≥2.0. It re-rasters only then, "to avoid bluriness or a proliferation of picture cache tiles", and picks a high-quality scale when the zoom ends ([picture.rs](https://github.com/mozilla-firefox/firefox/blob/main/gfx/wr/webrender/src/picture.rs)).
- **Low-res tiles were dropped.** They were once credited with "5-6x anecdotally" cheaper raster ([impl-side painting](https://www.chromium.org/developers/design-documents/impl-side-painting/)). cc has since removed low-res tiling ([commit 868290b](https://chromium.googlesource.com/chromium/src/+/868290b442ae97fa52aec8dd56da8b333baa9b89)).

### Layerization trade-offs
- **Blink's algorithm.** It merges paint chunks into layers. Merging is blocked by direct compositing reasons, overlap with composited content ("overlap testing") and wasted area ("sparsity"). The worst case is O(n²) ([compositing README](https://chromium.googlesource.com/chromium/src/+/main/third_party/blink/renderer/platform/graphics/compositing/README.md), [platform paint README](https://chromium.googlesource.com/chromium/src/+/main/third_party/blink/renderer/platform/graphics/paint/README.md)).
- **Measured gain from per-element layers.** Wrapping SVG elements in CSS-transformed DOM nodes gave 2–5× FPS (2014, iPad Mini) ([crmarsh](https://www.crmarsh.com/svg-performance/)).
- **Non-composited scrollers.** Chrome now shifts non-composited scrolling content at raster time through `DrawScrollingContentsOp` ([paint_op.h](https://chromium.googlesource.com/chromium/src/+/main/cc/paint/paint_op.h)) behind `RasterInducingScroll`, which is `stable` ([runtime_enabled_features.json5](https://chromium.googlesource.com/chromium/src/+/main/third_party/blink/renderer/platform/runtime_enabled_features.json5)).

### WebRender picture caching
- **Slices.** The scene is split into a few slices; more than 8 are squashed into one "to save GPU memory and compositing performance" ([picture docs](https://doc.servo.org/webrender/picture/index.html)).
- **Tile size.** Default tiles are 1024×512 in code; the doc comment says 2048×512. Scrollbars use 1024×32 or 32×1024.
- **Tile size is re-evaluated only every 120 frames**, so a picture whose size hovers near a threshold does not keep invalidating and reallocating tiles.
- **At most 4 compositor surfaces** per picture cache ([tile_cache/mod.rs](https://github.com/mozilla-firefox/firefox/blob/main/gfx/wr/webrender/src/tile_cache/mod.rs)).
- **Invalidation.** Each tile tracks its primitives, clips, images, opacity bindings and transforms. These lists are compared to the previous frame. A per-tile quadtree's dirty leaves give a scissored dirty rect, which also drives partial present. Solid-color tiles become "clear tiles" with no texture ([picture docs](https://doc.servo.org/webrender/picture/index.html)).
- **Adaptive dirty-tracking granularity.** A quadtree leaf splits, up to 3 levels, when it was dirty in more than 32 of the last 64 frames. Children merge when all of them are constantly dirty ("wasting CPU time tracking dependencies") or all are static ([quadtree.rs](https://github.com/mozilla-firefox/firefox/blob/main/gfx/wr/webrender/src/invalidation/quadtree.rs), [tile_cache/mod.rs](https://github.com/mozilla-firefox/firefox/blob/main/gfx/wr/webrender/src/tile_cache/mod.rs)).
- **Memory.**
  - Tiles not requested in the previous frame are evicted.
  - The needed rect is inflated by one tile height to keep tiles that just went offscreen.
  - The texture pool keeps at most 25% spare free textures ([picture_textures.rs](https://github.com/mozilla-firefox/firefox/blob/main/gfx/wr/webrender/src/picture_textures.rs), [tile_cache/mod.rs](https://github.com/mozilla-firefox/firefox/blob/main/gfx/wr/webrender/src/tile_cache/mod.rs)).
  - Caching all content adds texture memory ([bug 1584439](https://bugzilla.mozilla.org/show_bug.cgi?id=1584439)).
- **Invalidation-key lesson (2026).** Keys quantized to 1/4 px with `round()` let two corners that land on different device pixels share a key, leaving stale tiles. With the old rounding, 600 keys in a sweep mapped to two device pixels; the fix aligned quantization with the rasterizer's rounding ([bug 2052391](https://bugzilla.mozilla.org/show_bug.cgi?id=2052391)).

### Skia path caching and renderer choice
- **Small paths.** Paths up to 73 px (and 324 px once scaled) go to a cached atlas of 2048² bytes. The goal is "to accelerate rendering of lots of small paths that may be scaling" ([SmallPathRenderer.cpp](https://skia.googlesource.com/skia/+/main/src/gpu/ganesh/ops/SmallPathRenderer.cpp), [SmallPathAtlasMgr.cpp](https://skia.googlesource.com/skia/+/main/src/gpu/ganesh/ops/SmallPathAtlasMgr.cpp)).
- **Ganesh per-flush atlas.**
  - It takes simple fills only (no path effects), with device area ≤256² (≤128² when MSAA is available as a fallback) and width ≤1024.
  - The cache is keyed on path generation ID plus the full matrix, including translation, and is reset every flush ([AtlasPathRenderer.cpp](https://skia.googlesource.com/skia/+/main/src/gpu/ganesh/ops/AtlasPathRenderer.cpp)).
- **Software masks.**
  - The cache key requires an exact 2×2 matrix and allows 8 bits of subpixel translation.
  - Masks are cached only if more than 50% of the path is visible ([SoftwarePathRenderer.cpp](https://skia.googlesource.com/skia/+/main/src/gpu/ganesh/ops/SoftwarePathRenderer.cpp)).
- **Triangulator.** Its antialiased mode takes paths of at most 10 verbs ([TriangulatingPathRenderer.cpp](https://skia.googlesource.com/skia/+/main/src/gpu/ganesh/ops/TriangulatingPathRenderer.cpp)). On introduction it gave about 3× on the IE chalkboard demo without MSAA, and 30–40% on MotionMark "Fill Paths" ([codereview 1152733009](https://codereview.chromium.org/1152733009)).
- **Graphite.**
  - Default preference is Vello compute, then tessellation with a small-path atlas, then the raster atlas ([RendererProvider.cpp](https://skia.googlesource.com/skia/+/main/src/gpu/graphite/RendererProvider.cpp)).
  - Up to 256 small paths per flush are rasterized on the CPU "for higher quality".
  - Stencil "wedges" are used when a path has fewer than 50 verbs or an area of at most 256²; otherwise curves plus triangles ([Device.cpp](https://skia.googlesource.com/skia/+/main/src/gpu/graphite/Device.cpp)).
  - The raster atlas caches non-volatile paths: small (≤162 px) and larger cached atlases, then an uncached atlas, then a proxy cache ([RasterPathAtlas.cpp](https://skia.googlesource.com/skia/+/main/src/gpu/graphite/RasterPathAtlas.cpp)).
  - Results: almost 15% better MotionMark 1.3 on an M3 MacBook Pro. Depth testing removes overdraw. Recordings can be re-issued with a translation, which "reduce[s] the amount of GPU memory required to cache web content as tiles" ([Graphite blog](https://blog.google/chromium/introducing-skia-graphite-chromes/)).
- **MSAA surface reuse.** Allocating one MSAA surface instead of one per tile took the tiger SVG from about 40 to about 60 updates per second on some Android hardware ([codereview 1876363005](https://codereview.chromium.org/1876363005)).

### Mozilla's path-fill direction
- **Prototype comparison.** Among stencil-and-cover, MSAA tessellation, the WPF rasterizer and tiled algorithms, tiling "scales well to larger scenes" and needs no MSAA. Stencil-and-cover is fastest on simple scenes but "scales rather poorly". Tiled analytic AA "suffers from conflation artifacts"; 4× supersampling does not ([bug 1888089](https://bugzilla.mozilla.org/show_bug.cgi?id=1888089)).
- **Simple SVG as native primitives.** Firefox now renders simple SVG shapes, gradients and trivial clips as native WebRender primitives instead of blobs ([bug 2070993](https://bugzilla.mozilla.org/show_bug.cgi?id=2070993)).

---

## (c) Implications for the Capy design

### Sound, and matching what the engines converged on
- **Uploading geometry instead of pixels, with compute coverage.** Skia names GPU compute path rasterization as its direction ([Graphite blog](https://blog.google/chromium/introducing-skia-graphite-chromes/)). Mozilla's prototype found tiling scales best ([bug 1888089](https://bugzilla.mozilla.org/show_bug.cgi?id=1888089)).
- **CPU-expanded strokes filled with nonzero.** This handles self-overlap with transparency, which Ganesh could not do with GPU strokes and Graphite does with depth testing ([Device.cpp](https://skia.googlesource.com/skia/+/main/src/gpu/graphite/Device.cpp)). **(inference)**
- **Caching at native density and scaling cached windows during pinch.** This is the same hysteresis Chrome and WebRender use. When crisp zoom arrives, use power-of-two raster scales, re-raster at gesture end, and snap to an existing scale within about 1.2×.
- **Immutable `Arc` chunks.** Diffing becomes cheap: unchanged chunks compare equal by pointer. This avoids WebRender's per-frame dependency comparison cost. **(inference)**

### Risky: change or measure early (all inference, with arithmetic shown)
1. **Storing cached windows as RGBA32F.**
   - One 256² window is 1 MiB. A full layer is about 183 MiB at 12 MP, 366 MiB at 24 MP and 931 MiB at 61 MP, per layer. For comparison, Chrome's whole Android tile budget is 96 or 256 MB.
   - Reading a 12 MP RGBA32F layer once per frame at 60 fps is 11.5 GB/s, about 80% of the Mali-G52's 14.4 GB/s.
   - **Change:** accumulate in F32 inside the kernel, store results in the compositor's existing layer format, and set one per-tier cache budget across all vector layers. Evict by viewport priority: visible, then the area ahead in the motion direction, then anything that is neither visible nor predicted (Chrome's bins). Or evict what was not visible last frame (WebRender).
2. **Compositing at native density when zoomed out.** At fit-to-screen this touches every canvas pixel. Add power-of-two downscaled density levels for zoomed-out views, as Chrome rasters at the ideal (screen) scale.
3. **Dragging a few objects over thousands.** Re-rendering old∪new dirty windows each frame costs as much as all background segments in those windows.
   - Do what Chrome's `will-change` does, without its overlap explosion: at drag start, split the affected windows into "below", "moving" and "above" caches.
   - Render the moving set once and translate it in the compositor, so drags upload one offset per frame.
   - Build below and above lazily, ahead in the drag direction (as cc's skewport predicts).
   - Re-raster at drop.
   - Fall back to full re-render when blend modes or clips couple the moving set to its neighbours.
   - Keep flattened segments in item-local space. Flattening tolerance depends on scale, not translation, so translating or rotating an item never re-flattens it.
4. **Kernel cost per item.** If the kernel clears, prefix-sums or composites the whole window once per item, 1000 small items in one window means 1000 passes over 65k pixels.
   - Limit each item's passes to the rows it covers inside the window.
   - Note that cover accumulates from the window's left edge, so a dirty sub-rect still pays for segments to its left in each row.
   - Batch every dirty window into one dispatch: Firefox's measured problem was per-tile overhead with little parallelism.
5. **Damage computation.**
   - Union the old and new bounds, inflated by stroke width, joins and miters.
   - A reorder dirties both. A colour change dirties without geometry work.
   - Snap bounds exactly as the rasterizer rounds (WebRender bug 2052391).
   - When most items change, skip the diff and rebuild (Firefox's lesson).
   - Consider WebRender-style adaptive split and merge of dirty tracking.
6. **Complexity caps.** Cap per-window segment counts and dash counts (Skia: 16,384 verbs, 1M dash segments), cull dashes to visible windows, and bound the uploads. Watch for atomic contention where segment density exceeds pixel count; the coverage-counting renderer sent such paths to the CPU.
7. **Strokes.**
   - Cache expanded outlines per (item, scale bucket).
   - Add a hairline path for widths of 1 device px or less (as Skia does).
   - Stroke-width slider scrubs re-expand on the CPU every frame and are the most likely way to exceed the 4/3/2 ms CPU budget, given the 1 px vs 2 px slowdown above.
8. **Groups, clips and opacity.**
   - Do not allocate an offscreen window per group.
   - Fold opacity into a group's only child, as cc does.
   - Clip by geometric intersection or by multiplying coverage in the kernel, not with nested layers (the 2025 Chrome fix).
9. **Hit testing and bounds.**
   - Per-chunk bounds give a two-level tree: moving one item updates its chunk and the chain above it, not every sibling (unlike Blink's O(n) union).
   - Reuse the stroke-outline cache for hit tests.
10. **Symbols (later).** Share one geometry cache per (symbol, scale) across instances, unlike `<use>` cloning. Keep a reverse index so editing a symbol can damage all its instances.
11. **Conflation seams.** Analytic coverage composited item by item leaves seams where items share an edge, as Mozilla noted. Decide early whether that is acceptable.

---

## (d) What I could not verify
- **Comment threads on issues.chromium.org.** They need JavaScript, so I read only issue descriptions. A search snippet claims crbug 410960657 involved "4,314 paths" and "111 clip paths", and that crbug 41155737 profiled time in the "edge walker". I could not confirm either.
- **Design docs not retrieved.** The RasterInducingScroll and Composite After Paint docs (Google Docs). I verified only the source and the flag status.
- **Missing numbers.**
  - No GPU-time or power figures for WebRender picture caching.
  - No dynamic-MSAA rationale or numbers.
  - No path-renderer benchmarks beyond those cited.
  - No published LBSE figures.
  - No measured Blink costs for style recalc or path-data parsing; my evidence for those is structural (source reading).
- **Shipping status.** I could not tell whether Graphite, Vello compute or Skia's sparse-strips path is enabled in Chrome on Android today.
- **WebRender tile size.** The doc comment says 2048×512 and the code says 1024×512; I used the code.
- **Out of scope for this stream.** Integer-atomic throughput and MSAA cost on Mali-G52/G57 and Adreno 735. Capy should measure these.
- **Age of sources.** Chromium's filter design doc (CPU-only SVG filters) dates from about 2014 and is likely outdated. The impl-side painting doc and the 2014 crmarsh article are also old.
