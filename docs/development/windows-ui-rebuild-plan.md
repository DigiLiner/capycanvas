# Windows UI rebuild plan

[Windows development](windows.md)

Replace the WinUI 3 client's UI with a retained UI layer in Rust that works the
way the GTK, Android and iPad clients do. The goal is every motion at 120 Hz on
Windows, including low-end PCs. This plan covers Windows only: GTK stays on Linux
and the other clients don't change. The research behind it, with sources, is in
the [Windows UI rebuild review](https://claude.ai/artifact/GB8nfsJnWAYVVquTyXYg5U).
The October options study is in the
[Windows UI plan](https://claude.ai/artifact/CCCXZRuxCwJRTuDDtnX2rn).

This plan replaces the WinUI motion plan. If the M1 gate fails, restore that plan
with `git show 3f34c970e:docs/development/windows-winui-motion-plan.md` and
continue its unfinished steps.

## Why replace WinUI

- **The integration costs more than the framework.** A bare WinUI window
  spends about a tenth of Capy's UI-thread time per resize step
  ([window resize](../performance/top-tier.md#windows-window-resize)).
  - The client decodes JSON snapshots into WinRT objects and reads fields by
    name.
  - Each panel is drawn by three producers: XAML, composition fills and the
    canvas swap chain.
  - Divider drags and resizes keep the UI thread busy for most of each motion
    ([panel placement](../performance/top-tier.md#windows-winui-panel-placement-2026-10-28)).
- **The framework has limits we can't fix.** XAML layout runs on the UI thread
  and the WinUI title bar adds about 4 ms per resize step. Black resize strips,
  DirectManipulation taking over drags, and flyouts that fail over RDP are not
  ours to fix.
- **Ink trails the fastest path Windows offers.** Input-to-display p99 is about
  four times the bare D3D12 window's
  ([frame latency one](../performance/top-tier.md#windows-frame-latency-one-2026-10-28)).

## What the fast clients do

GTK, Jetpack Compose and SwiftUI with UIKit get eight habits from their
toolkits and system compositors. The new host builds each one.

| Habit | Fast clients | New Windows host |
| --- | --- | --- |
| The canvas has its own surface | GTK: a desynchronized `wl_subsurface` ([`wayland.rs`](../../apps/layer-linux/src/wayland.rs)). Android: a SurfaceView. iPad: a CAMetalLayer. | Two swap chains in one DirectComposition tree, canvas below and UI above, each presenting on its own schedule |
| Unchanged UI isn't redrawn | Render nodes, display lists and layer contents are kept until invalidated | Each node keeps its display list until its content changes; panel groups are cached layers |
| Slides and fades run in the compositor | Android's render thread, Core Animation's render server, GTK transform and opacity nodes | A compositor thread animates transform, clip, opacity and scroll offset, and takes scroll and drag input directly |
| Layout follows what changed | Dirty-branch measurement and per-constraint caches | Shared Rust places every group; panel rows have fixed heights from `panel_metrics`; Taffy's cached layout for dialogs |
| Long lists are virtual | `GtkListView`, `LazyColumn`, SwiftUI lazy stacks | The list widget builds visible rows plus overscan |
| Text is shaped once | Pango, Compose and Core Text layout caches | Parley layouts cached per widget; glyphs in a DirectWrite-rasterized atlas |
| Frames follow the display | Frame clock, Choreographer, CADisplayLink | The compositor waits on `DCompositionWaitForCompositorClock` |
| Images decode off the UI thread | Workers decode previews | Icons rasterized on a worker into an atlas; thumbnails sampled from renderer textures |

The Windows client already reads pen input on its own thread, which the other
clients don't. The new host keeps that.

## Options ruled out

None of these keeps the canvas on its own surface and also runs motion off the
UI thread.

| Option | Why not |
| --- | --- |
| Qt Quick 6.12 | Scrolling, drags and scrubs run on the GUI thread and wait on a render-thread sync every frame. Any change redraws the whole window. The canvas can sit under Qt's DirectComposition layer only through undocumented layering. IME is IMM32 only, UIA has no Scroll or ItemStatus, and pen input arrives on the GUI thread. |
| iced 0.15-dev | It lays out the whole tree and redraws every pixel on each change, and has no accessibility. |
| Avalonia 12 | Keeping the canvas independent needs a patch to its private Win32 backend; .NET garbage collection pauses threaten an 8.3 ms frame. |
| Flutter, Slint, NoesisGUI, Uno, Compose Multiplatform | Each composites the canvas inside its own frame and drives animation from the UI thread. NoesisGUI also lacks UIA and has a proprietary licence. |
| WebView2 over the canvas | It is capped at Chrome's speed, and every call to the Rust core crosses processes. |
| System XAML (WinUI 2) in islands, WPF | XAML layout stays on the UI thread. System XAML is feature-frozen, and WPF shares textures only through D3D9. |
| GPUI | It owns the window and its D3D11 device, can't show an external surface on Windows, and runs on one thread. Its shaders and Windows input code are useful references. |
| The GTK client on Windows, patching GTK | GTK 4.24 on Windows draws with Cairo on the CPU, and its D3D12 renderer is an unmerged branch. Graphics offload works only on Wayland, and scaling uses whole numbers until 4.26. The AccessKit backend is off and sets no AutomationId. There are no Snap Layouts, no HDR and no vblank frame clock. Closing these is an estimated 6–12 or more engineer-months of GTK Windows rendering work. |
| GTK drawing panels inside our compositor | Widgets draw only while mapped on a real surface, and event constructors are private, so this needs a GTK fork. |
| GTK for dialogs only | It saves the cheapest code to write, and adds a second toolkit, a 40–80 MB runtime and every dialog gap above. |

GTK misses 120 Hz on Linux too, because it animates and scrolls on its main
thread ([drawer](../performance/top-tier.md#gtk-sketch-layers-diagnostic),
[list scrolling](../performance/top-tier.md#gtk-layer-input-diagnostic)). The
GTK client stays the behaviour reference for the port.

## Design

```text
Window thread  pen WM_POINTER + history ──────────────► canvas input queue
(input only)   scroll and drag input ──────────────────► compositor
               other UI input, TSF, UIA, DPI, frame ──► UI thread
UI thread      UiState change bits → retained widgets → relayout what changed
               → re-record changed display lists → transaction
Compositor     layer motion → draw dirty layers → present UI swap chain (upper)
Render thread  canvas, unchanged → present canvas swap chain (lower)
               → shares its blur cache with the compositor
```

- **Two surfaces.** wgpu creates both swap chains on our own DirectComposition
  visuals (`SurfaceTargetUnsafe::CompositionVisual`). The canvas never waits on
  the UI, and the UI never waits on the canvas.
- **One owner per surface.** The compositor draws panel fills, glass and
  widgets. It samples the canvas blur cache directly, because both surfaces
  share one wgpu device. A panel's text, fill and glass come from the same
  display lists, so they can't drift apart.
- **Motion.** A drawer slide or fade changes a layer property; the UI thread does
  nothing. A scroll moves the offset on the compositor immediately, and the UI
  thread fills in newly visible rows from overscan. A divider drag re-lays out
  two groups and re-records only the rows whose size changed. While someone
  paints and the UI holds still, the UI swap chain doesn't present.
- **Testing without a window.** Scenes, widgets, text shaping and display lists
  don't depend on Windows. They are built and tested on Linux and on WARP.

### Parts

| Part | Approach | Adopt |
| --- | --- | --- |
| Pen input | The window thread reads `WM_POINTER` and calls `GetPointerPenInfoHistory` on the receiving thread, which the API requires. It covers pressure, tilt, twist, eraser, barrel button and cancel, and samples go into a queue for the canvas. Shared engine prediction replaces WinAppSDK's predictor. A canvas child window on another thread would share the UI thread's input queue, so the design doesn't use one. | windows-rs |
| Scene and compositor | Nodes with transform, clip (rect or squircle), opacity and a display list. Layers are cached textures. The UI thread sends transactions, and only the compositor commits DirectComposition. | Build |
| Widgets | About 45 kinds, built from typed `UiState` sections and sized from `panel_metrics`. Each keeps today's automation ID and emits its AccessKit node. | Taffy for dialogs |
| Text | Parley with `complex-scripts` provides dictionary line breaks for Thai and CJK. fontique's DirectWrite backend handles fallback. DirectWrite rasterizes glyph runs into a grayscale atlas with Windows Terminal's gamma correction. | Parley, fontique, etagere |
| Text fields and IME | Parley's `PlainEditor` handles editing and composition. A TSF bridge on the window thread is modelled on Windows Terminal's `ITfContextOwner` design. | Parley |
| Accessibility | The AccessKit Windows adapter maps `author_id` to AutomationId and covers Invoke, Toggle, Value, RangeValue, ExpandCollapse, Selection, SelectionItem, ScrollItem and Text. We add the Scroll pattern and ItemStatus. | AccessKit, accesskit_windows |
| Drawing | About six instanced pipelines: SDF rect and squircle, analytic shadows, glyphs, images, glass with `GlassPalette` tints, and small shaders for colour controls. They blend in sRGB space into an 8-bit UI swap chain, which DWM maps on HDR displays. | GPUI's WGSL as a starting point |
| Scrolling | Our own physics on the compositor, DirectManipulation for the precision touchpad, and touch through `WM_POINTER` | windows-rs |
| Window | A custom frame through `WM_NCCALCSIZE` and `WM_NCHITTEST`, including the maximize button for Snap Layouts, plus per-monitor DPI v2 | windows-rs |
| Menus and dialogs | Menus are popup windows with their own small swap chains. Dialogs use the same widgets. | Build |
| OS services | Win32 file dialogs, the existing clipboard, an OLE drop target, Windows.Gaming.Input, the AppContainer photo reader and MSIX | Keep |

The new host keeps most of [`apps/layer-windows/native`](../../apps/layer-windows/native),
the shared layout, metrics and copy, and the presenter's glass and panel shaders.
At parity it deletes the C++/WinRT client, the JSON C ABI in
[`capy_windows.h`](../../apps/layer-windows/native/include/capy_windows.h), and
the self-contained Windows App SDK payload. Its size is about the size of the
host it replaces: an estimated 15–20k lines for the toolkit and Windows shell,
plus 20–25k for panels and dialogs.

## Milestones

| Milestone | Work | Estimate |
| --- | --- | --- |
| M0 | Confirm two composition-visual swap chains on one window on x64 and ARM64. Run the new dependencies through `cargo deny`. Time Parley on every catalog. | 1–2 days |
| M1 | The decision slice: the host skeleton with the real `NativeHost`, both surfaces, the compositor and the pen thread. It shows a Layers group bridged to a Properties group, with glass, a drawer, a Zen fade and a tool bubble. Run T1–T8. | 2–3 weeks |
| M2 | The TSF bridge (T9), scroll and drag feel (T10), window frame and DPI (T11), colour on SDR and HDR (T12), and menus and the New Drawing dialog (T13) | 3–4 weeks |
| M3 | Port the slowest motion first: header and tabs, toolbars and drawers, columns and bridges, panel bodies, bubbles and the canvas bar, dialogs, Preferences. Each family's fixtures pass before the next starts. | 8–12 weeks |
| M4 | Run the full fixture and benchmark matrices and walk every journey in both themes. Switch the default and delete the WinUI host in the same change. Update [architecture](../architecture.md), which says hosts draw controls with their own toolkit. | About 2 weeks |

Each test records its prediction and stop signal before it runs, against WinUI
`main` built from the same commit, on the same device and input.

| Test | Checks | Pass | Stop if |
| --- | --- | --- | --- |
| T1 Two surfaces and ink | PresentMon present modes and ink p99 with stand-in panels, then the real slice, against panels drawn into the canvas swap chain | Ink p99 no worse than WinUI; independent flip kept on the Surface Pro 11. If two surfaces cost more than 1 ms of ink on the low-end PC, the UI draws into the canvas swap chain there. | Both layouts add more than one refresh to the bare window's ink p99 |
| T2 Motion without the UI thread | Drawer slide, floating-panel drag, Zen fade, bubble spring and a fling over 1,000 layers, with a 20 ms UI-thread stall injected | At least 114 fps with p99 ≤ 16.7 ms on both PCs, through the stall | A compositor frame costs more than 2 ms of CPU on the low-end PC; then run T8 |
| T3 Relayout and redraw cost | A value change, a divider drag and a window resize, split into layout, shaping, recording and transaction time | UI CPU per change ≤ 2 ms on the Surface Pro 11 and ≤ 4 ms on the low-end PC | A resize costs more than 8 ms on the low-end PC |
| T4 Pen thread | [`windows-pen-latency.ps1`](../../tools/performance/windows-pen-latency.ps1), with the UI busy and still | Every sample consumed with none unmatched; axes match WinUI's; ink unaffected by a busy UI | Any lost sample |
| T5 Text | Longest labels in all 15 languages at 100–200%, compared with [`tools/visual`](../../tools/visual/README.md) | Thai breaks at words, Han forms and wrapping match Web, and reviewers accept the crops beside WinUI's | Missing glyphs or wrong Han forms |
| T6 UI Automation | The slice driven with [`CapyUia.ps1`](../../apps/layer-windows/scripts/CapyUia.ps1) and Narrator; 3–5 existing fixtures | Fixtures pass with only the agreed acknowledgement change | The adapter can't carry the patterns the fixtures need without a fork we'd maintain alone |
| T7 Startup, memory, idle | First UI and paintable frames, private memory at New Drawing, idle wakeups | No worse than WinUI; no idle frames | A regression we can't fix in the milestone |
| T8 Windows.UI.Composition | The layers as system visuals, with T1 and T2 repeated (only when T2 needs it) | Switch only if it beats our compositor on the low-end PC with ink no worse | — |

## Benchmarks

Arms:
- WinUI `main`, the matched control;
- the Web build in Chrome on the same PC;
- the new host;
- the bare D3D12 window, as the ink floor.

Rules:
- Run release builds, warmed up, with at least three 6-second contacts per row
  in ABBA order, on outputs verified at 120 Hz.
- Record PresentMon present modes for both swap chains.
- Follow [measuring](../performance/measuring.md), and record before → after in
  the tier tables.

Unless a row says otherwise, the gate on both the Surface Pro 11 and the low-end
PC is:
- at least 114 fps;
- p99 interval ≤ 16.7 ms;
- UI CPU ≤ 2 ms per change on the Surface Pro 11 and ≤ 4 ms on the low-end PC;
- faster than the Web build in Chrome on the same PC.

| Workload | Harness | Row-specific gate |
| --- | --- | --- |
| Painting with a still UI | Pencil and G-Pen at 18 px and 1,000 px, 240 Hz injection; beside panels, under panels and in Zen | Ink p99 no worse than WinUI, aiming for ≤ 10 ms; brush rate within Zen's spread; no UI presents |
| Divider drag between stacked groups | The ±60 px, 2 s sinusoid | Default |
| Window resize | The [focused resize workload](windows-resize-120hz-handoff.md#focused-resize-workload) | UI CPU ≤ 4 ms per change; no black interior |
| Column width drag | The GPU VM column workload, on hardware | Default |
| Layer list scroll | 50, 200 and 1,000 layers by wheel, touchpad and touch | 1,000 layers cost the same as 50 |
| Layer reorder | Mouse, touch and pen, with auto-scroll, following the [drag convention](../ui/drag-and-reorder.md) | Default, and the drop lands where shown |
| Slider scrub while painting | A Properties slider and number-field drag during a stroke | Default, with ink as in the first row |
| Animations | Drawer, floating-panel drag, bubble spring, Zen fade, thumbnail selection outline | No UI-thread work during slides and fades |
| Text-heavy UI | Command search at 10 characters/s in English and Japanese, Preferences, theme switch | Key to display p99 ≤ 16.7 ms |
| Startup and switching | Launch to first UI and paintable frames; workspace switch | Better than WinUI |
| Memory | New Drawing, then every icon in both themes | No more than WinUI |
| Idle and battery | Idle, a blinking caret, painting on battery | No idle presents; drain no worse than WinUI |
| Stress | Fastest divider flicks, resize reversals and scroll flings while painting, before caches warm | No gap over two frame budgets after the first second; no ink stall |

## Devices

- **Surface Pro, 11th Edition:** 120 Hz motion, ink, pen axes and the ARM64
  build.
- **i7-1255U laptop with Iris Xe:** x64 diagnostics only, because its panel runs
  at 60 Hz.
- **Low-end Windows PC (to buy):** for example an Intel N100 or N150 with UHD
  graphics and 8 GB. It needs a 120–144 Hz output and a second monitor at
  another scale.
- **Windows VMs:** builds, fixtures, accessibility and IME automation. They can't
  measure frame rates.

[Devices](devices.md) covers reservations and private desktops.

## Gates and outcomes

M1 passes when all seven gates hold:
- **Ink:** T1, T4 and the painting row.
- **Motion:** T2, the scroll row and the animation row.
- **Change cost:** T3 and the divider row.
- **Beat Chrome:** every motion row measured.
- **Text:** T5.
- **Accessibility:** T6.
- **Cost:** the slice extrapolates to a port that will be funded.

The outcomes:
- **Go.** Continue to M2, starting with the TSF bridge, the largest remaining
  risk.
- **Change the compositor.** If only compositor cost misses, run T8 and move the
  layers to Windows.UI.Composition if it wins. Widgets, text, input and fixtures
  carry over.
- **Stay on WinUI.** If ink or accessibility can't match WinUI, restore the
  WinUI motion plan.

## Open decisions

- **A low-end Windows PC with a 120 Hz output.** Recommended: buy one, since no
  lab device represents the PCs this work targets.
- **AccessKit gaps.** Recommended: add the Scroll pattern and ItemStatus to the
  Windows adapter upstream, carrying a vendored patch until they land, so the 92
  fixture scripts keep working. The alternative is moving acknowledgements to
  the trace files.
- **WinUI motion work.** Recommended: pause it apart from bug fixes, since its
  remaining steps would be deleted at parity.
- **Funding.** The full port is an estimated 4–5 months of one engineer's time.
  M1 refines that estimate.

## Rules

- **Ink is the guardrail.** Measure it after every change to input, the render
  thread, presentation or glass.
- **Canvas frames present as soon as they're ready.** Chrome sync applies only
  to discrete transitions, never to continuous motion.
- **Shared Rust stays the source of behaviour, layout and UI text.** A shared bug
  found along the way ships separately with its regression test.
- **Scene and widget logic is tested without a window.** Production types get
  no test-only switches.
- **The new host lives beside the WinUI host until parity,** and the change that
  makes it the default deletes the WinUI host.
