# Liquify: proposed sessions and deformation math

[Developer guide](README.md)

This proposal replaces Liquify's raster feedback with a temporary deformation
session. All Liquify brushes share one coordinate map and one immutable source
until the artist leaves Liquify. Leaving bakes the result; returning starts a
new session. This describes proposed behavior, not the current implementation.

## Requirements and scope

Repeated dabs must not repeatedly interpolate the artwork's colors. Pen lifts,
switches between Liquify brushes and changes to their settings must preserve
the same source pixels. The initial scope includes Push, both Twirl directions,
Pinch, Expand and Crystals.

The session is automatic. It adds no Apply button, layer, saved filter or mode
setting. Shared Rust owns its lifecycle, targeting, input ordering and history;
the shared GPU renderer evaluates the deformation and pixels on every host.
The replacement removes the old per-dab color feedback path rather than keeping
it as a fallback.

This proposal chooses switching to another editing tool as the boundary, rather
than the first stroke with that tool. This starts necessary work while the
artist changes tools and avoids a separate prepared-but-uncommitted state. Its cost is real:
switching away and immediately back ends the original session and introduces
another possible resampling boundary. Automatic resumption, persistent Liquify
filters and speculative background baking are outside this proposal.

## Journeys and lifecycle

A session owns a fixed writable paint source, its immutable starting revision,
the accumulated map and the history needed to restore completed strokes.
Selecting a Liquify brush alone creates no session, document change or history
entry. The first effective dab starts the session.

| Artist action | Required behavior |
| --- | --- |
| Draw with Liquify, lift the pen, draw again | Both strokes deform the same original pixels. Pen-up completes a history step without baking. |
| Change Push to Twirl, Pinch, Expand or Crystals | Continue the session. Each new dab uses its own resolved mode and settings. Earlier dabs retain theirs. |
| Change size, strength, pressure mapping or another brush setting | Affect subsequent input without changing the accumulated result. |
| Change the working selection without changing the paint target | Keep the session; subsequent dabs use the new selection and earlier deformation stays unchanged. |
| Pan, zoom, rotate the view or temporarily sample a color | Keep the session. Sampling sees the deformed artwork. |
| Select another editing tool | Finish the current contact under normal input rules, close the session and start the bake immediately. |
| Draw with the newly selected tool before baking finishes | Capture the contact immediately; execute it after publication, against the baked source. Preserve sample order, coordinates and brush settings. |
| Return to Liquify, including while the exit bake is pending | Complete the requested exit. The next effective Liquify dab starts a new session from the published result. |
| Change the paint target or invoke an edit that changes its pixels or geometry | Resolve the session first, even when the command does not change the selected tool. Validate the queued edit against its intended target. |
| Switch documents or close the drawing | Preserve completed edits through the normal capture and close contracts. A session never follows input into another document. |
| Leave without making an effective change | End without a raster replacement or history entry. |

There are three lifecycle states: **inactive**, **active** and **baking**.
Inactive becomes active on an effective Liquify dab. Active remains active
across contacts and Liquify brush changes. A leaving action seals its final map
and enters baking. Successful atomic publication returns to inactive and
releases dependent edits in order. A later tool choice does not retract a sealed
bake or reinterpret a contact already captured for another tool.

While baking, keep the last complete deformed preview visible and keep native
input collection and view navigation responsive. Never expose partially baked
tiles. Failure preserves the complete session and its last valid appearance;
dependent edits must not run against the old source. Surface the failure through
the existing operation failure behavior and permit retry.

### History, captures and recovery

Each completed effective Liquify stroke is independently undoable. Cancelling
an unfinished contact restores its starting map. Undo and Redo restore map
states while the session is active; they do not attempt to invert a deformation.
History retains the original revision and the map or stroke checkpoints within
the normal history resource budget. After exit, Undo and Redo reconstruct the
requested stroke boundary from that original through ordinary atomic raster
publication. Do not pre-bake a separate raster for every history step. This
retention supports history restoration, not automatic continuation when returning
to Liquify. Baking adds no extra Undo step. History actions arriving during a
bake follow the existing ordered operation rules.

Save, export, copy and other read-only captures evaluate the current deformed
appearance from a fixed checkpoint without ending the live session. Portable
artwork contains ordinary raster results, not a persistent Liquify effect.
Saving successfully marks only the captured checkpoint clean. Recovery retains
enough source and deformation data to preserve completed edits without relying
on GPU resources surviving; reopening need not resume Liquify. Follow the
existing [document](../internals/documents.md) and
[recovery](../internals/session-recovery.md) contracts.

## Coordinate composition

Use paint-source coordinates, with pixel centers at half-integer positions.
Let $I_0$ be the immutable source and $F_n(x)$ the backward map from output
position $x$ to its position in that source after $n$ dabs. Initially:

$$
F_0(x)=x,\qquad u_0(x)=0,\qquad F_n(x)=x+u_n(x).
$$

The visible image is reconstructed from that source:

