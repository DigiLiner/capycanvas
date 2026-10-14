# Texture programs

[Developer guide](README.md) · [Paper](paper.md) · [Runtime filters](../reference/runtime-filters.md)

A texture program describes a paper or canvas surface as one WGSL function:
height in micrometers at a position in millimeters on a repeating tile. The app
bakes the function at the drawing's pixel density into a single-channel texture
that brushes read one texel per pixel. Built-in programs are entries in the existing
[filter catalog](../../assets/filters/manifest.json) with `kind: "texture"`, so
they reuse its parameter declarations, labels, WGSL modules, naga validation and
saved-value contract.

This plan defines the program contract, the `tx_` library, the bake and one
program for each of ten reference surfaces. [Paper](paper.md) covers the layer,
brushes, resolution model and library that use the result.

## Principles

- **One function.** A program maps a position to a height. Resolution, tiling,
  antialiasing, normalization and upload belong to the bake, so a program never
  sees pixels.
- **Physical units.** Positions and lengths are millimeters and the result is
  micrometers. The bake keeps the absolute relief, which is what makes a rough
  sheet rougher than a hot-pressed one under the same brush.
- **Periodic by construction.** Every library operator rounds its feature size
  to a whole number of repeats across the tile, so any composition of operators
  tiles without a seam.
- **Describe what the tip touches.** A drawing tip rides over slow undulation.
  Keep features longer than about 5 mm to a few micrometers; formation clouds
  and cockling are not tooth.
