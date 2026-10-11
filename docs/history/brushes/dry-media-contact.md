# Dry media: analytic relief and paper contact

[Brush design records](README.md)

This record fixes the intended dry-media model and explains its decisions. It
covers pencil, charcoal, chalk, pastel and crayon through one contact evaluator.
The equations describe the accepted design, not a claim that the editor or every
analytic integral is implemented. The
[contact engine guide](../../internals/contact-brush-engine.md) describes current
code. The [earlier redesign](../dry-media-brush-design.md) supplies the preceding
stroke-engine context.

## Accepted design

Dry media deposit pigment where an analytic contact surface engages a stationary
paper height field. Pressure recruits contacts; pigment loading strengthens
contacts that already engage. Relief varies with tilt through three authored
poses. The edge taper comes primarily from contact loss, including empty paper
contacts near the boundary, rather than a radial opacity fade.

**Authoring contract:**

- **One analytic relief field per pose.** Predefined shapes and constrained
  profiles supply efficient evaluation and integrals. There is no arbitrary
  author code, separately authored support mask or pressure distribution map.
- **Three poses.** Fixed upright, writing and side keys have authored dimensions
  and relief profiles within one brush-wide supported shape family.
- **Three shared material controls.** Paper strength `S`, contact softness `W`
  and loading pressure response `Lambda` apply across all poses.
- **Existing pigment and dynamics controls.** Brush size, pressure response,
  pressure-size response, flow, opacity and color retain their existing roles.
- **No separate relief strength or bite control.** The relief field contains its
  values directly. A fixed paper-relief coupling makes selectivity evolve with
  that field.
- **Paper can be a bitmap.** A procedural paper recipe can also be baked into a
  cached height texture. Bitmap contact/relief fields are outside this design.
- **No forced maximum-pressure contact.** Some valleys and recessed edge areas
  may remain empty at maximum pressure.

### Contact and loading

At one canvas point inside the resolved analytic shape:

```text
p = clamp(pressure_response(stylus_pressure), 0, 1)

h = clamp(0.5 + S * (paper_height - 0.5), 0, 1)
d = 0.5 - h

T = R + (1 + kappa * R) * d              kappa = 0.25
x = clamp((0.5 * p - T) / W, 0, 1)

C = x*x * (3 - 2*x)
L = (1 - Lambda) + Lambda*p
local_deposition = C * L
```

Outside the shape, `C = 0`. These equations are conditioned on an active painting
contact; pen-up and zero-force endpoint handling remain explicit input/stroke
rules. The nonzero floor in `L` does not establish a pen contact or independently
guarantee zero deposition at zero pressure.

| Quantity | Meaning and source | Authoring location |
| --- | --- | --- |
| `stylus_pressure` | Normalized device pressure. | Device input. |
| `pressure_response` | Existing input calibration and brush pressure mapping; the simple approximation is a clamped gain. | Existing pressure controls; no new per-pose map. |
| `p` | Effective pressure in `[0,1]`. | Calculated. |
| `paper_height` | Paper height in `[0,1]`; larger values are peaks. Coordinates are stationary in document space. | Selected paper asset and its feature scale/orientation. |
| `S` | Strength of paper-height variation; `S >= 0`. | Brush-wide Paper strength. |
| `h`, `d` | Adjusted height and centered paper depth. | Calculated. |
| `R` | Nonnegative local recess in the model's normalized height units; smaller values engage sooner. | Analytic relief profile in each pose. |
| `kappa` | Relief-dependent paper-selectivity coupling. | Engine constant `0.25`. |
| `T` | Engagement threshold. | Calculated. |
| `W` | Positive engagement interval from first contact to full transfer. | Brush-wide Contact softness. |
| `x` | Clamped local engagement, not radial position or pen pressure itself. | Calculated. |
| `C` | Contact response in `[0,1]`. | Calculated; cubic engine transfer for this design. |
| `Lambda` | Loading pressure response in `[0,1]`. | Brush-wide material control. |
| `L` | Pigment loading conditional on contact. | Calculated. |
| `0.5`, `3`, `2` | Height/approach normalization and cubic endpoint coefficients. | Engine constants. |

### Tilt and contact coordinates

Let `theta` be tilt away from upright, expressed in the same angle units as the
three ordered pose keys `theta_0 < theta_1 < theta_2`. Between keys `i` and `i+1`:

