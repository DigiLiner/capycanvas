# Curve fit smoother

[Brush design records](README.md)

**Status: implemented; further quality tuning remains open.** This record describes the
implementation snapshot at revision
`27de272963752b22219b63b9bc2b4369b95d20be` on `pen-smoothing-comparison`.
That implementation is separate from the accepted
[full-horizon stabilization objective](stroke-stabilization.md). It does not
solve that objective or establish an optimal approximation for any lag budget.
The production implementation retains the Curve fit and tail fixes from this
snapshot. Exponential, Moving average and the algorithm selector are omitted.

This is an algorithm design snapshot, not an empirical results log. Constants
below are values in that source revision, not validated perceptual optima or
claims about a particular digitizer.

## Source map and controls

The following paths refer to the named revision:

| Source | Responsibility |
| --- | --- |
| `crates/layer-engine/src/brush/smoothing.rs` | Strength, speed adjustment, interval estimation and retained smoother state. |
| `crates/layer-engine/src/brush/smoothing/curve.rs` | Positive polynomial kernel, clock interpolation, moment integrals, evaluation and derivative bounds. |
| `crates/layer-engine/src/brush/smoothing/preview.rs` | Canonical real-input model, forecast continuation, displayed endpoint and pen-up settlement. |
| `crates/layer-engine/src/brush.rs` | Adaptive curve evaluation between delivered samples, brush contacts and completion. |
| `crates/layer-engine/src/canvas.rs` | Real/provisional stroke ownership, forecast acquisition, preview updates and contact completion. |
| `crates/layer-core/src/lib.rs` | `BrushStabilization` and defaults. |
| `crates/layer-ui/src/tool_settings.rs` | Shared strength and speed controls. |

Production offers Curve fit with normalized strength `S` in `[0,1]`, displayed as
0–100, initially 6 with Adjust by speed off. These controls are independent of the
reference objective. Slider travel is nonlinear, with strength 30 at 75% travel.

Strength zero bypasses positional smoothing. Curve-model preview and its
additional pen-up settlement are disabled at zero. Generic input/presentation
timing and brush contact placement remain separate from this bypass.

## Committed position model

### Strength and speed-dependent support

The nominal time constant is:

$$
\tau_0=0.1S\quad\text{seconds}.
$$

Speed and input-interval estimates use the elapsed-time response
`alpha_v = 1-exp(-Delta_t/0.015)`. Target speed is the distance between delivered
positions divided by `Delta_t`, multiplied by the captured `input_scale`.
Adjust by speed sets:

$$
\tau=\tau_0\left[1+b\left(\max(0,1-\hat v/600)\right)^2\right],
\qquad b\in\{0,1\}.
$$

It increases low-speed time support by up to two times and keeps nominal support
at sufficiently high speed. The 600-unit speed scale and 15 ms estimator response
are engineering calibration constants. This policy changes filter support; it
is not the relative-motion fidelity term of the accepted reference.

Let `Delta_hat` be the estimated input interval. The support `H` is calibrated
to the discrete exponential smoother's independent sample-noise variance gain:

$$
g_{\rm EMA}=\tanh\left(\frac{\widehat\Delta}{2\tau}\right).
$$

The calibration computes the curve kernel's weights on linearly reconstructed
samples, sums their squares and searches for matching gain. The source uses a
closed-form short-support branch when `g_EMA >= 5/9`, otherwise 28 bisection
iterations. Before an interval estimate is available, and asymptotically at high
input rates:

$$
H\approx\frac{200}{63}\tau.
$$

Matching this noise gain does not establish equal curvature quality, perceptual
strength or lag between algorithms.

### Positive convolution in a continuous clock

For each sample, the target clock rate is `a_i = 1/H_i`. Between two delivered
times separated by `Delta_t`, write `u=(t-t_i)/Delta_t`. The clock uses:

$$
a(t)=a_i+(a_{i+1}-a_i)(3u^2-2u^3),
$$

$$
z(t)=z_i+\Delta t\left[a_i u+
(a_{i+1}-a_i)\left(u^3-\tfrac12u^4\right)\right].
$$

