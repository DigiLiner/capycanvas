# Windows skipped-presentation regression handoff

[Windows development](windows.md)

**Request:** whoever owns `d7b6a0fb6` ("Apply Windows model changes only to
affected views"), please pick this up. The input tracker skips presentation
after **independently applied packets**: command search, camera and probably
others. Native views stay stale, which fails about 40 Windows fixture rows on
`main`. The follow-ups `97225bc64` (skipped bodies keep their parent inputs) and
`f8e513d48` (refresh panel views on reattach) close other gaps. Neither covers
this channel. Please fix the tracker contract below. If it can't be fixed soon,
tell the user, so they can decide whether to revert `d7b6a0fb6` and its
follow-ups until it can.

## What fails

The shared-GPU session fixture round on `bd3f6953b` scored 409/490. Against the
previous round (448/490 at `838142c64`, before `d7b6a0fb6`), 48 rows went from
pass to fail. Most of them match stale native presentation, and the A/B below
attributes representative rows:

| First assertion | Rows |
| --- | ---: |
| `Command search did not close` | 11 |
| `The requested brush did not become ready` / `Tool did not activate` | 9 |
| `Missing control: tool-setting-dither_cell_size` | 6 |
| `Escape did not close sizes configuration` | 4 |
| `Mode selection_add did not select` | 4 |
| Other chrome stops (tab glyphs, rename, dial defaults, Create), not individually attributed | ~11 |

The nine brush/tool rows are the same search failure. Their failure models
show the requested brush applied, `brush_ready`, `shaders_ready` and
`canvas_ready` true, and shared `command_search` null. The only false term is
the fixture's `!(Find 'command-search' -Visible)`
(`exercise-idle-brush.ps1:202`, `exercise-cursors.ps1:63`): the native popup is
still on screen after the shared model closed it.

## Attribution

A matched A/B on `bd3f6953b` reverted only `d7b6a0fb6`. It used the same
session pool, two first attempts per arm, and changed no fixture:

| Row | `main` | `d7b6a0fb6` reverted |
| --- | ---: | ---: |
| enclose-fill | 0/2 | 2/2 |
| photo-tools:transform-dark | 0/2 | 2/2 |
| expansion | 0/2 | 2/2 |
| idle-brush | 0/2 | 2/2 |
| selection | 0/2 | 2/2 |
| liquify-pixels | 0/2 | 1/2 |
| cursors | 1/2 | 1/2 |
| tool-bubbles:SlidersOnly | 0/2 | 0/2 (unrelated fan stop) |
| zoom-readout:Navigation | 2/2 | 2/2 |

## Mechanism

1. A search packet changes shared state outside the full-model path.
   `WorkspaceView.cpp:543–544` inserts `state.command_search` and calls
   `CommandSearch::Apply` directly. It records nothing in `data->changes`.
2. The host deliberately keeps the full-model baseline independent of those
   packets: "Camera, search and workspace layout messages leave the baseline
   alone" (`crates/layer-host/src/model_update.rs:107`).
3. Closing search publishes a full model whose `command_search` equals the
   baseline, so its `model_update` delta has no `command_search` path.
   `CanvasModelUpdate.h:38–53` records changes only from those delta paths.
4. `WorkspaceView.cpp:602` runs `data->apply(searchInputs, …)`.
   `CanvasModelInputs.h:103,119` finds no intersection between the recorded
   reads and the changes, so the consumer is skipped and the native popup is
   never told to close.

Camera has the same shape: the partial packet at `WorkspaceView.cpp:550–552`,
the tracked full refresh at `:613`. An audit of the other channels found that
glass and color fill reconcile directly, and that localization waits for the
next full model. Search and camera are definite bypasses; workspace motion and
layout depend on the reflow path.

## Requested fix

- Make every independently applied packet take part in tracking. When a partial
  packet mutates `data->state` (or other tracked roots) outside a full model,
  record the paths it changed. The next full adoption then re-runs the
  consumers whose reads intersect them, even when the transport delta omits
  them. Alternatively, compare against the state actually presented rather
  than the transport delta. Choose whichever keeps the measured `d7b6a0fb6`
  wins.
- Add a windowless regression to `apps/layer-windows/tests/model-update.cpp`:
  full A (search closed), then a search packet B (open), then a full delta that
  restores A without a `command_search` path. The search consumer must run and
  close. Add the same case for camera.
- Validate with `windows-vm.py fixtures` on the rows above, at least
  `enclose-fill`, `photo-tools:transform-dark`, `expansion`, `idle-brush`,
  `liquify-pixels`, `selection`, `cursors`, `vector-edges` and
  `vector-layers:light`, in the default shared-session mode.

## Coordination

- The `origin/windows-winui-perf` branch (`216de8771`, chrome retention) uses
  the same tracker for background inputs. Its owner is making that path
  independent of the tracker. A fix here should still cover background inputs
  arriving out of band.
- The Windows fixture-reliability work does not touch these views. It will
  rescore the full round once a fix lands.
