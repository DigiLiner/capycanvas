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
- **Vector layers miss most motion targets once paths are selected or edited.**
  The reference tablets were measured on 2026-10-19 with the vector benchmark
  ([measuring](measuring.md#vector-layers-android)) on `crowded_vector_fixture`
  documents at each tier's canvas with equal path density: 10,000, 20,000 and
  50,000 hatching paths. Tier tables: [low](low-tier.md#vector-layers),
  [mid](mid-tier.md#vector-layers), [top](top-tier.md#vector-layers). The
  native library includes other sessions' uncommitted vector and package edits
  of that afternoon; the tables name it.

  | Presented frames/s, or updates/s for inking | Low (60) | Mid (90, panel at 60 Hz) | Top (120) |
  | --- | --- | --- | --- |
  | G-Pen 24 px on the crowded layer | 88.5–90.2 met | 185.3–187.3 met | 250.0–252.8 met |
  | Move drag, 1 / 100 / 5,000 paths | 59.8 / 59.8 / 32.4 | 59.7 / 59.5 / 52.8 | 120 / 120 / 96.6 |
  | Move drag start, 1 / 5,000 paths (limit 100 ms) | 53–79 / 260–279 ms | 79–94 / 198–211 ms | 108–135 / 147–178 ms |
  | Scale with the Move frame, 100 / 5,000 paths | 1.0–1.4 / 0.2 | 2.6–2.7 / 0.3–0.5 | 29.4–35.2 / 0.9–1.1 |
  | Edit Points drag, 1 path | 45.6 | 59.7 | 120 met |
  | Eraser: touched part / whole paths / to intersection | 13.5 / 35.4 / 19.3 | 32.4 / 59.1 / 43.3 | 100.2 / 116.1 met / 93.1 |
  | Outline width scrub, 100 / 5,000 paths | 2.2 / 0.2 | 4.5 / 0.2 | 20.6 / 0.4 |
  | Pan with no paths / 5,000 paths selected | 34.5 / 13.5 | 53.3 / 16.6 | 17.6 / 12.3 |

  Responses: Fill inside a path 47–58 ms and Undo 90–169 ms meet their limits
  on every tier, and a crowded document opens in 1.2 / 1.1 / 1.7 s. Fill
  outside the paths takes 1.0–2.6 s on low and mid, and on the top tier stops
  the renderer with "GPU buffer size overflow". Merge Down onto a paint layer
  and Rasterize Layer take 1.9–2.3 s on low, 2.3–3.0 s on mid and 3.6–3.7 s on
  top, with no change on screen until they finish on mid and top; Merge Down
  onto a vector layer takes 274–476 ms. Rate-limited rows also trail the pen:
  a 5 s Scale, Rotate or width scrub of 5,000 paths keeps the canvas thread
  busy for 18–41 s after release.

  Causes, from CPU profiles of the canvas thread (`simpleperf`, MovinkPad 11,
  `artifacts/vector-tablets-20261019/profiles/`) and GPU timestamps, by impact:
  1. **Previews run once per stylus sample.** Scale and Rotate re-run
     `transform_items` and `preview_edit` for every 200 Hz sample: 72% of the
     canvas thread at 100 paths, 5.4 ms per sample on the MovinkPad 11 and
     8.4 ms on the TCL, where `edit_chunks` re-packs whole chunks and the
     preview re-validates them. The width scrub does the same per dispatch
     (`set_path_property`, then `refresh_document`), and the eraser per sample
     (45% of the thread, `VectorIndex::near` alone 22%). At 200 Hz the queue
     outgrows the frame. **Fix:** in the shared session, keep only the latest
     pointer or slider value and apply it once per frame, and preview Scale
     and Rotate like translation by giving `VectorMotion` an affine for the
     cached moving image, re-deriving the paths only at release. Expected:
     Scale and Rotate at the Move rate (60 / 120+ renderer / 120 for 100
     paths; coalescing alone leaves the top tier GPU-bound near 50/s), no
     trailing work after release, and a 2–3× faster eraser and width scrub on
     the low and mid tiers.
  2. **Selection-sized UI state is rebuilt every frame.** With 5,000 paths
     selected, `refresh_commands` takes 60% of the canvas thread while panning:
     command checks such as `layer_step_edit`, `drop_layers_edit` and
     `vector_disabled_reason` build trial edits whose `repair_working` and
     `VectorObject::selected` clone every selected path. During a 5,000-path
     Move, model publication takes 31% (the Layer menu's `group_layers_edit`
     trial edit about 12%) and `refresh_layer_presentation` 20%
     (`vector_properties::controls` over the selection). **Fix:** derive
     command flags, menus and Properties controls once per selection or
     document change, make the vector checks constant-time predicates instead
     of trial edits, and skip republishing them while a drag or navigation
     gesture moves. Expected: navigation with a selection at the
     no-selection rate (13.5 → 34.5 / 16.6 → 53.3 / 12.3 → 17.6), and the
     5,000-path Move GPU-bound instead of CPU-bound (about 32 → 50 on low and
     52 → 60 on mid; the top tier stays near 100 with 10 ms of GPU per frame).
  3. **Drag start draws two heavy frames.** The first frames after pressing
     repaint the tiles the moving paths leave and draw the moving image: 47–76 ms
     of GPU per frame for one path on low and top, and 54–113 ms of CPU plus
     136–169 ms of GPU for 5,000 paths. **Fix:** prepare the layer without the
     selection and the moving image when the selection changes, so pressing a
     selected path presents at once, and repaint only the moving paths'
     footprint at the display level. Expected: drag start under 50 ms on every
     tier for selected paths.
  4. **Crossing into a finer level re-rasterizes the crowded layer at once.**
     A TCL pinch past zoom 0.5 presents 2.9–19/s with 1.5–1.7 s gaps, against
     33.8/s without vector layers; the canvas thread spends its time recording
     the vector jobs' barriers, buffer copies and passes in the Mali driver.
     **Fix:** keep showing the previous level scaled while the new one renders
     within a per-frame budget, and batch each frame's vector uploads and passes.
  5. **Baking derives every path at full resolution on the canvas thread.**
     Merge Down onto paint and Rasterize Layer spend it in `vector_tile`,
     `batch_with` and `derived_at` for every page. **Fix:** bake on a worker
     with immediate on-screen feedback. Fill outside the paths floods the whole
     canvas: at 9504 × 6336 its 4-byte-per-pixel flood buffer (241 MB) exceeds
     the storage-binding limit, and the refusal surfaces as a renderer failure.
     It needs a windowed flood bounded by memory, and an oversize request
     should be refused without losing the canvas.
  6. **Navigation also misses without vector content.** With the hatching
     removed, pan presents 38.9 / 57.7 / 19.4/s; the crowded layer costs 8–11% on
     top of that. On the MovinkPad Pro 14 the viewport presentation pass takes
     34–36 ms of GPU time for 5.2 MP even with both vector layers hidden.

  The headless renderer benchmarks (`scene::vectors::tests::crowded_vector_layer_drawing_frames`
  and `crowded_vector_layer_edit_frames`) still time renderer frames on a
  4000 × 3000 canvas; they do not account for per-sample session work or
  UI refresh, which dominate the tablet results above.