$$
I_n(x)=\mathcal S[I_0]\bigl(F_n(x)\bigr).
$$

Here $\mathcal S$ includes color reconstruction and the appropriate sampling
footprint. A new dab supplies a backward map $g_n(x)=x+d_n(x)$ from its output
to the previous appearance. Its exact update is:

$$
F_{n+1}(x)=F_n\bigl(g_n(x)\bigr),
$$

or, for a stored displacement field:

$$
u_{n+1}(x)=d_n(x)+u_n\bigl(x+d_n(x)\bigr).
$$

The old field is evaluated at the displaced position. Adding $d_n(x)$ to
$u_n(x)$ is generally wrong. Tracing an output position visits the newest dab
first, then the earlier dabs; changing their order changes the deformation.
Each dab retains its resolved center, motion, pressure response, footprint,
selection influence, mode and deterministic randomness. Rendering frequency
and GPU batch boundaries must not change the authored result.
Predictions affect provisional previews only; corrected samples replace their
earlier contribution rather than adding another deformation. Preserve the
existing [input correction contract](../internals/input.md).

This is the standard distinction between composing transformations and
repeatedly resampling an image; SimpleITK's
[transform and resampling reference](https://insightsoftwareconsortium.github.io/SimpleITK-Notebooks/Python_html/21_Transforms_and_Resampling.html)
demonstrates the interpolation loss and the importance of composition order.

### Why this preserves more detail

For a one-dimensional signal, bilinearly resampling a half-pixel translation
twice gives:

$$
I_1[k]=\tfrac12 I_0[k]+\tfrac12 I_0[k-1],
$$

$$
I_2[k]=\tfrac14 I_0[k]+\tfrac12 I_0[k-1]+\tfrac14 I_0[k-2].
$$

Composing those translations instead gives $F_2(k)=k-1$: a whole-pixel shift,
which can reproduce $I_0[k-1]$ exactly away from source boundaries. The first
route blurs even though the final mapping needs no fractional interpolation.

Every preview and the final bake reconstruct colors from $I_0$ using the
current map. A preview is never the next dab's color source. “One resampling”
means one reconstruction from the original for each evaluated result; it does
not mean one texture fetch or only one preview during the session. Implementing
the same pixel feedback in a shader would retain the blur.

## Dab transformations

For a dab, let $c$ be its center, $v$ its resolved motion in source pixels,
$w(x)$ its brush coverage and $M(x)$ its selection influence. Define
$s(x)=\operatorname{clamp}(k\,w(x)\,M(x),0,1)$, where $k$ includes strength
and the resolved input dynamics. A zero influence gives the identity map.
Selection attenuates deformation rather than crossfading the original and
warped colors, which would create double edges.

These mappings retain the existing modes' response scales while replacing
their accumulation rule:

| Mode | Backward map $g(x)$ | Visible effect |
| --- | --- | --- |
| Push | $x-(1+m)s(x)v$ | Move content with the stroke; $m$ is the resolved momentum parameter. |
| Twirl counterclockwise | $c+R(0.75s(x))(x-c)$ | Turn content counterclockwise. |
| Twirl clockwise | $c+R(-0.75s(x))(x-c)$ | Turn content clockwise. |
| Pinch | $c+(1+0.35s(x))(x-c)$ | Pull content inward by sampling farther from the center. |
| Expand | $c+(1-0.35s(x))(x-c)$ | Push content outward by sampling nearer the center. |
| Crystals | $x+24D\,s(x)(N(x)-\tfrac12)$ | Apply the mode's deterministic, cell-based irregular displacement; $D$ is distortion and $N$ is a two-component value in $[0,1)^2$. |

$R(\theta)$ is the usual matrix
$\left[\begin{smallmatrix}\cos\theta&-\sin\theta\\\sin\theta&\cos\theta\end{smallmatrix}\right]$,
with radians and a downward-positive canvas $y$ axis. A backward sampling
rotation turns visible content in the opposite direction, explaining the Twirl
signs. Crystals retains its source-coordinate cell pattern and dab-specific
randomness across previews, tile boundaries, Undo and Redo.

The coefficients describe brush response, not research-derived optimal values.
Smooth modes are not automatically invertible: for Push,
$J_g=I-(1+m)v(\nabla s)^T$, so
$\det J_g=1-(1+m)\nabla s\cdot v$. Large motion through a steep falloff can fold
the mapping despite clamped strength. Any step limiting must be deterministic,
independent of frame batching and qualified for brush feel. Crystals deliberately
permits discontinuities and must not be smoothed away by a coarse field.

The map can support additional coordinate deformations later. Reconstruct,
new Edge behavior and a global Amount control are outside the initial scope.
In particular, interpolating a map toward the identity is not universally safe:
halfway between the identity and a 180-degree rotation collapses every point
to the center.

## Precision and sampling requirements

Coordinate accumulation removes repeated color interpolation, but introduces
its own approximation if the map is stored on a grid. Qualify the representation
against direct reverse composition of the dab maps. Preserve subpixel motion at
large source coordinates, sharp changes in Crystals and continuity across
field tiles. Half-precision absolute coordinates cannot be assumed sufficient.
Storage density and precision must follow a measured error budget.

Reconstruct premultiplied color and alpha in the linear working space.
Identity leaves pixels unchanged; constant color regions remain constant;
transparent edges acquire no colored fringe. Respect the source's actual sparse extent and
off-frame authored content. Cache edges, missing residency and tile borders
are not image boundaries. Coordinate interpolation and image reconstruction
need their respective halos and correct source dependencies.

Stretching cannot recover detail absent from $I_0$. Compression requires
antialiasing: the Jacobian $J_F$ describes how an output pixel covers source
space; singular values greater than one indicate compression along those
directions. A fixed bilinear footprint is not sufficient for arbitrary strong
compression. Filtering must follow that footprint, with sharpness judged against
a single-resampling reference rather than an unfiltered, aliased image.
[Heckbert's research on image warping](https://www2.eecs.berkeley.edu/Pubs/TechRpts/1989/5504.html)
establishes the mapping-dependent filtering problem. Avoid claiming lossless
deformation or compensating for feedback blur with sharpening.

## Bake, responsiveness and shared primitives

At exit, seal the final map and produce the affected raster regions from the
same immutable source. An affected output region and its source sampling region
are different sets. Include all displaced source dependencies and filter
support; publish the complete replacement atomically. Tiling the bake is an
internal resource choice, not permission to expose partially committed artwork
or replace the session's source incrementally.

Start work on the leaving action. GPU execution and necessary backing work may
complete asynchronously through the existing ordered operation machinery; this
does not require an idle-time bake scheduler. Never wait for the GPU, copy
canvas pixels or perform blocking I/O on the UI or input thread. Capture new
contacts immediately, but do not execute dependent drawing on an unfinished
bake. A quick pen-down can still wait for that dependency: starting at tool
switch reduces this risk rather than proving it absent.

Target about **100 ms from the leaving action to readiness for the next edit**
on the supported tier workloads. This is an unverified transition budget, not
a measured result or a guarantee derived from physical tool-switch time.
Measure warm and cold transitions, broad deformation and long mixed-mode
sessions; report median, 95th-percentile and worst observed latency. Active
Liquify strokes still meet the existing
[frame targets](../PERFORMANCE_TARGETS.md) under their
[measurement rules](../performance/measuring.md).

Retain immutable source ownership and reuse sparse tile access, regional
sampling, raster publication, cancellation, ordered input and history
primitives. Avoid an unconditional full-canvas pixel copy at session start,
unbounded per-frame replay of every past dab and full-canvas work for a local
stroke. Resource limits must not silently trigger an intermediate color bake.
Do not require a generic resumable-tool framework. Painting and Smudge need
their own color accumulation semantics; they cannot inherit Liquify's
coordinate-only rule merely because they share operation infrastructure.

## Acceptance criteria

The replacement is ready when shared tests, independent GPU references and real
host journeys establish all of the following:

- Identity, zero influence and whole-pixel translations preserve source pixels;
  composed fractional translations avoid the repeated-resampling blur.
- Mixed-mode order matches direct map composition. Splitting identical resolved
  dabs across frames, batches or pen lifts leaves the same result within the
  declared numerical tolerance.
- Fine lines, checkerboards, transparent edges, strong compression, large
  coordinates, sparse sources and tile crossings meet the sampling and geometry
  error budgets. Crystals remains deterministic and retains its cell detail.
- Every lifecycle journey above works, including rapid exit and return, early
  pen-down, target changes, cancelled contacts, Undo/Redo across exit, captures
  during a session and bake failure. No dependent edit sees partial or old pixels.
- Measurements on each tier's reference hardware establish stroke frame rate,
  transition latency and bounded memory for long sessions. Host journeys verify
  native input preservation and responsive navigation while a bake is pending.

No GPU layout, filter kernel or latency claim is accepted solely from this
design; those choices must satisfy these criteria.

## Relevant precedents

[GIMP's Warp tool source](https://github.com/GNOME/gimp/blob/master/app/tools/gimpwarptool.c)
connects a coordinate buffer to `gegl:map-relative`, maintains a temporary
drawable filter and commits it at the tool's commit boundary.
[GEGL's warp operation](https://github.com/GNOME/gegl/blob/master/operations/common-cxx/warp.cc)
updates two-component displacement data. This is a concrete precedent for
separating deformation accumulation from color reconstruction; it is not a
requirement to copy GIMP's lifecycle or code.

[Procreate's Liquify handbook](https://help.procreate.com/procreate/handbook/adjustments/adjustments-liquify)
documents committing when leaving Adjustments and restoring the original with
Reconstruct. It does not disclose internal per-stroke rasterization or sampling,
so it supports a session-level interaction precedent, not proof of the proposed
algorithm or its performance.