```text
t = clamp((theta - theta_i) / (theta_(i+1) - theta_i), 0, 1)
w = t*t * (3 - 2*t)
resolved_parameter = (1 - w)*parameter_i + w*parameter_(i+1)
```

Use the nearest endpoint pose outside the authored angle range. Resolve
orientation along the shortest angular path. Shape transitions use predefined
morph rules within a supported family; an enum identifying a shape is not a
quantity to interpolate.

For resolved dimension ratios `(a,b)`, orientation `psi` and normalized offset
`o`:

```text
radii = brush_size * pressure_size_response(p) * (a,b)
(u,v) = rotate(canvas_point - stroke_center, -psi) / radii - o
```

The division is component-wise. `brush_size` is the nominal radius convention
used by the engine; `a` and `b` are positive pose dimensions relative to it.
`psi` combines the transformed stylus direction with the authored orientation
offset. The upright/absent-direction case uses a stable engine orientation
policy. Neither zoom nor canvas rotation changes the material's tilt magnitude.

### Analytic relief

The implemented family uses an ellipse or a beveled block:

```text
ellipse: q = u*u + v*v
block: edge = max(abs((u,v)) - face, 0) / rim
       q = (edge.x + edge.y)^2
support: q <= 1

R(u,v) = r0 + c2*q + c4*q*q
```

Use nonnegative `r0`, `c2` and `c4`. `r0` sets the central recess, `c2` gives a
rounded rise, and `c4` gives a flatter center with a steeper rim. These are field
values, without a separate relief-depth multiplier. The block's flat face has
constant recess; its bevel approximates a small rounded corner. The corner width
is brush-wide and scales with the smaller semiaxis. Three fixed tilt keys at 0°,
45° and 76.5° interpolate dimensions and coefficients; the pose editor is deferred.
Directional skew, offsets and changes between different shape families are also
deferred. Tilt azimuth and anisotropic dimensions provide the current alignment.

A future constrained relief widget can produce these coefficients from presets
and curve controls; built-in definitions currently provide them directly. They are the field definition itself. There is no additional
`relief_strength * R` multiplier. The polynomial family is fixed in code; a
freehand curve or expression does not become arbitrary executable shader math.

Resolve geometry and profile coefficients with tilt, then evaluate the resulting
field. Do not blend separately rendered marks or independently normalized maps.
Circle and ellipse share one family naturally. Rectangle and rounded-block
families need their own predefined support, profile and morph/integral rules.
The ellipse equation does not promise that all other shapes use the same `q`.

| Authoring quantity | Current definition |
| --- | --- |
| Tilt key | Engine-fixed 0°, 45° and 76.5°. |
| Contact shape | Brush-wide ellipse or beveled block. |
| Width and length | Positive relative dimensions in each pose. |
| Corner width | Brush-wide block rim fraction. |
| Relief profile | Nonnegative `r0`, `c2`, `c4` in each pose. |
| Alignment | Transformed stylus azimuth; no additional per-pose offset. |

The pose editor is deferred. Key editing, directional skew and morphing across
different shape families require their own authoring and integral decisions.

`S`, `W` and `Lambda` are shared brush controls, not additional controls on these
pose keys. Exact widget layout and shape-specific curve handles are not fixed by
this mathematical contract.

### Swept exposure and pigment accumulation

For a fixed resolved pose and a canvas point, let `I_segment` be the swept
segment's portion of the contact chord, and `ell_full` the full geometric chord
length through that point:

```text
E = (1 / ell_full) * integral_over_I_segment(C * L) d_ell
Delta_D = K * E
D_new = D_old + Delta_D
alpha = 1 - exp(-D_new)
```

The chord is normalized using geometric support, not the fragment length or the
number of engaged paper contacts. Empty/outside support contributes nothing.
The stationary-tap limit evaluates `C*L` using the engine's tap exposure rule,
without dividing by a zero chord.

`E` is renderer exposure. `K` folds in existing flow, color opacity, directional
feed and optical normalization; it is not another new editor control. `D` is a
conceptual additive optical dose, not a requirement to store a new physical
pigment layer in the artwork. `alpha` describes ordinary same-color flow
accumulation. Existing blending, selection and other accumulation modes retain
their own renderer contracts.

Resolve pose parameters on the CPU per retained input/contact, and send compact
resolved data to the GPU. Use predefined fixed-pose integrals and bounded
segmentation/interpolation for changing pressure and tilt. The design does not
claim a closed-form integral for an arbitrary continuously deforming tip.

