# Vector-layer performance lessons: Inkscape, Illustrator, Affinity Designer, CorelDRAW and Krita

[Vector layer performance](../vector-performance-research.md) · source report, 2026-10-16

Written against the rendering plan in [vector layers §7.5](../vector-layers-research.md#75-rendering) at `1f8fdd621`. The design this research produced is in the [performance record](../vector-performance-research.md).

Sources were Inkscape GitLab issues and MRs (read through the public API and discussion JSON), Inkscape source at master from 2026‑10‑15, KDE Bugzilla, Krita source, the Affinity forum and help pages, and Corel help pages. Adobe's helpx and community pages, and the Corel community, returned 403 to direct fetches. Claims from those sites come from search-engine extracts and are marked "(via search)". Anything marked **Inference** is my own reasoning, not something a source says.

---

## (a) Top bottlenecks found

### 1. Document update and geometry work on the main thread grows with object count, even after rendering became asynchronous
- Inkscape 1.3 moved rendering off the main thread. Its author says the remaining obstacle to a "guaranteed-fluid canvas" is the `update` function: it "can't be moved off-main-thread (it would need a total rework of the events system)". He says it is slow for "SVGs with lots of objects, especially imported text" ([MR 4876](https://gitlab.com/inkscape/inkscape/-/merge_requests/4876)).
- Zooming a multi-page document fell to "about 6 fps" because `CanvasItemGroup::update` recomputed geometry ([#3723](https://gitlab.com/inkscape/inkscape/-/issues/3723)).
- 1.4.3, a 17k-path city model: zoom hangs. Profiling found `DrawingItem::update` spent its time in `std::lower_bound` over a `std::list` of cache candidates ([#6110](https://gitlab.com/inkscape/inkscape/-/issues/6110)). The fix moved to an intrusive multiset, so insert became "O(log n) instead of O(n) with n number of items on canvas" ([MR 7876](https://gitlab.com/inkscape/inkscape/-/merge_requests/7876), April 2026). An earlier fix to the same list is [MR 2785](https://gitlab.com/inkscape/inkscape/-/merge_requests/2785).
- When an item's transform changes, every descendant gets `SP_OBJECT_PARENT_MODIFIED_FLAG` and its style is re-cascaded. Each `SPGroup::update` builds a referenced vector of all its children ([sp-object.cpp](https://gitlab.com/inkscape/inkscape/-/blob/master/src/object/sp-object.cpp), [sp-item-group.cpp](https://gitlab.com/inkscape/inkscape/-/blob/master/src/object/sp-item-group.cpp)).
- `dynamic_cast` took about 20% of runtime. Replacing it with tag-based RTTI cut an ungroup-and-export benchmark from 100.4 s to 80.7 s ([MR 4760](https://gitlab.com/inkscape/inkscape/-/merge_requests/4760)).
- A user report against 1.0 gave rough thresholds: about 5k objects "still usable", 10k "pretty bad", 20k "freezes" ([#709](https://gitlab.com/inkscape/inkscape/-/issues/709)).
- Krita rasterizes vector layers at image resolution. Its maintainer wrote that "the real solution probably would be to render the vector shapes at the current zoom level, LoD-like (and only for the visible part, of course)" ([bug 405698](https://bugs.kde.org/show_bug.cgi?id=405698)).

### 2. Change notifications fan out per object, and UI panels rebuild on each one
- Inkscape Layers and Objects dialog: the dialog was fully rebuilt on every "selection changed" event, and ungroup or undo emits one such event per object. Converting 1,600 rectangles to paths made 2,059,529,605 `isDummy()` calls (46% of time). Deferring the update to idle cut the calls to 6.4 M, and the dialog's slowdown fell from about 60× to about 3× ([MR 5183](https://gitlab.com/inkscape/inkscape/-/merge_requests/5183), [#3176](https://gitlab.com/inkscape/inkscape/-/issues/3176)).
- Shift-click selecting 2,425 text objects in the layer list took "more than **eight minutes**". The cause: "for each object, `Inkscape::ObjectSet::add` is called which recalculates a whole set of selection values (bounds, handles, center, etc.)" ([#6391](https://gitlab.com/inkscape/inkscape/-/issues/6391)). The fix was "avoid emitting signals for each object added" ([MR 8125](https://gitlab.com/inkscape/inkscape/-/merge_requests/8125), 2026).
- Ungrouping thousands of objects froze 1.4.2 for "5 to 10 minutes". The hot spot was the status-bar `LayerSelector` widget; the developer called batching a trivial workaround ([#5988](https://gitlab.com/inkscape/inkscape/-/issues/5988)). A later fix moves the ungrouped items "at once": 30,000 objects went from 85 s to 35 s ([MR 8161](https://gitlab.com/inkscape/inkscape/-/merge_requests/8161)).
- With the Object Properties dialog open, selecting a large path took about 15–30 s. A regex was reformatting the path's `d` string ([#6019](https://gitlab.com/inkscape/inkscape/-/issues/6019)).
- Inkscape developers describe their signals as "overly broad", so things "update far more often then they need to" ([#1562](https://gitlab.com/inkscape/inkscape/-/issues/1562)). Hidden dialogs kept their handlers active ([#709](https://gitlab.com/inkscape/inkscape/-/issues/709)).
- Affinity Designer 1.7.1: marquee-selecting 5,700 objects took about 22 s, against about 4 s in 1.6.1. 20,000 objects took over 100 s ([forum](https://forum.affinity.serif.com/index.php?/topic/94234-version-171-very-slow-at-selecting-large-number-of-objects/)).
- Affinity V2 Layers panel: drag-reordering stalls for 1–2 s every 1–2 s. Expanding a collapsed layer while dragging takes 20–30 s with all 8 cores busy ([forum](https://forum.affinity.serif.com/index.php?/topic/187139-dragging-objects-in-the-layers-tree-is-very-slow/)). Thumbnails are slow to load ([forum](https://forum.affinity.serif.com/index.php?/topic/192301-severe-lagging-in-affinity-designer-v2-layers-panel-extremely-delayed-vector-brush-strokes-slow/)).
- Adobe says the Layers thumbnail "can slow performance when working with complex files" and advises turning it off ([Adobe help](https://helpx.adobe.com/illustrator/desktop/manage-layers/create-and-organize-layers/change-the-display-of-the-layers-panel.html), via search). A 2023 user reports files with over 1,000 layers slowing drastically on hide/unhide ([Adobe community](https://community.adobe.com/t5/illustrator-discussions/multiple-layers-in-illustrator-now-drastically-slow-down-when-showing-and-hiding-layers-in-2023/td-p/13885942), via search).
- CorelDRAW 2025: a user reports a file with about 150 PowerClips freezing or crashing while scrolling the Object Docker, and fine with the docker closed ([Corel community](https://community.coreldraw.com/talk/coreldraw-graphics-suite-2025/f/coreldraw-graphics-suite-2025-for-windows/68384/constant-crashing-when-scrolling-through-object-docker), via search).

### 3. Linear scans with no spatial index, and all-pairs path intersection for snapping
- Inkscape renders each tile by walking every child of every group and testing bounding boxes (`DrawingGroup::_renderItem`). Picking walks the same way ([drawing-group.cpp](https://gitlab.com/inkscape/inkscape/-/blob/master/src/display/drawing-group.cpp)). I found no R-tree or quadtree in `src/` apart from a vendored routing library.
- Snapping candidate search walks the whole tree. For each item it linearly searches the ignore list, which is the selection ([snap.cpp](https://gitlab.com/inkscape/inkscape/-/blob/master/src/snap.cpp)).
- Path-intersection snapping froze Inkscape "for minutes" ([#3171](https://gitlab.com/inkscape/inkscape/-/issues/3171)), and 15-minute freezes were reported on an OpenStreetMap file ([inbox#12693](https://gitlab.com/inkscape/inbox/-/issues/12693)). The only guard skips paths with more than 500 nodes ([object-snapper.cpp](https://gitlab.com/inkscape/inkscape/-/blob/master/src/object-snapper.cpp)).
- Panning lags while the cursor is over objects, because `CanvasPrivate::repick` runs a hit query on every mouse event ([inbox#13459](https://gitlab.com/inkscape/inbox/-/issues/13459)).
- In Affinity, a user found snapping made moving objects slow, saying "it tries to analyse all the objects" ([forum](https://forum.affinity.serif.com/index.php?/topic/164658-slow-performance-with-moving-objects-to-layers/)).

### 4. Effects, clips, masks and patterns multiply the cost
- In Inkscape, any clip, mask, opacity, blend or isolation forces an intermediate surface for each item on each tile. Masks get a per-pixel luminance-to-alpha pass ([drawing-item.cpp](https://gitlab.com/inkscape/inkscape/-/blob/master/src/display/drawing-item.cpp)).
- Ungrouping a clipped group "is generating a clip for each separate object" in a 30k-object file ([#3801](https://gitlab.com/inkscape/inkscape/-/issues/3801)).
- Inkscape patterns were rendered to a tile sized from their on-screen size. Zooming in used multiple GB and turned the canvas black ([#1018](https://gitlab.com/inkscape/inkscape/-/issues/1018)).
- Affinity staff on a slow file: FX applied "to the top 'Layer' container … means that every layer is being calculated and re-rendered as you change the design" ([forum](https://forum.affinity.serif.com/index.php?/topic/181174-designer-v2-suddenly-slow-laggy/)).
- Illustrator: a community expert says raster effects such as blur and drop shadow render "even for hidden objects and layers and even in Artwork view" ([Adobe community](https://community.adobe.com/t5/illustrator-discussions/big-files-low-performance/m-p/14987640), via search).

### 5. Text-attribute document storage and undo by XML replay
- Inkscape bakes transforms into the path data by default when an edit is committed: `set_transform` changes the curve, then `updateRepr` rewrites `d`, and Live Path Effects (LPEs) are recomputed for each item ([sp-item.cpp](https://gitlab.com/inkscape/inkscape/-/blob/master/src/object/sp-item.cpp), [sp-path.cpp](https://gitlab.com/inkscape/inkscape/-/blob/master/src/object/sp-path.cpp)). While dragging, it only sets the in-memory affine ([seltrans.cpp](https://gitlab.com/inkscape/inkscape/-/blob/master/src/seltrans.cpp)).
- Undo records old and new attribute strings for each XML change. Undo replays them, which triggers a document re-read and update ([event.h](https://gitlab.com/inkscape/inkscape/-/blob/master/src/xml/event.h), [document-undo.cpp](https://gitlab.com/inkscape/inkscape/-/blob/master/src/document-undo.cpp)). The history is capped by number of steps, not by memory.
- Break Apart on about 3k objects took 10–20 s, and its undo 40–50 s, even after the freeze was fixed ([#5024](https://gitlab.com/inkscape/inkscape/-/issues/5024), [MR 7809](https://gitlab.com/inkscape/inkscape/-/merge_requests/7809)).
- A 0.92 user reports an undo taking "about an hour" after Inset on 8,000 objects ([#709](https://gitlab.com/inkscape/inkscape/-/issues/709)).
- 1.3 undo regressed to 2–5 s because an "LPE fixing function" ran over the whole document after every undo or redo ([#4720](https://gitlab.com/inkscape/inkscape/-/issues/4720)).

### 6. Derived geometry recomputed on unrelated events, and per-glyph or per-reference structures
- Inkscape recomputed LPEs on selection change (`SelTrans::_selChanged`) and on copy, through `fitToRect` and then a translation of every child ([#1351](https://gitlab.com/inkscape/inkscape/-/issues/1351)).
- Inkscape text has one `DrawingGlyph` display item per glyph. A test file produces nearly a million glyphs, each triggering `redraw_area`. Opening took 25 s, against 0.4 s for a file one tenth the size, so the cost is not linear ([#5483](https://gitlab.com/inkscape/inkscape/-/issues/5483), developer notes dated 2026‑10‑16).
- Many clones of clones load in quadratic time, because a cycle check runs on every cross-reference insert. Export times of 3.5, 14.2 and 77.3 s dropped to 0.55–1.45 s after unlinking the clones ([#4281](https://gitlab.com/inkscape/inkscape/-/issues/4281)).
- Affinity: 37,440 rectangles in nested symbols made dragging update "at 0.3 Hz". The same count as plain groups "went reasonably smoothly" ([forum](https://forum.affinity.serif.com/index.php?/topic/147364-symbols-brutally-slow-in-designer-1100/)).
- Affinity: a path built with the live Corner tool made resizing slow at some rotation angles. Baking the corners fixed it ([forum](https://forum.affinity.serif.com/index.php?/topic/148921-performance-issue/)).

### 7. Path algorithms that flatten and re-fit
- Livarot booleans flatten the input with back-references to the original curves, sweep the polygons, then re-fit curves ([path-boolop.cpp](https://gitlab.com/inkscape/inkscape/-/blob/master/src/path/path-boolop.cpp)). Inkscape 1.2 added an exact 2geom path-intersection-graph algorithm with livarot as fallback ([MR 3724](https://gitlab.com/inkscape/inkscape/-/merge_requests/3724)).
- A bad bounding box passed a tiny tolerance (0.27) into livarot's `Coalesce`. Loading one file then took "15gig of ram and 1 min" ([#5356](https://gitlab.com/inkscape/inkscape/-/issues/5356)).
- Illustrator is widely reported to cap a path at 32,000 points ([Esri bug note](https://support.esri.com/es-es/bug/the-svg-file-created-in-arcgis-pro-33-does-not-show-cor-bug-000168501), via search).

---

## (b) Mitigations adopted and whether they worked

**Inkscape asynchronous, multithreaded tile rendering (1.3).**
- It worked for rendering: zoom, pan and transform got "a 2–4× speedup" ([1.3 notes](https://wiki.inkscape.org/wiki/Release_notes/1.3)), and heavy rendering no longer blocks the main thread ([MR 4876](https://gitlab.com/inkscape/inkscape/-/merge_requests/4876)).
- How it is done: the object tree's changes to the display tree are queued while a render thread works on a snapshot ([MR 4759](https://gitlab.com/inkscape/inkscape/-/merge_requests/4759)). Making `render()` const required mutexes on the caches ([MR 4986](https://gitlab.com/inkscape/inkscape/-/merge_requests/4986)).
- Problems that came with it, all from [MR 4986](https://gitlab.com/inkscape/inkscape/-/merge_requests/4986):
  - The render cache does not work well with concurrency, so "it's advisable to turn the rendering cache size down to zero to get the best performance."
  - Patterns sit behind a mutex, which "reduces pattern rendering to single-threaded performance."
  - "Using all the cores can postpone the GUI thread … causing intermittent stuttering", so the default is now cores minus one.
  - OpenMP proved "unsuitable".
- MR 4986 also adds a "totally invalidating" rule for huge groups. In source, it applies when update complexity is at least 20 and the affine changed ([drawing-item.cpp](https://gitlab.com/inkscape/inkscape/-/blob/master/src/display/drawing-item.cpp)).
- Undo later redrew far more area than needed, starting with MR 4876 ([#4720](https://gitlab.com/inkscape/inkscape/-/issues/4720)).
- The experimental OpenGL mode still renders content on the CPU. It "crashes on Windows" and "is slower than software rendering on Linux X11" ([1.3 notes](https://wiki.inkscape.org/wiki/Release_notes/1.3)). In 1.4, the GTK4 work "can impact Inkscape's performance negatively" ([1.4 notes](https://wiki.inkscape.org/wiki/Release_notes/1.4)).

**Inkscape redraw scheduling.**
- [MR 3838](https://gitlab.com/inkscape/inkscape/-/merge_requests/3838) added three update strategies:
  - "responsive", which leaves stale trails;
  - "full redraw", which lags near the mouse;
  - "multiscale", a compromise the author calls "the worst thing ever … to implement".
- It also added a decoupled snapshot used during zoom. Visible desk-colour holes when tiles miss the frame are a known artefact.
- Current defaults are a 300 px tile, a 100 px prerender margin and an 80 ms render time limit ([canvas prefs.h](https://gitlab.com/inkscape/inkscape/-/blob/master/src/ui/widget/canvas/prefs.h)).
- Result: partly worked. It is smoother, but you can see incomplete frames.

**Inkscape per-item render cache.**
- Each item's score is drawbox pixel area times filter complexity, plus half the clip area. Items scoring below 50,000 are not considered. Candidates are cached greedily, highest score first, until the budget is spent, using 4 bytes per pixel ([drawing-item.cpp](https://gitlab.com/inkscape/inkscape/-/blob/master/src/display/drawing-item.cpp), [drawing.cpp](https://gitlab.com/inkscape/inkscape/-/blob/master/src/display/drawing.cpp)).
- The budget default went from 64 to 512 MB ([MR 1474](https://gitlab.com/inkscape/inkscape/-/merge_requests/1474), [preferences-skeleton.h](https://gitlab.com/inkscape/inkscape/-/blob/master/src/preferences-skeleton.h)).
- Result: it helps filters, but its bookkeeping became an O(n) bottleneck ([#6110](https://gitlab.com/inkscape/inkscape/-/issues/6110)) and it conflicts with multithreading.

**Inkscape panel and selection fixes.**
- 1.2 populates subtrees only when expanded and ignores some attributes ([MR 2466](https://gitlab.com/inkscape/inkscape/-/merge_requests/2466)). Dialogs no longer update when not needed or hidden ([1.2 notes](https://wiki.inkscape.org/wiki/Release_notes/1.2), [MR 3369](https://gitlab.com/inkscape/inkscape/-/merge_requests/3369)).
- Deferring selection updates to idle ([MR 5183](https://gitlab.com/inkscape/inkscape/-/merge_requests/5183)) and replacing an O(children) dummy-row check with O(1) took one case from about 12 s to under 2 s ([MR 7802](https://gitlab.com/inkscape/inkscape/-/merge_requests/7802)).
- The panel is still a GTK TreeStore with one XML observer per row ([objects.cpp](https://gitlab.com/inkscape/inkscape/-/blob/master/src/ui/dialog/objects.cpp)). 1.5‑dev still showed multi-second freezes on first selection ([#6263](https://gitlab.com/inkscape/inkscape/-/issues/6263)).
- Result: each fix worked, but the same class of failure keeps coming back.

**Inkscape geometry fixes.**
- Bounding-box caching ([MR 709](https://gitlab.com/inkscape/inkscape/-/merge_requests/709)) and a fast path for pure translation, after circles' bounding boxes alone took 10% of time while dragging ([MR 1374](https://gitlab.com/inkscape/inkscape/-/merge_requests/1374)).
- Multi-node selection that defers work until all nodes are added: 20 s down to 2 s ([MR 928](https://gitlab.com/inkscape/inkscape/-/merge_requests/928)).
- Intrusive lists turned O(N²) deselection of more than 100k items into O(N) ([MR 451](https://gitlab.com/inkscape/inkscape/-/merge_requests/451)).
- Patterns and filters now keep persistent display trees instead of rebuilding them during rendering. That fixed the memory spike in [#1018](https://gitlab.com/inkscape/inkscape/-/issues/1018) ([MR 4609](https://gitlab.com/inkscape/inkscape/-/merge_requests/4609), [MR 4641](https://gitlab.com/inkscape/inkscape/-/merge_requests/4641)).

**Illustrator.**
- GPU rendering uses NVIDIA's NV Path Rendering OpenGL extension, with a vendor claim of "up to 10-times faster" for the canvas ([CGW, 2014](https://cgw.com/Press-Center/News/2014/Nvidia-Brings-GPU-acceleration-to-Illustrator-Ad.aspx)).
- Adobe recommends at least 1.5 GB VRAM for "optimum pan and zoom" ([system requirements](https://helpx.adobe.com/lt/illustrator/system-requirements.html), via search). An older known-issues page says Outline, Pixel Preview and Overprint Preview always use the CPU ([known issues](https://helpx.adobe.com/th_th/illustrator/kb/gpu-performance-known-issues.html), via search).
- Users report GPU Preview switching itself back to CPU on some files ([Adobe community](https://community.adobe.com/t5/illustrator-discussions/illustrator-suddenly-switching-to-cpu-preview-gpu-preview-greyed-out/m-p/13603182), via search).
- The 2025 performance programme lists, on [performance enhancements](https://helpx.adobe.com/illustrator/desktop/new-features/performance-enhancements.html) (via search):
  - 29.2: a faster Layers panel for hide/unhide, expand/collapse and scrolling.
  - 29.6: smoother move, scale, rotate and duplicate.
  - 30.0: "real-time on-canvas editing … without lag", and saving "only the changes you make".
- I found no independent measurements.

**Affinity Designer.**
- 1.10 claims rendering "up to 10 times faster" for documents "potentially with hundreds of thousands of objects", mainly imported CAD PDFs ([ProVideo Coalition](https://www.provideocoalition.com/affinity-1-10-free-update-makes-serifs-apps-run-faster/)).
- Vector operations run on the CPU; the GPU handles canvas previewing and adjustments ([hardware acceleration](https://affinity.help/designer/en-US.lproj/pages/Extras/hardwareAcceleration.html)).
- "Retina Rendering: Automatic" "renders as non-retina followed by retina", and "View Quality" sets how the image displays while it is being modified ([preferences](https://affinity.help/publisher/en-US.lproj/pages/Workspace/preferences.html)). A user describes the file going "blurry while zooming" before it refines ([forum](https://forum.affinity.serif.com/index.php?/topic/181174-designer-v2-suddenly-slow-laggy/)).
- Snapping uses a "Candidate List" that only hovered or newly created objects join, or it can be limited to nearby layers ([snapping](https://affinity.help/designer/en-US.lproj/pages/DesignAids/snapping.html)).
- Designer 2 has a "Hide Effects" view mode (user post in the same [thread](https://forum.affinity.serif.com/index.php?/topic/181174-designer-v2-suddenly-slow-laggy/)).

**CorelDRAW.**
- 2020 is "optimized to better leverage your system's GPU" for zoom, pan and scroll ([Corel help](https://product.corel.com/help/CorelDRAW/540111138/Main/Contents/Resources/en.lproj/CorelDRAW-Performance.html)). Corel claims some operations are "up to 10x faster" ([press release](https://www.globenewswire.com/news-release/2020/03/12/1999554/0/en/coreldraw-2020-unveils-its-fastest-smartest-and-most-collaborative-graphics-suite-yet.html), via search).
- Later updates fixed zoom and pan "when moving objects" ([2022 notes](https://www.coreldraw.com/en/support/updates/cdgs2022/update.html)).
- A 2021 user saw 10–15 s attribute changes in large files that went away after converting symbols to plain objects ([thread](https://community.coreldraw.com/talk/coreldraw-graphics-suite-2021/f/coreldraw-graphics-suite-2021-for-windows/65019/coreldraw-2021-very-slow-on-certain-attribute-changes), via search).

**Krita.**
- Rendering is split in two. On the GUI thread it takes "a shallow copy of the shapes … (it takes about 1ms for complicated vector layers)". A worker thread then renders with QPainter in 256×256 patches ([kis_shape_layer_canvas.cpp](https://invent.kde.org/graphics/krita/-/blob/master/libs/ui/flake/kis_shape_layer_canvas.cpp)). Shapes are indexed in an R-tree ([KoShapeManager.cpp](https://invent.kde.org/graphics/krita/-/blob/master/libs/flake/KoShapeManager.cpp)).
- A signal compressor that coalesced updates made large SVG moves workable ([bug 399363](https://bugs.kde.org/show_bug.cgi?id=399363)). It also introduced lag on small images, and the delay was then made to depend on image size ([bug 405698](https://bugs.kde.org/show_bug.cgi?id=405698)). Result: it trades frame rate for throughput.

**Outside my stream: Figma.** Its Layers panel computes row data only for the "20–30 rows" actually visible, using lazy derived properties and a rope-like ordered-children structure. Memory went from O(n²) to O(n), and expand, collapse, hide and lock got 30–50% faster ([Figma blog](https://www.figma.com/blog/improving-performance-in-the-layers-panel/)).

---

## (c) Implications for the Capy design

### Where the design is sound
- **Immutable `Arc` chunks.** They give the render thread an O(1) snapshot, with no queue of deferred mutations as in [MR 4759](https://gitlab.com/inkscape/inkscape/-/merge_requests/4759) and no per-frame shallow copy as in Krita. Undo becomes a root swap, not an XML replay. **Inference:** this directly avoids failure classes 1 and 5.
- **GPU coverage accumulation.** It avoids Inkscape's CPU/Cairo rasterization ceiling, which multithreading could only raise 2–4×.
- **Caching per layer per window.** It avoids per-item cache bookkeeping, the failure in [#6110](https://gitlab.com/inkscape/inkscape/-/issues/6110).

### Where it is risky, and what to change (all Inference unless cited)

1. **Re-rasterizing at document resolution during motion is the Krita trap.**
   - A full-layer rewrite at RGBA32F is 61 MP × 16 B ≈ 0.98 GB. Doing that every frame at 120 fps is about 117 GB/s of writes, far beyond a 6 ms budget.
   - Even one 256² window is 1 MiB. A 61 MP canvas has about 930 windows, so one fully cached vector layer is about 930 MiB. On 12 and 24 MP canvases it is 192 and 384 MiB.
   - Change: during motion, rasterize only windows that intersect the viewport, at the displayed level of detail. Refine to document resolution once the motion stops (Affinity's "non-retina followed by retina").
   - Give vector-window caches a memory budget with LRU eviction, and measure whether RGBA16F is enough for layer caches.

2. **One vector object per layer means an imported 17k-path SVG becomes one layer, and any edit dirties windows full of untouched items.**
   - Change: from drag start until release, keep three planes per affected window: items below the selection, the selection, items above it. Each frame then re-rasterizes, or for a pure translation just offsets, only the selection.
   - Fall back to a full re-render when the selection lives under an isolating group with opacity, blend or effects. Affinity staff confirm effects on a container force re-rendering every child ([forum](https://forum.affinity.serif.com/index.php?/topic/181174-designer-v2-suddenly-slow-laggy/)).

3. **Add a spatial index now, built into the chunks.**
   - Store a bounding box per record, a union box per 64-record chunk, and per-subtree unions. That gives a two-level bounding-volume hierarchy for free.
   - Use it for:
     - binning segments into windows;
     - dirty regions, as the old ∪ new bounds of changed records;
     - picking and marquee selection;
     - snapping candidates.
   - Inkscape's linear walks for render, pick and snap are the root of failure class 3.

4. **Compute dirty regions by diffing chunk pointers** between the old and new roots. Cost then scales with changed chunks, not document size, for edits, undo and redo alike. When many items change at once, use the changed subtree's union box, as Inkscape's "totally invalidating" node does.

5. **Watch the CPU budget for flattening and stroke expansion (2 ms on the top tier).**
   - Cache flattened and expanded outlines per record, keyed by geometry, style and transform. Because the tolerance is in document pixels, pan and zoom never invalidate these caches; keep it that way.
   - Run the work per window on worker threads, never the UI thread. Cap work per frame and degrade to lower detail rather than drop frames. Measure stroke expansion on thousand-stroke selections early.

6. **Transforms of N items.**
   - Keep per-record affines and do not bake them into geometry. Inkscape's bake-on-commit rewrites every path ([sp-item.cpp](https://gitlab.com/inkscape/inkscape/-/blob/master/src/object/sp-item.cpp)).
   - Make records hold `Arc` geometry, so copying a chunk copies 64 small records.
   - Record one undo entry per gesture, coalescing like Inkscape's merge keys ([document-undo.cpp](https://gitlab.com/inkscape/inkscape/-/blob/master/src/document-undo.cpp)).

7. **Notifications.** Emit one change-set per transaction, covering item ranges and chunk versions. Never emit per object; that single pattern caused #6391, #5988, #3176 and #709.

8. **Layers panel, if items become browsable.**
   - Shared Rust exposes a virtualized projection of the expanded tree. Store subtree row counts in chunks, so row-index lookup is O(log n).
   - Hosts fetch only the visible rows (Figma).
   - Render thumbnails lazily, off-thread, only for visible rows, within a budget, and invalidate them by chunk version. Adobe and Affinity both identify thumbnails as a cost.
   - Do shift-click range selection as one core operation, not per-row adds.
   - Panel cost must not depend on whether it is open. Measure journeys with the panel open, since "close the dialog" is the standard workaround in every app studied.

9. **Selection.** Store it as an ID set, with bounds, handles and centre derived lazily once per frame from cached bounds. Select-all should cost O(chunks), never per-add work like `ObjectSet::add` ([#6391](https://gitlab.com/inkscape/inkscape/-/issues/6391)).

10. **Snapping and picking.**
    - Query the spatial index within the snap radius only. Cap candidates by count and time, in the spirit of Affinity's candidate list.
    - Compute intersections only between candidates near the pointer, and never all pairs.
    - Prepare per-gesture candidate data on a worker thread at drag start.
    - Skip hover picking during pan and zoom ([inbox#13459](https://gitlab.com/inkscape/inbox/-/issues/13459)).

11. **Future items: text, symbols, live effects, booleans.**
    - Text as glyph runs inside one item, with outlines cached by font and glyph. No item per glyph ([#5483](https://gitlab.com/inkscape/inkscape/-/issues/5483)).
    - Symbols as shared immutable definitions that can only reference earlier definitions, so they are acyclic by construction and need no quadratic cycle check ([#4281](https://gitlab.com/inkscape/inkscape/-/issues/4281)). Cache rasterized instances at quantized scales (Affinity's 0.3 Hz nested symbols).
    - Live effects as cached derived geometry, recomputed only when their inputs change. Never on selection change, copy or undo ([#1351](https://gitlab.com/inkscape/inkscape/-/issues/1351), [#4720](https://gitlab.com/inkscape/inkscape/-/issues/4720)).
    - Booleans and Simplify off-thread and cancellable, with tolerance floors and limits on output size ([#5356](https://gitlab.com/inkscape/inkscape/-/issues/5356)).

12. **Prefer automatic level-of-detail to user display modes.** Outline, No Filters and Hide Effects exist because those renderers cannot keep up. AGENTS.md prefers better behaviour to new settings.

13. **Test corpus for the performance rows.** All of these are attached to the issues cited above:
    - the 17k-path city SVG ([#6110](https://gitlab.com/inkscape/inkscape/-/issues/6110));
    - the 30k-object clipped map ([#3801](https://gitlab.com/inkscape/inkscape/-/issues/3801));
    - the 2,425-label schematic ([#6391](https://gitlab.com/inkscape/inkscape/-/issues/6391));
    - the lorem1000p text file ([#5483](https://gitlab.com/inkscape/inkscape/-/issues/5483));
    - the gnuplot clone files ([#4281](https://gitlab.com/inkscape/inkscape/-/issues/4281));
    - a 50×50 rectangle grid ([#6391](https://gitlab.com/inkscape/inkscape/-/issues/6391)).

    Most of these are third-party SVGs, so check their licences before vendoring any.

---

## (d) What I could not verify
- **Adobe pages** (helpx, community) and the **Corel community** returned 403. Those claims rest on search-engine extracts; I could not read full context. Adobe's 10× and Corel's 10× figures are vendor marketing without published methodology.
- **Affinity's renderer internals** (tiling, threads, caches) are not publicly documented. I found only the CPU/GPU split, progressive retina rendering and candidate snapping. The 1.10 "10×" claim is unbenchmarked.
- **CorelDRAW evidence is thin:** no root-cause analyses, only user reports and release-note lines.
- **Krita:** I found no benchmarks of vector layers with thousands of shapes, only lag bugs and design comments.
- **Inkscape GPU plans:** there is no official 1.5 GPU roadmap. The 2020 Pathfinder GSoC proposal shows no outcome ([thread](https://lists.inkscape.org/hyperkitty/list/inkscape-devel@lists.inkscape.org/thread/DE7K4WHUAOWCM6EKUQ3XHYBYQCNJGSPJ/)). The open "New Rendering Engine" MR is about CMYK and colour spaces, not GPU ([MR 7767](https://gitlab.com/inkscape/inkscape/-/merge_requests/7767)). I could not open the 2023 GSoC GTK4 project page.
- **Measurement quality:** most Inkscape timings come from one reporter on unspecified hardware, and none is on mobile GPUs. The Capy bandwidth and memory figures are my arithmetic and need measuring on the tier devices.
- **Undo default:** I could not confirm Inkscape's default undo-limit setting from the source alone.
