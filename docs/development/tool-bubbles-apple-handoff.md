# Apple tool bubbles handoff

[Apple development](apple.md) · [Apple porting guide](../APPLE_PORTING_GUIDE.md)

Implement tool bubbles on macOS and iPadOS, validate both clients, then fetch,
rebase and push the completed milestone to `origin/main`. Apple bubble presentation
and input are not implemented. The Rust bridge, project tests and Swift syntax
checks described below are baseline checks, not evidence of an Apple bubble port.

## Authority and scope

GTK is the behavioral and visual authority for this port. Start with the Host
contract in [tool bubbles](../ui/tool-bubbles.md), then read
[architecture](../architecture.md), [panel transparency](../ui/panel-transparency.md),
[drag and reorder](../ui/drag-and-reorder.md), [shared UI](../ui/shared-ui.md),
[squircle corners](../ui/squircle-corners.md) and
[color management](../ui/color-management.md). Preserve every specified behavior;
document any necessary native-platform difference with its reason.

Reference implementations:

| Area | Source |
| --- | --- |
| GTK presentation and native journeys | `apps/layer-linux/src/workspace_bubbles.rs`, `glass.rs`, `bubble_tests.rs` |
| Geometry, hit testing and constants | `crates/layer-ui/src/bubbles.rs` and `bubble_desk.rs` |
| Shared actions, menus, ownership and rollback | `crates/layer-ui/src/session_bubbles.rs` |
| Executable behavior contract | `crates/layer-ui/src/bubble_tests.rs` and `session_bubble_tests.rs` |
| Native presentation transport | `crates/layer-host/src/bubbles.rs`, `snapshot.rs`, `lib.rs` and `scene.rs` |
| Web and Android integrations | Web milestone `66f532bb1`; Android milestone `514fe9d1f`; inspect subsequent fixes in current `main` |
| Retained Windows integration | `apps/layer-windows/WorkspaceBubbles.cpp`, `BubbleInput.cpp`, `LayerInspector.h` and `native/src/shared_controls.rs` |

Use the current shared code and GTK tests when a snapshot or older host differs.
Keep the Apple porting ledger locally as the porting guide requires. This handoff
does not advance the Apple completion watermark or request an unrelated feature audit.

## Integration work

1. Add a retained bubble overlay to `Shared/Editor/EditorView.swift` in the
   `editor-workspace` coordinate space. Keep it present in Zen. Publish models
   through `Shared/Bridge/EditorStore.swift`; use existing `NativeOwner` and
   `CanvasFrameDriver` scheduling for motion. Avoid full panel/model rebuilds
   during movement and timer loops while idle.
2. Consume the existing `NativeHost` bubble presentations and queries
   (`bubble_motion`, `bubble_grab`, `can_take_bubble`, wheel and stamp requests).
   Expose stateless `layer_host::bubbles::query` through the Apple bridge, following
   Windows' `capy_bubble_ui`. Keep hit geometry, value conversion, collision
   resolution, authored position, option persistence, text and history in Rust.
3. Extend `WorkspaceReorderInteraction.swift`, `NativeReorderModel.swift` and
   `ReorderContact.swift` for header and toolbar tear-out. Reuse native source
   identity, lifecycle and capture arbitration. The UIKit `NativeReorderInput`
   currently admits one touch; implement two-finger takeover and release rather
   than treating another touch only as cancellation.
4. Forward native mouse, touch, Pencil and keyboard input through the shared
   actions. Coordinate with `MacMetalCanvas.swift`, `MetalCanvas.swift` and the
   existing platform wheel adapters. Bubble gaps must remain usable canvas.
   A second finger must cancel the first wheel, numeric, curve or row edit before
   moving/resizing the bubble; a second control must not commit the first edit.
5. Extend `GlassRegistry` and `GlassRegistration` in `GlassSurface.swift` to
   publish circles, squircles, strength and arcs. `capy_apple_glass_regions` in
   `native/src/previews.rs` already deserializes the shared `GlassLayout` and
   accepts those fields; update its payload documentation with the Swift caller.
   Scale positions and lengths once; preserve angles and strength. Fade-only
   changes must present immediately. Remove duplicate card glass registration.
