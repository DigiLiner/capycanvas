# Known gaps

[Performance targets](../PERFORMANCE_TARGETS.md)

- **Wet, smudge and blending brushes run out of memory on every tier at 512 px.**
  - On the TCL (12 MP), Android's low-memory killer stops Opaque Gouache, Loaded
    Oil and Palette Knife at about 4.2 GB resident. The rest complete only
    0.3–0.5 updates/s.
  - On the MovinkPad 11 (24 MP), every one of them fails:
    - Opaque Gouache, Watercolor Wash, Wet Watercolor, Loaded Oil, Wet Round and
      Natural Blender abort in the native allocator (a Scudo map failure).
    - Smudge and Palette Knife stop with a canvas GPU out-of-memory error.
  - On the MovinkPad Pro 14 (61 MP), only Smudge completes, at 20 updates/s.
    - The others crash, mostly inside the Adreno Vulkan driver (a SIGSEGV at
      address 0x1c).
    - Opaque Gouache aborts in the native allocator.
    - Smudge also crashed in the driver in one of two runs.
  - On the TCL the process runs out of RAM.
  - On the MovinkPads the likely cause is exhausting the process's memory
    mappings instead. This is not yet verified.
    - Successful Smudge and Liquify Push strokes on the MovinkPad Pro 14 reach
      41,000–51,000 mappings. The G-Pen reaches about 11,000.
    - Android's default `vm.max_map_count` is 65,530.
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
  - Size denotes the 45° writing-pose long axis. Current Strength 100%, Material
    hardness and default Smoothing 6% profiles miss the nominal Pencil, Pointy
    and Charcoal rate targets on all native tiers. Only native Low Pointy's
    gap meets its limit. Current native Pencil reaches 30.46–30.94/s on Low,
    26.28–26.29/s on Mid and 61.57–61.93/s on Top.
  - Compile-time aggregate specialization restores substantial throughput
    relative to the matched runtime branch. Same-APK Pencil Smoothing 0%
    diagnostics reach 54.80–55.52/s on Low, 50.91–51.63/s on Mid and
    83.51–86.23/s on Top, still below nominal rates. This isolates Curve fit
    work without attributing all remaining cost to it. Partial Opacity and
    other tilts remain unmeasured; earlier recipes do not qualify them.
  - TCL Web remains unqualified; see the
    [low-tier table](low-tier.md#dry-media-contact-model).
  - Passing native rows cover only their measured profiles and poses; Shading
    Pencil, Chalk and Crayon remain unmeasured. Physical input-to-present latency
    is unmeasured.
  - First dry-brush selection remains slow despite the source refactor: TCL
    Web Pencil takes 14.80 s and native takes 35.53 s, with driver caches retained.
    Cached selection is quick; that does not qualify first-use latency.
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
