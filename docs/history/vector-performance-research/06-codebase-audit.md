# Vector-layer performance audit: composition, object layers, memory, CPU and UI

[Vector layer performance](../vector-performance-research.md) · source report, 2026-10-16

Code audit at `7c71b731c`. The design this audit informed is in the [performance record](../vector-performance-research.md); file:line references apply to that baseline.

The audit read the code at `7c71b731c`; it built and ran nothing. Paths below are relative to the repository root. There is no vector object code yet; the recent "vector" commits change docs only (`git show --stat 726cb6cdc`).

## 1. Composition

**Storage and formats**
- Paint is stored in sparse 256×256 pages (`PAGE_SIZE: u32 = 256`, `crates/layer-render-wgpu/src/lib.rs:137`).
- Pages, masks, display levels and intermediates are `Rgba32Float` (one page is 1 MiB) or `R32Float` on every device:
  - `docs/internals/rendering.md:1012`;
  - the display mip storage format (`display_mips.rs:105-110`);
  - the native-edit scratch (`raster/native_edit.rs:66-71`).
- `Rgba16Float` appears only as a presentation surface format (`view_color.rs:44`, `present.rs:513-515`). Nothing internal is half-float.

**Two evaluators share one stack traversal**
- The native per-tile compositor is `scene/stack.rs:138` (`tile`). It calls `Scene::layer` (`scene.rs:1027`), which goes to `object_tile` for object layers (`scene.rs:1044`).
- The display graph is `scene/scale/graph.rs`, an `Expression` tree (`graph.rs:6-14`).
- Masks, effects and clipping are applied above either leaf by the generic `owner_image` (`stack.rs:118-136`).
- Normal-blended siblings form a balanced binary tree: `Expression::over` splits at `next_power_of_two()/2` (`graph.rs:23-28`). This is the "balanced root" in the performance docs.
- The graph keeps subtree results as cached **branches**. Candidates are `Objects`, `Combine` and `Effect` nodes, ranked by damage area and cost and cut to `budget / level_bytes` (`graph.rs:197-210`).

**What is cached, per layer or per window**
- Per window: the display cache, with `CACHE_BYTES = 608 MiB` (`scene/scale.rs:6`). It holds:
  - the output window plus the adjacent level and an overview;
  - shifted and spare caches for pan and zoom reuse (`Cache::select`, `scale.rs:669-747`).
- Per paint or mask source: reduced levels, kept within the budget by `retain_levels` (`scale/sources.rs:19-45`, `307-330`).
- Optional, per document: a resident complete pyramid (`Hierarchy`, `scale/hierarchy.rs:46-99`). It is admitted from driver or system headroom (`display_memory.rs:29-47`; on Android the allowance is headroom/2).

**Damage flow**
1. Dab batches go through `brush_tiles::plan` (`lib.rs:~3920`).
2. They become a sparse `composite_tiles` set plus a `dirty` union, joined by `transform_damage` and `document_damage` (`lib.rs:4631-4672`). `composite_all` forces the full extent (`lib.rs:4676-4678`).
3. `Scene::compose` runs (`scene.rs:1743-1812`), then `Cache::render_graph`. That step turns damage into invalid pages (`scale.rs:1018-1027`), page regions (`update_regions`, `scale.rs:67-77`), and one root evaluation per region or page (`scale.rs:1083-1150`).
4. `Damage::push` merges rectangles by linear rescan, restarting the scan on every merge (`scale.rs:49-62`).

**Pan, zoom and rotate**
- `scale::request` chooses the level with `display_mips::view_level(transform, 4)` (`scale.rs:394`). That level makes a texel no wider than a surface pixel and is never finer than 0 (`display_mips.rs:13-20`). Nothing is evaluated above native resolution.
- `ViewportPresenter` samples the composed window plus its next and coarse levels (`scale.rs:925-936`; `present.rs`). Camera motion recomposes only newly exposed pages.
- The cache falls back to `Evaluation::Native` in these cases (`scale.rs:403-407`):
  - the uniform records exceed `max_buffer_size`;
  - the allocation exceeds `CACHE_BYTES`;
  - a native-resolution effect is present.

