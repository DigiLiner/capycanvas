# Stroke stabilization: full-horizon reference objective

[Brush design records](README.md)

This record fixes the intended objective for the committed stroke and explains
the decisions behind it. The full-horizon solution is the reference, or source
of truth, for developing a filter with a fixed lookahead in milliseconds. It is
an agreed artistic target, not an estimate of unknowable artist intention and
not a claim that the current editor implements this optimization.

The [curve-fit work-in-progress record](curve-fit-work-in-progress.md) describes
the separate implementation snapshot. Measurements, trace comparisons and
parameter-sweep results belong in artifacts, not in this decision record.

## Accepted design

The reference trades fidelity to delivered input against changes in curvature
and curvature oscillation. Fidelity measures displacement relative to local
movement, with a positional floor and a bounded amount of speed relaxation.
Contact endpoints preserve reach independently of speed. Interior corners use
the same objective as the rest of the stroke: their local slowdown naturally
increases positional fidelity.

The committed target uses real contact samples only. Predictions and the choice
to show stroke preview do not change it. Strength and commitment lookahead have
separate meanings: strength defines the target line; lookahead determines how
well a real-time estimator can approximate that target.

### Quantities and coordinates

For one completed contact, let the valid delivered samples be
`D = {(t_i, p_i)}` on `[t_start, t_end]`. Times are source timestamps in seconds,
not frame counts or event-delivery times. `p(t)` is their continuous,
timestamp-preserving positional interpolant. Piecewise linear reconstruction of
the input is permitted; it does not make the fitted output piecewise linear.

Write the fitted motion as:

$$
r(t)=C(s(t)),\qquad s'(t)\geq0,\qquad
s(t_{\rm start})=0,\quad s(t_{\rm end})=\operatorname{length}(C).
$$

`C` is the ordered geometric curve, parameterized by its own arc length `s`.
`s(t)` is continuous and describes progression along it, including stationary
intervals. On moving
parts, use a regular curve with continuous tangent and curvature, at least
piecewise third-order differentiability, and finite squared curvature-change
energy. A twice-continuously-differentiable spline is a candidate representation,
not a prescribed solver or knot spacing.

Positions, speeds and tolerances use one captured drawing-coordinate metric.
Convert consistently between that metric and document coordinates. Let
`ell_unit` and `t_unit` be fixed spatial and temporal unit conversions, independent
of stroke length, sample rate, strength and lookahead. Normalize geometric terms
as below so changing units does not change the balance of the two penalties.
The exact view/document scaling policy remains an implementation calibration
decision; mixed units are not permitted.

`v_hat(t)` is a local input-speed estimate produced before the stabilization
solve. Its estimation interval is short, fixed in physical time and independent
of stabilization strength. Freeze these weights during a solve so an optimizer
cannot reduce its fitting cost by inventing a faster candidate trajectory.

### Complete reference objective

Define the movement scale and bounded positional scale:

$$
z(t)=(\hat v(t)T)^2,
\qquad
d(t)^2=d_0^2+\frac{D_{\rm cap}^2z(t)}{D_{\rm cap}^2+z(t)}.
$$

The fidelity cost is:

$$
E[r,D]=\int_{t_{\rm start}}^{t_{\rm end}}
\frac{\|r(t)-p(t)\|^2}{d(t)^2}\,\frac{dt}{t_{\rm unit}}.
$$

For signed curvature `kappa(s)`, define:

$$
q(s)=\ell_{\rm unit}^2\frac{d\kappa}{ds},
\qquad
R[C]=\int_C\left(q(s)^2+\eta|q(s)|\right)
\frac{ds}{\ell_{\rm unit}}.
$$

For positive strength, the full-horizon reference is:

$$
\boxed{
r_S^*(D)\in\operatorname*{arg\,min}_{r=C\circ s}
\left(E[r,D]+\lambda(S)R[C]\right)
}
$$

