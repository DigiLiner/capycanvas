# Liquify: proposed sessions and deformation math

[Developer guide](README.md)

This proposal replaces Liquify's raster feedback with a deformation session. All
Liquify brushes share one coordinate map over an immutable starting image, and
every result is reconstructed from that image once. Strokes write the deformed
result into the layer as they go, so leaving Liquify needs no bake. The session
continues across tool switches until something else changes the layer, and each
undo step stores the map with the pixels. This describes proposed behavior, not
the current implementation.

## Goals and scope

- **No repeated color interpolation.** Repeated dabs, pen lifts, changes of
  Liquify brush or settings, tool switches and Undo keep deforming the same
  starting pixels.
- **Tier frame rates.** Every mode reaches the very complex brush rate at 512 px
  on each tier's canvas ([performance targets](../PERFORMANCE_TARGETS.md)).
  Liquify currently misses the [low](../performance/low-tier.md) and
  [mid](../performance/mid-tier.md) tier targets.
- **Familiar tools.** Artists coming from Procreate find its Liquify modes and
  controls. As in GIMP and Krita, a session keeps deforming the original pixels;
  unlike them, every stroke stays individually undoable after the artist moves
  on.
- **Shared ownership.** Shared Rust owns the lifecycle, targeting, input ordering
  and history. The shared GPU renderer evaluates the map and the pixels on every
  host.
- **Nothing new to manage.** There is no Apply button, extra layer, saved filter
  or mode setting.
- **One path.** The per-dab color feedback path is removed, not kept as a
  fallback.

| Milestone | Contents |
| --- | --- |
| 1 | Push, Twirl Counterclockwise, Twirl Clockwise, Pinch, Expand and Crystals on the session, with Reset |
| 2 | Edge, Reconstruct, Adjust, Momentum after lift-off, Distortion where Procreate applies it, and continuous application while the pen is held |

Persistent, re-editable Liquify filters are outside this proposal.

## Sessions

A session belongs to one paint source. It holds the starting pixel version
$I_0$, an immutable revision that is referenced rather than copied, the current
map and the pixel version its last stroke produced. Selecting a Liquify brush
creates nothing; the first effective dab starts a session.

