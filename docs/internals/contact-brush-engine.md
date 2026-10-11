# Contact brush engine

[Editor internals](README.md) · [Brush design decisions](../history/brushes/dry-media-contact.md)

The shared CPU engine resolves ordered input into swept contacts. The GPU evaluates
those contacts against stationary paper and deposits pigment into sparse pages.
GTK, Web, Android, Apple and Windows use the same definitions and evaluator.
Brush definitions are in [contact_presets.rs](../../crates/layer-core/src/contact_presets.rs).

## Dry media

Pencil, Pointy Pencil, Shading Pencil, Charcoal, Chalk, Pastel Block and Crayon use
[analytic relief contact](../../crates/layer-render-wgpu/src/relief.wgsl).
Their edge taper comes from missing paper contacts, with a narrow transition in
pigment transfer. Contact geometry does not multiply the result by a broad opacity
feather. Ink, paint, scumble and bristles retain their own material response.

Each dry brush has three built-in poses: upright, writing at 45°, and side contact
at 76.5°. Cubic Hermite interpolation resolves the dimensions and relief coefficients
between keys; endpoint poses clamp outside their range. Tilt azimuth rotates the
contact. Pressure changes its size through the existing size mapping. Displayed
size bounds the largest pose of each built-in dry brush.

The accepted design defines the [analytic relief](../history/brushes/dry-media-contact.md#analytic-relief)
and [contact and loading equations](../history/brushes/dry-media-contact.md#contact-and-loading).
Elliptical relief rises away from the point; beveled sticks have a flat face and
a narrow rising rim. There is no separate pressure map or support bitmap.

Zero pressure deposits nothing. Increasing pressure recruits lower paper contacts
and increases their transfer; maximum pressure need not reach every recessed area.
`S`, `W` and `Lambda` belong to the brush. The existing Texture depth control
scales the authored paper strength; zero removes height variation. Tilt changes the relief itself, including
its interaction with paper, without additional per-pose paper or bite controls.
The [decision record](../history/brushes/dry-media-contact.md) explains the constants,
controls and rejected alternatives. The pose editor is not implemented.

Paper is an original cached 1024² R8 height texture. Sampling depends on document
coordinates and authored grain scale/rotation, excluding stroke seed, tip size and
position. Existing brush assets also accept uploaded height textures. No bitmap is
generated or uploaded during a stroke.

## Swept deposition

[The generator](../../crates/layer-engine/src/brush.rs) retains input vertices when
curvature, pressure, tilt or twist changes enough. Its geometric tolerance is
0.25 document pixels or 1% of the previous minor radius, whichever is larger.
Straight steady runs advance at half the nominal diameter, at least 4 px. Pen-up
flushes the endpoint. Segmentation follows input, and prediction clones the same
state; rendering cadence does not decide where contacts are retained.

For each pixel, the dry evaluator integrates `contact*loading` over the current
segment and divides by the full geometric contact chord. Subdividing a fixed-pose
sweep therefore preserves exposure. Ellipse relief gives a quartic engagement
polynomial, whose cubic transfer has degree twelve. The evaluator clips empty and
fully engaged intervals and evaluates only the remaining polynomial moments.
A quadratic relief profile uses the shorter degree-six path. The quartic path uses
the center and three symmetric Gauss–Legendre pairs on the clipped transition
interval. Seven nodes integrate a polynomial of degree thirteen exactly; the
even/odd pair identity evaluates this degree-twelve polynomial without expanding
its square and cube. The [NIST quadrature reference](https://dlmf.nist.gov/3.5#v)
gives the exactness criterion. This is an algebraic integral, not sparse sampling
of the unclipped contact field. Blocks split at their four inner face boundaries, yielding at most five
quadratic pieces. Flat pieces have constant contact. Neither shape uses rotated stamps.

Brush recipes specialize ellipse versus block and whether any pose has quartic
relief. Fully engaged intervals return their length directly. Blocks find the
full chord by intersecting four pairs of parallel planes, then split only the
portion traversed by the current segment. Segments wholly inside the flat face
use its constant contact directly.
The CPU rotates whitened ellipse coordinates so motion follows one axis, preserving
their squared radius, and caches motion/rim constants in the GPU contact record.
Each piece computes its closest approach once and reuses it for the geometric,
empty and fully engaged interval bounds.

Fine elliptical contacts integrate the pixel-square footprint before optical
transfer. A moving contact uses the square's projection onto its normal; a tap
uses a bounded two-dimensional integral. Paper remains anchored to the output
pixel. Contacts up to four pixels in minor radius use this filtering fully,
then blend to the point evaluator at eight pixels. These fixed pixel thresholds
control antialiasing cost, independently of material softness `W`. Large brushes
specialize the filter out of their shader.

Changing poses use a local resolved profile within the engine's retained segments.
They are an approximation to a continuously deforming contact, rather than an exact
integral of arbitrary time-varying geometry. The same approximation applies during
live drawing, prediction rollback and replay.

Dose is `6*flow*color_alpha*exposure`. Source-over deposition with
`alpha=1-exp(-dose)` adds optical depth across segments and repeated passes.
Unselected, unlocked normal batches with a single pigment sum their dose before
one exponential and one blend. Erasing sums dose regardless of pigment color.
Transparent paint and paint already carrying that same pigment also avoid a
per-pixel color-space round trip; arbitrary underpaint retains the normal blend.
Changing pigment colors, selections and locked alpha retain ordered deposition.
Read/write destinations update only the dirty rectangle. Readonly destinations
first restore pixels outside that rectangle through a small copy kernel, then
evaluate contact only inside it. All readonly copies finish in a separate compute
pass before contact dispatches begin. Page jobs have distinct destinations and
stable original sources. Reduced display
previews preserve block averages and partial document edges. Tracked previews
compare final pixels, avoiding changes caused by intermediate restoration.
Shader preparation includes every reachable contact/profile/paper combination
and its actual color and antialiasing variants before a stroke.

Selections apply after this transfer. Empty contacts remain empty through subsequent
passes with the same pose and pressure. Brush opacity remains the existing canvas
opacity control.

The compact contact payload carries pressure and three relief coefficients in the
existing contact lanes. Shared dabs remain 128 bytes; renderer records remain
160 bytes and style records 288 bytes. Relief evaluation adds no texture or
per-pixel state; the readonly pass boundary does not wait on the CPU.

Dry ellipse Flow tile bounds share the evaluator's midpoint geometry. Cached
display levels reduce only their stale regions, rounded outward to complete
averaging texels; cold, shifted and streamed inputs retain their full-page path.

Dry-flow prediction can render its full-resolution pigment contribution over a
transparent GPU target, then combine it with the persistent paint. This uses the
same paper samples and integral, avoiding reads of old paint during prediction.
Fine grain remains at native resolution. Alpha lock, selections and other material
responses retain their existing destination path.

Updates submitted without presentation keep their source damage pending until
composition succeeds. Prediction can replace its own footprint without dropping
committed paint elsewhere on the same page.

Artwork masks preserve dry-media paper and relief. Temporary mask contacts fork
GPU pages from persistent coverage and retire on the next update; they cannot
accumulate into the committed mask. Scalar selection painting accumulates dry
optical coverage before applying its stroke opacity. Native mask painting includes
its paint/erase opacity in each dose, preserving segment partitioning.

## Other contact media

[The ink and paint evaluator](../../crates/layer-render-wgpu/src/contact.wgsl)
projects the sweep in the nib's metric, avoiding scalloped edges on angled nibs.
Ink retains the maximum requested coverage during a stroke and deposits only its
increment. Boundary roughness uses stationary noise. Brushed ink uses coherent
strand gaps and declining ink supply; pooling follows the transverse profile.
The [Bristle Paintbrush](../development/bristle-paintbrush.md) retains its hair fan
and paint-loading evaluator. These responses are distinct from solid dry-media
relief; a textured paintbrush is not classified as dry pigment by its texture alone.

## Verification

The renderer's `dry_media` integration suite compares elliptical and beveled-block
sweeps with independent f64 numerical integration. It checks pressure recruitment,
empty contacts, paper stationarity, tilt continuity, segmentation, prediction and
zoom. Contact preview fixtures compare live and committed color and coverage.
The `project` suite checks every contact preset through save/reopen and exact
Undo/Redo. GTK and Web journeys exercise catalog selection, sensor input and
history in both themes. Native sensor fixtures do not establish physical pen-driver
behavior.

```sh
cargo test --locked -p layer-render-wgpu --test dry_media -- --test-threads=1
cargo test --locked -p layer-render-wgpu --test project -- --test-threads=1
bash tools/performance/workspace-motion.sh gtk --native-test=native_contact_brushes
bash tools/performance/workspace-motion.sh web --contact-brushes
CAPY_CONTACT_PREVIEWS_ONLY=1 cargo run --locked --release -p layer-bench -- --brush-previews
```

Current reference-hardware results belong in the [performance tier tables](../PERFORMANCE_TARGETS.md).