Sample clock increments therefore use the mean of adjacent rates. The smooth
rate interpolation makes `z(t)` twice continuously differentiable across
intervals even when support changes between samples.

Let `q(z)` be piecewise linear through all real delivered positions in clock
coordinates, with constant extension before the first and after the latest
sample. Evaluate:

$$
h(u)=20u(1-u)^3\quad(0\leq u\leq1),
\qquad h(u)=0\quad\text{otherwise},
$$

$$
\boxed{y(t)=\int_0^1h(u)q(z(t)-u)\,du.}
$$

The kernel is normalized and nonnegative. Positions are convex combinations of
the reconstruction within support, so the kernel itself adds no excursion
outside its input convex hull. This does not prove that every curved input is
free of loops, that the nearest point stays close, or that forecasts are correct.

For continuous piecewise-linear input, convolution with this compact continuous
kernel produces continuous second derivatives in clock coordinates. Composition
with the smooth clock retains temporal `C2` continuity in the mathematical
model. The delivered input and rendered contacts remain finite precision.

At fixed support, the kernel's mean age is `H/3`. Its steady straight-ramp
following delay is therefore approximately:

$$
\delta_{\rm intrinsic}\approx H/3
\approx\frac{200}{189}\tau.
$$

These are kernel properties, not measured input-to-photon times. They explain why
the snapshot still has intrinsic following lag even without a separate hold.

### Exact moments and output resolution

The kernel is a degree-four polynomial. The source retains prefix integrals of
`q(z)*z^j` for `j=0..4`, using block-local coordinates and positional origins to
limit cancellation. Evaluation combines differences of these moments across at
most two clock blocks. Ordered-node searches locate interval boundaries; input
samples are not discarded to create a coarse fitting grid.

Curve evaluation is distinct from brush-contact placement. The generator
recursively subdivides an input interval using its acceleration bound `A` until:

$$
A(\Delta t)^2\leq8\epsilon,
\qquad
\epsilon=0.05/\max(\text{input_scale},10^{-6}).
$$

It also stops subdivision at intervals of at most one microsecond. This evaluates
the continuous position field between low-rate samples instead of drawing only
chords between filtered sample positions. Swept-contact simplification uses the
same surface-pixel tolerance for enabled Curve fit.

Moment evaluation is bounded apart from ordered searches. Support calibration
uses 28 fixed steps with a constant-work noise-gain formula. Acceleration bounds
reuse cached slope lengths and scan only overlapping nodes. Model cloning copies
retained nodes; contact subdivision depends on the curve. Preview uses the fitted
position field directly while retaining sensor stabilization.

## Provisional curve and prediction

### Real-input model and forecast continuation

`CurveModel` clones the committed smoother and appends all newer real samples.
It defines the canonical real-input field. `CurvePreview` clones that curve once
more and appends an attenuated portion of the forecast at the current support.
Historical positions at or before the latest real sample use the canonical field;
later positions use the continuation.

Because both fields use the same causal convolution and clock up to the join,
appending forecasts does not change the position, tangent or second derivative
at that join. Forecast input does not enter the committed smoother. Prediction
visibility can change forecast availability and generic publication timing, but
is not a parameter of canonical real-input geometry.

The snapshot does not move each provisional vertex using a separate age-dependent
correction. Its retained geometric memory comes from the common convolution.
This avoids creating independent catch-up rates along one preview curve, but
does not prove optimal motion during every forecast revision.

### Displayed endpoint and existing calibration

For `S>0`, the helper `reference_delay` computes a calibration delay
`delta_ref(S)`. Its complete compact form is:

$$
R=\frac{805}{286}(0.1S),\quad
a=0.10026573857943435(1-S)^2,\quad m=0.032S,
$$

$$
P(x)=-\frac{35}{512}+\frac{x}{2}-\frac{105x^2}{128}
+\frac{175x^4}{256}-\frac{49x^6}{128}+\frac{45x^8}{512},
$$

$$
\delta_{\rm ref}=\frac{3m}{7}-R\,\mathbb E[P(a+(m/R)U)],
\qquad U\sim\operatorname{Beta}(3,4).
$$

