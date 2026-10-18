# Brush renderer

This document specifies the brush behavior implemented in the current
milestone. The destination-aware extension is specified in
[gpu-brush-engine.md](reference/gpu-brush-engine.md).

## Input and placement

`layer-engine` drains ordered platform samples, applies pressure calibration and
sensor mappings, and places contacts by traveled distance. Each resolved `Dab`
is a fixed-size GPU record:

| Field | Meaning |
| --- | --- |
| `center: f32x2` | Document-space contact center |
| `radii: f32x2` | Ellipse axes after size and aspect dynamics |
| `rotation: f32x2` | Precomputed cosine and sine |
| `motion: f32x2` | Document-space movement for destination-aware stages |
| `color: f32x4` | Resolved linear per-contact color and alpha |
| `flow: f32` | Resolved pigment feed; analytic dry media use one and carry Strength-scaled pressure in contact |
| `hardness: f32` | Analytic edge hardness |
| `texture_sign: f32x2` | Pre-resolved horizontal and vertical tip flips |
| `material: f32x4` | Grain depth, pull, remaining paint charge, and deform strength |

Tip identity, execution class, shared texture transforms, paint/erase mode,
layer, per-stroke Opacity ceiling, and conservative damage are stored once per contiguous batch. Brush
properties are frozen at pen-down, and the deterministic seed is mixed with the
stroke ID.

## GPU raster

`layer-render-wgpu` uploads the contiguous contacts once. One four-vertex
instanced quad bounds each rotated contact. The fragment stage either evaluates
analytic elliptical coverage or samples a filterable `R8Unorm` tip. An explicit
textured-dry pipeline handles moving/canvas grain without adding texture work to
ordinary ink. Contacts outside the ellipse are discarded.

Ordinary contact accumulation is:

```text
request = tip coverage × strength × color alpha
canvas = (1 − request) × canvas
       + request × ((1 − opacity) × stroke_start + opacity × source)
```

Paint uses premultiplied `source=(color,1)`; Erase substitutes zero. Partial
Opacity retains immutable starting paint for touched pages, allowing later
colors to replace earlier pigment within the same ceiling. Full-opacity
Flow brushes retain their deposition path without extra stroke state. Swept
dry media derive each request from their analytic optical-dose integral.

Raster is incremental. A normal display frame processes only contacts generated
from newly drained samples, applies the batch damage scissor, and keeps the
updated layer texture for later frames. Predicted contacts use replaceable
preview storage and never mutate persistent paint, stroke coverage, watercolor
wetness, reservoir state, or undo history.

## Brush range

One contact primitive supports G‑Pen, pencil, eraser, paintbrush, airbrush,
chalk, marker, scatter/spray, dual texture, blend, smudge, wet mix, and liquify.
Sensor mappings control geometry, coverage, color, material transfer, and
deformation. Dry flow configuration remains the exact zero-state fast path.
Full-opacity Uniform contact profiles, Uniform retouching, bristles and wet materials use
stroke-ID-keyed R32Float coverage. Partial-opacity pointwise paint retains one
immutable RGBA starting page per touched page; loaded wet paint
uses the spatial GPU reservoir; and smudge uses ordered backtrace advection. Watercolor uses coverage
plus same-layer pigment advection, with its edge derived live during composition
and a separate sparse R32Float wetness channel, but no reservoir or drying
clock. An optional brush-owned conductance texture triggers one bounded GPU
exchange per submitted stroke update. Internally that exchange is three coarse-to-fine GPU
stages that advance water/pigment along the gradient/curvature-derived tangent
of the scalar field. Independent wet and dry flow rates let watercolor
favor wet mixing and let a future ink preset favor capillary spread into dry
paper through the same kernel.
The optional after-stroke edge remains only for non-watercolor brushes that
request it.

## Correctness rules

- Place contacts by traveled distance, not event count.
- Interpolate angular sensors across the shortest wraparound path.
- Keep contact order unless an operation is proven order-independent.
- Clamp diameter, aspect, scatter, spacing, and mappings before loop bounds.
- Save real samples only; predicted samples remain visual-only.
- Never perform an implicit GPU readback for drawing or presentation.
- Capture drawing selection coverage per stroke. It limits stored pigment and
  water; watercolor capillary transport applies the same coverage to donor
  wetness and color mixing. Changing the selection does not rewrite old paint.
- Watercolor edges remain a live effect of the combined wet layer, not of each
  stroke's past selection. The outside band may extend beyond deposited paint.
  Use **Mask: reveal selection** for a strict final-appearance boundary; the
  layer mask clips pigment and its halo without baking or drying either field.

## Performance rules

The live path has one contiguous contact upload and one small style-table upload.
Shaders, pipelines, and samplers are prepared. Contact flags and accumulation
select the material variant for the captured stroke. Startup prepares the brush's
direct, destination, gather and reservoir recipes before input is ready;
revisiting that style reuses its compiled recipes. Full-opacity Flow contact needs
no stroke-state page. Destination brushes build small neighborhood bind groups
for touched sparse pages. Two generic material target variants plus one
watercolor-state variant
prevent an unused coverage feature from attaching and writing its state target. A single pass plan supplies the same attachment decision to page
allocation, committed rendering, and prediction. Only damaged
pixels are rasterized and recomposited. Capillary transport uses three
incommensurate scalar-field-aligned hops rather than iterating one-pixel diffusion
up to the requested distance. Its first stage synchronizes ping-pong pages;
the following stages overwrite the same scissor without additional page
copies. Export readback and completed-work waits are explicit cold or benchmark
operations.

The current measured results and exact workload are in
[gpu-raster-benchmarks.md](development/gpu-raster-benchmarks.md).