**Idle refinement and batching**
- `Cache::refine` (`scale/refinement.rs:101-143`) writes exact pages into the display and the hierarchy:
  - up to `SOURCE_SLOTS = 16` pages, or 4 when spatial effects are present;
  - a pixel-work limit applies;
  - one batch is in flight at a time.
- `render_native` batches up to 16 pages (`refinement.rs:25`).

## 2. Object layers today

**Content**
- One `ImageObject` per occurrence. `SpatialIndex::content` stores `vec![key]` and `Content::len()` is hard-coded to `1` (`scene/object_spatial.rs:21,44`).
- The "spatial index" is a per-owner `HashMap` holding whole-layer bounds. `query_at` returns all keys or none (`object_spatial.rs:18-20`).
- `prepare` recomputes `ObjectKey::new` for every cached layer whenever the object or occurrence store root changes (`object_spatial.rs:30-37`).

**Native leaf: `object_tile` (`scene/objects.rs:264-287`)**
- It builds a 256² `CollectionJob` (`collection_job`, `objects.rs:159-169`).
- It copies the canonical result if the cache has it; otherwise it uses a mip preview on display, or returns `DeferredObjectWork`.

**Display leaf: `Compositor::objects` (`scale.rs:1403-1438`)**
- The window is aligned to the display level.
- If *any* layer is moving (`moving || r.moving_layer.is_some()`, `scale.rs:1415`), it uses `collection_preview`. That fuses up to 16 objects from up to 8 mip sources (`object_sampling.rs:44-46`).
- Otherwise it calls `canonical_cover` (`objects.rs:192-207`) and copies the completed pieces.

**Canonical results: `ObjectCache` (`scene/object_cache.rs:9-15`)**
- 64 MiB per cache, with separate live and exact caches; at most 256 tasks.
- Tap budgets: 2²⁰ per interactive frame, 2²³ per idle frame.
- Least-recently-used eviction (`object_cache.rs:59-69`, `140-144`).
- Windows larger than 512 px per side are split into parts (`COLLECTION_PART`, `objects.rs:91,126-132`).

**Damage: `edited_damage` (`objects.rs:49-78`)**
- Returns `None` when:
  - the object store root is unchanged;
  - the order or composition changed;
  - any paint or mask raster identity changed;
  - non-object metadata changed.
- Otherwise it pushes the *whole* old and new object bounds plus a halo, then expands them by `stack::support`.
- It is called twice per frame (`lib.rs:3840`, `scale.rs:872`).
- A `None` result leaves the engine's `composite_all` set, which recomposes the full extent.

**Document-wide opt-outs.** Any visible object layer anywhere in the document triggers each of these:
- refinement disabled (`refinement.rs:96,105`);
- native batch size of 1 page (`refinement.rs:25`);
- no resident hierarchy (`hierarchy.rs:55`);
- no overview derived from finer detail, and no valid-page pruning (`scale.rs:1008-1009,1022`);
- the Navigator skips its own composition (`scale/navigator.rs:42`);
- forced decode-path checks for captures (`artwork.rs:214`, `scene.rs:1491,1635`);
- filter inputs take a separate object path (`scene_images.rs:443`).

**A transform or placement drag**
1. `preview_object_motion` applies a real affine `RecordChange` through `engine.preview_edit` on each pointer move (`crates/layer-ui/src/object_motion.rs:28-33,56-70`).
2. `Document::apply` takes the `presents_same_images` fast path, with no full validation (`crates/layer-core/src/lib.rs:1443-1450,1820-1826`).
3. The renderer computes old and new bounds damage. It re-renders the moving layer from immutable image mips with the new affine, every frame (`scale.rs:1415-1437`).
4. `MovingProjection::update` rebuilds requests over all object layers when the store root or the view changes (`moving_projection.rs:33-63`).
5. Mip building gets 2 steps per input frame and 32 per idle frame (`lib.rs:138-140,3351`).
- There is no dedicated split below/moving/above cache. `docs/internals/rendering.md:603` says: "no separate placement-drag composite". Static siblings can survive only as graph branches.
- Commit is a single undo step (`object_motion.rs:35-44`).