The minimization respects ordered traversal, the stated curve regularity and
the endpoint conditions below. It is nonlinear in curve geometry; writing
`arg min` defines the desired target, not proof that a particular numerical
solver finds its global minimum. Numerical representation, convergence and any
tie-breaking convention still need to be fixed before using it as an oracle.

For each contact boundary `j` in `{start, end}`, let `T_j` be the unit direction
of the neighboring input trajectory and `N_j` its perpendicular. These directions
come from a stable local neighborhood, not a single raw difference or an assumed
stop at contact change. Impose:

$$
\boxed{
T_j^\top\bigl(r(t_j)-p_j\bigr)=0,
\qquad
\left|N_j^\top\bigl(r(t_j)-p_j\bigr)\right|\leq d_{\rm end}.
}
$$

These conditions preserve the contact's start/end location along travel and
permit a small perpendicular fitting offset. A stationary point contact retains
point-contact semantics; a direction of travel is meaningful only for a contact
that travels. Neither boundary imposes zero velocity, zero acceleration or zero
curvature.

At strength zero, bypass optimization and stabilization. Preserve delivered
positions and the existing no-stabilization stroke behavior, with no added
algorithmic wait. An underdetermined zero-penalty spline solve is not the
definition of the zero setting.

### Controls and internal parameters

| Quantity | Ownership and meaning | Accepted setting or remaining calibration |
| --- | --- | --- |
| Strength `S`, mapped to `lambda(S)` | Artist control: more geometric regularization relative to fitting fidelity. | Display range 0–100; zero bypasses. Preserve useful precision at low settings, including 30 at 75% slider travel. The monotone mapping to `lambda` and a modest default need calibration against this objective. |
| Lookahead `L` in milliseconds | Independent filter budget or candidate comparison control: newer real input available before committing source time `t`. | Fixed for each candidate. Its default and eventual UI exposure are not fixed here. Increasing it must not redefine the full-horizon target. |
| Movement interval `T` | Internal parameter: distance traveled over this interval sets relative-error scale. | Fixed physical time, not event spacing, display cadence, prediction horizon or `L`; numerical value not yet fixed. |
| Positional floor `d_0 > 0` | Internal parameter: finite fidelity scale for slow or stationary input. | Calibrate against required precision and ordinary input noise; it is not a digitizer-noise classifier. |
| Speed-relaxation cap `D_cap > 0` | Internal parameter: upper bound on the movement contribution to the fidelity scale. | Larger values allow greater relaxation during fast movement; numerical value not yet fixed. |
| Curvature balance `eta > 0` | Internal parameter: total curvature variation relative to squared curvature change. | Keep both penalties active. Fix one shared balance after visual comparisons; no independent artist slider is required. |
| Endpoint offset `d_end >= 0` | Internal boundary tolerance: permitted perpendicular endpoint displacement. | Small and independent of movement speed; numerical value not yet fixed. |
| Local speed estimation interval | Internal estimator support. | Preserve corner slowdowns while rejecting unstable sample-to-sample derivatives; numerical value not yet fixed. |
| `ell_unit`, `t_unit` | Fixed unit conversions. | Neither is an artistic control or a fitted stroke-dependent scale. |

Strength controls `lambda`, not all the tolerances simultaneously. The numerical
slider mapping is separate from the strength-to-objective calibration. The
implementation snapshot's default strength 6 and its time-constant conversion
are not validated values for this new reference objective.

Speed normalization is built into the accepted reference. This record does not
add another speed slider or decide whether to remove the existing Adjust by
speed checkbox. If an explicit speed policy remains, it selects a reference
configuration; it must not select different committed geometry according to
whether preview is visible. In particular, the current checkbox's slow-motion
boost is not automatically equivalent to this relative-error formulation.

## Decision history and mathematical rationale

### 1. Ordered geometry and progression: `r(t) = C(s(t))`

**Purpose.** Smooth the drawn shape without treating every variation in drawing
speed as a geometric defect. Preserve input order through loops, reversals and
stationary contacts.

**Choice.** Curvature is measured along distance, while fidelity retains source
time. With tangent `T_curve`, normal `N_curve`, speed `s'` and curvature `kappa`:

$$
r''(t)=s''(t)T_{\rm curve}+(s'(t))^2\kappa(s(t))N_{\rm curve}.
$$

Thus squared temporal acceleration mixes changing travel speed with bending.
The geometric representation separates them. A deliberately retraced stroke
can advance in ordered curve parameter while reversing its spatial direction;
ordered traversal does not mean that canvas coordinates must be monotone.

**Alternatives.** Penalizing `||r''||^2` or temporal jerk alone makes shape
regularization depend strongly on drawing speed. Unordered nearest-point
matching can hide delayed following or incorrect traversal. A global prohibition
on loops or direction reversal would reject deliberate marks.

**Scope.** This record fixes positional fidelity and geometric stabilization.
It does not prescribe an additional tangential-acceleration objective or pressure,
tilt and roll losses. Those affect progression or brush appearance and require
their own calibration. Keep attributes aligned with the same source trajectory;
do not infer sensor smoothing merely from a smooth centerline.

### 2. Quadratic fidelity after movement normalization

**Purpose.** Treat a displacement as more significant during precise slow motion
than during a rapid sweep, while maintaining a cost for large displacement.

**Choice.** Divide spatial error by a movement-dependent positional scale before
squaring. Below the relaxation cap, when `v_hat*T` dominates `d_0`, the cost is
approximately `||error||^2/(v_hat*T)^2`. Doubling speed and displacement together
then leaves relative error approximately unchanged. This normalization applies
to sideways displacement as well as displacement along travel.

For a locally straight segment with following delay `delta`, along-travel error
is approximately `v*delta`. In the same regime its normalized cost is
approximately `delta^2/T^2`: increasing speed does not make a fixed temporal
delay arbitrarily cheap. This local argument is not a global perceptual law or a
bound on input-to-photon latency.

**Alternatives.** Absolute-distance MSE lacks the requested relative-motion
scale. Changing MSE to MAE does not fix that missing scale. Dividing by movement
between consecutive events makes the objective depend on input rate and can
become singular at a pause. Normalizing only the along-travel component leaves
the sideways relative-error requirement unaddressed. An unrestricted temporal
warp can improve a score by moving features to the wrong source times.

**Research basis.** [Speed-adaptive interactive filtering][one-euro] motivates
the distinction between slow-motion jitter and fast-motion latency. It does not
derive this normalized loss or establish its constants for painting.

### 3. A positional floor and smooth relaxation cap

**Purpose.** Keep fidelity finite near rest and prevent high speed from making
the settled shape effectively unconstrained.

**Choice.** For `z >= 0`, the rational contribution
`D_cap^2*z/(D_cap^2+z)` is smooth, monotone, approximately `z` near zero and bounded
above by `D_cap^2`. Consequently:

$$
d_0^2\leq d(t)^2<d_0^2+D_{\rm cap}^2.
$$

There is no speed threshold or alternate algorithm. The cap bounds relaxation
of the loss, not the actual displacement of the fitted curve. Large displacement
still has an increasing quadratic cost; a strict interior displacement guarantee
is not part of the accepted objective.

**Alternatives.** Unbounded speed normalization can make fast regions too cheap
to distort. A hard switch at a speed threshold changes the objective abruptly.
A separate patch for each visible deformation obscures the intended tradeoff.
If a strict displacement bound becomes necessary, prefer one uniform constraint
for every point over a list of corner-specific guards.

### 4. Time integration and input-rate independence

**Purpose.** Give the same continuous input approximately the same target across
device rates and event batching, without discarding delivered detail.

**Choice.** Integrate fidelity over source time. For irregularly spaced samples,
quadrature uses interval weights, not sample counts. For example, trapezoidal
weights are half the adjacent interval at a contact boundary and half the sum
of neighboring intervals in the interior. Divide these weights by `t_unit`.
More samples improve input reconstruction and integration accuracy rather than
automatically increasing fitting strength.

Time integration gives slow sections more influence per unit distance because
`dt = ds/v` during motion. This is an explicit policy choice, in addition to
movement normalization, not an accidental consequence to ignore during tuning.
Preserve all real input samples. Output evaluation and rendering subdivision
must preserve perceptible curvature even when input is sparse.

**Alternatives.** An unweighted sample sum changes with sample frequency.
Distance integration is a legitimate alternative for stronger drawing-speed
invariance of static geometry, but gives no weight to stationary intervals and
does not implement the chosen time-sensitive fidelity. Coarse input buckets
lose detail rather than merely improving computational efficiency.

### 5. Squared curvature change

**Purpose.** Discourage rapid changes in bending while retaining natural arcs and
gradual transitions. Continuous position alone does not prevent segmented or
uneven-looking output.

**Choice.** Squared `kappa_s` penalizes abrupt variation more strongly than gradual
variation. The normalized term `q^2` is always active. A straight line or circular
arc has constant curvature and zero cost. On a small-slope sinusoidal perturbation
of amplitude `a` and spatial wave number `k`, squared curvature-change density
scales approximately as `a^2*k^6`; this explains strong suppression of fine wiggles
without assuming a hardware ripple wavelength.

**Alternatives.** Squared curvature itself penalizes every deliberate arc and
favors flattening. Requiring continuity alone permits smooth but wavy curves.
Higher curvature derivatives add regularity but primarily increase suppression
of fine variation; they are not the first choice for broad waviness.

**Research basis.** [Sketching piecewise clothoid curves][clothoid] motivates
evaluating the curvature profile rather than relying on a smooth-looking spline
representation alone. Its particular segmentation solver is not adopted here.

### 6. Total curvature variation

**Purpose.** Reduce repeated increases and decreases in bending, including
waviness on a gradual curve, rather than only remove fine jitter on straight
strokes.

**Choice.** Add the absolute curvature-change term with fixed balance `eta`:

$$
\int_a^b|\kappa_s|ds\geq|\kappa(b)-\kappa(a)|.
$$

Monotonic curvature attains equality. Additional reversals add total variation.
The penalty applies to intrinsic curvature, not deviation from a fitted straight
line. It also penalizes intentional undulations; input fidelity and strength
determine whether those survive. It does not guarantee a particular number of
curvature extrema or distinguish intent from noise.

Retain the squared term because total variation alone assigns the same cost to
different monotonic profiles with identical endpoint curvatures. Representation
regularity and the squared term discourage abrupt concentration of changes.

**Alternatives.** Total variation alone is not the whole smoothness objective.
Hard limits on curvature-extremum count require feature selection and introduce
discrete decisions. Downweighting samples because the current fit considers them
wavy risks erasing intentional detail through circular reasoning.

**Research basis.** [Total curvature variation fairing][curvature-tv] supplies
the geometric total-variation precedent. The exact mixed objective, weights and
real-time approximation here are our design choices, not established production
ink results from that paper.

### 7. Local speed without a corner or noise classifier

**Purpose.** Increase fidelity during precision work and corner slowdowns without
classifying every bend or pause.

**Choice.** Estimate local movement from a short neighborhood in physical time.
Preserve the slowdown at a sharp corner instead of assigning it the speed of
the incoming stroke. All interior points use the same loss. A rounded turn that
continues quickly receives the fast-motion policy; it is not automatically
classified as a protected sharp corner.

Use uniform confidence weights initially. The positional floor prevents a
singularity; the regularizer supplies noise suppression. A small residual is not
proof of deliberate detail, and a large residual is not proof of bad input.
Speed estimation must itself be stable under ordinary positional noise.

**Alternatives.** Instantaneous raw differences amplify noise as sample spacing
shrinks. A heavily averaged speed can hide corner stops. Detecting corners from
every speed minimum or curvature spike introduces false features. Residual-based
confidence can classify a corner omitted by the smoother as noise and erase it.
Device-specific uncertainty weights remain deferred until measurements support
them. [Weighted least squares][weighted-ls] explains the role of reliable noise
weights; it does not justify inferring confidence from speed alone.

### 8. Contact endpoint reach as a boundary condition

**Purpose.** Preserve where a moving contact begins and ends, and prevent a
settled stroke from ending noticeably short along travel.

**Choice.** Contact can begin or finish while the stylus is moving. The endpoint
conditions therefore override speed-dependent relaxation only at the two actual
contact boundaries. Along-travel offset is zero; normal offset is bounded by
`d_end`. Solve these conditions together with the rest of the curve so the
neighboring shape remains smooth.

Endpoints have zero measure in the continuous fidelity integral. Average error
alone cannot supply their required positional importance. The conditions use
the actual contact domain, not a rule that tries to recognize an endpoint from
low speed. Ending contact does not imply ending physical movement.

**Alternatives.** Waiting for a stop misses moving lifts. Pinning both coordinates
to a raw endpoint prevents the permitted perpendicular fit. Snapping after the
solve or appending a straight finishing segment can introduce a visible kink.
Large endpoint weights provide only a soft preference and do not guarantee reach.
Artificial zero-velocity or zero-curvature boundary conditions can distort a
moving start or end.

## Fixed-lookahead real-time approximation

For a fixed commitment lookahead `L`, a candidate may use real input through
source time `t+L` when committing the estimate associated with source time `t`.
Its access to older history, retained state and computational cost are separate
constraints. A finite total history window is not the definition of `L`.

Conceptually, choose an estimator from the admissible class `F_L` to minimize
error to the full-horizon reference over representative contacts:

$$
F^*_{S,L}\in\operatorname*{arg\,min}_{F\in\mathcal F_L}
\mathbb E_D\left[
\mathcal D_{\rm ref}\left(F(D;S,L),r_S^*(D)\right)
\right].
$$

`D_ref` is a source-time-aligned comparison of the estimated and reference
trajectories. Its exact weighting remains under development; it must assess
position together with the relevant direction/curvature quality and endpoint
behavior. Matching positional MSE alone does not establish visually equivalent
smoothness. Compare the reference and estimate at the same source times rather
than hiding phase error with unrestricted nearest-point matching.

This is the development objective for choosing and tuning an estimator. The
online filter cannot evaluate the unavailable future or query the completed
reference during drawing. No real-time filter formula, polynomial degree,
kernel, optimizer, prediction model or preview-settlement equation is forced by
this record. A full-horizon target must first have acceptable artistic behavior.

The implementation must keep target strength fixed when changing `L`, preserve
all delivered input resolution, keep committed geometry independent of preview,
and respect its publication and computational budgets. Lookahead, the target's
intrinsic following behavior, event-delivery delay and frame/presentation latency
are different quantities. A fixed lookahead alone does not establish low
input-to-photon latency or perfect equivalence to a full-stroke solve. Unknown
future endpoints are one unavoidable source of approximation error.

Preview is a separate provisional display of the same evolving committed target.
Its final geometry must converge to that target. Its formula and temporal
settlement behavior remain outside this reference decision.

## External references

- [Casiez, Roussel and Vogel, *1 Euro Filter*][one-euro]: speed-dependent jitter
  and latency tradeoffs in interactive input; not our proposed fidelity law.
- [McCrae and Singh, *Sketching Piecewise Clothoid Curves*][clothoid]: geometric
  fairness, curvature profiles and sketch approximation.
- [Buchegger, Jüttler and Kapl, *Total curvature variation fairing for medial axis
  regularization*][curvature-tv]: curvature total variation as a fairing objective.
- [NIST, *Weighted Least Squares Regression*][weighted-ls]: confidence weighting
  when measurement precision genuinely varies.

[one-euro]: https://gery.casiez.net/publications/CHI2012-casiez.pdf
[clothoid]: https://www.dgp.toronto.edu/~mccrae/projects/clothoid/sbim2008mccrae.pdf
[curvature-tv]: https://www.ag.jku.at/pubs/2014bjk.pdf
[weighted-ls]: https://itl.nist.gov/div898/handbook/pmd/section1/pmd143.htm