## Decision history and mathematical rationale

The choices below separate accepted approximations from physical laws. A rejected
default can still be useful for another brush family. In particular, linear
contact remains an open alternative for a later visual/performance comparison;
the current cubic is not declared universally optimal.

### 1. Pressure to approach: `A = 0.5p`

**Purpose.** Convert effective pen pressure into a normalized approach distance
that recruits paper peaks before valleys. The pressure response is brush-wide;
there is no authored two-dimensional pressure map inside the tip.

**Choice and constant.** Linear approach keeps the response monotone and cheaply
integrable. The factor `0.5` matches the chosen centered paper-height units. It
is a calibration convention, not a measured force/displacement coefficient.
Increasing it makes a given pressure reach more recessed contacts; changing it
requires coordinated recalibration of relief and response widths. Existing
pressure mapping already handles sensitivity without changing this constant for
every pose.

**Alternatives.** A mechanical force-balance solve relates applied force to
indentation more faithfully but requires material properties, contact-area
feedback and potentially iterative work. It is outside this inexpensive model.
An additional power curve for approach is unnecessary while the existing
pressure response supplies that authoring behavior. A local multiplier
`p * relief_weight` is rejected as the default: a small weight can keep an edge
weak at every pressure, rather than letting it engage once approach exceeds a
geometric threshold. Separately authoring that multiplier also restores the
pressure map the design removes.

**Research basis.** [Greenwood and Williamson][rough-contact] motivate recruiting
asperities by separation and distinguish load from indentation. The
[crayon model][crayon] solves contact displacement from force. Our linear
conversion intentionally approximates that mechanical step; neither source
derives the coefficient `0.5`.

### 2. Paper response: `h = clamp(0.5 + S(H - 0.5), 0, 1)`

**Purpose.** Keep one stationary surface responsible for both interior tooth and
edge contact loss. `S=0` removes height variation, `S=1` preserves the asset's
height values, and larger `S` increases contrast until heights clip.

**Choice and constants.** Centering around `0.5` changes variation around a fixed
reference instead of translating the whole surface. The `[0,1]` bound keeps the
threshold response well behaved. Clipping at high strength can create plateaus;
it is intentional bounded contrast, not a physically simulated deformation.
Paper feature size and orientation come from texture coordinates, not `S`.

**Alternatives.** Multiplying final opacity by noise produces pale pigment even
where contact should be firm, and does not give geometry-based empty contacts.
Randomly rotating rough dabs ties grain to stamp placement and adds another
changing field; it is not needed for this stationary-paper model. A nonlinear
paper remapping can be baked into the asset if useful, rather than adding
another response function to every deposited pixel. Automatic histogram
normalization is rejected at runtime because changing an asset's level or tooth
should retain its authored meaning. Paper strength per pose duplicates controls
that the relief profile is intended to supply.

**Research basis.** [Sousa and Buchanan][graphite] describe height-dependent pencil
deposition. [Rudolf, Mould and Neufeld][crayon] represent paper as a height field.
The centered affine strength adjustment is our inexpensive authoring convention,
not an equation taken from either paper.

### 3. Tilt interpolation: three keys with cubic weights

**Purpose.** Represent the recognizable upright, writing and side postures while
allowing tilt to change continuously within a stroke. The same key weights
resolve dimensions, supported shape parameters and relief coefficients.

**Choice and constants.** The cubic weight has zero slope at each key. Adjacent
intervals meet smoothly, including at the writing pose. Authored key angles
determine transition locations and widths. There are no universal pencil-angle
constants: device normalization and brush design are separate concerns.

**Alternatives.** Hard three-state selection introduces visible size/profile
jumps and is rejected. Linear key weights are inexpensive but introduce slope
changes at keys; they remain a possible simplification if those changes prove
irrelevant. A single universal tilt-to-shape formula is elegant for one material
but constrains authors to its particular upright/writing/side relationship.
Arbitrary author functions need validation and lose the bounded predefined
integral contract. Dense lookup tables add storage and interpolation error
without being necessary for these short formulas. The selected three-key model
is one continuous evaluation rule with two interpolation intervals, not three
disconnected rendering modes.

**Research basis.** Pencil position and tip shape are inputs to the
[graphite model][graphite]. The easing polynomial is
[standard Hermite smoothstep][smoothstep]. Neither source prescribes three poses;
the key structure is an authoring decision.

### 4. Contact coordinates and analytic relief