## 3. GPU memory

**Budgets**

| Budget | Value | Evidence |
|---|---|---|
| Display or composition cache | 608 MiB | `scene/scale.rs:6` |
| Filter or native image windows | 256 MiB minimum | `scene/windows.rs:8`; `native_edit.rs:152-157` |
| Native paint color cache | 256 MiB; trimmed by evicting canonical pages that already have backing | `raster/native_edit.rs:80`; `raster/residency.rs:137-200` |
| Decoded source tiles | 64–1024 slots × 1 MiB; uploads ≤ 64 MiB; mip reservation taken from slots | `scene/sources.rs:23-24,37-48,191-215` |
| Object caches | 2 × 64 MiB | `object_cache.rs:9` |
| Snapshot planning | 512 MiB | `snapshot.rs:13` |
| Optional pools (`resource_budget`) | ≤ 512 MiB | `display_memory.rs:208-220` |
| Complete hierarchy | driver/system headroom ÷ 2 (Android) or ÷ 4, minus `CACHE_BYTES` | `display_memory.rs:6-11,29-47`; `hierarchy.rs:46-52` |
| Undo history (CPU) | 512 MiB and 256 entries | `crates/layer-core/src/history_budget.rs:6-7` |

**Full-canvas textures**
- No layer has a mandatory full-canvas texture; pages are allocated on touch.
- Per document:
  - the display window, its next level and the overview;
  - the exact working strip, at most 16 pages (`scale.rs:7-11`);
  - the optional complete pyramid;
  - per-source reduced levels, within budget.

**Sizes at Rgba32Float (16 B/px)** (my arithmetic)

| Canvas | One full-canvas texture | Full complete pyramid (≈4/3) | Fully painted layer |
|---|---|---|---|
| 12 MP | 183.6 MiB | ≈245 MiB | 17×12 pages = 204 MiB |
| 24 MP | 366.2 MiB | ≈488 MiB | 24×16 pages = 384 MiB |
| 61 MP | 918.8 MiB | ≈1.2 GiB | 38×25 pages = 950 MiB, above the 256 MiB color cache |

I could not confirm which tablets actually admit the hierarchy at each canvas; that depends on runtime headroom.

**Windowed-work rules**
- Filter windows of 2048, 1024, 512 or 256 px (`windows.rs:27-47`).
- Bakes in 512² regions (`rendering.md:120`).
- Object parts of 512 px; Nearest geometry in chunks of ≤65,536 output pixels (`rendering.md:192-193`).
- Exact captures of ≤16 tiles per wait (`rendering.md:906`).

## 4. CPU side

**Scheduling**
- Each session has one ordered owner. The GTK renderer runs on a worker thread (`apps/layer-linux/src/render_thread.rs:1-2`); the Web host runs synchronously.
- `FramePacket` borrows a `SceneView` and does not copy the document (`crates/layer-render/src/lib.rs:287-314`; `crates/layer-engine/src/canvas.rs:1438-1452`).

**Per-frame work that grows with layer count L**
- `ArtworkFrame::same_artwork` is evaluated up to 5 times per frame (`lib.rs:3816,3897,4737,4763,4792`).
  - Each call walks every layer (`crates/layer-core/src/artwork_query.rs:161-163`).
  - Its object check falls back to structural `a == b` when the object store root differs (`artwork_query.rs:165-173`).
- `same_metadata` builds two `Metadata` values per layer, cloning the `Occurrence` and the `ObjectKey`s (`scene.rs:11-23`, `scene/metadata.rs:82-95`).
- The display graph is rebuilt every frame (`graph.rs:186-196`). `scratch_images` builds another one by cloning the `SpatialIndex` (`graph.rs:216-219`) inside `allocation_with_tiles` (`scale.rs:583`).
  - `allocation_for` is reached from `request`, `select`, `admit_source_overlap` (twice), `prepare_graph`, `source_budget` and `auxiliary_budget`.