Each completed chunk of dabs writes $I_0$ through the updated map into the
layer's tiles in its region, and the unfinished chunk draws as a replaceable
preview ([update points](#evaluation-and-fixed-update-points)). Those tiles are
output only. The next dab reads $I_0$ and the map, never the previous result, so
at pen-up the layer is current and leaving Liquify has nothing to finish.

**Continuation rule.** At a Liquify dab, shared Rust compares the target's
current pixel version (`RasterRevision::identity`) with the versions that carry
a session map: the live session's last version and every undo step that stores
map tiles. A match continues that session from its map. Any other version starts
a new session from the current pixels. One check covers every tool and command,
including ones added later.

| Artist action | Behavior |
| --- | --- |
| Draw with Liquify, lift the pen, draw again | Both strokes deform the same starting pixels. Pen-up completes one undo step. |
| Change Liquify mode, size, strength or pressure response | The session continues. New dabs use the new settings; earlier dabs keep theirs. |
| Change the working selection | The session continues. Later dabs use the new selection; earlier deformation is unchanged. |
| Pan, zoom, rotate or sample a color | The session continues. Sampling sees the deformed artwork. |
| Select another tool, then return to Liquify | Nothing happens on the switch; the layer already shows the result. Liquify continues the session. |
| Paint or edit another layer | The session continues. |
| Paint, fill, filter, transform or paste on the target | The edit applies to the current pixels. The next Liquify dab starts a new session from them, because the new paint must deform too. |
| Undo or Redo to a Liquify step | The next Liquify dab continues from that step's map. |
| Draw with another tool right after a Liquify stroke | The contact queues behind the pending Liquify stroke under the existing ordered operation rules. There is no bake to wait for. |
| Switch drawings | Each drawing keeps its own sessions in its history. |
| Restart | Pixels and undo history are restored without map tiles. The next Liquify dab starts a new session. |
| Liquify without an effective change | No undo step and no new pixel version. |

A new session starts from pixels that were already reconstructed once, so each
session boundary adds one color interpolation. Boundaries come only from edits
that change the target some other way, from map tiles the history budget
dropped and from restart.

## History

Each effective Liquify stroke is one undo step. The step stores the pixel tiles
and the map tiles the stroke changed, and refers to the session's $I_0$. Versions
share unchanged tiles, and history charges each tile once.

- **Undo and Redo restore pixels at once** through ordinary raster history. The
  map is needed only when Liquify continues: the next Liquify dab loads the map
  tiles whose identity differs between the step and the GPU's current map. The
  work is bounded by the liquified area, however many steps were undone.
- **Undo runs in order.** A middle stroke cannot be removed while later strokes
  are kept, because each later stroke deformed the result of the earlier ones.
  Reconstruct takes back a deformation in one area.
- **Readback follows pixel history.** Map tiles are read back with the stroke's
  pixel tiles after commit and compressed on the same worker. History does not
  wait for the GPU; pending map tiles reserve their capture like pending pixel
  tiles.
- **The budget drops map tiles before steps.** Map tiles count against the
  history budget. A step whose map tiles were dropped still restores its pixels;
  Liquify from it starts a new session. Once history spills the active drawing
  to disk, a retention limit keeps map tiles in only the most recent steps.
  Choose that limit from measured disk use.
- **Steps keep the starting image.** Steps with map tiles keep $I_0$ alive. It is
  released with the last step that refers to it.
- **Pen corrections replay one stroke.** A late correction replays the corrected
  stroke from the previous step's map, as `CanvasEngine::replay_completed` does
  for other brushes, and amends that step's pixel and map tiles.
- **Cancelling restores the map.** Cancelling an unfinished contact restores its
  starting map.
- **Recovery and files hold pixels only.** Session recovery keeps pixels and undo
  history but not map tiles. Save, export and copy read the layer's tiles, and
  portable artwork never contains the map. Follow the
  [document](../internals/documents.md) and
  [recovery](../internals/session-recovery.md) contracts.

## Coordinate composition

Use paint-source coordinates, with pixel centers at half-integer positions.
Let $I_0$ be the starting image and $F_n(x)$ the backward map from output
position $x$ to its position in $I_0$ after $n$ dabs. Initially:

$$
F_0(x)=x,\qquad u_0(x)=0,\qquad F_n(x)=x+u_n(x).
$$

The visible image is reconstructed from the starting image:

$$
I_n(x)=\mathcal S[I_0]\bigl(F_n(x)\bigr).
$$

Here $\mathcal S$ includes color reconstruction and the appropriate sampling
footprint. A new dab supplies a backward map $g_n(x)=x+d_n(x)$ from its output
to the previous appearance. Its exact update is:

$$
F_{n+1}(x)=F_n\bigl(g_n(x)\bigr),
$$

or, for the stored displacement field:

$$
u_{n+1}(x)=d_n(x)+u_n\bigl(x+d_n(x)\bigr).
$$

The old field is evaluated at the displaced position. Adding $d_n(x)$ to
$u_n(x)$ is generally wrong. Tracing an output position visits the newest dab
first, then the earlier dabs; changing their order changes the deformation.
Each dab retains its resolved center, motion, pressure response, footprint,
selection influence, mode and deterministic randomness. SimpleITK's
[transform and resampling reference](https://insightsoftwareconsortium.github.io/SimpleITK-Notebooks/Python_html/21_Transforms_and_Resampling.html)
demonstrates the interpolation loss and the importance of composition order.

### Why this preserves detail

For a one-dimensional signal, bilinearly resampling a half-pixel translation
twice gives:

$$
I_1[k]=\tfrac12 I_0[k]+\tfrac12 I_0[k-1],
$$

$$
I_2[k]=\tfrac14 I_0[k]+\tfrac12 I_0[k-1]+\tfrac14 I_0[k-2].
$$

Composing those translations instead gives $F_2(k)=k-1$: a whole-pixel shift,
which reproduces $I_0[k-1]$ exactly away from source boundaries.

In general, one bilinear interpolation at subpixel offset $f$ spreads each
sample with variance $f(1-f)$ along that axis, and repeated interpolations add
their variances. At its default spacing, Push re-samples carried paint about 12
times per brush width of travel. That adds no blur if every step happens to be a
whole pixel, a standard deviation of about 1.7 px if every step is half a pixel
and about 1.4 px if the fractions are spread evenly. These are estimates, not
measured preset results. Composing interpolates once.

Every result is reconstructed from $I_0$ through the current map, and a previous
result is never a dab's color source. “One resampling” means one reconstruction
from $I_0$ for each evaluated result, not one texture fetch. Implementing the
same pixel feedback in a shader would retain the blur.

### Evaluation and fixed update points

The map is stored at pixel centers, so reconstruction reads it without
interpolation. An update for dabs $n$ to $n+K-1$ evaluates, at every pixel
center $x$ in their footprint,

$$
u_{n+K}(x)=G(x)-x+\tilde u_n\bigl(G(x)\bigr),\qquad G=g_n\circ g_{n+1}\circ\dots\circ g_{n+K-1},
$$

tracing $G$ analytically from the newest dab and reading the stored field
$\tilde u_n$ bilinearly at $G(x)$. That read is the only interpolation. It
slightly smooths the deformation but, unlike color interpolation, does not
soften pixels. The result depends on where updates fall, so:

- **Updates follow the stroke, not the frame.** The stored map is updated after
  every $K$ dabs and at pen-up. The same dabs and pen lifts give the same result
  at any frame rate, batching or prediction.
- **One preview pass per frame.** A frame evaluates the unfinished chunk as a
  replaceable preview: each pixel traces the chunk's dabs, reads the stored map
  once and reconstructs color once. Predicted dabs appear only in this preview
  and are replaced when real samples arrive, under the
  [input correction contract](../internals/input.md).
- **One update pass per completed chunk.** A frame that reaches an update point
  also runs the update above over the completed chunk's footprint, writing its
  map tiles and result tiles.
- **$K$ comes from measurement.** A larger $K$ means fewer map interpolations and
  a longer per-pixel loop.

A pen lift is an update point, so one stroke and the same dabs split across two
strokes can differ slightly.

## Dab transformations

For a dab, let $c$ be its center, $v$ its resolved motion in source pixels,
$w(x)$ its brush coverage and $M(x)$ its selection influence. Define
$s(x)=\operatorname{clamp}(k\,w(x)\,M(x),0,1)$, where $k$ includes strength
and the resolved input dynamics. A zero influence gives the identity map.
Selection attenuates deformation rather than crossfading the original and
warped colors, which would create double edges.

These mappings keep the current modes' response scales and replace their
accumulation rule:

| Mode | Backward map $g(x)$ | Visible effect |
| --- | --- | --- |
| Push | $x-(1+m)s(x)v$ | Move content with the stroke; $m$ is the current momentum parameter. |
| Twirl Counterclockwise | $c+R(0.75s(x))(x-c)$ | Turn content counterclockwise. |
| Twirl Clockwise | $c+R(-0.75s(x))(x-c)$ | Turn content clockwise. |
| Pinch | $c+(1+0.35s(x))(x-c)$ | Pull content inward by sampling farther from the center. |
| Expand | $c+(1-0.35s(x))(x-c)$ | Push content outward by sampling nearer the center. |
| Crystals | $x+d(x)$ | Scatter content by whole pixels, one random direction per 4 px cell; see below. |
| Edge (milestone 2) | $c+\bigl(I+0.35s(x)\,nn^{T}\bigr)(x-c)$ | Pull content toward the stroke line from both sides; $n$ is the unit normal of $v$. |

$R(\theta)$ is the usual matrix
$\left[\begin{smallmatrix}\cos\theta&-\sin\theta\\\sin\theta&\cos\theta\end{smallmatrix}\right]$,
with radians and a downward-positive canvas $y$ axis. A backward sampling
rotation turns visible content in the opposite direction, explaining the Twirl
signs. `LiquifyMode::TwirlClockwise` currently names the sampled rotation and
turns content counterclockwise; rename the variants to the turn they make and
keep their stored IDs. Edge takes $n$ from the dab's motion. A dab without
motion keeps the previous dab's normal, and a stroke's first dab, which has
none, leaves content unchanged.

The coefficients describe brush response, not research-derived optimal values.

### Crystals

Each 4 px cell has one random direction, and every pixel moves by a whole
number of pixels:

$$
d(x)=\bigl\lfloor 24D\,s(x)\,(N_{\text{cell}}-\tfrac12)+\tfrac12\bigr\rfloor,
$$

per component, with $D$ the distortion and $s(x)$ the per-pixel influence every
mode uses. The two components of $N_{\text{cell}}\in[0,1)^2$ come independently
from an integer hash of the cell $\lfloor x/4\rfloor$, the stroke seed and the
dab index. CPU and GPU both round with $\lfloor\cdot+\tfrac12\rfloor$, because
WGSL's `round` rounds halves to even and Rust's rounds them away from zero.

- **Selection and coverage apply per pixel.** An unselected pixel never moves,
  even in a partly selected cell, and nothing moves outside the tip. Where $s$ is
  constant, as in the tip's hard core, a cell moves rigidly; in the soft edge,
  offsets inside a cell step by whole pixels.
- **Whole pixels keep updates exact.** Within a chunk of Crystals dabs, offsets
  land map reads on grid points, so updates never blend neighboring cells;
  interpolating a map that jumps at every cell edge would smear them. Over
  undeformed content, reconstruction copies source pixels exactly. Where earlier
  dabs left a fractional displacement, it stays fractional, and reconstruction
  interpolates once, as everywhere else.
- **The current hash must go.** `fract(sin(…) * 43758.5453)` depends on `sin`,
  which WGSL bounds only on $[-\pi,\pi]$, to $2^{-11}$; the multiplier turns
  that error into about 21 whole units, so the result differs between GPUs and
  from a CPU reference. Its second component, `fract(7.13 n)`, is a function of
  the first, and keying it on the dab center repeats the pattern wherever dabs
  share a center.
- **Smooth dabs over Crystals still interpolate across cell edges.** Measure how
  visible that is.

### Folding

A map folds where $\det J\le 0$, and content is then repeated or mirrored.
Composing maps that do not fold gives a deformation that does not fold, so
limiting each dab keeps the exact deformation fold-free. Crystals moves cells
independently by design and is excluded.

The round analytic tip with radius $r$ and hardness $h$ falls off linearly over
a relative edge width $e=\max(1-h,1/r)$, so without a selection
$s(x)=\sigma(\rho)$ depends only on $\rho=|x-c|$, with $-k/(e\,r)\le\sigma'\le0$.
For that tip, with $a=0.35$ and $q=n\cdot(x-c)$:

| Mode | $\det J_g$ | Folds exactly when |
| --- | --- | --- |
| Push | $1-(1+m)\nabla s\cdot v$ | $(1+m)k\lvert v\rvert\ge e\,r$ |
| Pinch | $(1+a\sigma)(1+a\sigma+a\rho\sigma')$ | $ak\ge e$ |
| Edge | $1+a\sigma+a\sigma'q^2/\rho$ | $ak\ge e$ |
| Expand | $(1-a\sigma)(1-a\sigma-a\rho\sigma')$ | Never |
| Twirl | $1$ | Never |

At one spacing step, $\lvert v\rvert=2r\cdot\text{spacing}$, so Push folds once
spacing reaches $e/(2(1+m)k)$. That is about 0.31 for the preset, but 0.048 at
hardness 0.9, below the 0.08 default. With a selection, or a textured or
non-round tip, $s$ is not radial and only the general determinant applies; Twirl
can then fold too.

- **Sub-steps.** Split a dab that would fold into $n$ sub-steps, each applying
  $1/n$ of its motion (Push) or of $a$ (Pinch, Edge), so every sub-step keeps
  $\det J\ge\delta$: $n=\lceil(1+m)k\lvert v\rvert/(e\,r\,(1-\delta))\rceil$ for
  Push and $n=\lceil ak/(e(1-\delta))\rceil$ for Pinch and Edge. The sub-steps
  approximate the unsplit dab. With $\delta\le0.7$, every preset needs $n=1$ at
  its default hardness and spacing.
- **The tip sets the count.** $n$ is computed once per dab from the tip, so
  frames and batches cannot change it. A selection edge is much steeper, about
  $k$ per pixel when antialiased, and would need tens of sub-steps per dab
  (87 for a 41 px Push step at $\delta=\tfrac12$), so it does not count. A Push
  dab that moves content away from a hard selection edge therefore copies a
  strip of the unselected content beside the edge, up to $(1+m)k\lvert v\rvert$
  wide, into the selection.
- **The stored map can still fold.** It samples the exact deformation at pixel
  centers, and updates read it bilinearly. Where the deformation turns or
  squeezes faster than one pixel can represent, the bilinear interpolant can
  fold between pixel centers although no dab and no exact composition does. For
  example, three Twirl taps of radius 2 px, hardness 0.5 and strength 0.72, each
  followed by an update, give a negative determinant inside one cell.
  Reconstruction reads only pixel centers, so such a fold acts through later
  updates that read inside the cell and through the filter's footprint. The
  interpolant's determinant is affine within a cell, so a cell is fold-free
  exactly when its four corner determinants, cross products of the cell's edge
  differences, are positive. A single central difference can miss a fold
  ([Liu et al.](https://arxiv.org/abs/2212.06060) analyze the same problem for
  pixel grids). Use the corner test to size the filter footprint and in the
  acceptance checks, and measure how often and how visibly stored maps fold at
  ordinary brush sizes.

### Map edits

These operations edit the displacement $u$ directly instead of composing another
backward map. Their scope is the session: everything since the target was last
changed some other way.

| Operation | Map edit | Behavior |
| --- | --- | --- |
| Reset (milestone 1) | $u\leftarrow 0$ | The layer returns to $I_0$ exactly, in one undo step. |
| Reconstruct (milestone 2) | $u(x)\leftarrow(1-s(x))\,u(x)$ per dab | Paint restores the original under the brush. |
| Adjust (milestone 2) | $u\leftarrow t\,u_A$ for $t\in[0,1]$, with $u_A$ the map when Adjust opened | Reduces everything applied in the session. While the slider moves, preview at screen resolution, as full-screen filters may; render full resolution on release. One undo step. |

A Reconstruct dab fits the newest-first trace: it blends the remaining trace's
result with the position the trace reached it at, $F'(y)=y+(1-s(y))(F(y)-y)$, so
a pixel still reads the stored map once. Scaling a displacement can collapse
content: halfway between the identity and a 180-degree rotation maps every point
to the center, so Reconstruct and Adjust are not fold-free. GIMP's Erase scales
its field the same way; accept it rather than restrict these tools.

### Procreate behaviors (milestone 2)

- **Momentum.** Procreate's Momentum keeps the effect going after the pen lifts,
  as if the stroke overshot. The current parameter instead makes Push move
  content faster than the pen, through the $1+m$ factor. Give it Procreate's
  meaning: after pen-up, dabs continue along the final direction with decaying
  strength, derived from the final pen samples so replay and undo stay exact.
- **Distortion.** Procreate presents Distortion as making effects “more twirly,
  jagged, or twisted”; Capy Canvas exposes it only for Crystals. Check which
  modes it changes in Procreate before designing it.
- **Holding the pen still.** Photoshop, through Rate, and Clip Studio Paint keep
  twirling, pinching and expanding while the pen is held. Here, Liquify dabs
  come only from distance traveled, and the first contact applies a full dab at
  once. Confirm Procreate's behavior, then emit Twirl, Pinch and Expand dabs at a
  fixed rate from sample timestamps while the pen is held, never from frame
  timing.

## Precision and sampling

- **32-bit displacement.** Store $u$ as two 32-bit floats per pixel; the identity
  is exactly zero, and steps stay below 0.001 px for displacements under
  16384 px. Half precision steps by 0.5 px past 512 px and by 2 px past 2048 px.
  Filtering 32-bit float textures is optional in WebGPU, so map reads
  interpolate manually, like `canvas_sample`.
- **Sparse tiles.** Map tiles are 256 px, aligned with paint tiles. Only touched
  tiles exist, and a tile that returns to the identity is freed.
- **Linear premultiplied color.** Reconstruct premultiplied color and alpha in
  the linear working space, as Transform does; resampling stays linear in both
  blend spaces ([documents](../internals/documents.md)). Identity leaves pixels
  unchanged, constant color regions remain constant and transparent edges
  acquire no colored fringe. In a Perceptual document a squeezed dark line
  therefore comes out lighter than resampling encoded values would make it,
  exactly as with Transform; change both together if that changes.
- **True source extent.** Respect the source's sparse extent and off-frame
  authored content. Cache edges, missing residency and tile borders are not
  image boundaries. Map interpolation and image reconstruction need their own
  halos and source dependencies.
- **Filtering where content is squeezed, from milestone 1.** Push piles content
  at its leading edge and Twirl compresses as it winds, so ordinary strokes need
  it. Reuse the transform pass's filter helpers
  ([resampling](../internals/rendering.md#resampling),
  `pixel_transform/area.wgsl`) with Liquify's own source access and map. The
  footprint vectors are $\partial F/\partial x$ and $\partial F/\partial y$,
  central differences of the stored map. A destination pixel averages a grid of
  bilinear taps over that parallelogram, as many per axis as source pixels it
  spans, with Transform's caps: a low cap in the live preview and up to 16 for
  committed tiles. Where a cell's corner determinants are not all positive, or
  the map changes faster than its grid resolves, as after several turns of
  Twirl, clamp the footprint to the cap. Sharing the helpers also gives both
  tools the same color handling.
  [Heckbert's research on image warping](https://www2.eecs.berkeley.edu/Pubs/TechRpts/1989/5504.html)
  describes the mapping-dependent filtering problem. Judge sharpness against a
  single-resampling reference, not an unfiltered, aliased image.
- **Stretching stays soft.** Stretching cannot recover detail absent from
  $I_0$. Compare Transform's clamped Catmull-Rom filter for magnified areas, and
  never compensate with sharpening.
- **Error budget.** Storage precision and filtering follow a measured error
  budget against direct reverse composition of the dab maps.

## Performance and memory

- **Current cost.** Liquify at 512 px meets the top-tier target and misses the
  low and mid tiers. The presets with the closest spacing, Push and Crystals,
  are the slowest, and `push_batches` evaluates one Liquify dab per batch. That
  suggests per-dab passes dominate; profile before choosing kernels.
- **Per-frame work.** Every frame runs one preview pass over the unfinished
  chunk's footprint. Each pixel does up to $K$ dab evaluations, skipping dabs
  whose coverage is zero at the traced position, one four-tap map read, one
  color reconstruction, with more taps where content is compressed, and one
  write. A frame that reaches an update point also runs one update pass over the
  completed chunk's footprint, with the same per-pixel work plus a map tile
  write. Before implementing, model each tier's cost of both passes from those
  reads, writes and taps against kernels measured on the reference device, under
  the [measuring rules](../performance/measuring.md).
- **Transitions.** Tool switches do no work, so they need no latency target.
  Undoing and continuing loads at most the liquified area's map tiles; how long
  that delays the first dab, from RAM and from disk, is unmeasured.
- **Readback.** Each Liquify stroke reads back 8 bytes of map per changed pixel
  beside its pixel tiles. Check on the Mali tablets that this does not slow the
  next stroke.
- **Current map.** The map lives in GPU memory, which is system RAM on the
  tablets: 8 bytes per pixel of every touched tile, up to 92, 183 and 459 MiB
  for a fully liquified low, mid and top tier canvas. Count it in the same
  accounting as history and open drawings, and measure a whole-canvas session on
  the TCL TAB 11 Gen 2 (6 GB).
- **History cost.** Before compression, a Liquify step costs about three times
  an ordinary stroke on an 8-bit layer, twice on a 16-bit or half-float layer and
  one and a half times on a float layer.
- **Thread rules.** Never wait for the GPU, copy canvas pixels or block on I/O on
  the UI or input thread. Avoid a full-canvas copy at session start, per-frame
  replay of past dabs and full-canvas work for a local stroke. Painting and
  Smudge keep their own color accumulation; they do not inherit Liquify's
  coordinate-only rule.

## Acceptance criteria

The replacement is ready when shared tests without a window, independent GPU
references and real host journeys establish all of the following:

- Identity, zero influence and whole-pixel translations preserve source pixels;
  composed fractional translations avoid the repeated-resampling blur.
- The same dabs and pen lifts give identical results at any frame rate, batching
  or prediction. Mixed-mode order matches direct map composition within the
  interpolation of its update points.
- Crystals matches a CPU reference exactly, moves pixels by whole pixels and
  moves no unselected pixel.
- No dab folds after sub-stepping at any spacing, hardness or pressure. The
  corner test reports how often stored maps fold for each preset across brush
  sizes, and the visible effect is compared with the exact composition.
- Fine lines, checkerboards, transparent edges, strong compression, large
  coordinates, sparse sources and tile crossings meet the sampling and geometry
  error budgets.
- Undoing any number of Liquify strokes and drawing again equals a session in
  which those strokes never happened. This holds across tool switches and edits
  to other layers; after Liquify, painting on the target, undoing the paint and
  liquifying again; while the last stroke is still pending on the GPU; and after
  a late pen correction.
- A step whose map tiles were dropped restores exact pixels and starts a new
  session, and so does restart.
- Every journey in the session table works on each affected host, with native
  input preserved and navigation responsive.
- Measurements on each tier's reference hardware establish the stroke rate of
  every mode at 512 px, the readback cost per stroke, the peak memory of a
  whole-canvas session and the delay from Undo to the first continued Liquify
  dab, with map tiles in RAM and on disk.

No GPU layout, filter kernel or latency claim is accepted solely from this
design; those choices must satisfy these criteria.

## Relevant precedents

- **GIMP.** [GEGL's warp operation](https://github.com/GNOME/gegl/blob/master/operations/common-cxx/warp.cc)
  updates its field as $D_{\text{new}}(p)=v(p)+D_{\text{old}}(p+v(p))$, reading
  $D_{\text{old}}$ bilinearly: true composition. Its `map-relative` operation
  gives the sampler the field's finite-difference Jacobian. GIMP's
  [Warp tool](https://github.com/GNOME/gimp/blob/master/app/tools/gimpwarptool.c)
  commits on tool switch, and the session becomes one undo step.
- **Krita.** Its [Liquify worker](https://invent.kde.org/graphics/krita/-/blob/master/libs/image/kis_liquify_transform_worker.cpp)
  moves a grid of nodes spaced 8 px apart and re-renders from the original
  pixels. Strokes undo individually during the transform and as one step after
  it is applied. Returning to the tool resumes the transform when it is the last
  undo step ([continuous transform](https://docs.krita.org/en/reference_manual/tools/transform.html)).
- **Procreate.** [Liquify](https://help.procreate.com/procreate/handbook/adjustments/adjustments-liquify)
  offers Push, Twirl Left and Right, Pinch, Expand, Crystals, Edge and
  Reconstruct, with Size, Pressure, Distortion, Momentum, Adjust and Reset. It
  commits when the artist leaves Adjustments.
- **Photoshop.** [Liquify](https://helpx.adobe.com/photoshop/using/liquify-filter.html)
  keeps applying Twirl, Pucker and Bloat while the tool is held still, at the
  speed set by Rate, and offers Reconstruct and Restore All.
