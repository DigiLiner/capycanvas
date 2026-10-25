# Architecture

[Technical documentation](README.md)

Capy Canvas separates what a drawing means from how it is displayed. Shared Rust
code owns the document, tools and editor state. A platform client owns native
controls and operating-system services. One GPU renderer implements painting and
composition for every client.

This separation lets a new client reuse drawing behavior without recreating the
brush engine, layer rules or preference validation. It also lets the shared code
be tested without launching a window.

## The shared editor

`Document`, in `layer-core`, owns one typed `Artwork` and transient
`WorkingState`. Artwork stores compositions, stacks, occurrences, paint and
coverage sources, effect applications and immutable programs, saved selections, guides
and outputs. Stacks own order; occurrences own integer offsets and presentation;
sources own editable pixels; image objects and paint bases share immutable
ID-bearing images. Object collections own their child order and retain no
writable raster cache. Compact typed handles identify runtime records, while stable
portable IDs identify records in files. `Editor` applies atomic reversible record
changes and retains immutable owners for undo/redo.

Working selection, image selection, the canvas view origin, saved-selection overlay
visibility/color/opacity and editing targets belong to `WorkingState`; they do not
enter the portable artwork manifest. A captured `ArtworkCapture` retains shared
immutable artwork roots, an output evaluation context and a checkpoint. Shared
resource enumeration works independently of ZIP construction, so history, parked
tabs and accepted jobs can retain the same resources.

`layer-core::package` resolves portable IDs and reads or writes the lossless
package through bounded backing. The codec returns a candidate for shared color
and renderer admission. Unsupported artwork remains a preserved package with
its original bytes and any independently verified preview. See the
[authored model](reference/authored-model.md) and
[package contract](reference/capy-package.md).

Photo, native-package and stored-paper decoders share two active execution slots
per process. Native file workers wait before decoding and check a caller's
cancellation flag while waiting; stored-paper reads have no cancellation argument.
Browser worker heaps have independent counters and also need controller admission.
An execution slot ends when decoding stops. It does not reserve memory for the
returned document, profile choices, clipboard, history or GPU resources.

`layer-core::memory` separately accounts retained image payloads and instrumented
decode/preparation scratch. Native capacity is half the initial available process
headroom, capped at 4 GiB; an unavailable headroom query uses 256 MiB. Input work
may occupy three quarters of that capacity, leaving the rest for live canvas
preparation. JPEG reserves retained input and parser scratch before reading,
then adds the header-derived decoder estimate and retained gain-map base before
pixel allocation. Other photo decoders reserve twice their admitted codec
allowance, capped at one eighth of the pool. Source admission is capped at one
quarter. The browser controller admits at most two concurrent
input decoders across its workers; each Wasm instance has its own address-space
ceiling rather than a measured process-memory budget.

Reservations follow decoded buffers through row caches and pending uploads.
Immutable byte, word and text Arcs register their actual allocation once, so
document, clipboard and history aliases share one charge. The ledger's private
Weak owner pins that allocation until the last strong alias disappears and
collection releases it. Published payloads must remain immutable: do not use
`Arc::make_mut`, consume their storage, or create raw-payload Weak aliases.
Uninitialized transfer/readback buffers register only after complete validation
and initialization. The disposable decoded sample cache is shared across native
windows, bounded separately, and evicted before refusing work under pressure.
Owner retirement advances the preparation retry generation after payload fields
drop; the worker reclaims dead ledger entries before admitting the next allocation.
The render owner reads that generation without scanning owners or querying memory.
Native preparation under memory pressure registers a shared collector that scans
on a worker every 250 ms while watched and sleeps when no work is parked. This
also catches a raw Arc alias released after its enclosing owner's notification.
Existing host service ticks wake preparation after a generation change. Decode
results retain their attempt generation, excluding their own temporary cleanup;
asynchronous attempts count releases within each poll and final future cleanup,
so releases by other tasks between polls remain retry signals.
Interactive exact image preparation also decodes parked samples on workers.
Native image, raster and coordinate preparation shares one bounded pool across
windows and renderers: at most three workers and eighteen queued jobs. Dropping
a queue invalidates its queued jobs and discards any result already in progress;
workers finish at most the pool's active jobs. Each queue retains its existing
outstanding/result limit and ordered receipt allowance. Worker decoder caches
include the destination color space. Pool-capacity waits use their own generation,
so freeing a queue slot does not trigger memory-pressure retries.
Memory and worker-capacity refusals preserve their typed result: interactive work
pauses without marking the source corrupt, while snapshot jobs return the refusal.
Raster restoration prepares private color, coverage and wetness pages before
publishing them together. Source preparation includes transform and retouch
footprints, displaced Liquify inputs and brush neighborhoods before authored
frame work begins. Preparation admits projected outputs alongside retained
outputs and published pages under the renderer's composition and cache policy.
Native and browser document candidates finish their initial canvas preparation
before restoring editor history or adopting the drawing, using the operation's
existing cancellation and deadline.
Private buffer owners release their storage before returning its reservation,
including when a fallible ownership handoff is refused.