- `targets()` calls `stack::has_content` for every layer. `has_content` scans `r.paint_layers` linearly (`stack.rs:19-28`), so this is O(L²). The same scan runs per layer per tile in the native compositor.
- `ensure_document_metadata` runs `tiled_sources.retain(... placed_targets(scene).any(...))`, also O(L²) (`lib.rs:1733`).
- Uniform records grow as layers × pages: `256 × ((2·pages+12)·layers+2)` B (`scale.rs:655-667`).
  - At 61 MP (950 pages) that is about 0.47 MiB per layer.
  - Beyond `max_buffer_size` (about 550 layers at a 256 MiB limit) this forces Native evaluation (`scale.rs:403`).

**Every committed edit costs O(document)**
- `Document::apply`, on the general path:
  - calls `artwork.images()` on the old and new document, each enumerating all roots (`lib.rs:1456-1458`);
  - calls `validate_payloads()` (`lib.rs:1464`). That validates every paint raster index, every object (`object.validate()`, `lib.rs:1626`), coverage, occurrence, effect and selection (`lib.rs:1587-1680`).
- `prepare_history_edit`:
  - applies forward and then inverse, so validation runs twice (`lib.rs:2292-2295`);
  - builds a full `RootInventory` (`lib.rs:2290,2297`);
  - seeds `Accounting::for_admission` twice (`lib.rs:2322-2323`).
- `trim_history` seeds `Accounting::new(document)`, which inserts every tile identity of the document, then re-charges up to 256 entries (`lib.rs:2370-2384`; `history_budget.rs:31-50`).
- `Store::change`/`reserve` call `Arc::make_mut` on the entries `Vec` and the ids `BTreeMap`. Because history shares them, that is a full clone of that store (`authored/store.rs:39-40,69`).

**History charging**
- Charging is by `Arc` and pointer identity: images, tiles, selection contours per inner `Arc`, mesh arrays and programs (`history_budget.rs:10-23,132-151`).
- `RecordChange<T>` holds `T` by value (`store.rs:79`).

**Autosave and save**
- A recovery checkpoint runs every 2 s when the session stamp has changed:
  - GTK: `apps/layer-linux/src/recovery.rs:301`;
  - Android: `Recovery.kt:106,208`;
  - Windows: `native/src/recovery.rs:507`.
- Preparation runs on a worker (`apps/layer-android/native/src/session_recovery.rs:71-75`).
- `PreparedSession::prepare`:
  1. replays every undo and redo state with a full `Document::apply` (`package/session.rs:28-41`);
  2. JSON-encodes *every record of every state* through `encode_with_inventory`;
  3. dedupes by serializing each record to bytes as a `BTreeMap` key (`session.rs:170-182,202-208`).
- That is O(document × history) per autosave.
- Only raster tile indexes are chunked, 64 per chunk (`session.rs:86-99`). The whole session manifest is one JSON of at most 64 MiB (`session.rs:223-224`).
- Session transfer re-encodes each changed record per entry (`session_transfer.rs:33-53`).
- Package save encodes every record as JSON (`package/artwork_records.rs:274-292`). Binary resources keep their encoded bytes (`package/resources.rs:20-48`).

## 5. UI

**Shared state**
- `refresh_layer_presentation` rebuilds a `Vec<LayerState>` with strings for every unhidden row (`crates/layer-ui/src/session.rs:5865-5970`).
- Each row calls `can_delete_layers(&[id])`, whose `deletion_ids` loops over `scene.order()` (`layer-core/src/layers.rs:808-829`). The refresh is therefore O(L²).
- Notifications are coarse bit flags: the layer list is part of `regions::DOCUMENT` (`layer-ui/src/lib.rs:1733-1748`). There is no per-row granularity.
- Object drags return `changed(0, …)`, so no list refresh happens during motion. A commit sends `DOCUMENT`.