- **Only what the surfaces need.** There are no stages, buffers, image inputs or
  user programs. Each was checked against the ten surfaces below; see
  [decisions](#decisions).

## Program contract

### Catalog entry

```json
{
  "category": "paper",
  "icon": "paper",
  "program": {
    "abi": 5,
    "id": "watercolor",
    "kind": "texture",
    "label": {"message": "resources-texture-watercolor"},
    "period": 128,
    "entry": "capy_watercolor",
    "wgsl": ["texture_library.wgsl", "watercolor.wgsl"],
    "parameters": [
      {"key": "felt", "default": {"kind": "number", "value": 1.0},
       "kind": {"kind": "number", "min": 0.4, "max": 5, "step": 0.1, "decimals": 1, "unit": "mm"},
       "label": {"message": "resources-parameter-watercolor-felt"}},
      {"key": "pressing", "default": {"kind": "number", "value": 45},
       "kind": {"kind": "number", "min": 0, "max": 100, "step": 1, "decimals": 0, "unit": "%"},
       "label": {"message": "resources-parameter-watercolor-pressing"}},
      {"key": "seed", "dimension": "count", "default": {"kind": "number", "value": 1},
       "kind": {"kind": "number", "min": 0, "max": 16777215, "step": 1, "decimals": 0, "unit": ""},
       "label": {"message": "resources-parameter-texture-variation"}}
    ]
  },
  "presets": [
    {"id": "hot_press", "label": {"message": "resources-paper-hot-press"}, "values": {"felt": 2.0, "pressing": 85}},
    {"id": "cold_press", "label": {"message": "resources-paper-cold-press"}, "values": {"felt": 1.0, "pressing": 45}},
    {"id": "rough", "label": {"message": "resources-paper-rough"}, "values": {"felt": 3.0, "pressing": 5}}
  ]
}
```

| Field | Rule |
| --- | --- |
| `kind` | `"texture"`. The filter picker, Layer › New and command search list only adjustments and generators. |
| `period` | Tile edge in millimeters, 8 to 400. Choose at least five to ten repeats of the largest feature. |
| `entry`, `wgsl` | As for filters. Programs list `texture_library.wgsl` before their own module. |
| `parameters` | Number, Toggle and Choice, with `scalar` or `count` dimension. Units are display labels such as `mm`, `%` or `/cm`. A seed is a count from 0 to 16777215 labelled Variation. |
| `presets` | Entry-level `{id, label, values}` items. Omitted keys take their defaults. Presets are library items; drawings store program values, not preset IDs. |
| Rejected | `passes`, `lookups`, `auxiliary`, `time`, `resolution`, `constant_color`, pixel dimensions and image parameters. |

### Entry point

```wgsl
fn capy_watercolor(p: vec2f, b: u32) -> f32
```

- `p` is a position in millimeters inside `[0, period)²`.
- `b` addresses the parameter block. `fx_parameter(b, i).x` reads parameter `i`
  in declaration order, exactly as filters do.
- The result is height in micrometers. Larger values are peaks; the offset is
  arbitrary.
- A program is pure and deterministic. Every loop bound is a constant or
  `min(n, constant)`.
- A program may use WGSL built-ins, `fx_parameter` and the `tx_` library. It
  declares no bindings and calls no filter helper that reads an image or time.

## The `tx_` library

`assets/filters/texture_library.wgsl` holds the whole library. Sizes are
`vec2f` in millimeters so any operator can be anisotropic; write `vec2f(s)` for
an isotropic size. Seeds and salts are `u32`; operators called with different
seeds are independent.

| Function | Result |
| --- | --- |
| `tx_period() -> f32` | Tile edge in millimeters, supplied by the bake. |
| `tx_texel() -> f32` | Texel edge in millimeters for the current bake. Only thin-feature operators use it. |
| `tx_grid(size: vec2f) -> TxGrid` | A lattice with `count = max(1, round(period / size))` cells per axis and `cell = period / count`. |
| `tx_cell(g: TxGrid, p: vec2f) -> vec2i` | The unwrapped cell containing `p`. Neighbors are `c + vec2i(x, y)`. |
| `tx_random(g: TxGrid, c: vec2i, salt: u32) -> vec4f` | Four uniform numbers in `[0, 1)` for a cell, wrapped into the tile. |
| `tx_noise(p: vec2f, size: vec2f, seed: u32) -> f32` | Periodic gradient noise, roughly `[-1, 1]`, with features of `size`. |
| `tx_fbm(p: vec2f, size: vec2f, octaves: u32, seed: u32) -> f32` | Up to eight octaves of `tx_noise`, each half the previous size and amplitude, normalized to the same range. |
| `tx_warp(p: vec2f, amount: f32, size: vec2f, seed: u32) -> vec2f` | `p` plus a periodic offset of up to about `amount` millimeters. |
| `tx_cells(p: vec2f, size: vec2f, jitter: f32, seed: u32) -> vec4f` | Cellular noise: distance to the nearest and second-nearest site in cell units, and two random numbers of the nearest site. `jitter` from 0 (grid) to 1 moves the sites. |
| `tx_lines(p: vec2f, spacing: f32, spread: f32, width: f32, wobble: f32, seed: u32) -> vec2f` | Lines of constant `x`: a ridge profile from 0 to 1 and the distance in millimeters to the nearest line. `spread` places each line within a band of that fraction of the spacing; `wobble` bends lines by up to that many millimeters. Pass `p.yx` for horizontal lines. |
| `tx_weave(p: vec2f, pitch: vec2f, irregularity: f32, seed: u32) -> f32` | Plain weave from 0 to 1 with warp threads `pitch.x` apart and weft threads `pitch.y` apart. Over and under alternate at each crossing. `irregularity` from 0 to 1 varies thread position, thickness and slubs. |
| `tx_fibers(p: vec2f, len: f32, width: f32, density: f32, angle: f32, alignment: f32, curl: f32, seed: u32) -> f32` | Scattered fibers from 0 to 1. Lengths vary from 0.5 to 1.5 times `len`; `density` is fibers per square millimeter, at most 64 per cell; `alignment` runs from 0 (any direction) to 1 (all along `angle` in radians); `curl` bows each fiber by up to that fraction of its length. Fibers rise and dip along their length. |
| `tx_ellipse(q: vec2f, radii: vec2f, angle: f32) -> f32` | Approximate signed distance in millimeters from the origin-centered ellipse. |
| `tx_thread(across: f32, radius: f32) -> f32` | Round cross-section: 1 on the center line, 0 at `radius`. |
| `tx_smin(a: f32, b: f32, k: f32) -> f32`, `tx_smax(a: f32, b: f32, k: f32) -> f32` | Smooth minimum and maximum over a blend width `k`. `tx_smin(h, level, k)` is a pressing plateau. |
| `tx_disc(i: u32, n: u32) -> vec3f` | Offset of tap `i` of `n` in the unit disc and its weight `1/n`, on a Vogel spiral. |

The lattice functions are the core of the periodicity guarantee:

```wgsl
struct TxGrid {
    count: vec2i,
    cell: vec2f,
}

fn tx_hash(c: vec2i, salt: u32) -> vec4f {
    var v = vec4u(bitcast<vec2u>(c), salt, 0x9e3779b9u) * 1664525u + 1013904223u;
    v.x += v.y * v.w; v.y += v.z * v.x; v.z += v.x * v.y; v.w += v.y * v.z;
    v ^= v >> vec4u(16u);
    v.x += v.y * v.w; v.y += v.z * v.x; v.z += v.x * v.y; v.w += v.y * v.z;
    return vec4f(v >> vec4u(8u)) / 16777216.0;
}

fn tx_grid(size: vec2f) -> TxGrid {
    let count = max(vec2i(round(tx_period() / max(size, vec2f(1e-4)))), vec2i(1));
    return TxGrid(count, tx_period() / vec2f(count));
}

fn tx_cell(g: TxGrid, p: vec2f) -> vec2i {
    return vec2i(floor(p / g.cell));
}

fn tx_random(g: TxGrid, c: vec2i, salt: u32) -> vec4f {
    return tx_hash(((c % g.count) + g.count) % g.count, salt);
}
```

`tx_hash` is a PCG-style integer hash over a 4D state. Randomness comes only
from integer hashes of wrapped cell coordinates, so every GPU produces the same
layout.

### Rules that keep tiles seamless

- **Position reaches the height only through `tx_` operators**, lattice cells
  and the positions derived from them. Raw trigonometry of `p` does not tile.
- **Put alternating phases in one cell.** A whole-repeat count can be odd, so
  parity between neighboring cells does not survive the wrap. The honeycomb
  program places two staggered sites in each cell; `tx_weave` pairs threads
  internally for over and under.
- **Warp with `tx_warp`.** Its offset is periodic, so warped coordinates stay
  periodic.
- **Expect sizes to snap.** Rounding to whole repeats changes a feature by at
  most half a repeat across the tile, which five or more repeats make
  negligible.

### Neighborhood effects without stages

The ten surfaces need blur, dilation and slope only of shapes the program
already describes, so the program evaluates them directly:

- **Blur:** average a helper over `tx_disc` taps. The canvas program levels its
  weave this way to model gesso filling the hollows.
- **Dilation and covering:** offset a signed distance before shaping it. The
  handmade program drapes pulp over its inclusions this way.
- **Slope:** take central differences of a helper.

Each tap multiplies the helper's cost, so taps are used on the cheap parts of a
program.

## The bake

1. **Size.** The Paper's mapping gives drawing pixels per paper millimeter on
   each axis ([resolution model](paper.md#resolution-and-scale)). For each
   axis, `texels = round(period × pixels per millimeter)`; an imported image
   uses its physical size as the period. The period is stretched to that whole
   number of pixels, by under half a pixel per tile. Texel `i` covers document
   pixels `i + k × texels` and is evaluated at the paper position of its
   document pixel, so the mapping's offset is baked in and a brush reads the
   texel at the document pixel modulo the tile. Above 4096 texels on an axis,
   the bake uses 4096 and brushes magnify bilinearly.
2. **Probe.** The source is evaluated at 65,536 positions chosen by a fixed
   hash. The values are read back asynchronously and their 0.5th and 99.5th
   percentiles become `low` and `high`; `relief = high − low` micrometers.
3. **Bake.** Fragment draws fill an R8Unorm texture in 256 × 256 pages, one page
   group per submission, so no submission runs long. Each texel evaluates the
   source once, at its center plus a Gaussian offset drawn from a hash of the
   texel, and stores `(h − low) / (high − low)` clamped to `[0, 1]`. The offset
   is σ = 0.5 texel for programs; for images it depends on the shrink factor
   ([imported images](#imported-images)). Positions are wrapped into
   `[0, period)` first; evaluating far outside the tile loses f32 precision.
4. **Publish.** The texture and its relief replace the previous paper in the
   brush binding. Brushes scale their paper strength by `relief / 100 µm`.

A bake is a cache, not authored data. Transcendental functions may round
differently on different GPUs; the integer hashes keep the layout identical.

### One jittered sample per texel

Each texel is a true sample of the surface, so the height histogram, which is
what a dry-media threshold acts on, is the same at every resolution. Detail
finer than a texel becomes independent noise instead of moiré. The Gaussian
offset leaves under 1% of a regular pattern's alias at one cycle per texel, 6% at
0.75 and 29% at the Nyquist frequency, and costs one evaluation per texel.

`tx_lines` and `tx_fibers` draw at least one texel wide and scale their height
by true width over drawn width. A 0.03 mm fiber is 0.35 pixels wide at 300 ppi;
without that rule it would break into dots.

| Alternative | Why not |
| --- | --- |
| Point sample at the texel center | Same cost, but laid lines, honeycomb rims and fine canvas show moiré. |
| Supersample and average | Removes the noise but compresses sub-texel variation, so pencil tone changes with resolution. Costs k² evaluations. |
| Band-limit each operator | Crisper near the Nyquist frequency, but every operator and every composition must account for the footprint, and nonlinear shaping still aliases. |

### Imported images

An imported paper is a gray `capy.image/1` with a physical resolution
([library](paper.md#paper-library)). The bake reads it through a built-in
`image` program. With `m` image samples per drawing pixel:

| Shrink factor | Each texel |
| --- | --- |
| `m ≤ 1` | Bilinear interpolation, with no offset. At exactly 1, with the image grid on the pixel grid, each texel is an exact copy of one image sample. |
| `1 < m < 2` | The nearest image sample at an offset with σ = 0.5 × (m − 1) drawing pixel. |
| `m ≥ 2` | The nearest image sample at an offset with σ = 0.5 drawing pixel: the moiré-free, histogram-preserving shrink. |

A program is a continuous surface, so its offset never discards detail. An
image near 1:1 would lose exact samples to the full offset, which is why the
offset grows only as the image shrinks. An image at its own resolution in a
drawing of the same paper resolution, at Size 100%, is copied exactly; that
includes an image without a resolution in a 300 ppi drawing.

The probe and normalization are the same as for programs. An imported image
has no measured relief, so it uses the 100 µm reference.

## Validation and tests

- **Catalog load:** the same naga validation as filters, with `tx_period`,
  `tx_texel` and `fx_parameter` supplied by the bake wrapper. Rejected fields,
  periods outside 8 to 400 mm, unsupported parameter kinds and source over the
  filter size limit fail the package.
- **Built-in limits:** 4096 texels per axis; cellular operators search 3 × 3
  cells; `tx_fibers` draws at most 64 fibers per cell; `tx_fbm` stops at eight
  octaves.
- **Periodicity:** a headless GPU test evaluates two adjacent tiles without the
  offset and requires equal heights within f32 tolerance for every preset.
- **Statistics:** a fixed-value test per preset checks relief, mean and skew
  against recorded tolerances.
- **Moiré:** a Size sweep for laid, honeycomb and canvas requires blurred
  coverage to stay flat.
- **Cost:** bake time per preset at 1024², 2048² and 4096² on each tier's
  reference hardware, recorded in the [performance targets](../PERFORMANCE_TARGETS.md).

## Saved data

A Paper stores its texture as
`{"builtin": "watercolor", "version": 1, "values": {...}}` with every key, like a
built-in effect. Library operators, seeds and noise belong to each program's
data version, as `fx_noise` belongs to its filters. Texture programs are
artistic surfaces: improving a built-in's appearance keeps its ID and version,
while a change of parameter meaning needs a new version.

Custom texture programs are a development feature. They load through
`CAPY_FILTERS_DIR`, live only in private sessions and are refused by portable
save, exactly like [custom filters](../internals/session-recovery.md#private-custom-programs).

## Decisions

Each line weighs a capability against the simpler option. A capability stays
only when it improves performance, has a meaningful artistic effect, or makes
the app simpler to use.

| Question | Choice | Simpler option and why it loses, or wins |
| --- | --- | --- |
| Language | WGSL programs in the filter catalog | An embedded interpreter or a new language. Krita's SeExpr fork is GPL-3; JIT runtimes cannot run on iOS; CPU interpreters bake for seconds to minutes and would block Web's single thread; no CPU interpreter for WGSL exists. A new language adds a parser and an interpreter for a subset of what the catalog already validates. |
| Stages: fiber splatting, blur buffers, histogram passes | None | A stage graph like Material Maker's. The simpler option wins. Gathering fibers from neighboring cells with a reach test costs a few thousand operations per texel for dense kozo, under 4 ms per 2048² bake on a desktop GPU, and the surfaces' blur and dilation are expressible in the program. Revisit only if a measured Low-tier bake exceeds two seconds. |
| Normalization | Probe percentiles in the bake | Each program normalizes itself. Every parameter change would then shift the paper's contrast, and Texture strength would mean something different on each paper. The probe adds about 2% to a 2048² bake. |
| Relief | Programs return micrometers | Normalized height only, plus a separate relief setting. Hot press, cold press and rough would then feel the same under a pencil unless the artist adjusted each brush. Returning physical units costs nothing and needs no extra control. |
| Sampling | One jittered sample per texel | See [one jittered sample per texel](#one-jittered-sample-per-texel). |
| User programs | Development only | Portable user programs need a portable contract for custom code, shared with custom filters. That design is separate from Paper. |

## Reference surfaces

Eight programs cover ten surfaces; presets are listed with each. Their relief
values are estimates until the surfaces are measured ([measurements](paper.md#measurements)).
A prototype of the library and these programs compiled with naga 30, tiled
exactly and baked each 300 ppi tile in under 4 ms on an NVIDIA RTX PRO 6000;
mobile bake times are not measured.

### Hot-press, cold-press and rough watercolor

All three come off a cylinder mould and take their relief from woolen felts.
Rough is never rolled, cold press passes cold rollers and hot press passes hot
rollers, which flatten the felt peaks
([Jackson's Art](https://www.jacksonsart.com/a-guide-to-paper)). One program
models the felt as warped cells with a pressing plateau; fine cotton fibers
dominate once pressing has erased the felt.

| Parameter | Unit | Range | Default |
| --- | --- | --- | --- |
| `felt` | mm | 0.4–5 | 1.0 |
| `pressing` | % | 0–100 | 45 |
| `seed` | count | 0–16777215 | 1 |

Presets: Hot press (`felt` 2.0, `pressing` 85), Cold press (1.0, 45), Rough
(3.0, 5). Period 128 mm.

```wgsl
fn capy_watercolor(p: vec2f, b: u32) -> f32 {
    let felt = fx_parameter(b, 0u).x;
    let pressing = fx_parameter(b, 1u).x / 100.0;
    let seed = u32(fx_parameter(b, 2u).x);
    let q = tx_warp(p, 0.6 * felt, vec2f(3.0 * felt), seed);
    let cells = tx_cells(q, vec2f(felt), 0.9, seed + 1u);
    let mounds = 1.0 - cells.x * cells.x;
    let tooth = tx_fbm(p, vec2f(0.3 * felt), 3u, seed + 2u);
    let cloud = tx_fbm(p, vec2f(15.0), 3u, seed + 3u);
    let pressed = tx_smin(mounds + 0.3 * tooth + 0.15 * cloud, mix(1.3, 0.3, pressing), 0.25);
    let fibers = tx_fibers(p, 1.0, 0.02, 2.0, 0.0, 0.0, 0.4, seed + 4u);
    return mix(300.0, 15.0, pressing) * pressed + 12.0 * fibers;
}
```

### Bristol and cartridge

Bristol is a machine-finished board of pasted plies, calendered to a toothy
vellum or a smooth plate finish
([Bristol board](https://en.wikipedia.org/wiki/Bristol_board)). Machine-made
sheets align more fibers along the machine direction. The program uses a
slightly stretched fine tooth, partly aligned fibers and a calender plateau.

| Parameter | Unit | Range | Default |
| --- | --- | --- | --- |
| `tooth` | mm | 0.1–1 | 0.3 |
| `finish` | % | 0–100 | 20 |
| `seed` | count | 0–16777215 | 1 |

Presets: Bristol vellum (`tooth` 0.3, `finish` 20), Bristol smooth (0.2, 90),
Cartridge (0.35, 35). Period 64 mm.

```wgsl
fn capy_drawing(p: vec2f, b: u32) -> f32 {
    let tooth = fx_parameter(b, 0u).x;
    let finish = fx_parameter(b, 1u).x / 100.0;
    let seed = u32(fx_parameter(b, 2u).x);
    let grain = tx_fbm(p, vec2f(1.3 * tooth, tooth), 4u, seed);
    let fibers = tx_fibers(p, 2.0, 0.03, 3.0, 0.0, 0.5, 0.2, seed + 1u);
    let cloud = tx_fbm(p, vec2f(6.0), 3u, seed + 2u);
    let calendered = tx_smin(grain + 0.6 * fibers + 0.2 * cloud, mix(0.8, -0.1, finish), 0.3);
    return mix(50.0, 12.0, finish) * calendered;
}
```

### Laid Ingres

Laid paper records its mould: fine laid wires, held by chain wires that leave
thicker pulp beside them. Across 205 Rembrandt sheets the mean chain spacing is
24.8 mm, with a spread of 0.86 mm within a sheet
([Johnson et al.](https://people.ece.cornell.edu/johnson/JSEH.pdf)). Laid lines
run at roughly one per millimeter. Jittered line positions give the irregular
spacing; a whole number of chain intervals fits the 175 mm period.

| Parameter | Unit | Range | Default |
| --- | --- | --- | --- |
| `laid` | mm | 0.5–2 | 1.0 |
| `chain` | mm | 10–50 | 25 |
| `seed` | count | 0–16777215 | 1 |

Preset: Ingres. Period 175 mm.

```wgsl
fn capy_laid(p: vec2f, b: u32) -> f32 {
    let laid = fx_parameter(b, 0u).x;
    let chain = fx_parameter(b, 1u).x;
    let seed = u32(fx_parameter(b, 2u).x);
    let wires = tx_lines(p, laid, 0.04, 0.5 * laid, 0.04, seed);
    let chains = tx_lines(p.yx, chain, 0.06, 0.6, 0.15, seed + 1u);
    let shadow = exp(-pow(chains.y / 3.0, 2.0));
    let felt = tx_fbm(tx_warp(p, 0.3, vec2f(1.5), seed + 2u), vec2f(0.7), 4u, seed + 3u);
    let cloud = tx_fbm(p, vec2f(10.0), 3u, seed + 4u);
    return 16.0 * wires.x * (1.0 - chains.x) - 8.0 * chains.x + 5.0 * shadow + 9.0 * felt + 5.0 * cloud;
}
```

### Honeycomb pastel paper

Canson describes the textured side of Mi-Teintes as "a texture of crisscrossing
lines" that accepts heavy layering
([Canson](https://en.canson.com/expert-advice/selecting-your-pastel-paper));
its geometry is unpublished. The program places two staggered oval dimples in
each cell, so the stagger survives any whole number of rows, and leaves rims
between them.

| Parameter | Unit | Range | Default |
| --- | --- | --- | --- |
| `pitch` | mm | 0.5–3 | 1.1 |
| `seed` | count | 0–16777215 | 1 |

Preset: Pastel honeycomb. Period 64 mm.

```wgsl
fn capy_honeycomb(p: vec2f, b: u32) -> f32 {
    let pitch = fx_parameter(b, 0u).x;
    let seed = u32(fx_parameter(b, 1u).x);
    let g = tx_grid(vec2f(pitch, 1.7 * pitch));
    let q = tx_warp(p, 0.1 * pitch, vec2f(5.0 * pitch), seed);
    let c = tx_cell(g, q);
    var dimple = 0.0;
    for (var y = -1; y <= 1; y += 1) {
        for (var x = -1; x <= 1; x += 1) {
            let n = c + vec2i(x, y);
            let r = tx_random(g, n, seed + 1u);
            for (var k = 0; k < 2; k += 1) {
                let site = vec2f(0.25 + 0.5 * f32(k)) + 0.08 * (select(r.xy, r.zw, k == 1) - 0.5);
                let d = tx_ellipse(q - (vec2f(n) + site) * g.cell, pitch * vec2f(0.56, 0.44), 0.15 * (r.z - 0.5));
                dimple = max(dimple, smoothstep(0.0, -0.3 * pitch, d));
            }
        }
    }
    let felt = tx_fbm(p, vec2f(0.3 * pitch), 3u, seed + 2u);
    return 40.0 * (1.0 - dimple) + 6.0 * felt;
}
```

### Sanded pastel paper

Sanded papers carry grit, fiber or pumice in a binder. Grades from 240 to 800
correspond to particles from about 59 to 12 µm
([grit table](https://steeldata.info/semi/demo/help/others/grit_size_table.html)),
under a fifth of a pixel to two thirds of a pixel at 300 ppi. Each grain is a
round particle of random size; the jittered bake turns them into per-pixel
statistics.

| Parameter | Unit | Range | Default |
| --- | --- | --- | --- |
| `grit` | mm | 0.012–0.1 | 0.035 |
| `seed` | count | 0–16777215 | 1 |

Preset: Sanded pastel. Period 64 mm.

```wgsl
fn capy_sanded(p: vec2f, b: u32) -> f32 {
    let grit = fx_parameter(b, 0u).x;
    let seed = u32(fx_parameter(b, 1u).x);
    let grains = tx_cells(p, vec2f(1.5 * grit), 1.0, seed);
    let size = 0.6 + 0.8 * grains.z;
    let grain = size * tx_thread(grains.x, 0.6 * size);
    let binder = tx_fbm(p, vec2f(5.0), 4u, seed + 1u);
    return 40.0 * grain + 8.0 * binder;
}
```

### Kozo and unryu washi

Kozo fibers average 10 mm long and 25–35 µm wide
([University of Melbourne](https://cultural-conservation.unimelb.edu.au/papermulberry.html)).
Unryu adds long fibers "swirled onto the wet kozo"
([Legion Paper](https://legionpaper.com/unryu)). Fibers are gathered from
neighboring cells; the swirled strands are ordinary fibers drawn in warped
coordinates, which bends them along the warp field.

| Parameter | Unit | Range | Default |
| --- | --- | --- | --- |
| `fiber` | mm | 2–25 | 10 |
| `density` | /mm² | 0–2 | 0.4 |
| `strands` | % | 0–100 | 0 |
| `seed` | count | 0–16777215 | 1 |

Presets: Kozo (`fiber` 10, `density` 0.4, `strands` 0), Unryu (10, 0.3, 100).
Period 175 mm.

```wgsl
fn capy_washi(p: vec2f, b: u32) -> f32 {
    let fiber = fx_parameter(b, 0u).x;
    let density = fx_parameter(b, 1u).x;
    let strands = fx_parameter(b, 2u).x / 100.0;
    let seed = u32(fx_parameter(b, 3u).x);
    let cloud = tx_fbm(tx_warp(p, 2.0, vec2f(12.0), seed), vec2f(12.0), 4u, seed + 1u);
    let pulp = tx_fbm(p, vec2f(0.5), 3u, seed + 2u);
    let fibers = tx_fibers(p, fiber, 0.03, density, 0.0, 0.0, 0.5, seed + 3u);
    let swirl = tx_fibers(tx_warp(p, 6.0, vec2f(30.0), seed + 4u), 4.0 * fiber, 0.3, 0.01 * strands, 0.0, 0.0, 1.0, seed + 5u);
    return 4.0 * cloud + 10.0 * pulp + 20.0 * fibers + 30.0 * swirl;
}
```

### Handmade paper with inclusions

Inclusions are mixed into the vat or sprinkled on and covered with a thin
second layer of pulp before the sheet air-dries
([Atlas of Making](https://atlasofmaking.kenjiliu.com/papers/leaf-paper)).
Each sparse cell may hold one petal or seed; offsetting its signed distance
drapes pulp past its edge.

| Parameter | Unit | Range | Default |
| --- | --- | --- | --- |
| `amount` | % | 0–100 | 50 |
| `size` | mm | 2–20 | 6 |
| `seed` | count | 0–16777215 | 1 |

Preset: Handmade with petals. Period 175 mm.

```wgsl
fn capy_handmade(p: vec2f, b: u32) -> f32 {
    let amount = fx_parameter(b, 0u).x / 100.0;
    let size = fx_parameter(b, 1u).x;
    let seed = u32(fx_parameter(b, 2u).x);
    let cloud = tx_fbm(tx_warp(p, 1.5, vec2f(15.0), seed), vec2f(20.0), 3u, seed + 1u);
    let felt = tx_fbm(p, vec2f(0.8), 4u, seed + 2u);
    let g = tx_grid(vec2f(3.0 * size));
    let c = tx_cell(g, p);
    var lump = 0.0;
    for (var y = -1; y <= 1; y += 1) {
        for (var x = -1; x <= 1; x += 1) {
            let n = c + vec2i(x, y);
            let r = tx_random(g, n, seed + 3u);
            if r.w > amount {
                continue;
            }
            let s = tx_random(g, n, seed + 4u);
            let shape = tx_ellipse(p - (vec2f(n) + r.xy) * g.cell, size * vec2f(0.5 + 0.6 * s.x, 0.25 + 0.2 * s.y), 6.283185 * r.z);
            lump = max(lump, (0.4 + 0.6 * s.z) * smoothstep(0.5 * size, -0.2 * size, shape));
        }
    }
    let fibers = tx_fibers(p, 2.5, 0.03, 2.0, 0.0, 0.0, 0.4, seed + 5u);
    return 4.0 * cloud + 12.0 * felt + 40.0 * lump + 10.0 * fibers;
}
```

### Linen and cotton canvas

Measured canvases run from 10 to about 27 threads per centimeter; the weft
varies more than the warp
([Johnson et al.](https://sethares.engr.wisc.edu/paperspdf/dsp2009.pdf)). Thin
gesso coats follow the weave and later coats level it
([Golden](https://goldenartistcolors.com/resources/black-and-white-gesso)). The
program levels the weave toward its own disc average, which fills the hollows
first.

| Parameter | Unit | Range | Default |
| --- | --- | --- | --- |
| `threads` | /cm | 8–30 | 13 |
| `irregularity` | % | 0–100 | 50 |
| `priming` | % | 0–100 | 70 |
| `seed` | count | 0–16777215 | 1 |

Presets: Primed cotton (`threads` 13, `irregularity` 50, `priming` 70), Raw
linen (16, 60, 0). Period 64 mm.

```wgsl
fn capy_canvas(p: vec2f, b: u32) -> f32 {
    let threads = fx_parameter(b, 0u).x;
    let irregularity = fx_parameter(b, 1u).x / 100.0;
    let priming = fx_parameter(b, 2u).x / 100.0;
    let seed = u32(fx_parameter(b, 3u).x);
    let pitch = vec2f(10.0 / threads, 10.0 / (0.9 * threads));
    let weave = tx_weave(p, pitch, irregularity, seed);
    var level = 0.0;
    for (var i = 0u; i < 12u; i += 1u) {
        let t = tx_disc(i, 12u);
        level += t.z * tx_weave(p + 0.7 * pitch.x * t.xy, pitch, irregularity, seed);
    }
    let ground = mix(weave, max(weave, level + 0.2), priming);
    let skin = tx_fbm(p, vec2f(0.4), 3u, seed + 1u);
    return mix(500.0, 120.0, priming) * ground + 8.0 * priming * skin;
}
```
