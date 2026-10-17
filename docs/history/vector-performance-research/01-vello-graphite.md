# Vello and Graphite: performance lessons for Capy vector layers

[Vector layer performance](../vector-performance-research.md) · source report, 2026-10-16

Written against the rendering plan in [vector layers §7.5](../vector-layers-research.md#75-rendering) at `1f8fdd621`. The design this research produced is in the [performance record](../vector-performance-research.md).

**State of the projects (October 2026).** Vello now ships two renderer families:

- The original compute renderer has moved under `research/` and is called "experimental". It does most of its work in compute shaders. ([README](https://github.com/linebender/vello), [ARCHITECTURE](https://github.com/linebender/vello/blob/main/ARCHITECTURE.md))
- "Sparse strips" renderers flatten and tile on the CPU and rasterize with vertex/fragment shaders. "Vello Hybrid" was renamed "Vello GPU" in 0.3.0 on 2026-10-02 ([vello_gpu changelog](https://github.com/linebender/vello/blob/main/vello_gpu/CHANGELOG.md)).

Graphite still renders by "returning an SVG to the browser" on every frame by default in browsers. Vello is opt-in on the web and used natively on desktop ([FOSDEM 2026 transcript, auto-generated](https://ftp.fau.de/fosdem/2026/ua2220/FWXQLF-graphite-2026.vtt); [2024 review](https://graphite.art/blog/year-in-review-2024-highlights-and-a-peek-at-2025/)).

---

## (a) Top bottlenecks found

### Vello compute renderer ("classic")

**1. GPU-generated intermediates of unknown size: blank or frozen frames.**
- Tile grids, segment lists and per-tile command lists (PTCL) depend on the scene. "Just changing a transform can significantly affect the number of tiles" ([vello#366](https://github.com/linebender/vello/issues/366)).
- Compute shaders cannot allocate. Blocking readback was rejected for production because it hurts UI responsiveness and accessibility. The chosen policy is "fire and forget": skip the frame on failure and reallocate next frame (#366).
- The maintainers' own trace analysis showed that failure is only detected about a frame later. The GPU keeps running doomed frames, which adds latency ([#366 comments](https://github.com/linebender/vello/issues/366)).
- The default buffer sizes are "hand picked to accommodate the vello test scenes as well as paris-30k" ([config.rs](https://github.com/linebender/vello/blob/main/research/vello_encoding/src/config.rs)):
  - 2^21 entries each for lines, segments and tiles.
  - 2^23 PTCL words.
  - Blend spill of 2^20 words, which the code comments as "4096 spills".
- Symptoms users hit:
  - More than about 130 animated tigers gives a black screen on an RTX 4080, with a buffer limit of 50,331,648 bytes. Users also report it on "a few thousand rectangles and labels" and on "135k linestrips" ([#720](https://github.com/linebender/vello/issues/720)).
  - Five nested layers at scale >1 render black on windows above 2^20 px, because the blend stack runs out of memory ([#668](https://github.com/linebender/vello/issues/668)).

**2. Estimation to avoid overflow is expensive and inaccurate.**
- The bump estimator makes encoding 1.75–2.3× slower ([#541](https://github.com/linebender/vello/issues/541)).
- In the [stroke paper](https://arxiv.org/html/2405.00127) on M1 Max, mmark-120k encoding goes from 4.28 ms to 11.59 ms with estimation.
- Estimates overshoot badly. For waves, actual usage is 10.89 MB against an estimate of 80.69 MB (same paper).
- Raph: reliable estimation "is also going to have a CPU cost" ([Zulip](https://xi.zulipchat.com/#narrow/channel/197075-vello/topic/Priorities.20and.20vision.20for.20Vello/near/494412090)).

**3. Zoom performance cliffs.**
- Zoomed out: with paris-30k shrunk to a tiny square, the `fine` stage took 567 ms on an M1 Pro. The whole scene lands in a few tiles, so there is little parallelism ([#305](https://github.com/linebender/vello/issues/305)).
- Users report about 200k PTCL commands per tile and under 20 fps ([#419](https://github.com/linebender/vello/issues/419)).
- Zoomed in: flatten memory grows without bound because flattened lines are not culled to the viewport. This caused full-system hangs on Apple machines ([#548](https://github.com/linebender/vello/issues/548)) and driver timeouts in the web demo ([#1045](https://github.com/linebender/vello/issues/1045), [#1752](https://github.com/linebender/vello/issues/1752)).

**4. Immediate mode: re-encode and re-upload every frame.**
- A user test of 10k children rotating under a parent ran at 10–20 fps on an 8th-gen i7 with integrated GPU, with a full core spent on encode/append. Patching transforms in place gave 40–60 fps ([Zulip](https://xi.zulipchat.com/#narrow/channel/197075-vello/topic/.28Showcase.29.20Vello.20Retained.2FFine.20Granular.20Mode.20WIP/near/459506834)). Even then the whole scene was still uploaded ([DJMcNab](https://xi.zulipchat.com/#narrow/channel/197075-vello/topic/.28Showcase.29.20Vello.20Retained.2FFine.20Granular.20Mode.20WIP/near/462117330)).
- Raph: the Scene API "does not give paths an id, which in turn makes it difficult to cache intermediates" ([Zulip](https://xi.zulipchat.com/#narrow/channel/197075-vello/topic/Priorities.20and.20vision.20for.20Vello/near/494257989)).
- A request for damage-region (partial) re-rendering in #366 got no answer.

**5. Mobile compilers and register pressure.**
- On Adreno 640, fine rasterization took about 6 ms versus 38 ms depending only on whether clip-write code was present, even with no clips in the scene ([#83](https://github.com/linebender/vello/issues/83)).
- The fix was to stop writing to the shared read/write buffer in that stage and to segment buffers. Raph's hypothesis: "the ubershader approach doesn't scale on this class of hardware."
- Mobile numbers:
  - Pixel 6 runs the demo at about 90 fps; an Adreno 613 at about 10 fps ([#765](https://github.com/linebender/vello/issues/765)).
  - GPU stroke expansion of waves.svg (13,308 input segments, 475,855 output lines at 0.25 px) takes about 3.5 ms on a Mali-G78 MP20, roughly 14× slower than an M1 Max ([paper](https://arxiv.org/html/2405.00127)).

**6. Driver and compiler faults that kill compute.**
- Mali, device lost: lane 0 evaluates the discarded arm of `select(…, sh[local_id.x-1])`, an out-of-bounds index into workgroup memory ([#1816](https://github.com/linebender/vello/issues/1816)). Earlier, inactive lanes reading shared memory gave black frames on Android/Vulkan ([changelog, #1637](https://github.com/linebender/vello/blob/main/research/CHANGELOG.md)).
- Adreno 7xx under Chrome WebGPU: 8 of 23 pipelines fail to build when one struct type is used both in workgroup memory and in a whole-struct storage write ([#1957](https://github.com/linebender/vello/issues/1957)).
- D3D12 with the FXC compiler: TDR hangs at about 60–90k paths. DXC renders the same scene at about 14 ms/frame ([#1991](https://github.com/linebender/vello/issues/1991)).
- Browser WebGPU: zero-initialising workgroup memory leads to first-frame stalls of 1.5–3 s on Windows, against 75 ms on an M2 Mac ([#936](https://github.com/linebender/vello/issues/936)).

**7. Images.**
- Images are copied into one atlas that grows from 1024 to 8192, is repacked and re-uploaded during render. Images larger than 8192 are silently dropped.
- An iPad Pro could not hold 120 fps with a single 1835×8192 image ([#1545](https://github.com/linebender/vello/issues/1545)).

**8. Maximum target size.** A commenter says the compute pipeline supports at most 256 bins, each bin being 16×16 tiles of 16×16 px ([#1660](https://github.com/linebender/vello/issues/1660)). Tile size is 16×16 per [config.wgsl](https://github.com/linebender/vello/blob/main/research/vello_shaders/shader/shared/config.wgsl).

### Vello sparse strips (Hybrid / Vello GPU)

**9. CPU coarse rasterization ate the GPU's advantage.**
- "The CPU overhead for doing coarse rasterization is so big that it basically diminishes any of the performance characteristics we would expect from a GPU-based renderer" ([#1759](https://github.com/linebender/vello/pull/1759)).
- Raph: "we now have pretty strong data it's expensive" ([Zulip](https://xi.zulipchat.com/#narrow/channel/197075-vello/topic/Removing.20coarse-rasterization.20based.20clipping/near/590149794)).
- Blitz rendering old.reddit.com: Hybrid 110 ms, Vello CPU 36 ms, Classic 18 ms, Skia 6 ms. After fixes, GPU time was under 2 ms and glyph rasterization on the CPU dominated ([#1380](https://github.com/linebender/vello/issues/1380)).
- Bunnymark with 10k images: Hybrid 36 ms (later 26), Classic 14 ms, Skia Ganesh 8.3 ms ([#1427](https://github.com/linebender/vello/issues/1427)).
- Hybrid is "fragment bound for scenes with many rects and CPU bound for scenes with many paths" ([Zulip](https://xi.zulipchat.com/#narrow/channel/197075-vello/topic/Early.20z.20culling.20in.20hybrid.20via.20front.20to.20back.20rendering/near/582436173)).

**10. Low-tier fragment cost is far from desktop.**
- A Samsung A05s (Adreno 610 per [SamMobile](https://www.sammobile.com/news/galaxy-a05s-snapdragon-processor-details/)) takes about 35 ms for 200 rects of 200×200; an M4 Max takes about 0.4 ms ([Zulip](https://xi.zulipchat.com/#narrow/channel/197075-vello/topic/Early.20z.20culling.20in.20hybrid.20via.20front.20to.20back.20rendering/near/582436628)).
- Image draws went from 127 ms to about 35 ms through two changes ([#1547](https://github.com/linebender/vello/pull/1547)):
  - Stop querying texture size per pixel and move per-pixel maths to the vertex shader (30–40%).
  - Strip the shader's branch on image quality (a further 2×).
- Uniform size hurt performance, and an opaque-only shader saved 30–40%. Rive's renderer beat Vello on the A05s, for example GhostTiger 10 ms versus 33 ms ([Zulip](https://xi.zulipchat.com/#narrow/channel/197075-vello/topic/Early.20z.20culling.20in.20hybrid.20via.20front.20to.20back.20rendering/near/582916385)).
- Raph: the "current ubershader model is not giving optimal performance (especially on low tier devices)" ([Zulip](https://xi.zulipchat.com/#narrow/channel/197075-vello/topic/Thoughts.20on.20specialised.20shaders/near/592866647)).

**11. CPU stroking.** The old kurbo stroker took tiger from about 5 ms to 20+ ms ([#955](https://github.com/linebender/vello/issues/955)). It was replaced by least-squares offsets ([kurbo#427](https://github.com/linebender/kurbo/pull/427)).

**12. Layer intermediates on low tier.** Rendering whole layers into atlas textures is bounded by `max_texture_dimension_2d`, and "on tile-based deferred renderers bandwidth is the bottleneck" ([Alex Gemberg](https://xi.zulipchat.com/#narrow/channel/197075-vello/topic/Removing.20coarse-rasterization.20based.20clipping/near/592884169)).

### Graphite

**13. Per-object editor overhead dominates.**
- A grid of 50 px squares became laggy after the second row and unusable after the eighth (about 160 squares): pasting a row froze the window for over 10 s. This was in 1.0.0-rc6, September 2026. The suggested workaround is procedural Repeat nodes ([#4572](https://github.com/GraphiteEditor/Graphite/issues/4572)).
- With 10,000 layers, the Svelte Layers panel takes 8 s to render ([#4627](https://github.com/GraphiteEditor/Graphite/issues/4627)).
- Maintainers recommend "<100 layers and avoid bitmap images" ([#3123](https://github.com/GraphiteEditor/Graphite/issues/3123)).
- Removing unused delegated event handlers "quadruples the performance" of creating and removing layers ([#1877](https://github.com/GraphiteEditor/Graphite/issues/1877)).

**14. Graph compilation and evaluation on every change.**
- Graph compilation "must occur after every change before it's rendered" ([Q3 2024 report](https://graphite.art/blog/graphite-progress-report-q3-2024/), [#1924](https://github.com/GraphiteEditor/Graphite/pull/1924)).
- Caching made Painted Dreams go from 85 ms to 5.5 ms; Red Dress stayed at 115 ms. Caching also broke click targets, because Monitor nodes are skipped on cache hits ([#1946](https://github.com/GraphiteEditor/Graphite/pull/1946), with regressions fixed in [#1986](https://github.com/GraphiteEditor/Graphite/pull/1986)).
- Context-dependent caching is unresolved: "How do we know if the outcome would be the same without computing the outcome?" ([#1607](https://github.com/GraphiteEditor/Graphite/issues/1607)).
- Graph execution blocks the event loop ([#1608](https://github.com/GraphiteEditor/Graphite/issues/1608)). An async refactor is in progress ([#3781](https://github.com/GraphiteEditor/Graphite/issues/3781)).

**15. Rendering pipeline.**
- Generating the SVG string costs time in Rust and across the Rust-to-JS boundary ([#608](https://github.com/GraphiteEditor/Graphite/issues/608), [#1094](https://github.com/GraphiteEditor/Graphite/issues/1094)).
- Render caching only covers view changes:
  - [#3147](https://github.com/GraphiteEditor/Graphite/pull/3147) reuses the built Vello scene or SVG string with a new final transform.
  - [#3722](https://github.com/GraphiteEditor/Graphite/pull/3722) adds a cache of 256 px tiles with a 512 MB LRU budget at 4 B/px. Its key includes zoom, rotation, time and pointer, so any zoom change throws away the whole cache ([render_cache.rs](https://github.com/GraphiteEditor/Graphite/blob/master/node-graph/nodes/gstd/src/render_cache.rs)).
- Follow-ups: bind groups recreated while zooming ([#4029](https://github.com/GraphiteEditor/Graphite/issues/4029)), a texture pool added to cope with WebGPU garbage-collection lag ([#3804](https://github.com/GraphiteEditor/Graphite/pull/3804)), and an SVG renderer memory leak on each zoom or pan ([#4722](https://github.com/GraphiteEditor/Graphite/pull/4722)).

**16. Thumbnails.** Every monitored layer's thumbnail is re-rendered as an SVG string after each graph update. Layers with `render_complexity() > 1000` are skipped ("Dense thumbnail omitted for performance"). The code notes the monitor loop is O(m·n²) ([runtime.rs](https://github.com/GraphiteEditor/Graphite/blob/master/editor/src/node_graph_executor/runtime.rs)). A move to Vello thumbnails is still open ([#2341](https://github.com/GraphiteEditor/Graphite/issues/2341)).

**17. Hit testing.** Clicks walk the layer tree linearly, with a bounding-box precheck added in [#1887](https://github.com/GraphiteEditor/Graphite/pull/1887). Clip paths are intersected with a boolean operation at hit time ([document_message_handler.rs](https://github.com/GraphiteEditor/Graphite/blob/master/editor/src/messages/portfolio/document/document_message_handler.rs)).

**18. Hidden superlinear work elsewhere.**
- SVG import: about 10 s for a test file, from O(n²) node layout ([#2515](https://github.com/GraphiteEditor/Graphite/issues/2515)).
- Alt-drag with snapping froze for seconds or minutes ([#2017](https://github.com/GraphiteEditor/Graphite/issues/2017)).
- Path-node edits were only made linear-time in October 2026 ([#4735](https://github.com/GraphiteEditor/Graphite/pull/4735), [#4736](https://github.com/GraphiteEditor/Graphite/pull/4736)).
- Moving node-graph wires to the backend took a frame from 120 ms to 20 ms zoomed in, but from 120 ms to 200 ms zoomed out ([#2795](https://github.com/GraphiteEditor/Graphite/pull/2795)).

**19. Memory and autosave.**
- Undo history stored the entire document, images included, at every step ([#847](https://github.com/GraphiteEditor/Graphite/issues/847)).
- Autosaving once per second caused hitching with raster content ([#3339](https://github.com/GraphiteEditor/Graphite/issues/3339)), and autosave serialization ran out of memory on large images ([#1895](https://github.com/GraphiteEditor/Graphite/issues/1895)).

---

## (b) Mitigations adopted and whether they worked

**Vello: skip the frame on overflow** (#366). It avoids blocking, but black and frozen frames are still being reported (#720, December 2025).

**Vello: move away from the compute renderer.** The project put its effort into sparse strips, and the compute renderer is now "experimental". Raph's view of CPU strips plus GPU painting ([Zulip](https://xi.zulipchat.com/#narrow/channel/197075-vello/topic/Priorities.20and.20vision.20for.20Vello/near/494411965)):
- It would "basically completely solve the reliability problems from exceeding buffer allocations".
- It avoids the zoomed-out cliffs and can retain rendered paths.
- It is not as efficient for highly dynamic vector content.

Two other voices:
- Olivier Faure, giving his own opinion, says Vello GPU (classic) was superseded "for good reasons", because it has memory and correctness problems current GPUs cannot solve ([Zulip](https://xi.zulipchat.com/#narrow/channel/197075-vello/topic/Linebender.202026.20roadmap.20-.20Vello/near/601357432)).
- Raph: there is "no clear path to shipping it at Canva" ([Zulip](https://xi.zulipchat.com/#narrow/channel/197075-vello/topic/Linebender.202026.20roadmap.20-.20Vello/near/601584843)).

**GPU stroke expansion** (Vello 0.2, #496; [paper](https://arxiv.org/abs/2405.00127)). It works and is fast on desktop. It is about 14× slower on Mali. The sparse-strips renderers went back to CPU stroking and needed a faster stroker.

**Removing coarse rasterization from Hybrid** ([#1759](https://github.com/linebender/vello/pull/1759)). Each layer is now drawn whole into an intermediate texture and composited. The PR reports it "greatly improves performance of layers, especially on low-tier devices"; atlases are now allocated lazily. The matching Vello CPU rewrite improved performance "all across the board" ([#1701](https://github.com/linebender/vello/pull/1701)). Trade-off: more pressure on atlas space.

**Path caching through recordings** ([#1137](https://github.com/linebender/vello/pull/1137)). It was removed in [#1611](https://github.com/linebender/vello/pull/1611):
- Laurenz: Alex and Taj concluded it "consumes too much memory", it went unused internally, and it constrained the architecture ([Zulip](https://xi.zulipchat.com/#narrow/channel/197075-vello/topic/Removing.20recordings.20from.20vello_common/near/590147396)).
- Alex disputes the memory point and measured about 10× from a naive translated replay ([Zulip](https://xi.zulipchat.com/#narrow/channel/197075-vello/topic/Removing.20recordings.20from.20vello_common/near/592180594)).
- Raph: plan for "integer translations only, for any other transform I'm pretty sure it would be hard to beat re-rendering" ([Zulip](https://xi.zulipchat.com/#narrow/channel/197075-vello/topic/Translating.20Cached.20Path.20Sparse.20Strips/near/562754277)).

**Glyph caching.** Up to 10× in Hybrid ([Zulip](https://xi.zulipchat.com/#narrow/channel/197075-vello/topic/Sparse.20Strips.20profiling/near/618234952)), but it regressed Vello CPU on a text-heavy page. A 4096² atlas is about 67 MB. Vello is moving images to external textures, since "even low-tier phones can handle a few hundreds of external texture switches" ([Zulip](https://xi.zulipchat.com/#narrow/channel/197075-vello/topic/Sparse.20Strips.20profiling/near/619110955)).

**Occlusion of opaque content.** Drawing opaque strips front to back so early-z discards hidden fragments made complex SVGs about 30% faster on a low-tier phone (proof of concept). Depth buffering is now optional (0.3.0). WebRender's opaque pass "makes a very big difference" ([Nicolas Silva](https://xi.zulipchat.com/#narrow/channel/197075-vello/topic/Early.20z.20culling.20in.20hybrid.20via.20front.20to.20back.20rendering/near/582442270)). In his own Vello-like renderer, tile occlusion prunes about 20% of the tiger's mask tiles ([Zulip](https://xi.zulipchat.com/#narrow/channel/197075-vello/topic/Translating.20Cached.20Path.20Sparse.20Strips/near/560745861)).

**Specialised shaders.** Proposed as "ubershader subsets" chosen by a per-draw bitset of features. Runtime JIT of shaders was rejected: Ganesh does it "and it's a massive source of jank" ([Zulip](https://xi.zulipchat.com/#narrow/channel/197075-vello/topic/Thoughts.20on.20specialised.20shaders/near/592893674)). Pipeline caching ([#524](https://github.com/linebender/vello/pull/524)) gave "drastically improving Android startup times" ([TMIL May 2025](https://linebender.org/blog/tmil-17/)).

**Graphite: caching.**
- Memoised nodes ([#4065](https://github.com/GraphiteEditor/Graphite/pull/4065), [#4066](https://github.com/GraphiteEditor/Graphite/pull/4066)).
- Pan/zoom caches (#3147, #3722).
- Partly worked: Red Dress did not improve, and zoom still invalidates everything. A memoisation bug (cached hash used for both identity and equality) "caused stale cached renders to be reused" ([#3646](https://github.com/GraphiteEditor/Graphite/issues/3646)).

**Graphite: boolean operations.** They went from high-overhead calls into a JS library, to a Rust port (path-bool, [#1952](https://github.com/GraphiteEditor/Graphite/pull/1952)), to [linesweeper](https://github.com/jneem/linesweeper) ([#2670](https://github.com/GraphiteEditor/Graphite/pull/2670)), which is a robust sweep-line. This was driven by robustness, and booleans are now memoised.

**Graphite: history.** Delta-based CRDT history replaced full-document snapshots ([#3646](https://github.com/GraphiteEditor/Graphite/issues/3646)).

**Graphite: still open.** Virtual scrolling for the Layers panel (#4627) and the async executor (#3781).

---

## (c) Implications for the Capy design

### Already sound

1. **Exact sizes before upload.** CPU flattening gives exact per-window segment counts, so the GPU never generates variable-size data. This sidesteps the problem behind #366, #720, #668 and the paper's Table 4. Keep it as an invariant: no GPU-side append buffers, no readback. When a window exceeds a cap, split it into sub-dispatches rather than skip the frame.
2. **Rasterizing at canvas density.** Pan and zoom never invalidate vector caches, which avoids Graphite's zoom-keyed cache and Vello's zoom cliffs. When above-native zoom arrives, expect Graphite's problem: use quantized levels and show the stale, scaled cache while re-rendering.
3. **Arc chunks with stable identity.** This fixes Graphite #847 and gives the per-path IDs Raph said Scene lacks. Key caches on held identity or a generation number, never on raw addresses or bare hashes (Graphite's MemoHash bug).
4. **Retained per-layer window caches.** This is where Graphite ended up, at finer granularity. Nicolas Silva describes the cost of losing retained identity and re-deriving it by hashing ([Zulip](https://xi.zulipchat.com/#narrow/channel/197075-vello/topic/Priorities.20and.20vision.20for.20Vello/near/500964042)).

### Risky, with suggested changes (all inference unless cited)

1. **RGBA32F bandwidth and memory.**
   - A full-coverage layer cache is 192 MB at 12 MP, 384 MB at 24 MP and 976 MB at 61 MP.
   - Mali-G52 at 14.4 GB/s moves at most about 173 MB in a 12 ms GPU budget. Re-rasterizing a full-canvas vector layer during a motion cannot fit the low tier, whatever the raster algorithm.
   - For comparison, Vello keeps blend-stack intermediates as `pack4x8unorm` ([fine.wgsl](https://github.com/linebender/vello/blob/main/research/vello_shaders/shader/fine.wgsl)) and Graphite caches at 4 B/px.
   - Suggested changes:
     - Keep item compositing in registers or workgroup memory and write each pixel once per window, as Vello's `fine` does. Do not read-modify-write global memory once per item.
     - Consider RGBA16F or sparse window caches.
     - Re-render only the windows hit by the union of the old and new dirty bounds.
     - For drags and scrubs of large items, render the moving items as a separate transformed sub-layer, at display resolution if needed, and re-rasterize at canvas density on release. Still frames are allowed to be slow.
2. **Accumulation buffer traffic.** Integer area and cover at 8 B/px is 512 KiB per 256² window. If it is cleared and accumulated per item in global memory, traffic scales with the number of items. Bound per-item work to the item's bounds within the window, or accumulate per 16×16 tile in workgroup memory. No source gave integer-atomic throughput for Mali; measure it.
3. **CPU budget for flattening and stroking.**
   - waves.svg flattens to 475k lines at 0.25 px. At 1/8 px, expect roughly 1.4× more, assuming segment count scales with 1/√tolerance.
   - Cache flattened and stroked lines per item, in item-local space, keyed by chunk identity and the scale part of the transform.
   - Translation-only drags then just re-bin cached lines. Re-flatten only when scale leaves the tolerance band.
   - Never replay authoring logs per evaluation (the Graphite Path-node lesson).
   - Clip lines to each window. Replace the parts left of the window with vertical edges at its left edge, so prefix sums stay inside the window. Vello found its cross-tile "backdrop" stage was "a performance bottleneck in simple scenes" ([#670](https://github.com/linebender/vello/issues/670)).
   - Keep prefix sums inside one workgroup. Single-pass decoupled look-back needs forward-progress guarantees that mobile GPUs may lack ([Zulip](https://xi.zulipchat.com/#narrow/channel/197075-vello/topic/Progress.20for.20GPU.27s.20Without.20Forward.20Progress.20Guarantees)).
4. **Many items in one window.** This is the shape of Vello's zoomed-out cliff (#305, #419). Use tight per-item bounds, skip items fully covered by later opaque items (20–30% gains reported above), and split very dense windows into several dispatches.
5. **Specialised kernels.** Variants per fill rule and paint, compiled ahead of time, with constants instead of uniforms, and spills checked with malioc. Avoid known driver traps:
   - out-of-bounds `select` arms into workgroup arrays (Mali);
   - one struct type shared between workgroup memory and whole-struct storage writes (Adreno under Chrome);
   - FXC on D3D12 (use DXC, as Graphite does in [#3515](https://github.com/GraphiteEditor/Graphite/issues/3515));
   - large workgroup memory on the Web (zero-init stalls).

   Use pipeline caches.
6. **Groups, clips and blends (future).** Allocate intermediates per window from a bounded pool. Skip isolation for groups that are plain src-over at opacity 1; Raph notes the blit can be skipped when there is no blending ([Zulip](https://xi.zulipchat.com/#narrow/channel/197075-vello/topic/Removing.20coarse-rasterization.20based.20clipping/near/592861653)). Degrade by adding passes, never by dropping the frame (#668).
7. **Images in vector items.** Sample the existing tiles directly and avoid atlas repacking (#1545, #1381).
8. **Editor scale (Graphite lessons).**
   - Virtualize the Layers panel.
   - Make thumbnails GPU downsamples of the cached windows, throttled and off the input thread.
   - Hit testing and snapping through a hierarchy of bounding boxes. The 64-record chunks are a natural box hierarchy.
   - No O(n²) layout on import.
   - Autosave and serialization never on the UI thread during motion.
9. **Responsiveness.** Never block on the GPU; Vello rejected blocking readback for exactly this reason. Re-render visible windows first and show the stale cache until new content is ready. Graphite's proposal to decouple overlays from artwork rendering points the same way ([#2314](https://github.com/GraphiteEditor/Graphite/issues/2314)).
10. **Measure on the tier devices.** A 35 ms versus 0.4 ms gap between phone and desktop means desktop numbers do not predict low-tier results.

---

## (d) What I could not verify

- Zulip charts and images (strip-based clipping, early-z, uniform-size results) and Raph's RustWeek 2025 slides. The Potato and sparse-strips Google Docs were not read.
- No published Vello numbers on Mali-G52, Mali-G57 or Adreno 735. The closest are the Mali-G78 MP20 (paper) and Adreno 610/613 (issues).
- No data on integer-atomic throughput or contention on any mobile GPU.
- The 256-bin limit (#1660) is a commenter's claim, not a maintainer's.
- No Graphite measurements comparing Vello and SVG rendering on large documents. Graphite discussion on Discord was not accessible.
- The FOSDEM quote comes from an auto-generated transcript.
- All Capy cost estimates above are my arithmetic from the tier figures, not measurements.
