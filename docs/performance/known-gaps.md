# Known gaps

[Performance targets](../PERFORMANCE_TARGETS.md)

- **Wet, watercolor, Smudge and blending brushes fall behind the pen at 512 px.**
  - A frame consumes input only until its material contacts reach 512 page
    jobs ([brushes](../internals/brushes.md)), so these brushes never run out
    of memory; a slower device keeps the rest of the input queued and the
    stroke trails the pen.
  - On the TCL (12 MP) they complete 1.0–2.5 updates/s, at most 1.8 GB
    resident; Liquify completes 5.2 (Push) to 35 (Twirl) updates/s.
  - On the MovinkPad 11 (24 MP) they complete 1.6–3.1 updates/s, at most
    2.6 GB resident; Liquify completes 5.7 (Push) to 28 (Twirl) updates/s.
  - On the MovinkPad Pro 14 (61 MP) they complete 7.7 (Loaded Oil) to 54
    (Smudge) updates/s; every Liquify preset meets 120 updates/s.
  - Multiply Glaze at 1024 px completes 86.8 updates/s on the MovinkPad Pro 14
    and 12.1 on the MovinkPad 11, where its compute kernel is slower than the
    fragment path it replaced (14.4).
  - On the MovinkPad Pro 14, the eraser fix in the dry contact loop made Pencil
    at 16 px about 10% slower (258 -> 233 updates/s); Pencil at 2048 px is
    unchanged.
