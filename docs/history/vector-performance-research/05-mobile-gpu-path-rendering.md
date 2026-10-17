# GPU path rendering on mobile tilers: options, constraints, and Capy's vector kernel

[Vector layer performance](../vector-performance-research.md) · source report, 2026-10-16

Written against the rendering plan in [vector layers §7.5](../vector-layers-research.md#75-rendering) at `1f8fdd621`. The design this research produced is in the [performance record](../vector-performance-research.md).

The short answer: Capy's geometry plan is sound. CPU flattening, exact signed-area coverage and fixed-point integer accumulation are reasonable choices. Four parts of the plan are risky on these tiers:

- writing the layer caches as RGBA32F;
- any per-path read-modify-write of the window buffer in memory;
- atomics that are not local to one shader core;
- a per-frame workload that grows with canvas size instead of with what moves.

On the low tier, rewriting a full 12 MP RGBA32F layer takes 192 MB. That is more than the whole 12 ms bandwidth budget of 173 MB. No full-canvas re-render fits on any tier at RGBA32F.

Labels used below: **[source]** means a cited document says it; **(inference)** or **(estimate)** means it is my reasoning or arithmetic.

---

## (a) Approach comparison

| Approach (examples) | Per-path setup | Thousands of small paths | One huge path | Mali Bifrost/Valhall, Adreno | Memory | Determinism | Complexity |
|---|---|---|---|---|---|---|---|
| **GPU compute, tile-binned** (Vello compute, Pathfinder D3D11, Skia Graphite compute atlas) | Low on the CPU; a long multi-dispatch GPU pipeline | Good on desktop GPUs; a fine shader that gathers per pixel per segment costs O(tile pixels × segments) (inference) | Good | No published Mali numbers found. Vello hit Adreno 640 slowdowns from register and buffer-aliasing problems. Graphite marks its compute strategies EXPERIMENTAL | Buffers must be sized in advance; dynamic sizing is an open problem | The per-pixel gather is deterministic | Very high |
| **Sparse strips, CPU/GPU hybrid** (Vello CPU, Vello GPU, formerly vello_hybrid) | CPU flattens, tiles into 4×4, builds strips and 8-bit alphas | CPU cost scales with segments; GPU cost scales with covered pixels | CPU-bound | Fragment shading plus fixed-function blending, no compute needed, suits tilers (inference) | Compact | Coverage is computed on the CPU; GPU blending is fixed-function | Moderate |
| **Sort-based compute** (forma GPU, Li et al. 2016) | Pixel segments, then a sort | Good on desktop | Good | forma says its GPU backend is "known to be poor" on mobile | Sort buffers | Deterministic if the sort is stable | High |
| **Stencil-then-cover** (NV_path_rendering, Impeller, Skia Ganesh and Graphite tessellation) | 2 draws per path plus a stencil reset | Draw-call heavy. Fan geometry and micro-triangles are what Arm says to avoid | Stencil overdraw grows with winding complexity | MSAA resolves on chip and is cheap. NV_path_rendering is an NVIDIA extension (inference) | MSAA attachments can be lazily allocated | Hardware raster rules | Low to moderate |
| **CPU tessellation to triangles** (Lyon; libtess2 in early Impeller; Skia's triangulator) | High CPU cost (libtess) | CPU-bound | O(n log n) on the CPU | Micro-triangles are bad for tilers | Vertex data | Deterministic on the CPU | Moderate |
| **Atlas / mask caching** (Graphite kTessellationAndSmallAtlas and kRasterAtlas) | CPU rasterizes small paths into an atlas | Good when paths repeat or stay static | Not used for large paths | Plain texture sampling | Atlas memory | Deterministic | Moderate |
| **MSAA as the AA method** | — | — | — | Arm: "Use 4x MSAA as it is not expensive". Qualcomm: 2x "practically free" | Lazily allocated | — | Low |
| **Fragment analytic coverage with raster order, PLS or atomics** (Rive) | Triangle patches tessellated on the GPU | Only non-overlapping draws batch together | Good | Raster ordering on Mali. On Qualcomm raster ordering is slow, and the atomic mode is off on Android | Per-pixel coverage plane | Fixed-point coverage | Very high, with many driver workarounds |
| **Per-pixel analytic from curve lists** (Slug, RAVG, MPVG) | Heavy preprocessing (cell lists, shortcut tree) | Not built for thousands of ordered, overlapping paths (inference) | Good | Desktop research; no mobile data found | Acceleration structures | Deterministic | High |
| **Signed distance fields** | Low | Cost per pixel grows with nearby segments without acceleration (inference) | Poor | Impeller turns its SDF path off by default on mobile | Low | Deterministic | Low for simple primitives |

### Discussion and evidence

**Vello compute**
- The compute-centric `vello` crate "remains an experimental implementation for compute-capable GPUs". Vello GPU, which "preprocesses paths on the CPU and uses the GPU for rasterization and compositing", is "intended to become the primary renderer for production GPU use cases" ([Vello README](https://github.com/linebender/vello/blob/main/README.md), [ARCHITECTURE](https://github.com/linebender/vello/blob/main/ARCHITECTURE.md)).
- Its fine shader runs 64-thread workgroups (`@workgroup_size(4, 16)`) at 4 pixels per thread. Each pixel loops over every segment in its tile with a closed-form area formula; workgroup atomics are used only in the MSAA modes; output is an `rgba8unorm` storage texture ([fine.wgsl](https://github.com/linebender/vello/blob/main/research/vello_shaders/shader/fine.wgsl)). (Inference: that makes the cost per path-tile O(pixels × segments).)
- On a Pixel 4 (Adreno 640) the fine stage took 38 ms against about 6 ms in the good case. The cause was the compiler emitting `ldib` instead of the texture-cache `isam` path, because one buffer was both read and written. The fix was to split read-only and read-write buffers ([vello#83](https://github.com/linebender/vello/issues/83)).
- GPU memory can't be allocated on demand. Every recovery strategy needs either a blocking readback, which WebGPU lacks, or advance sizing ([vello#366](https://github.com/linebender/vello/issues/366)).
- I found no published Vello results on Mali.

**Vello GPU (sparse strips)**
- Paths are "flattened, binned into tiles, and represented as sparse horizontal strips" on the CPU. The GPU backends "use vertex and fragment rendering rather than requiring compute shaders" ([ARCHITECTURE](https://github.com/linebender/vello/blob/main/ARCHITECTURE.md)).
- Strips are instanced quads. Alphas are packed 16 one-byte values per texel, with a strip height of 4 ([render.wesl](https://github.com/linebender/vello/blob/main/vello_gpu_shaders/shaders/render.wesl), [alpha.wesl](https://github.com/linebender/vello/blob/main/vello_gpu_shaders/shaders/helpers/alpha.wesl)).
- The CPU pipeline uses 4×4 tiles and a 0.25 px flattening tolerance ([Vello CPU thesis](https://ethz.ch/content/dam/ethz/special-interest/infk/inst-pls/plf-dam/documents/StudentProjects/MasterTheses/2025-Laurenz-Thesis.pdf)).
- Removing overdraw under opaque image fills gave a 30% gain on one test scene ([Linebender Dec 2025](https://linebender.org/blog/tmil-24/)).
- The project calls Vello GPU "roughly beta quality" ([Linebender 2026 Q1](https://linebender.org/blog/tmil-25/)).

**Pathfinder**
- The D3D11 fill pass gives each 16×16 alpha tile a 16×4 workgroup. Each thread walks the tile's fill list without atomics and `imageStore`s rgba8 coverage ([fill.cs.glsl](https://github.com/servo/pathfinder/blob/main/shaders/d3d11/fill.cs.glsl)).
- The project describes "tile-based occlusion culling" and a D3D9 mode (CPU tiling, hardware raster) aimed at "integrated and mobile GPUs" ([README](https://github.com/servo/pathfinder/blob/main/README.md)).

**forma**
- forma's pipeline is flatten, then pixel segments, then a sort, then paint. Its README says GPU performance is "known to be poor" on mobile, "where the CPU back-end is currently advised instead" ([forma README](https://github.com/google/forma/blob/main/README.md)).
- Li et al. work in parallel over boundary fragments and treat interiors as spans. They report a 2.5× speedup at Ultra HD on desktop hardware ([SIGGRAPH archive](https://history.siggraph.org/?p=210555)).

**Stencil-then-cover and CPU tessellation (Impeller, Skia)**
- Impeller's Android path cost was dominated by CPU tessellation: "67% `FillPathGeometry::GetPositionBuffer` -> 45% `Tessellator::Tessellate`". Adopting stencil-then-cover cut a Rive animation from 43.7 to 10.6 ms per frame on an iPhone 12 mini ([flutter#143077](https://github.com/flutter/flutter/issues/143077), [flutter#123671](https://github.com/flutter/flutter/issues/123671)).
- Impeller uses 4x MSAA "for all rendering calls"; its SDF path is off by default on mobile ([Flutter docs](https://docs.flutter.dev/perf/antialiasing)).
- Skia Graphite's production strategies are tessellation with MSAA, tessellation plus a CPU-rasterized small-path atlas, and a raster atlas. Its compute strategies are marked "EXPERIMENTAL" ([RendererProvider.h](https://skia.googlesource.com/skia/+/main/src/gpu/graphite/RendererProvider.h)).
- Skia's older CCPR accumulated signed coverage counts into a float alpha target with additive blending ([GrCCCoverageProcessor.h](https://skia.googlesource.com/skia/+/chrome/m71/src/gpu/ccpr/GrCCCoverageProcessor.h)). That is the fragment equivalent of Capy's accumulation.
- Arm warns against fans ("avoid the use of fans") and micro-triangles, and asks for "at least 10-20 fragments per primitive" ([Arm GPU Best Practices 3.4, §5.8 and §6.3](https://documentation-service.arm.com/static/67a62b17091bfc3e0a947695)).

**Rive**
- Rive tessellates "triangles around vector edges" with signed coverage and uses a "Clockwise" fill rule for feathering ([Rive blog](https://rive.app/blog/how-rive-reinvented-feathering-for-the-vectorian-era)).
- Atomic mode keeps a per-pixel R32UI holding `pathID << 17 | coverage` in 6:11 fixed point. An `atomicMax` detects the first fragment of a new path, which then resolves the previous path ([atomic_draw.glsl](https://github.com/rive-app/rive-runtime/blob/4efe82a106e0223056c97c137d1ae735aed4f0c2/renderer/src/shaders/atomic_draw.glsl)).
- That format means "the winding number breaks if a shape has more than 32 levels of self overlap" ([constants.glsl](https://github.com/rive-app/rive-runtime/blob/4efe82a106e0223056c97c137d1ae735aed4f0c2/renderer/src/shaders/constants.glsl)).
- Draws are sorted because "we can only batch non-overlapping draws" ([render_context.cpp](https://github.com/rive-app/rive-runtime/blob/4efe82a106e0223056c97c137d1ae735aed4f0c2/renderer/src/render_context.cpp)).

**Per-pixel analytic, SDF and stroking**
- Slug rasterizes a bounding box and computes coverage from glyph curves in the pixel shader ([JCGT 2017](https://jcgt.org/published/0006/02/02/paper.pdf)).
- RAVG stores per-cell primitive lists that a pixel shader interprets ([project page](https://hhoppe.com/proj/ravg/)). MPVG builds a "shortcut tree" and is tuned for wide filters ([SIGGRAPH archive](https://history.siggraph.org/?p=209956)).
- Polar Stroking and GPU stroke expansion address the stroking stage, not coverage ([arXiv 2007.00308](https://arxiv.org/abs/2007.00308), [arXiv 2405.00127](https://arxiv.org/abs/2405.00127)).
- **Inference on SDFs:** a 1 px distance ramp misjudges coverage for sub-pixel hatching and at corners. Exact area does not, so SDFs suit feathering and simple primitives, not Capy's general paths.

**Conflation artifacts**
- Any compose-each-path-by-its-coverage scheme, Capy's included, shows conflation artifacts where paths meet ([vello#49](https://github.com/linebender/vello/issues/49)).

---

## (b) Mobile and WebGPU constraints

### Mali architecture
- **Core configuration ([Arm data sheet](https://documentation-service.arm.com/static/661e91e25d66282bc2cf9cd2)):**
  - Warp width: G52 8, G57 16.
  - FP32 ops per cycle per core: G52 32/48, G57 64.
  - Pixels per cycle per core: 2 on both.
  - Load/store cache: 16 KB.
  - Tile storage: 256 bits per pixel.
  - Threads per core drop from 512/768 to 256/384 on G52, and from 1024 to 512 on G57, when a shader uses more than 32 registers.
- **Shared memory isn't on chip:** "Arm GPUs do not implement dedicated on-chip shared memory for compute shaders. The shared memory... is system RAM that is backed up by the load-store cache." Arm also says "Do not copy data from global memory to shared memory" ([Arm BP §9.3](https://documentation-service.arm.com/static/67a62b17091bfc3e0a947695)).
- **Atomics ([Arm BP §10.13](https://documentation-service.arm.com/static/67a62b17091bfc3e0a947695)):**
  - "The key performance problem with atomics is contention". Hitting the same line from different cores "requires data coherency snooping through L2".
  - "Atomics are efficient when a shader core controls the necessary cache line in its L1."
  - Arm suggests spacing atomics 64 B apart, and accumulating into a shared-memory atomic before pushing one global atomic.
  - "If better solutions that use multiple passes are available, then do not use atomics."
  - Chips and Cheese found G52 "Throughput for INT32 atomic adds isn't great" ([Chips and Cheese](https://chipsandcheese.com/p/arms-bifrost-architecture-and-the)).
- **Workgroups ([Arm BP §9.2](https://documentation-service.arm.com/static/67a62b17091bfc3e0a947695)):**
  - "Use 64 as a baseline workgroup size", and "Do not use more than 64 threads per workgroup".
  - With barriers or shared memory, the whole workgroup must be resident at once. Spilling to the stack costs load/store throughput and bandwidth.
  - "Arm GPUs currently use subgroup sizes of 16 and smaller".
  - Arm's offline compiler reports register use and spilling ([malioc guide](https://developer.arm.com/documentation/101863/latest/)).

### Compute versus fragment on tilers
- **Arm ([BP §9.1, §3.8, §8.9](https://documentation-service.arm.com/static/67a62b17091bfc3e0a947695)):**
  - Fragment shaders get tile write-back, compression and transaction elimination.
  - "Do not use compute to process images generated by fragment shading". A fragment-to-compute dependency "creates a pipeline bubble"; compute-to-fragment is "low cost".
  - The image usage flag `VK_IMAGE_USAGE_STORAGE_BIT` "stops AFBC being used".
- **Qualcomm ([Adreno best practices](https://docs.qualcomm.com/bundle/publicresource/80-78185-2/topics/mobile_best_practices.md)):**
  - "Prefer fragment shaders": compute must write its output to memory before the next kernel starts.
  - Compute shaders "almost certainly" disable UBWC.
  - Atomics between local groups are preferred over barriers.
  - "MSAAx2 is likely to be practically free."
- **Inference:** a compute rasterizer on these GPUs pays the full uncompressed byte cost for every pixel it writes. It is only competitive if each output pixel is written once.

### Formats, blending and MSAA
- **Arm on HDR ([BP §7.7](https://documentation-service.arm.com/static/67a62b17091bfc3e0a947695)):** "do not use RGBA16F unless... Alpha is a requirement". RGBA32F isn't recommended at all.
- **Arm on blending ([BP §7.9](https://documentation-service.arm.com/static/67a62b17091bfc3e0a947695)):**
  - "Valhall GPUs [have] accelerated hardware blending for FP16 and R11G11B10".
  - "Do not use blending on floating-point framebuffers". Bifrost has float blending but no acceleration.
- **Arm on MSAA ([BP §7.5](https://documentation-service.arm.com/static/67a62b17091bfc3e0a947695)):**
  - Samples stay in tile memory and resolve at write-back.
  - "Use 4x MSAA as it is not expensive".
  - An inline resolve of 4x MSAA at 1080p60 costs 500 MB/s, against 3.9 GB/s for a manual write and resolve.
- **Tile budget (inference from the 256 bits/pixel figure):** RGBA16F with 4x MSAA uses exactly 256 bits per pixel. RGBA32F with 4x MSAA does not fit.

### Driver fragility on Android
- **Rive's Vulkan backend ([render_context_vulkan_impl.cpp](https://github.com/rive-app/rive-runtime/blob/4efe82a106e0223056c97c137d1ae735aed4f0c2/renderer/src/vulkan/render_context_vulkan_impl.cpp)):**
  - Atomic mode is disabled on Android: "We will focus on stabilizing MSAA first".
  - Qualcomm: "advertises EXT_rasterization_order_attachment_access, but it's slow".
  - ARM: raster ordering "is known to work... as long as you define a subpass self-dependency".
  - Before Vulkan 1.3, Mali and PowerVR devices "crash when a single render pass is too complex", so Rive caps them at 8191 instances.
- **Rive's GL backend:** it routes feathers through an atlas on Mali-G52 under Panfrost, because of corrupted pixel local storage ([render_context_gl_impl.cpp](https://github.com/rive-app/rive-runtime/blob/4efe82a106e0223056c97c137d1ae735aed4f0c2/renderer/src/gl/render_context_gl_impl.cpp)).
- **Rive on Qualcomm:** pixel local storage is disabled "because framebuffer fetch is slow", and Rive uses 4x MSAA instead ([Rive commit](https://skia.googlesource.com/external/github.com/rive-app/rive-cpp/+/6f378ca55373219b370bb6ef4c0ee462c2f3432f)).
- **Skia:** "All Mali Job-Manager based GPUs have maxDrawIndirectCount==1" ([VulkanCaps.cpp](https://skia.googlesource.com/skia/+/main/src/gpu/graphite/vk/VulkanCaps.cpp)).
- **Impeller:** the recent Mali reports I found are crashes and corruption, not path cost ([#187237](https://github.com/flutter/flutter/issues/187237), [#192798](https://github.com/flutter/flutter/issues/192798)).
- **Inference:** plain storage buffers and 32-bit integer atomics are the least fragile option. Fragment atomics, pixel local storage and raster ordering are where vendors diverge.

### WebGPU and wgpu ([WebGPU CRD, 15 Sep 2026](https://www.w3.org/TR/webgpu/), [WGSL](https://www.w3.org/TR/WGSL/))
- **Limits:**
  - `maxComputeWorkgroupStorageSize` 16384 B.
  - `maxComputeInvocationsPerWorkgroup` 256 (128 in compatibility mode).
  - `maxComputeWorkgroupsPerDimension` 65535.
  - `maxStorageBufferBindingSize` 128 MiB; `maxBufferSize` 256 MiB.
  - `maxStorageBuffersPerShaderStage` 8 (fragment stage 4 in compatibility mode).
  - `maxStorageTexturesPerShaderStage` 4.
  - `maxColorAttachmentBytesPerSample` 32.
  - Sample count "must be either 1 or 4".
- **rgba32float:** renderable. Blendable only with `float32-blendable`, filterable only with `float32-filterable`, no multisampling. Storage is write-only or read-only in core; read-write needs `texture-formats-tier2`.
- **rgba16float:** blendable, resolve and multisampling (outside compatibility mode). Storage is write-only or read-only in core.
- **Atomics:** `atomic<T>` with "T must be either u32 or i32", in workgroup or read-write storage. The only 64-bit option is the new `atomic-vec2u-min-max` feature (min/max only).
- **Floating point:** "An implementation may reassociate operations" and "may fuse". Subnormals may be flushed to zero. So f32 compositing is not guaranteed bit-identical across devices.
- **Missing from the spec:** there is no framebuffer-fetch or pixel-local-storage feature. `subgroups` and `timestamp-query` are optional.
- **Chrome:** timestamps are quantized to 100 µs unless the developer flag is set ([Chrome docs](https://developer.chrome.com/docs/web-platform/webgpu/developer-features)). WebGPU is on by default on Android 12+ with Qualcomm or ARM GPUs since Chrome 121 ([Chrome 121](https://developer.chrome.com/blog/new-in-webgpu-121)).
- **Native wgpu:** offers `TIMESTAMP_QUERY_INSIDE_PASSES`, `SHADER_INT64_ATOMIC_ALL_OPS`, `FLOAT32_BLENDABLE` and `TEXTURE_ATOMIC` ([wgpu Features](https://docs.rs/wgpu/latest/wgpu/struct.Features.html)). None of these are portable to the web.
- **Consequences (inference):**
  - Whole-canvas RGBA32F buffers exceed the binding limits (12 MP × 16 B = 192 MB), so caches must stay per window.
  - 61 MP in 16×16 tiles is 238k tiles, which needs a 2D dispatch grid.
  - Row prefix sums need workgroup memory and barriers; subgroups can't be assumed.

---

## (c) Assessment of the Capy kernel

### Budgets per moving frame (peak bandwidth × GPU budget)

| Tier | Budget | Full-canvas cache write: RGBA32F / RGBA16F / RGBA8 | Cache memory per layer (32F / 16F) |
|---|---|---|---|
| Low (G52 MC2, 12 MP, 12 ms) | 173 MB | 192 (111%) / 96 (56%) / 48 MB | 192 / 96 MB |
| Mid (G57 MC2, 24 MP, 8 ms) | 137 MB | 384 (280%) / 192 (140%) / 96 MB | 384 / 192 MB |
| Top (Adreno 735, 61 MP, 6 ms) | 403 MB | 976 (242%) / 488 (121%) / 244 MB | 976 / 488 MB |

These are write-only figures; compositor reads come on top. Even RGBA16F can't fully rewrite a mid- or top-tier canvas within one frame. Per-frame work has to scale with what moves.

### Workload for a full low-tier re-render (my estimate)

**Assumptions:**
- G52 MC2 at about 1 GHz ([Notebookcheck, Helio G85](https://www.notebookcheck.net/Helio-G99-vs-Helio-G85-vs-Helio-G70_14649_12751_12223.247596.0.html)) with 32–48 FP32 ops per cycle per core (Arm). That is about 32–48 G lane-instructions/s peak; I assume about 30 G sustained.
- About 15 instructions per tile-pixel per path for prefix, fill rule and RGBA source-over.
- 40 segments per path, at 16 B per segment.
- About 2 atomics per segment-row crossing (the font-rs style scatter, [raster.rs](https://github.com/raphlinus/font-rs/blob/master/src/raster.rs)).

**Scenes:**
- **A:** 5,000 paths, 200k segments, about 20 M covered path-pixels, about 32 M pixel visits in 16×16 tiles.
- **B:** 50,000 hatching strokes (about 100×3 px), 2 M segments, about 20 M covered path-pixels, about 128 M pixel visits in 16×16 tiles (about 40 M with 4-row strips).

| Cost term | A (5k) | B (50k) |
|---|---|---|
| Segment upload, CPU write + GPU read | 6.4 MB | 64 MB (4.4 ms of bandwidth) |
| Scatter atomics | ~2 M | ~13–25 M → 7–25 ms at an assumed 1–2 G/s (no published Mali rate) |
| Per-path RGBA32F read-modify-write in memory (32 B per covered px) | 640 MB → 44 ms | 640 MB → 44 ms |
| Accumulator clear and read in global memory (16 B per visit) | 512 MB | 2 GB |
| ALU with tile-resident accumulation: 16×16 tiles / 4-row strips / covered px only | 16 / ~12 / 10 ms | 64 / 20 / 10 ms |
| Single output write, RGBA32F / RGBA16F | 13 / 6.7 ms | 13 / 6.7 ms |

**Reading the table (inference):**
1. A per-path read-modify-write of the window buffer is about 4× over budget. That holds even for 5k paths.
2. With accumulation kept in registers or workgroup memory and one write per pixel, scene A is marginal at RGBA16F and over budget at RGBA32F. Scene B can't re-render the whole canvas in a frame under any structure.
3. At 50k paths, the waste from coarse tiles (128 M visits for 20 M covered pixels) dominates. Strip or row granularity matters more than the atomics.
4. A rough per-frame capacity on the low tier is about 10–20 M tile-pixel visits and about 3–5 MP of dirty RGBA16F output.

**A typical edit:** dragging a 500-path group across about 2 MP of windows in scene B re-renders about 3.4 M covered path-pixels and about 21 M visits in 16×16 tiles. That is roughly 3–10 ms of ALU, 16 MB of output and about 5 MB of segments. It fits only with fine granularity.

### Risks, ranked
1. **RGBA32F caches.** They double bandwidth and memory against RGBA16F. They have no core blending, filtering or MSAA in WebGPU, and Arm advises against them. Compute writes to storage images also lose AFBC and UBWC compression.
2. **Composite-per-path traffic.** Sequential compositing is fine as long as the accumulator stays in registers across an ordered per-tile list. It is catastrophic if each path round-trips memory.
3. **Bounding-box-proportional work.** Clearing, prefix-summing and blending over each path's bounding box multiplies cost by 5–8× for hatching.
4. **Atomic contention.**
   - Self-overlapping scribbles hammer the same cells.
   - Neighbouring cells share 64 B lines; Arm suggests 64 B spacing.
   - A window accumulator (256×256 × 8 B = 512 KB) can't sit in a 16 KB L1. Atomics stay core-local only if one workgroup owns a small tile.
5. **CPU and upload.** 2 M segments per full rebuild is about 32 MB, plus the CPU time to flatten and expand them. Mobile CPU throughput is unmeasured.
6. **Fixed-point closure.** If per-row cover deltas don't cancel exactly, winding leaks along the row. Rive's 17-bit format shows the overflow trade-off.
7. **Determinism.** Integer atomics make coverage order-independent, but WGSL allows reassociation, fusion and flush-to-zero in f32 blending. Determinism holds per device, not across devices.

---

## (d) Recommended changes, simplest first

1. **Store layer caches as RGBA16F, accumulate in f32 registers, round once at the write.** This halves output bytes and memory. rgba16float is core-blendable, filterable and storage-writable in WebGPU, and Valhall accelerates FP16 blending. Check that the existing compositor's precision needs allow it.
2. **Write each output pixel exactly once.** One workgroup per tile loops over that tile's ordered command list, keeps RGBA in registers, and does one `textureStore` at the end. Never read the window buffer back between paths. Stay write-only so there is no fragment-to-compute dependency (Arm §3.8).
3. **Bin on the CPU into small tiles with ordered per-tile lists.** Capy already bins segments per window, so this extends existing work.
   - Emit (path, segment range, backdrop) per tile. A backdrop computed on the CPU (the winding entering from the left) keeps prefix sums tile-local.
   - Fully covered tiles become "solid" commands with no segments.
   - Drop commands under an opaque solid tile (the occlusion culling used by Pathfinder and Vello).
   - Start at 16×16 tiles with 64 threads and measure 8-row or 4-row strips for hatching.
4. **Keep accumulation tile-local in workgroup memory.**
   - Use one i32 per cell in the font-rs style (cover delta folded into the next cell) instead of separate area and cover arrays. 16×16 × 4 B = 1 KB, well under both WebGPU's 16 KB and Mali's L1.
   - Quantize vertices to the fixed-point grid on the CPU so row sums cancel exactly.
   - For dense path-tiles, give each thread exclusive ownership of a row of cells (gather from the tile's segments). This removes atomics and contention at the cost of more ALU.
5. **Fit the kernel to Mali.**
   - 64-thread workgroups.
   - At most 32 registers (check with malioc for spills); 2–4 pixels per thread; consider f16 for the colour math.
   - Separate read-only and read-write bindings, which is the lesson of vello#83.
   - Use `workgroupUniformLoad` for list lengths so barriers stay uniform.
6. **One dispatch per layer per frame.** Cover all dirty tiles from a tile-ID list, not one dispatch per path or per window. Use a 2D grid once tiles exceed 65,535.
7. **Shrink uploads.**
   - Cache flattened outlines per path across frames.
   - Upload tile-relative i16 fixed-point segments (8 B instead of 16 B).
   - Consider a 1/4 px tolerance (Vello CPU's 0.25). Segment count scales as tolerance^-1/2 ([Levien](https://raphlinus.github.io/graphics/curves/2019/12/23/flatten-quadbez.html)), so this means about 29% fewer segments, atomics and bytes.
8. **Bound motion cost.** Source-over is associative, so during a drag:
   - cache each static run of the ordered list as its own premultiplied texture, and re-render only the moving items;
   - the existing compositor stacks the runs;
   - do one exact re-render when the motion ends.
9. **Same-paint merging: only opaque, z-contiguous paths.** For those it changes only AA at seams, and it reduces conflation. Translucent merges change overlap colour, so don't do them.
10. **If measurements still miss the targets on G52:** move coverage to CPU sparse strips and blend 8-bit-alpha strip quads with fixed-function blending into RGBA16F. This is the Vello GPU design: on-chip blending, compressible output, no atomics. It costs more CPU and makes results depend on each GPU's blend unit.

---

## (e) What I could not verify

- Per-cycle atomic throughput on Mali Bifrost or Valhall (global, shared, same-cache-line). My 1–2 G/s figure is an assumption.
- Any Vello, Vello GPU or forma timings on Mali, or on Adreno 7xx.
- Adreno 735 ALU rate, GMEM size and wave size, and how its atomics behave.
- Whether rgba16float storage writes or render targets stay AFBC or UBWC compressed on these devices.
- Chrome-on-Android availability of `subgroups`, `timestamp-query` and `float32-blendable` on G52, G57 and Adreno 735.
- Flattening and strip throughput on Cortex-A55/A76 class CPUs.
- Per-dispatch overhead in wgpu and on Mali job-manager GPUs.
- The contents of the "Potato" design doc (it didn't render for me).
- All per-pixel instruction counts and scene parameters in (c) are my estimates. They should be replaced by a microbenchmark on the reference G52 device: one kernel, both scenes, malioc register report, Streamline counters.
