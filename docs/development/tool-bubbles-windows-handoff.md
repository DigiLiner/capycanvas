# Windows tool bubbles hardware handoff

[Windows development](windows.md) · [Windows porting guide](../WINDOWS_PORTING_GUIDE.md)

Finish real-hardware qualification of the Windows bubble port, fix reproduced
parity or performance failures, then fetch, rebase and push the completed milestone
to `origin/main`. Use a normal optimized hardware build. WARP fixture results
establish software-adapter correctness only; they do not establish hardware
presentation, physical pen/touch behavior, HDR or mixed-DPI behavior.

## Implementation and authority

GTK is the source of truth. Follow the Host contract and journeys in
[tool bubbles](../ui/tool-bubbles.md), the GTK presenter and tests in
`apps/layer-linux/src/workspace_bubbles.rs` and `bubble_tests.rs`, and
[panel transparency](../ui/panel-transparency.md). Reuse the shared actions,
geometry, hit testing, collision solver, menu text and history in
`crates/layer-ui/src/bubbles.rs`, `bubble_desk.rs` and `session_bubbles.rs`.

| Windows component | Responsibility |
| --- | --- |
| `WorkspaceBubbles.cpp`, `BubblePresentation.h` | Retained presentation, native animation, glass, thumbnails and worker-produced wheel/dab content |
| `BubbleInput.cpp` | Native timing, pointer routing/capture, tear-out, two-finger takeover and owned menus |
| `LayerInspector.h` | Inspector reused by drawers and Layers cards, including both pages and shared footer captions |
| `LayerThumbnails.cpp`, `ThumbnailOwners.h` | Shared cache with combined targets from lists and folded faces, guarded replies and owner retirement |
| `PanelScroll.h`, `LayerRowDrag.cpp` | Native scrolling/inertia and handoff between row/control input and bubble movement |
| `NumberControl.cpp`, `CurveView.cpp`, `PointerCompletion.h` | Existing controls, explicit cancellation and multi-owner capture retirement |
| `CanvasWindow.cpp` | Canvas input through overlay gaps; event coordinates and their view revision captured before queueing |
| `native/src/shared_controls.rs`, `native/src/workspace.rs` | Stateless shared geometry, native queries, stamp transport and glass publication |

Keep semantic fixes in Rust with headless regression tests. Native input capture,
timing and toolkit presentation stay in the host. Do not reintroduce local wheel
rollback or separate Layers inspector implementations. Preserve the normal
Slider's keyboard and accessibility behavior while its common pointer surface
uses shared Rust value mapping and gesture transactions.

## Hardware journeys