These limits cover instrumented payloads and scratch, including imported
selections, profiles, lookup tables, previews, vector encoding buffers and
transfer chunk copies. They are not an allocator or whole-process quota:
codec internals, object metadata, host copies, Wasm heap high-water marks and
GPU/driver allocations need separate limits or process isolation. Vector imports
bound expanded structures and preserved-field copies before allocation through
project admission; their retained owners remain outside the image-payload ledger. Small fixed built-in lookup tables
remain available independently of input admission.

`CanvasEngine`, in `layer-engine`, connects that model to drawing input. It
interprets ordered pen samples, evaluates brush dynamics and produces work for a
renderer. Brush dynamics are the rules that map inputs such as pressure to
properties such as size or opacity.

`UiSession`, in `layer-ui`, coordinates the engine with application behavior. It
owns the active tools, camera, workspace, command availability, preferences and
file-operation state. A frontend sends typed actions and renders the resulting
views. It keeps native widget objects, focus and accessibility outside the session.
Settings restoration owns its full payload separately, keeping shared input
actions small as preferences grow.

History availability is independent of GPU completion. Brush settings update
without waiting for unrelated raster backing. Settings queued behind earlier
edits keep their order and validate their original edit target. `UiSession` keeps
edits that arrive during raster capture in one ordered queue with subsequent pen
samples. Frames drain that queue after earlier edits finish, validating each
action against the resulting document. Completed contacts wait for preparation;
only unfinished contacts expire. Both share a bounded sample queue. Delayed
contacts keep their captured view and brush settings. Transform mode and grid
choices stay available during background rendering, preserving queued input and
edit order. Camera navigation stays available;
document activation and renderer replacement retire queued input. Hosts forward
Undo and Redo without waiting for raster capture themselves.
An accepted ready raster replacement updates any queued history restore for its
target. Pending raster operations retain the earlier revision they need to replay.

Android, Apple and Windows use `NativeHost`, in `layer-host`, to share session and
renderer integration. GTK and web integrate `UiSession` directly. The host layer
is not another document model and does not own native surfaces or widgets.
The shared native GPU factory keeps renderers on the heap during construction
and transfer to their owning sessions, leaving room on native thread stacks.
Retained scene caches stay on the heap through frame encoding and capture too.

## From a pen event to a frame

```text
Platform input callback
    │ ordered pen records
    ▼
UiSession / CanvasEngine
    ├── interpret the current tool and camera
    ├── update the document
    └── resolve brush contacts
    │ FramePacket
    ▼
CanvasRenderer (implemented by WgpuRasterizer)
    ├── update affected paint textures
    ├── compose changed layer regions
    └── prepare the visible canvas
    │
    ▼
Platform surface and presentation
```

1. The platform collects the samples supplied by its input API, including any
   history delivered with an event. It normalizes timestamps and pen axes and
   preserves whether a sample is real, predicted or a correction.
2. The shared editor decides whether the gesture paints, pans or operates another
   tool. For a stroke, it converts input into document coordinates and places
   brush contacts along the path. A resolved contact is called a *dab*.
3. The engine builds a `FramePacket`, defined by `layer-render`. This packet
   borrows the prepared batch data and a `SceneView` over typed artwork records.
   It includes explicit source targets, document changes and affected regions. It is not a bitmap or a complete document replay
   on every frame.
4. `WgpuRasterizer`, in `layer-render-wgpu`, records GPU commands to draw new dabs
   into retained source textures and update their composition. The shared viewport
   presenter draws the canvas at the current zoom and rotation.
5. The platform presents the result using its surface and frame scheduling rules.
   Native integration must also handle resize, suspension and surface loss.

## CPU and GPU responsibilities

The CPU performs ordered, comparatively small operations: input interpretation,
brush dynamics, document edits, layout, dependency tracking and command encoding.
The GPU evaluates canvas pixels, including brush coverage, blending, masks,
filters, selection masks, fills, gradients, transforms and composition; the CPU
never processes canvas pixels. Native widgets are drawn by their own toolkit; the
canvas renderer does not draw the application's controls.