**Purpose.** Describe where the tip lies and how recessed each point is, using
one field that controls contact onset, cross-sectional taper and paper selection.
Geometry accounts for the long side of a pencil or the face of a pastel stick.

**Choice and constants.** Translation, rotation and division by resolved radii
put each shape in normalized coordinates, with `q=1` at its boundary. The
`r0 + c2*q + c4*q*q` basis provides onset and curvature without arbitrary
exponents. A central recess is useful for a lifted shading face; it is part of
the authored field rather than an additional strength parameter. Nonnegative
coefficients keep recess monotone in `q` and permit its inverse through one
quadratic root. No universal coefficient values are prescribed.

Along a fixed-pose straight sweep, `q` is quadratic within each admitted piece.
Relief is at most quartic, and cubic contact is at most degree twelve. This bounds
integration and clipping work. A stick uses the square of the sum of normalized
excursions beyond its flat face. Its support is an octagon; four slab intersections
provide the chord without circular corner roots. Only the small corners differ
from a rounded rectangle. The flat face, outer dimensions and monotone rim retain
the useful stick controls.

**Alternatives.** Circular corner arcs were considered for sticks; they require
quadratic boundary intersections without adding a necessary media control.
Superellipse exponents introduce higher-degree pieces or numerical roots. A
multiplicative directional skew `(c2*q+c4*q*q)*(1+b_skew*u)` keeps recess
nonnegative when `abs(b_skew)<=1`, but can lose outward monotonicity and raises
relief to degree five. Inverting that field for engagement needs more complex
roots; it is deferred while elongated tilt poses provide the required shading
behavior.

A separately authored mask duplicates shape support. A separate
pressure map supplies independent local transfer but adds a second field and
can restore a spatial opacity gradient; the intended dry-media marks do not
require it. An additional relief-strength multiplier duplicates the vertical
scale already contained in the authored field. Arbitrary analytic expressions
cannot guarantee cheap swept integrals. Bitmap relief may be inexpensive for
sampled evaluation, but has no general closed-form swept integral and is
deferred, not declared inherently too slow. Irregular or wedge shapes are not
forbidden in principle; they need predefined geometry and justified artist value
rather than unrestricted mask editing.

**Research basis.** The [crayon model][crayon] separates tool surface geometry from
material transfer. The [graphite model][graphite] considers tip position and
shape. Our polynomial basis and absence of a separate pressure field are
computational and interface approximations.

### 5. Threshold: `T = R + (1 + kappa R)d`

**Purpose.** Compare approach with a paper-and-tip threshold. Centered depth
`d=0.5-h` is negative at peaks and positive in valleys; it is a signed offset,
not a claim that physical valley depth is negative. Larger recess delays
engagement. The interaction makes paper selectivity change naturally with the
relief field and therefore with tilt.

**Choice and constant.** The additive baseline `T=R+d` is a valid geometric
approximation. The selected weak coupling uses `kappa=0.25` to add some
relief-dependent selectivity without another authored parameter. It is an engine
calibration choice, not a physical material constant. For `R>=0`, `d` in
`[-0.5,0.5]` and `0<=kappa<2`:

```text
dT/dR = 1 + kappa*d > 0
dT/dh = -(1 + kappa*R) < 0
```

Thus recess always reduces contact, peaks always engage more easily, and raising
pressure increases contact for a fixed pose. At `kappa=0.25`, the first derivative
lies between `0.875` and `1.125`. For fixed paper, the threshold remains affine in
relief: `T=d+(1+kappa*d)R`. The interaction therefore preserves the predefined
relief's polynomial degree. Increasing `kappa` strengthens coupling; approaching
or exceeding `2` risks losing strict monotonicity in recess.

**Alternatives.** `kappa=0` remains the simpler additive baseline, not a physically
invalid alternative. Stronger coupling, including `kappa=1`, is not the default
because it changes onset/selectivity more strongly than needed. A raw valley
depth product `R+(1+kappa*R)(1-h)` needs a consistent approach offset and relief
calibration; otherwise centering differences get mistaken for a different
material response. A bounded interaction such as `1+R/(1+R)` adds nonlinear
relief dependence and complicates the integral without a current requirement.
Independent per-pose paper response/bite adds a control the shared field is
intended to replace.