The code expands this expectation using
`E[U^k] = product_(i=1..k)((i+2)/(i+6))`; it performs no random sampling. This is
an existing preview-length calibration, not the committed smoothing kernel or a
new stochastic process model. It is a candidate for replacement during tuning.

The fraction of available forecast retained is:

$$
\ell=\operatorname{clamp}(1-\delta_{\rm ref}/0.008,0,1).
$$

Forecasts are appended through `t_latest + ell*forecast_horizon`; a fractional
final interval is interpolated. Define:

$$
g=\frac{\max(\delta_{\rm ref}-0.008,0)}{H},
\qquad f(r)=r^5(1-2r/3).
$$

The source searches `r` in `[0,1]` with 28 bisection iterations, saturating at the
interval boundaries when necessary. If the continuation's last sample time is
`t_cont`, the displayed curve endpoint is evaluated at:

$$
t_{\rm display}=t_{\rm cont}+H-Hr.
$$

The endpoint is a point on the convolved curve. It is not selected by snapping
to an older delivered sample. At stronger stabilization, forecast contribution
is reduced and the displayed endpoint can remain behind the latest delivered
position. Positive convolution bounds any additional kernel excursion relative
to its real and forecast input; it does not validate the predictor's own lead.

## Contact completion

The constant extension of the final real position completes the kernel by
`t_end + H`. At that curve time, the fitted position equals the delivered final
position. `DabGenerator::finish` evaluates and emits this continuation and closes
the remaining contact spacing. This completes geometry rather than appending a
raw straight segment from a lagged endpoint.

With an active curve preview and frame clock, the snapshot retains a pen-up
settlement interval of 32 ms. Let:

$$
Q(u)=10u^3-15u^4+6u^5,
\qquad u=\operatorname{clamp}(\text{elapsed}/0.032,0,1).
$$

The displayed endpoint's curve time moves from a retained start to the complete
curve time using `Q`. The source clamps that start to the interval from the last
real sample time to the complete curve time, and replaces the displayed field
with the real-input completion field. Predicted taper scales by `1-Q`. Contact
corrections retain the current taper when restarting this settlement. Completion
can happen earlier without a frame clock or when a new contact interrupts it.

`Q'` and `Q''` vanish at both ends. For fixed start/end values and a fixed smooth
field, this eases endpoint motion at the boundaries. It does not guarantee
continuity across the field replacement, clamp, corrections or restarts. Nor does
it give an update distance that decreases from the beginning: scalar progress
initially accelerates and then decelerates. The complete final curve is emitted
when the contact finishes; this display interval does not mean that real
post-lift samples exist.

Continuation evaluates virtual curve time while retaining the final real
contact's timestamp and sensor values for brush characterization. Virtual support
completion is not additional physical motion or newly measured pressure/tilt.

The snapshot's exact endpoint completion differs from the accepted reference's
along-travel reach with permitted perpendicular offset. It must not silently
become the endpoint definition for that future objective.

## Optimization still required

- **Reference approximation.** Define and validate an offline solver for the
  accepted objective before claiming approximation error or optimality.
- **Strength calibration.** Independent sample-noise gain is insufficient to
  match visible waviness, corner placement and curvature quality.
- **Lag.** Reduce intrinsic following lag at comparable artistic quality. Kernel
  support, publication hold and input-to-photon latency must remain distinct.
- **Preview.** Preserve continuous curve joins and bounded forecast influence
  while improving revision motion and convergence to the eventual committed
  reference. The current convolution is not proof of that final behavior.
- **Boundaries.** Replace constant-tail exact endpoint completion with the
  agreed constrained fit when implementing the reference approximation.
- **Cost.** Reassess calibration searches, retained-node cloning, acceleration
  scans and adaptive contact work under the real-time budget.

These are remaining design and implementation questions. This record fixes no
performance claim, hyperparameter optimum or measured advantage over the two
comparison algorithms. Future changes should update the snapshot description
without turning the accepted reference record into a benchmark log.