**Transport to hosts**
- Web: `state_update` serializes the whole `layers` field to bytes on every update to compare it with a cache (`apps/layer-web/src/lib.rs:672-680,702`).
- Native hosts serialize the full model and diff it as JSON. An array keeps per-element paths only while its length is unchanged; otherwise it is replaced whole (`crates/layer-host/src/model_update.rs:16-56,74-120`).

**Thumbnails**
- 32×32 RGBA8 asynchronous readbacks (`thumbnails.rs:181-189`) with a four-page preparation budget (`thumbnails.rs:122,131`).
- Object rows evaluate the whole layer on a grid of at most 256² (`objects.rs:227-263`).
- The row revision is a hash of the raster identity and preview revisions (`session.rs:5942-5950`).
- Hosts poll every 120 ms and keep at most 8 requests pending.
- Native hosts accept **one** request per query, gated on `background_readback_idle` (`layer-host/src/lib.rs:315-320`).

| Host | List | Thumbnails | Evidence |
|---|---|---|---|
| GTK | Virtualized `ListView` with `SignalListItemFactory` (recycled). Rows are compared with `PartialEq`; the whole model is spliced when ids change | Requested only for bound, visible rows; each visible row does a linear `state.layers.iter().find` | `apps/layer-linux/src/layers.rs:425-426,921,1310-1328,1371-1376,1421`; timer at `1051` |
| Web | **Not virtualized**: one DOM row per layer is kept. Each row is keyed with `JSON.stringify`, plus a whole-list stringify | The 120 ms timer calls `getBoundingClientRect` on every row, one request per tick | `apps/layer-web/layers.js:372,378,428,433-458` |
| Android | `LazyColumn` keyed by id (virtualized) | Visible rows from `visibleItemsInfo`, JSON transport | `Layers.kt:170-171,242` |
| Apple | `LazyVStack` + `ForEach(rows)`; `rows = layers.map` on every body | Driven by `onAppear` and `onDisappear`, at most 8 | `Shared/Editor/LayerPanel.swift:20,31-32`; `Bridge/LayerThumbnails.swift:5,60` |
| Windows | `ItemsRepeater` with a recycling `ElementFactory` | At most 8 pending; 128-image cache | `LayersView.cpp:16,104`; `LayerThumbnails.cpp:60,102` |

## 6. Existing measurements

- **Low tier, image objects** (`docs/performance/low-tier.md:1855-1950`; build M2J/M2K, 2026-10-06, 12 MP):
  - painting above 4 objects: 62.1–63.3 fresh updates/s, which meets 60;
  - painting below 4 objects: 57.7–58.8;
  - 8 objects: 56.6–57.8;
  - moving one of four shared objects: 47.9–50.4 poses/s, below 60;
  - the same with attached Gaussian Blur: 21.3–21.4 poses/s;
  - opening with eight distinct objects takes 11.8–12.1 s to settle.
- **Object Move, Scale/Rotate and placement** are unmeasured on all reference tablets (`low-tier.md:116`, `mid-tier.md:78`, `top-tier.md:81`, `known-gaps.md`). A Windows Move preflight failed (`top-tier.md:2088-2092`).
- **Many layers**:
  - low tier, eight layers: 50.12/s, not met (`low-tier.md:147,919`);
  - mid tier, 17 layers: 74.7–75.4/s, not met (`mid-tier.md:98`);
  - navigation with 8 or 16 layers: unmeasured (`low-tier.md:146`, `mid-tier.md:97`);
  - list scrolling: unmeasured on tablets. GTK with 1,000 layers manages only 97–110 presents/s at 120 Hz on a desktop (`top-tier.md:136,285-293`).
- **Gaps that bear on vector content** (`known-gaps.md`): no timing harness for opacity scrubs, reorder or list scrolling; navigation untested on tier canvases; Mid presents at 60 Hz.
- There are no vector-content rows.

## 7. Bottlenecks at 10,000–50,000 paths

