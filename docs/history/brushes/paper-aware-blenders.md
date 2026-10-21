# Paper-aware blenders

[Brush design records](README.md)

This record fixes how blenders meet the paper and explains the constants. The
[brush guide](../../internals/brushes.md#blenders-and-the-paper) describes current
behavior, and the [GPU brush engine](../../reference/gpu-brush-engine.md) the
kernel.

## Accepted design

A blender that imitates a tool touching paper blends only the paper its tip
reaches. Pressure sets how deep the tip reaches; paint in valleys it cannot
reach stays where it is, so a light pass keeps the tooth and a firm pass pushes
pigment into it. The contact reuses the [dry-media contact
equations](dry-media-contact.md#contact-and-loading) with three constants per
tip, evaluated once per pixel per Smudge chunk:

```text
p      = clamp(pressure_gain · peak pen pressure in the chunk, 0, 1)
h      = clamp(0.5 + S·(paper − 0.5), 0, 1)       S = Texture strength × paper relief
d      = 0.5 − h
R      = recess · (1 − peak coverage)
T      = R + (1 + 0.25·R)·d
x      = clamp((0.5·p − T) / engagement, 0, 1)
C      = x²·(3 − 2x)

exposure *= C                                     blend amount and spread
trace     = pixel − C·(pixel − trace)             Smudge drags only where it touches
kept      = 1 − (1 − exp(−lift · exposure)) · opacity   Tissue lifts pigment
```

Texture strength 0% compiles the paper read away and leaves the blender exactly
as it was. The Blending Knife multiplies its pressure by its run-out load before
the contact, so a long stroke rides higher on the tooth.

| Tip | Pressure gain | Engagement | Recess | Presets |
| --- | --- | --- | --- | --- |
| Finger | 0.93 | 0.08 | 0 | Smudge |
| Brush | 1.0 | 0.1 | 0.4 | Natural Blender, Bristle Blender |
| Round | 1.4 | 0.1 | 0.4 | Soft Blender (0% by default), Grainy Blender |
| Blade | 1.2 | 0.05 | 0 | Blending Knife |
| Stump | 1.6 | 0.08 | 0.4 | Blending Stump |
| Tissue | 1.0 | 0.2 | 0 | Tissue, lift 0.15 |

Blur, Sharpen, Hue, Texture-Keeping, Painterly, Crystal, Marbling, Comb,
Edge-Keeping, Scatter and Dither are digital effects and stay paper-blind. Wet
paint settles into the valleys instead, so the wet blenders keep their own rule.

## Decisions

**Contact, not multiplied grain.** Multiplying the blend by the paper height
saturates: `1 − exp(−rate·E)` reaches one in every valley after enough passes,
and charcoal's tooth filled 83–95% at every pressure. Contact gives valleys
deeper than the reach zero exposure, so only pressure fills them.

**Smudge's calibration.** Four criteria were fixed from artist reports before
sweeping Texture strength on five papers: a light touch (0.3) fills at most 35%
of the bare valleys on toothy papers; a firm finger (1.0) fills at least 90% on
Bristol and Drawing paper; at medium pressure the papers differ by at least 20
points; fill rises from 25% to 75% over at least 0.2 of pressure. At 60% a light
touch filled 96–99%; at 100%, 60% on Drawing paper with papers 4 points apart.
1.5× the dry-media response passed the first three (14%, 99%, 25 points) and
missed the fourth by 0.01, inside that measurement's noise. The finger's gain
and engagement carry that factor (1.4 and 0.12 divided by 1.5), so Texture
strength means the same on every brush and starts at 100%.

**A Blending Stump.** At the same size the paper-aware blenders fill at most
36% of the valleys at medium pressure; the Stump fills 74% and darkens a firm
pass by 58% while carrying paint 28 px past an edge, against Smudge's 110 px.
It is the only blender that fills the tooth without moving the drawing.

**A Tissue only with lifting.** Without lifting it matched the Grainy Blender at
the same size to within 7 points of valley fill and 0.1 on every other measure.
Lifting is what a tissue adds, and no other blender lowers paint.

**One read per pixel.** On the MovinkPad 11 the blender kernels are limited by
texture reads, not bytes or arithmetic: one paper read on top of twelve colour
taps cost 11–13% at 512 px, whether contact was evaluated per contact or per
pixel. Paper-aware kernels take eleven colour taps, which changed the blend by
under 0.5/255 on average and brought the cost to about 3%. The Bristle Blender
has no disc to shorten: its two side samples, already jittered across the tip,
read the nearest pixel instead of interpolating four, which changed it by
0.23/255 on average.