6. Generalize the existing private `LayerInspector` in `WorkspaceDrawers.swift`
   for both drawer and bubble ownership/actions. Retain its layer list, Properties
   pages, footer, measurements and page transition. Both panels must stay in the
   shared snapshot while an open card needs them. Keep the full-card disclosure
   caption shared with the ordinary inspector.
7. Reuse the native components below, with retained identities and worker-owned
   image conversion. Keep thumbnail demand from every list and folded face in
   the shared cache; retiring one viewport must not discard another's requests.
   Extend existing menu models to preserve `is_heading` and
   `keep_open`, and guard asynchronous menus by drawing, source and request identity.

| Bubble content | Existing Apple component |
| --- | --- |
| Wheel and color field | `ColorWheelDrawing.swift`, platform `ColorWheelInput.swift`, `ColorFieldImageCache` |
| Swatches | `HDRColorSwatch`, existing `ColorPanel` swatch faces and `PaintSwatchMenu` |
| Tool and brush faces | `ToolbarTileContent.swift` and shared themed assets |
| Dab alpha images | `ToolbarComponents.swift`, `SliderPreviewSession.image` and existing preview request lifecycle |
| Layer thumbnails | `Shared/Bridge/LayerThumbnails.swift` owner tokens, request bounds and revision cache |
| Menus | `AppleContextMenu.swift`, `AppleContextMenuRequest.swift`, `EditorActionMenu.swift` and platform `NativeContextMenuSource.swift` |
| Corners and outside shadows | `Squircle.swift` and `GlassSurface.swift`; match the specified bubble shadow profile |

Cancel native contacts, holds and pending replies on owner retirement, drawing
replacement and application deactivation. Keep valid canvas contacts across
recoverable surface replacement. Do not duplicate Rust wheel rollback in Swift.
Use native timing and reduced-motion settings with the Host contract's animations.
Support iPadOS 17.5; newer APIs need an available fallback that keeps the feature.

## Validation and completion

Baseline checks passed before Apple implementation: 24 Python tests; 112 Rust
bridge tests with one optional photo test ignored; Swift 6.3.3 syntax parsing in
Swift 5 mode for Shared plus each platform. Syntax parsing does not resolve Apple
SDKs, type-check SwiftUI, link an application or validate native input. No Mac,
simulator or physical iPad was used for this handoff.

Start with the existing checks and regenerate the tracked project when adding files:

```sh
python3 -m unittest discover -s apps/layer-apple/tests -p 'test_*.py'
cargo test --locked -p layer-apple --lib -- --test-threads=1
python3 apps/layer-apple/scripts/project.py
bash apps/layer-apple/scripts/build.sh macos
bash apps/layer-apple/scripts/build.sh simulator
```

Use the Apple guide for device signing, isolated bundle IDs, mounted Swift fixtures
and XCTest commands. Extend the existing workspace, wheel, numeric, menu and
reorder harnesses. Mirror GTK bubble journeys separately on macOS and iPadOS in
light and dark themes, including native keyboard/accessibility behavior.

Cover fast tear-out/put-back, persistent menu options and source removal, every
chooser/ring/palette option, wheel drag-out and two-finger rollback/pinch, Layers
full/simple cards and rapid reopen during folding, scrolling/inertia/reordering,
two different numeric controls, exact artwork Undo/Redo, canvas gaps, focus loss,
resize, drawing switch, stale asynchronous replies, reduced motion and idle work.
Test physical Pencil and touch; injected input does not establish device parity.

Measure affected motion on reference hardware using the prescribed tier canvas,
optimized build and actual presented frames. Record CPU, GPU and presentation
attribution separately. Existing mid-tier Web and Android failures are documented
in [the tier table](../performance/mid-tier.md); they do not qualify Apple behavior
or performance. Complete the port only after both native builds and required
journeys pass, report unavailable checks, update the affected guides and push.