Clamping the upper threshold to ensure every point engages at maximum pressure
is rejected: permanently empty valleys or edge areas are permitted. Pure
`pressure*relief_weight-paper_depth` is rejected for the onset/ceiling coupling
described in section 1. A reference-relative interaction
`1+kappa*(R-R_reference)` is a possible refinement if arbitrary height-reference
changes must be invariant. It is not part of this contract: the predefined
field's height reference is fixed, and it must not be silently renormalized per
pose.

**Research basis.** [Rough-surface contact theory][rough-contact] motivates
separation thresholds. Drawing-specific height engagement is described by the
[graphite][graphite] and [crayon][crayon] models. The weak bilinear interaction is
our approximation; those sources do not derive `kappa=0.25`.

### 6. Engagement width: `x = clamp((A-T)/W, 0, 1)`

**Purpose.** Give eligible contacts a finite interval over which transfer becomes
strong. With fixed `T`, first contact is at `A=T`, half response at `A=T+W/2`, and
full response at `A=T+W`. Since `A=0.5p`, the unclipped pressure interval is `2W`.

**Choice and controls.** `W>0` prevents division by zero. Small `W` makes eligible
contacts strengthen quickly; large `W` leaves more partial transfer. It does not
change the empty set `A<=T`. `W` is artistically distinct from relief, which
changes onset. It is therefore exposed as one brush-wide Contact softness
control with a calibrated default. It is not another pose field or a pixel blur
radius. This record does not prescribe a universal default or maximum.

**Alternatives.** A universal hidden constant would be simpler, but independent
engagement softness has a meaningful authoring role. Automatically choosing `W`
from paper variance can counteract intentional paper-strength changes.
Automatically choosing it from relief range/slope can counteract an intentional
contact profile. Per-pixel or pressure-dependent widths add another varying
field and may complicate integration. Per-pose widths expand the editor before
there is a demonstrated design need. None is the selected default.

Map analysis can recommend a starting value or identify a large partial-contact
fraction across pressure and tilt. It cannot uniquely infer material behavior
from geometry. Antialiasing must account for pixel coverage separately; widening
`W` with zoom or brush size would change the material response.

**Research basis.** [Smoothstep][smoothstep] supplies a finite transition interval.
The [crayon model][crayon] includes material compression separately from surface
geometry. Our width is a phenomenological engagement control, not its compression
constant or a solved elastic modulus.

### 7. Contact transfer: `C = x*x*(3-2*x)`

**Purpose.** Suppress barely engaged transfer and approach full transfer smoothly.
The function can approximate unresolved engagement within a sampled paper grain;
it is not a literal microscopic contact simulation or final opacity.

**Choice and constants.** Requiring `C(0)=0`, `C(1)=1`, `C'(0)=0` and `C'(1)=0`
uniquely gives `3x*x-2x*x*x` as the lowest-degree polynomial. Its derivative
`6x(1-x)` is nonnegative. Near first touch it behaves as `3x*x`; near full
engagement the shortfall behaves as `3(1-x)*(1-x)`. The coefficients `3` and `2`
are fixed by endpoint conditions, not independent artistic settings.

**Alternatives.** A hard step provides binary grains and can simplify clipped
chord coverage, but needs appropriate spatial/swept averaging to avoid abrupt
changes. Linear `C=x` is the cheapest continuous candidate and can materially
simplify swept integrals; it remains open for the later comparison. Quadratic
`C=x*x` is inexpensive but asymmetric and has a slope discontinuity at its upper
clamp. Quintic `6x^5-15x^4+10x^3` smooths the second derivative at the endpoints
but adds polynomial work without a requirement for that continuity. General
sigmoids, powers and lookup curves add calibration or evaluation work without a
current authoring need.

The cubic is optimal only within the stated lowest-degree endpoint constraints.
It is not proven fastest or visually best for every dry material. For quadratic
engagement along a fixed sweep, the unclipped linear, cubic and quintic
integrands have degrees two, six and ten respectively. Clipping and integration
strategy still determine actual runtime.

All these positive continuous curves retain the same exactly empty set. The
desired missing contacts near edges come from the threshold and relief. A broad
`W` can still produce a gradient with cubic contact; changing its coefficients
is not a substitute for appropriate contact recruitment.