1. **Per-frame structural comparisons.** `same_objects` falls back to `a == b`, and `Metadata`/`ObjectKey` are rebuilt per frame (sections 4 and 2). Any per-item data in `Metadata` or `ObjectKey`, or in `PartialEq` without an `Arc::ptr_eq` short-circuit, costs O(items) several times per frame.
2. **Full validation on every edit.** `validate_payloads` calls `object.validate()` on every object for every edit (`lib.rs:1626`), at least twice per committed edit plus once for each history replay during autosave. Path drags take the full path unless they get a fast path like `presents_same_images`.
3. **Autosave scales with history.** Up to 512 history states are re-encoded and re-validated every 2 s. Vector record JSON must stay tiny, and geometry needs content-addressed chunks that dedupe without serialization.
4. **Store cloning.** `Store` `Arc::make_mut` clones whole record stores, and the limits are 65,536 graph objects and 4,096 occurrences (`authored/topology.rs:55`; `project.rs:74-80,110`). One record per path is ruled out.
   - Other limits: 262,144 resources, 64 MiB metadata, 4,194,304 traversal nodes (`package/manifest.rs:10`), 16,384 tiles (`project.rs:78`).
5. **Not a spatial index.** `query_at` returns everything or nothing (`object_spatial.rs:18-20`), and `edited_damage` damages whole-object bounds (`objects.rs:62-74`). Editing one path would dirty the whole layer.
6. **Damage merging.** `Damage::push` is quadratic (`scale.rs:49-62`), so pushing thousands of per-path rectangles is risky. Damage should be coalesced to page sets.
7. **Document-wide opt-outs.** A single vector layer, if it counts as an object layer, would disable refinement, the hierarchy, batching and the Navigator for the whole document (section 2).
8. **Fixed fan-out limits.** The fused preview takes ≤16 objects and ≤8 sources (`object_sampling.rs:44-46`); the object cache takes ≤256 tasks within 64 MiB. Neither fits per-path work.
9. **O(L²) host and shared paths.** The layer list, `has_content` scans and the Web DOM-per-row panel, together with records that grow as layers × pages, limit "many vector layers".
10. **No density above native.** Nothing in either evaluator requests levels finer than native (`display_mips.rs:19`).

## Top risks

1. O(document) and O(document × history) authored-model costs: validation, inventories, history accounting and session encoding run on every edit or every 2 s.
2. Document-wide object-layer opt-outs that would penalise every vector document.
3. Per-frame per-layer structural comparisons that become O(items) unless they use `Arc` identity.
4. Whole-object damage and the absence of a real spatial index.
5. Drags re-render the whole damaged stack each frame with no split cache; measured object motion is already below 60 on the low tier.
6. 61 MP memory: a full layer is about 950 MiB at Rgba32Float, above the 256 MiB color cache; the hierarchy depends on runtime headroom.
7. Layer-list O(L²) and the Web non-virtualized panel.

## Mechanisms to reuse

- The `object_tile`/`Compositor::objects` leaves under `owner_image`; `ObjectWindow`, which accepts any density.
- `ObjectCache` admission, LRU eviction, tap budgets and window parts.
- Graph branch caching over the balanced `over` tree, as the drag split-cache substrate.
- Sparse `composite_tiles`/`document_damage` page sets.
- `presents_same_images`-style fast edit validation.
- `Arc`-identity history charging (`charge_selection` precedent).
- 64-entry chunking (`RASTER_INDEX_CHUNK`).
- `Resource` encoded-byte caching and lz4 resources.
- Windowed `windows::Plan`; `MovingImages`-style budgeted per-frame steps.
- 32² thumbnails with ≤8 in flight.
- Visible-row thumbnail scheduling; GTK, Android and Windows list virtualization.

## Not confirmed

- Real hierarchy admission on each tablet.
- Exactly how many times `allocation_for` runs per frame; I inferred it from call sites.
- Apple `LazyVStack` row retention, which I took from general SwiftUI behaviour rather than the code.
- Whether any autosave path skips unchanged documents beyond the stamp check.