Build through `apps/layer-windows/scripts/build.ps1` so the native executable and
Rust DLL agree. Follow [private desktop fixtures](windows.md#ui-fixtures), use
disposable profiles, and operate only assigned/reserved hardware. Do not inject
input into a personal desktop or reuse real settings and documents.

Run `exercise-tool-bubbles.ps1` in all six light/dark × mouse/touch/pen variants.
Its `NumericOnly`, `CurveOnly`, `NativeControlsOnly`, `ScrollEpochOnly`,
`HistoryOnly`, `HistoryLightOnly`, `MenuOnly` and `RapidOnly` switches isolate
the sensitive journeys. For example, from the repository root:

```powershell
./apps/layer-windows/scripts/test-desktop.ps1 `
  -Script ./apps/layer-windows/scripts/exercise-tool-bubbles.ps1 `
  -Parameters @{
    Executable = (Resolve-Path ./artifacts/windows/Release/CapyCanvas.exe).Path
    Variant = 'light-touch'
  }
```

The example uses the default x64 output. Native ARM64 builds use
`artifacts/windows/arm64/Release`; follow the Windows guide's explicit target commands.

Repeat these interactions with actual mouse, fingers and a physical pressure pen:

- Tear out every supported source quickly, move through neighbours, put it back,
  restore the workspace and confirm retained options. Paint through hollow gaps;
  verify exact artwork Undo/Redo through both halves of the history bubble using
  pointer, keyboard and accessibility input. Check their shared enabled states.
- Exercise all chooser, halo, palette and size choices, disabled states, dab
  previews, wheel drag-out and cancel. Verify two fingers restore the first color
  or numeric edit before moving or pinching, including two different controls.
- Scroll an overflowing Layers card with one finger, continue through inertia,
  introduce another finger during scrolling, then confirm fresh scrolling,
  selection and reorder still work. Scroll far enough to recycle rows and repeat
  with native controls inside ordinary drawers and docked panels.
- Check full/simple Layers cards, Properties pages and their shared captions,
  keyboard/RangeValue input, valid expression-to-slider Undo units, refused text,
  curve insertion and cancellation, and reopening during the actual 150 ms fold.
  A gesture delivered after that interval does not qualify the rapid-reopen case.
- Hold for native menus, drag after a hold, release over a menu, then immediately
  use a card control. Keep-open options must update without dismissing the menu.
  Switch drawings while a menu request is pending; stale actions must be inert
  and a new menu on the current source must still open.
- Check cancellation, Escape, focus loss, minimize/restore, owner removal,
  drawing replacement, resize, Zen, reduced motion and return from a stroke fade.
  Move the window between displays with different scale factors during a contact;
  preserve coordinates paired with their original view revision.

Review composed captures against GTK at matched sizes, scale and theme. Inspect
glass under rounded corners and arcs, fade strength, disabled alpha, chooser
labels, shadows, the 30 px palette pill and native card content. Include HDR
output and wheel colors where the hardware supports them.

## Required validation and measurement

The six light/dark × mouse/touch/pen bubble journeys pass in the VM. That matrix
precedes the final history, row-cancellation and thumbnail-owner changes;
focused reruns cover the changed behavior instead of repeating the whole matrix.
The numeric, curve, menu and drawing-switch scroll journeys also pass.
Both history themes pass shared labels, enabled states and half geometry,
accessibility and native keyboard Undo/Redo against exact pixels, disabled-action
rejection, and painting through a hollow corner with exact Undo.
The ordinary effects curve, color-editor, color-picker and vector-edge journeys
pass in both themes, and native prediction with the engine fallback passes.
The color-editor checks include numeric editing, exact sampled-color transfer
and Paper thumbnails. Vector edges include same-DPI resize during contact,
document reopening and localized controls. The accessibility variant of the
new-features journey also passes. Both complete Layers drawer journeys pass
after the final Escape fix, including keyboard Add Paper, reorder and overflow.
These establish their asserted behavior on injected input and WARP; they do
not qualify physical input or moving-frame performance.

An actively scrolling Layers list still has a reproducible VM takeover failure:
`NativeControlsOnly` reports “Second finger from actively scrolling list did not
move the shared bubble”. Both root captures initially succeed, then WinUI
receives `WM_POINTERROUTEDAWAY` while the fingers remain down. The capture-loss
guard cancels the drag and restores the bubble position. Artwork, editing layer,
row order and history checkpoint remain unchanged. Reproduce this first on real
touch hardware; fix it if it reproduces, and rerun scrolling, inertia interruption,
selection, recycling, reorder and one-step Undo. Do not remove the safe loss guard
or count cancellation as successful takeover. No native capture recipient has
been established beyond the XAML routed-away event.
The same VM journey passes subsequent fresh scrolling, observed live-inertia
interruption with an opposite pan, another fresh pan, row selection, recycled
row identities, reorder and exact one-step Undo. Native manipulation completion
before capture transfer is retained; allowing it to continue after handoff fails
to fix takeover and prevents the next pan.

The VM's Release build, native C++ input assertions and PowerShell syntax checks
pass. The private-desktop Bitmap fixture passes all 14 checks, including multiple
thumbnail owners, and exits with no pooled upload slots left. The normal executable
is restored afterward. Its fixture build retains the existing C4244 warning from
the integer fill value in `BitmapUploadFixture.cpp:31`.
Rapid-reopen qualification requires observing native card retirement
before delivering the reopen contact within the 150 ms fold; observing only the
shared folded model does not establish native event ordering. The VM's strict
`RapidOnly` case remains unqualified: its last-open-to-contact upper bound is
306.7 ms, despite only 0.071 ms between detecting retirement and injecting the
contact. It reports “not exercised”; this does not establish a rapid-reopen
application failure. Settled folding and reopening pass in the six general
journeys. Repeat the strict case on hardware and verify the live footer and
Properties controls during the actual fold. Switching drawings
during an active native pan, returning and scrolling again passes with the
original history checkpoint intact. The required software-adapter runner also
passes color (143), core (711), engine (195), host (132), render (4), Windows (131)
and workspace (98) tests, plus
the isolated hidden-profile test and four D3D12 document tests. These counts
exclude ignored tests.

The shared UI suite has an existing failure in
`session::tests::slider_stamp_plan_accepts_every_builtin_tool_context`:
`toolbar_component_tests.rs:1390` expects 66 presets and receives 65. It reproduces
in both the Linux run and Windows software-adapter check; the other 1,791 UI
tests pass, with two ignored. Strict Clippy also fails in unchanged `layer-core`
code on `too_many_arguments` and `collapsible_if`. Retain these failures separately
from the bubble journeys; do not weaken their assertions to qualify the port.

The ordinary pen-button fixture reaches its documented baseline failure:
“Fixture did not begin with an empty paint layer”, because Transform is enabled.
The separate canvas-touch sequence remains unqualified. Its final diagnostic
fails in the injection driver with “Touch keepalive failed” during the two-finger
pan check, before the camera assertion; cleanup separately reports
`ERROR_INVALID_PARAMETER` while cancelling injected touch. Preserve the original
exception when cleanup also fails. This does not establish a camera regression
or qualify the remaining pinch, rotate and pen-after-touch sequence.

The pointer new-features journey passes its strengthened Escape regression:
the swipe closes while the Layers drawer and document revision stay unchanged.
It also reaches the subsequent outside-press cancellation, then fails with
“Disabled layer-1-swipe-delete”. The retained shared state has Current ink locked,
its Delete action disabled, both original rows intact and the document not busy.
The native disabled state agrees with Rust. The cause of locking during the
preceding swipe sequence is not established; trace that sequence on real input
before claiming pointer swipe/delete parity. Do not force-unlock the row or
weaken the enabled-state assertion. Later pointer deletion/history phases are
unqualified; the separate accessibility journey passes them.

Run the required Windows checks with fresh test storage:

```powershell
./apps/layer-windows/scripts/test-without-gpu.ps1
$env:CAPY_STORAGE_DIR=Join-Path (Get-Location) ('artifacts/windows/test-storage/'+[Guid]::NewGuid().ToString('N'))
$env:PATH=(Resolve-Path artifacts/windows/Release).Path+';'+$env:PATH
cargo test --locked --release -p layer-windows --lib d3d12_ -- --ignored --test-threads=1
```

Use the architecture's matching build folder so direct Cargo tests load the
pinned DXC. Debug hardware tests additionally need `RUST_MIN_STACK=8388608`.
Restore the previous environment after the checks; keep each storage directory
private and fresh, as the [Windows test guide](windows.md#test) specifies.

The hardware command includes checks the VM cannot qualify. Run affected
drawer, color-editor, numeric, curve, layer-row, pointer and resize journeys when
changing their common components. Follow the [GPU reconstruction checks](windows.md#gpu-reconstruction)
for actual device-loss coverage; do not treat software-adapter recovery as raw
hardware removal coverage.

Measure moving bubbles, neighbour collisions, halo/chooser expansion, wheel
pinch, card scrolling/folding/reopening and stroke fade with the tier's prescribed
canvas and optimized build. Use PresentMon/actual presentation records plus
separate CPU/GPU attribution under [the measurement rules](../performance/measuring.md).
Check idle frames and stale preview work after release. Record current results
in the tier tables; WARP rates, frame callbacks and submission counts cannot
qualify the targets.

The [mid-tier table](../performance/mid-tier.md) records unresolved Web and Android
performance misses. Windows has no reference-hardware bubble qualification yet.
The broader Web workspace-rendering comparison also retains a baseline-identical
opaque-sharp crop failure at thumbnail corners: 80 of 168,000 channels differ,
with a maximum difference of 27 against the limit of 8.
Treat performance and functional parity as separate completion requirements.
Retain source/binary identities, exact failed assertions and composed captures
locally, update affected guides, and report every remaining unverified check.