`wgpu` provides access to Vulkan, Metal, D3D12 and browser WebGPU. The renderer
requires a hardware GPU and has no CPU painting fallback. Imported image and
brush source bytes may be retained in CPU memory for upload and project saving;
this is different from maintaining a CPU copy of the rendered canvas.

[Rendering and composition](internals/rendering.md) explains the GPU resources
and the explicit export, thumbnail and color-sampling readbacks.

## Scheduling and responsiveness

Each session has one ordered owner for engine and renderer mutations. Native
hosts use queues and render workers to keep input collection and native UI work
independent of GPU waits. The web host runs synchronously on the browser event
loop, using animation callbacks to schedule updates; shared code does not require
threads or shared memory.

Frame work is incremental. New samples produce new contacts, unchanged layer
results can be reused, and camera movement can redraw the viewport without
rasterizing the artwork again. UI notifications separately identify affected sections of editor state, such as
the layer list or tool settings. These are not pixel rectangles: they tell hosts
which controls need refreshing.

Shader startup is staged. The host can show controls and constant bottom fills
while the renderer prepares the current document and selected brush. Other tools
prepare on first use and reuse their compiled recipes. A prepared drawing uses its renderer's readiness for its first
presentation, including after recovery or a tab change; the startup background
must not replace its rendered artwork. Painting
waits for the required resources; showing the first frame is not the same as being
ready for a stroke. Native compilation workers and incremental web preparation
implement the same dependency ordering.
Requested thumbnail readback prepares after the document and current brush
dependencies; unused tools do not enter the startup queue.

WebGPU pipeline preparation is asynchronous and shared per recipe and device.
New and resumed canvases await only their presentation and constant-fill dependencies;
document and brush preparation follow through the existing compiler queue.
Snapshot workers prepare the kernels for a capture before encoding its tiles.
Encoding cannot replace a pending Web compilation with a blocking one. Native
compiler and snapshot workers retain their background execution. Unencoded
raster edits also hold later document edits in the ordered input queue.

Use the [testing and performance guide](development/testing.md) to measure frame
cost and input-to-display latency on the target device.

## Files and other host services

Shared document compression and photo codecs use Rust implementations, including
the explicitly selected Rust deflate backend. ICC transforms use Rust too.
Linux external photo imports run in a one-shot `capy-photo-reader` process.
Open, Place, Paste and paper import use the same shared import policy with that
transport. The helper receives a read-only anonymous input file, bounded output
and diagnostic handles, no inherited environment or other file descriptors.
It requests termination if its parent dies and checks the startup parent race.
Before decoding it enforces Landlock filesystem denial and an architecture-checked
seccomp syscall allowlist. Missing confinement fails the import without an
unconfined retry. It cannot open files, connect to the network, create processes,
execute programs, access a display or use the GPU.
The parent retains the existing two-decoder admission and reserves the child's
assigned memory while it runs. The assignment starts at half the image pool and
shrinks under existing ownership; source and codec limits fit that assignment.
The child bounds additional virtual memory, CPU time and diagnostic output;
the parent bounds the child/output phase to 120 seconds, kills and reaps
canceled children, and spools bounded output. It validates the returned package through the existing
foreign-package reader, extracts only the photo samples and metadata, and applies
shared photo policy again. App-owned stores, generated caches and bundled assets
keep their local readers. Other native hosts retain their local photo decoder.
Native workspace storage builds its bundled SQLite; it does not require system
SQLite development packages. Apple builds link the SQLite that ships with macOS
and iPadOS instead, so its file-system checks stay inside Apple's library. Toolkit dependencies belong to the Linux client,
and GPU drivers and platform APIs remain host requirements.

Shared code defines file requests, save checkpoints and close decisions. The host
opens pickers, reads or writes bytes and reports completion. Package decoding and
encoding belong off the input path. An incoming document is validated before it
replaces the live one; a failed or cancelled save must not mark a document clean.

The [document guide](internals/documents.md) explains this contract. Platform
storage APIs differ, so a shared request does not by itself establish that every
client implements the corresponding UI or provides the same storage guarantees.

## Where to read next

- [Workspace and UI](ui/README.md) follows a command from a widget to shared state.
- [Brushes](internals/brushes.md) follows pen samples through stroke generation.
- [Platform integration](platforms/README.md) describes the native boundaries.
- The [package table](../README.md#package-layout) links to each crate's entry point.