**Research basis.** The exact cubic is [WGSL's Hermite smoothstep][smoothstep].
Its use as contact transfer is our approximation, not a transfer law established
by the drawing-material papers.

### 8. Loading: `L = (1-Lambda) + Lambda*p`

**Purpose.** Control pigment amount at an engaged contact separately from
recruiting contacts. A paper peak already at `C=1` can continue depositing more
as pressure rises. Relief alone cannot supply that behavior while keeping the
contact saturated.

**Choice and constants.** `L` blends constant loading and pressure-proportional
loading. `Lambda=0` makes pressure act through engagement and existing geometry;
`Lambda=1` makes per-contact loading equal to pressure. Intermediate values
retain strong individual grains during light shading while allowing additional
pressure buildup. The high-pressure endpoint is normalized to one because flow
already controls overall strength. The affine form is the unique straight line
between `L(0)=1-Lambda` and `L(1)=1`, with one free parameter.

For `0<=Lambda<=1`, loading is bounded, monotone and lies between `p` and `1`.
Its slope is `Lambda`. The floor is conditional on contact, not a minimum stroke
opacity. Multiplication by `C=0` still deposits nothing. Increased loading can
darken an engaged grain without filling a genuinely empty contact.

**Alternatives.** Constant loading and pure `p` are retained as endpoints of the
family. `(1-Lambda)+Lambda*p*p` cheaply delays loading until firmer pressure and
remains a legitimate future material response. A smoothstep or general exponent
of `p` provides more shaping but lacks a current requirement. Such loading can
often be resolved per contact on the CPU, so it is not inherently expensive per
pixel; nonlinearity can still increase integration complexity when pressure
varies within a sweep. Tying pigment amount to radial position or separately
authored local pressure restores a spatial dose fade that this design avoids.

`Lambda` is separate from bite: it changes pigment amount, not the contact
threshold. No universal material values are prescribed by this record.

**Research basis.** The [graphite model][graphite] distinguishes pigment transfer
from paper damage. The [crayon model][crayon] relates transfer to contact and
friction. They motivate a separate material response, not universally affine
loading. Their material coefficients must not be confused with our `Lambda`.

### 9. Swept exposure: normalized contact integral

**Purpose.** Integrate the material response along the traveled contact rather
than inserting many randomly rotated stamps. Normalize by the full geometric
chord so partitioning a steady, fixed-pose passage does not add pigment.

**Choice and constants.** A split segment integrates over disjoint chord
intervals with the same denominator; its exposures add to the unsplit result.
For a full chord with `C=L=1`, `E=1`. The geometric denominator does not expand
when paper loses contact, so sparse grain remains a smaller dose rather than
being normalized back to solid coverage. Feed and world-travel scaling remain
part of the existing stroke-engine calibration.

**Alternatives.** Normalizing each fragment by its own length lets splitting
increase pigment and is rejected. Normalizing by only engaged contacts restores
dose lost through grain and defeats the paper response. An unnormalized length
integral changes dose with contact size unless feed is recalibrated; it is not
the chosen continuation of this engine. Dense stamp subdivision approximates
the field but increases work with path length. An exact integral through an
arbitrarily moving/deforming shape is outside the fixed analytic-family
contract. Resolve poses and bound changes with shared stroke segmentation.

**Pixel footprint.** Point sampling aliases fine pencil contacts as they move
between pixel rows. Filtering geometric exposure alone and then applying an
exponential can overfill a partially covered pixel, because optical transfer is
nonlinear. The fine-contact path averages optical alpha over the pixel footprint
and converts it back to equivalent dose. A moving ellipse uses the pixel
square's normal projection; a stationary tap uses two dimensions. Dense stamp
subdivision is rejected because its cost grows with distance and can change dose
with partitioning. Filtering is fully enabled below four pixels in minor radius,
blends out by eight pixels, and is specialized away for large contacts. These are
fixed approximation thresholds, not new brush controls.

**Research basis.** The [existing contact engine](../../internals/contact-brush-engine.md)
provides normalized swept exposure. The drawing-material papers motivate local
deposition, not this particular chord normalization. Segment additivity is a
mathematical requirement of the chosen integral, not an empirical claim. The
clipped seven-node Gauss–Legendre identity integrates the degree-twelve contact
polynomial exactly; the [NIST quadrature reference](https://dlmf.nist.gov/3.5#v)
gives its degree-thirteen exactness bound. The pixel-footprint approximation
follows area coverage and optical averaging rather than a new physical law.

### 10. Dose and opacity: `alpha = 1-exp(-D)`

**Purpose.** Let repeated engaged passes deepen color smoothly while keeping
opacity bounded. With nonnegative additive dose, `alpha` rises monotonically
toward one. A contact can become optically dark without requiring every nearby
paper contact to engage.

**Choice and constants.** Scaling is folded into `K`, leaving the displayed
exponent coefficient at one. Small doses give approximately linear opacity;
large doses have diminishing visible effect. For ordinary same-color source-over
flow, adding doses is equivalent to multiplying transmittances:

```text
1 - alpha_combined = exp(-D1) * exp(-D2) = exp(-(D1+D2))
```

This supports batched dose accumulation. It does not require per-pixel pigment
mass, paper deformation or a new artwork storage representation.

**Alternatives.** Directly clipping a linear opacity sum saturates abruptly and
does not preserve this compositing identity. Applying a nonlinear tone curve
before integrating changes the accumulation behavior. Full Kubelka-Munk
scattering, material transport, crayon smearing and persistent paper damage
represent additional phenomena beyond this contact model. They are deferred,
not claimed to be reproduced by tuning `Lambda` or relief.

**Research basis.** [Physically Based Rendering's transmittance treatment][transmittance]
gives exponential attenuation and additive optical thickness. Here it inspires
a simple opacity approximation, not complete graphite or wax optics. The
[crayon model][crayon] uses a richer scattering treatment that this design does
not adopt.

## Intended artistic behavior and remaining choices

| Scenario | Intended behavior | Primary authoring decisions |
| --- | --- | --- |
| Heavy pencil outlines | Concentrated contact, dark buildup, with optional surviving tooth and edge gaps. | Upright/writing geometry and relief; loading and flow. |
| Normal pencil drawing | Pressure recruits a wider set of contacts and strengthens engaged peaks. | Writing profile, pressure-size response and `Lambda`. |
| Light pencil shading | Sparse, individually visible grain instead of a uniformly pale radial fade. | Broad side profile, paper strength and modest loading pressure response. |
| Charcoal line | Strong engaged grains with a distinct contact boundary. | Narrower geometry, paper asset and relief taper. |
| Broad charcoal shading | Broad contact with interior tooth and increasing empty contacts toward its edges. | Side dimensions and rim relief; `W` governs partial transfer. |
| Chalk, pastel or crayon face | A block or rounded face whose coverage and grain can range from broken to more uniform. | Shape family, flatter profile and paper strength. |

These are design goals, not empirical results or claims of a complete material
simulation. Hard-pencil paper flattening, tip wear, powder transport and wax
redistribution need history/state beyond a static paper field.

The contract fixes the equations and ownership of controls. Built-in recipes
author `S`, `W`, `Lambda`, pressure response and relief coefficients in the
shared [preset definitions](../../../crates/layer-core/src/contact_presets.rs).
The pose editor and curve widget remain deferred. Linear versus cubic contact
remains open for the later benchmark; no change of editor interface is required
to compare them. Bitmap relief and arbitrary contact math remain outside scope.

## External references

- [Greenwood and Williamson, *Contact of Nominally Flat Surfaces* (1966)][rough-contact],
  Proceedings of the Royal Society A, 295, 300–319; DOI
  `10.1098/rspa.1966.0242`. Basis for rough-surface separation/contact reasoning.
- [Sousa and Buchanan, *Observational Models of Graphite Pencil Materials* (2000)][graphite],
  Computer Graphics Forum, 19(1), 27–49. Drawing-specific height, pressure,
  transfer and paper-damage model.
- [Rudolf, Mould and Neufeld, *A Bidirectional Deposition Model of Wax Crayons*][crayon].
  Contact geometry, force, friction, wax redistribution and scattering model.
- [W3C, WebGPU Shading Language: `smoothstep`][smoothstep]. Normative definition
  of the cubic Hermite transfer used for contact and pose interpolation.
- [Pharr, Jakob and Humphreys, *Physically Based Rendering*, fourth edition:
  Transmittance][transmittance]. Exponential attenuation and optical thickness.

[rough-contact]: https://ks3-cn-beijing.ksyun.com/attachment/919c460af08014143cb9478ff11fbab7
[graphite]: https://visgraf.impa.br/Courses/npr07/materials/2%20lapis/Observational%20Models%20of%20Graphite%20Pencil%20Materialspdf.pdf
[crayon]: https://people.scs.carleton.ca/~mould/papers/crayon-cgf.pdf
[smoothstep]: https://www.w3.org/TR/WGSL/#smoothstep-builtin
[transmittance]: https://www.pbr-book.org/4ed/Volume_Scattering/Transmittance
