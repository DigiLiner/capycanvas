# GPU capability fallbacks

[Developer guide](README.md) · [Architecture](../architecture.md) · [Performance targets](../PERFORMANCE_TARGETS.md)

Capy Canvas should start and paint on every hardware GPU that meets the WebGPU
downlevel limits. Today several optional GPU features are hard requirements, so
whole device families fail at startup. This plan gives each missing feature a
fallback. Capable GPUs keep their current path; the fallbacks are as cheap as the
feature allows and approximate only where an exact result costs too much.

## Rules for every fallback

- **Decide once, in shared Rust.** `PipelineDevice` reports what the device can
  do, from the features the device was created with. Layouts, shader assembly and
  pass planning read that report. Hosts only request the optional features their
  adapter has.
- **Capable GPUs compile today's shaders.** A fallback replaces a small shared
  WGSL helper or a pass, chosen when the shader source is assembled, rather than
  branching inside every shader.
- **One implementation per fallback.** Every caller of a capability uses the same
  helper; no per-host or per-feature copies.
- **Tested without special hardware.** Renderer tests create a device without
  the feature, which runs the fallback on any desktop GPU. No environment
  switches or test-only fields in production types.
- **Approximations are bounded.** An approximation states its error bound,
  applies only where the feature is missing, and never changes stored artwork
  formats.

## Who lacks what