- **The simple-brush class remains unqualified.**
  - On the current contained 60 × 10 path, 1536 px G-Pen reaches only
    75.36–75.54 fresh updates/s on Mid (target 90), although its gap passes.
    The preceding larger-path result was 55.6/s with p99 31.9–33.5 ms.
    Other simple brushes need measurements at 1536 px; their older 2048 px
    results are above the
    revised guarantee. See the [mid-tier measurement](mid-tier.md#g-pen-at-the-1536-px-guarantee).
  - The current low-tier 1024 px G-Pen measured stroke meets 60/s at both full
    and partial Opacity; other simple brushes
    and stacked-photo drawing retain gaps. See the [low-tier table](low-tier.md#brushes).
  - Top 2048 px G-Pen passes on the current 60 × 10 path at 175.65–179.08/s;
    this does not qualify the preceding larger path, which reached 87.6/s with
    p99 31.6–38.8 ms. The matched base with the same
    bounded command-pool cleanup reaches 87.0/s. See the
    [top-tier table](top-tier.md#dry-media-contact-model).
- **Most complex brushes at 1024 px remain unqualified.**
  - Pastel Block at nominal Size 1024 meets the current native Top writing
    pose targets at 173.74–175.88 fresh updates/s, but reaches only
    51.93–52.17/s on Mid and 21.27–23.43/s on Low. Its longer Low side pose
    reaches 3.32–3.44/s with p99 gaps up to 483.20 ms despite fully contained support. Mid and Top side
    poses remain unmeasured on this build.
  - Earlier measurements of the other complex presets reach 76–118 updates/s
    on Top, about 6–25 on Mid and 3–19 on Low. They need current measurements.
- **Dry-media qualification remains incomplete.**
  - Size denotes the 45° writing-pose long axis. With display-resolution
    predictions, native Pencil, Pointy, Charcoal and Pastel writing profiles meet
    rate and gap on all three tiers, including Pencil at Smoothing 100%, except
    MovinkPad 14 Pencil's 17.11 ms gap against 16.7 ms. Current native Pencil
    reaches 86.92–90.00/s on Low, 123.49–124.82/s on Mid and 149.18–156.04/s on
    Top.
  - Those predictions cost MovinkPad 14 Pencil at 256 px in screen strokes:
    313.7 → 261.9/s, with the worst gap 11.3 → 17.2 ms. GPU time falls; the
    extra display gather adds frame latency.
  - TCL Web remains unqualified; see the
    [low-tier table](low-tier.md#dry-media-contact-model).
  - Passing native rows cover only their measured profiles and poses; Shading
    Pencil, Chalk and Crayon remain unmeasured. Physical input-to-present latency
    is unmeasured.
  - First dry-brush selection remains slow. After clearing app data, native
    Pencil takes 1.81 s on Low, 0.89 s on Mid and 0.78 s on Top. On the earlier
    build TCL Web Pencil took 14.80 s and the primed native probe 35.53 s, with
    driver caches retained; neither has been remeasured. Cached selection is
    quick; that does not qualify first-use latency.
- **Liquify at 512 px** completes 28–74 updates/s on the top tier, 1.2–6.5 on
  the mid tier and 0.8–12 on the low tier.
- **The retouching brushes at 512 px** (Clone Stamp, Healing and Spot Healing)
  complete 57–62 updates/s on the mid tier (target 90). Their soft edge keeps
  a wide band of pixels in the dab loop. The low and top tiers are not
  measured.
- **The MovinkPad 11 presents at 60 Hz.** Before any display-paced mid-tier row
  can pass, it must present at 90 Hz.
- **Navigation on the tier canvas is untested on two tiers.** The low and mid
  tiers have no pan, zoom or rotate measurements on their canvases. The top tier
  has only pinch data at 61 MP.
- **Operations with no timing harness:**
  - Layer opacity scrub, reorder and blend changes.
  - Gradient and curve-point drags.
  - List scrolling.
  - Android canvas rotation.
  - Magic Wand and fill latency.
- **Retained wet-photo transforms on the top tier** reach 36.3–37.0 renderer
  updates/s on the 61 MP canvas. Presentation is unmeasured; this does not meet
  the 120 fps target. See the [material-transform measurements](top-tier.md#retained-wet-photo-transforms).
  Projective and Warp motion remain unmeasured on that reference tablet.
- **Object-layer simplification has no current motion qualification on the reference tiers.**
  Imported-image placement, Move and Scale/Rotate still need warmed, repeated
  measurements on the three reference canvases. Existing object and placed-photo
  results in the tier tables describe earlier builds. Desktop browser callbacks
  and renderer timings do not establish presented-frame rates or qualify a tier.
- **Vector layers are measured only by headless renderer benchmarks.**
  `scene::vectors::tests::crowded_vector_layer_drawing_frames` and
  `crowded_vector_layer_edit_frames` (`VECTOR_ZOOM` sets the zoom) ran on the
  three reference tablets at `62c9c4c61` (2026-10-18), on a 4000 × 3000 canvas
  of hatching paths with a 1920 × 1200 view. They time renderer frames, not
  presented frames or a session, and the canvas is not each tier's canvas. At
  1.0×:

  | | Low (TCL) | Mid (MovinkPad 11) | Top (MovinkPad Pro 14) |
  | --- | --- | --- | --- |
  | Open 10,000 / 50,000 paths | 2.2 / 4.9 s | 1.1 / 2.6 s | 0.5 / 1.3 s |
  | Drawing frame p50 / p95, 50,000 paths | 10.9 / 16.8 ms | 8.0 / 12.0 ms | 4.1 / 17.0 ms |
  | Drag start, 1 / 100 / 5,000 paths | 67 / 110 / 453 ms | 63 / 116 / 249 ms | 36 / 55 / 160 ms |
  | Drag frame p50, 1 / 100 / 5,000 paths | 2.8 / 4.4 / 18.4 ms | 1.9 / 2.3 / 10.8 ms | 0.6 / 1.3 / 2.8 ms |
  | Stroke commit / undo, 50,000 paths | 96 / 93 ms | 51 / 60 ms | 22 / 30 ms |

  - Gaps against the targets: a 5,000-path drag on the low tier runs slower
    than 60 fps, drag starts of 100 or more paths exceed the 100 ms press response
    on the low and mid tiers, and committing or undoing a long stroke across a
    crowded layer repaints its tiles for 50–100 ms on the low tier.
  - Drawing on the top tier spends about 14 ms of CPU on every eighth frame;
    the desktop and the Mali tablets do not. The cause is not yet traced.
  - At 0.4× the low tier opens in 1.9 / 3.7 s and drags 5,000 paths at 7.0 ms
    p50.
  - The present shader's motion branch costs nothing measurable while
    painting: against a control without it, canvas updates per second changed
    by −0.67% (Pencil 1024 px) and +0.09% (G-Pen 1024 px) on the TCL and
    +0.39% (G-Pen 1024 px) on the MovinkPad 11, in 4 ABBA sets each.
  - Scale and rotate drags with the Move frame, Edit Points drags, Properties
    scrubs and eraser strokes preview by re-deriving the edited paths each frame;
    none is measured on a tablet.