| Capability | Missing on | Current behaviour |
| --- | --- | --- |
| Float32 filtering (`FLOAT32_FILTERABLE`) | Apple A12–A16 iPads (Apple's Metal feature tables grant it to Apple9 and later and only some Apple7/8 devices; an iPad mini 6 reports it missing), the iOS simulator, Adreno 6xx/7xx on Qualcomm drivers older than 512.700, Adreno 4xx/5xx, PowerVR, Samsung Xclipse | Startup fails: "This device cannot sample Float32 working tiles" |
| Float32 blending (`FLOAT32_BLENDABLE`) | Every Mali generation, including the low- and mid-tier reference tablets; Adreno 5xx | Works through the slow `portable_blend` path |
| Android shared presentation (`SharedDemandRefresh`) | Probably Android 10–13, which lack the platform swapchain extension the mode needs; `minSdk` is 29 | Startup fails: "This Android GPU driver does not support front-buffer rendering…" |
| Read-write Float32 storage (`TEXTURE_ADAPTER_SPECIFIC_FORMAT_FEATURES`) | Unknown; possibly older D3D12 GPUs | Android and Windows request the feature without checking the format, so in-place dry-media pipelines can fail validation at startup |
| Wayland mailbox presentation | Unknown | The GTK host refuses to start |
| Timestamp queries, pipeline cache | Many mobile GPUs, Metal, Web | Already optional |
| Large textures (`max_texture_dimension_2d`) | 8192 on Web and some PowerVR | Already adaptive: the canvas size is capped |

Device data comes from Apple's
[Metal feature set tables](https://developer.apple.com/metal/Metal-Feature-Set-Tables.pdf),
the [Vulkan hardware database](https://vulkan.gpuinfo.org/) and the
[Web3D survey](https://web3dsurvey.com/webgpu). Confirm each row on a device
before relying on it.

The renderer uses no subgroups, f16, texture atomics, immediates, dual-source
blending, MSAA or binding arrays, and every host already requests
`downlevel_defaults()` limits, so bind groups and pipelines are validated
against the same ceilings on every device.

## Float32 filtering

The renderer stores pages, masks and display images as `Rgba32Float` and
`R32Float` and samples them with a linear sampler in the viewport
(`present.wgsl`, `area_sample.wgsl`, `mapped_sample.wgsl`), display reduction
(`scene/scale.wgsl`), transforms (`display_resample.wgsl`), export, thumbnails
and dry-media previews. Brush tips, grain and paper are 8-bit and filter on
every device.

**Fallback: a texel sampler with the same interface.** One WGSL helper,
`sample_float(texture, sampler, uv, level)`, replaces each Float32 filtered
sample. Capable devices get `textureSampleLevel`. Other devices get a version that
loads the four neighbouring texels with `textureLoad` and interpolates them with
clamp-to-edge addressing, and interpolates two levels for a fractional level of
detail. `PipelineDevice` chooses the source when shaders are assembled, and
Float32 bindings declare `filterable` from the same report.

- **Results:** interpolation in full Float32 precision. Hardware filtering
  quantizes its weights, so the two paths differ far below one 8-bit step. Stored
  artwork is unchanged.
- **Cost:** a filtered tap becomes four cached loads. Display reduction samples
  exact 2×2 corners, so it reads the same texels either way. The heaviest case is
  the edge of a perspective or mesh-warped layer, up to 64 trilinear taps per
  pixel.
- **If an A-series tier misses its target:** share texels between the four taps
  of an area sample (9 loads instead of 16) before considering approximations.
  Keeping display images in `Rgba16Float` on these devices would restore hardware
  filtering and halve bandwidth for display-only data, at the cost of format
  conversions where display images are copied.

## Float32 blending

`portable_blend` renders each blended draw into a scratch texture without
blending, copies the target, then composites both in a compute pass. Its cost
falls on Mali, which is most of the low and mid tiers:

- mask painting repeats a render pass, a copy and a dispatch for every dab, into
  a scratch that is four channels wide for a one-channel mask;
- scene over-draws each take their own pass and blend the whole target, not
  their clip;
- every merge allocates a uniform buffer and a bind group.

**Fallback: accumulate in 16-bit float, merge once.** `R16Float` and
`Rgba16Float` support hardware blending on every WebGPU device. A batch draws
all its dabs, or consecutive over-draws to one target, into a cleared 16-bit
scratch with hardware blending, then one compute pass merges the scratch into
the Float32 target over the batch's damage. Over, erase and max are associative,
so one merge equals blending each dab in turn:

| Mode | Scratch blend | Merge |
| --- | --- | --- |
| Paint | Over | `scratch + target × (1 − scratch.a)` |
| Erase | Over (alpha only) | `target × (1 − scratch.a)` |
| Max | Max | `max(scratch, target)` |

- **Error bound:** the scratch rounds the batch's contribution to 11 significant
  bits (relative error at most 2⁻¹¹), below one 8-bit step. The target and
  stored artwork stay Float32. Only devices without Float32 blending use it.
- **Cost:** one pass and one merge per batch instead of one per dab; masks keep
  a one-channel scratch; merges cover the damage only and reuse one
  dynamic-offset uniform buffer.

## Android presentation

When the surface has no `SharedDemandRefresh`, present through the buffered FIFO
path the canvas already uses while the camera moves (`android.rs`), instead of
failing. Pen latency rises by about one frame and ink cannot tear. Stroke
prediction continues to hide part of that latency. The alternative, raising
`minSdk` to 34, removes Android 10–13 from the Play Store; keep it only if the
buffered path cannot meet the low tier.

## Smaller fixes

- **Read-write storage:** every host requests in-place features through
  `native_tiles::native_in_place_features`, which checks the formats; the copy
  path already serves devices without them.
- **Wayland:** fall back to FIFO when mailbox is unavailable, and drop the GTK
  host's Float32 checks once both fallbacks exist.
- **Timestamps:** selection refinement sizes its chunks from GPU timestamps;
  without them, time `on_submitted_work_done` on the CPU instead of keeping the
  initial rate.
- **Memory:** the 512 MiB decoded-tile cache (`pipeline_device.rs`) and the
  256 MiB native colour cache (`raster/native_edit.rs`) are fixed. Size them from
  the device memory budget the display caches already use, after measuring on a
  4 GB tablet.

## Failures that remain

A device that lacks a hard requirement (a hardware adapter, the downlevel
limits) should be told which one, in plain words, in shared localized text.
Today:

- startup errors are raw English strings;
- the Web page tells every iOS user to update to iPadOS 26
  (`apps/layer-web/gpu.js`), including users already on 26;
- Android titles a failure during painting "Could not initialize canvas";
- the Web reports only out-of-memory after startup, so later validation errors
  reach the console only.

The edit validator (`native_tiles/validity.wgsl`) rejects any non-finite value
or alpha outside 0–1. On a Galaxy Tab S6 Lite (Mali-G68) a soft brush inside a
selection produced black regions that spread while smudging, then painting
stopped. Report which validator status fired, reproduce on the Mali reference
tablets, and make every shader that writes working pages finish with one shared
helper that maps non-finite values to zero and clamps alpha and colour to the
storage range. The validator stays as the backstop.

## Order of work

1. **Float32 filtering.** Unblocks A-series iPads and their Safari, older
   Adreno, PowerVR and Xclipse. Verify on the iOS simulator, which lacks the
   feature, and on M-series iPads for the hardware path.
2. **Diagnostics** from the previous section.
3. **Float32 blending accumulation.** Measure mask painting and composition on
   the low- and mid-tier tablets before and after.
4. **Android buffered presentation.** Verify on an Android 13 tablet.
5. **Read-write storage, Wayland, timestamps and memory.**
6. **Numeric robustness** of working-page writes.

## Not yet verified

- Which Apple7/8 iPads report Float32 filtering; only the iPad mini 6 has been
  observed without it.
- That Android 10–13 lack shared presentation; no Android 13 device has been
  tested.
- Whether any current GPU lacks read-write Float32 storage.
- The cost of each fallback on its tier's reference hardware.
- The cause of the Galaxy Tab S6 Lite failure.
