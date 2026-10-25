# Pen tilt

[Workspace and UI](README.md) · [Pen pressure](pen-pressure.md)

GTK exposes **Preferences → Input → Pen response → Pen tilt → Adjust…** directly
after Pen pressure. It closes Preferences and opens the same modeless, draggable
utility used for pressure. Only one pen response editor opens at a time; switching
editors discards the previous draft. Other hosts do not expose this editor yet.

Curve, sensitivity, direction and Reset controls work immediately, including
before any pen has been used. Hover or press the pen anywhere over the GTK
dialog, including the direction circle, to identify the device and enable Apply.
The circle and graph preview the sensor without painting, changing a selection
or moving the canvas. Canvas input also identifies the pen; no stroke is required,
even with the brush cursor hidden.
The caption identifies the pen being adjusted. Edits made before identification
are retained; saved response or direction settings that were not edited remain
intact. An open editor keeps its target when another pen is used; close and reopen
it to adjust the last pen used. Drawing stays available.
First identification preserves the measured window and any active window drag.
When a curve gesture began before identification, binding waits until that
gesture ends. Cancelling restores the pen's saved response unless earlier
explicit edits are still pending.

The graph maps physical lean from upright, with Input and Output axes from 0° to
90°. The neutral response is a straight line. The origin stays at zero; interior
controls use the pressure editor's add, drag, delete, drag-out removal and keyboard
behavior. The final control moves along the top or right graph boundary. Moving
it rescales the other controls while preserving the normalized curve shape.
**More sensitive** moves it left along the top edge, reaching 90° output earlier.
**Less sensitive** reverses that movement, then moves down the right edge to
reduce output even from the neutral response. Each step changes an endpoint
coordinate by 2.5 percentage points; its minimum is 2.25°.

The direction preview shows input in gray and adjusted tilt in blue. It sits above
the joined segmented icon bar used by Tool Options, on the same centerline as
the graph. Its independent **Reverse horizontal**, **Reverse vertical** and
**Swap axes** toggles retain their text in tooltips and accessible labels. They
can be combined to represent all eight signed axis mappings.
Swapping happens before the reflections. Direction correction preserves the lean
magnitude; the response curve changes magnitude without changing direction.

GTK gives both calibration utilities a 384-pixel minimum width and 16-pixel
content margins. The pen name sits below the title; the title and Close share a
vertical center. Both units use the same square plot size, with equal side gutters
measured from the common maximum-label width in the current font. Sensitivity
and footer actions share the 36-pixel toolbar tile height. Direction segments
have 36-pixel square targets and 16-pixel icons, joined outer corners and the
normal selection fill.
Body groups have 12-pixel gaps; sections and the footer have 16-pixel separation.
Joined direction controls touch, and other adjacent actions have 8-pixel gaps.
Apply is the suggested action.
The utility is shown after shared placement accepts its measured native size.
Like pressure, it opens within the clear canvas with a 16-pixel inset and stays
above expanded workspace columns and drawer connections.

**Reset** previews the neutral response and direction. **Apply** saves this pen's
profile and closes. **Cancel**, Close and Escape discard the draft. Escape first
cancels an active point or frame drag. Test strokes stay in ordinary artwork
history. Calibration itself does not edit artwork or workspace history.

## Shared implementation and coordinate frames

`layer_core::TiltResponse` stores authored controls and reuses `PressureResponse`
for the normalized spline and constraints. `TiltAxes` owns signed permutations,
composition, inverse and frame conversion. `TiltCalibration` stores the response
and direction. Settings keep profiles by stable input-source key.

The shared response editor supplies curve actions, sensitivity, window placement
and view projection for both pressure and tilt. GTK reuses one panel and the same
`CurveEditor`. Hosts register `PenDeviceInfo` and forward native samples; they do
not evaluate response curves or own calibration transactions.

`PenDeviceInfo.native_to_surface` describes the stable input frame in current
surface axes. Shared code stores a correction C in the input frame and presents
R C R⁻¹ in surface axes. A horizontal reflection therefore becomes a vertical
reflection after a quarter turn, retaining its physical meaning. All eight
mappings and four quarter-turn frames have headless regression coverage. Canvas
rotation, zoom and mirroring happen after calibration and do not change it.

GTK keys combine tablet vendor/product/name and pen serial/hardware identifiers.
Pens without serials share a profile when those identifiers are indistinguishable.
GTK currently uses its delivered logical-tablet axes as the calibration frame.
GDK does not expose the physical tablet-to-monitor rotation mapping through its
public device API. Automatic reprojection after a physical monitor or compositor
tablet-mapping change is therefore not established on GTK; screen dimensions must
not be used to guess it. Native hosts with a known frame supply that frame to the
same shared model.

`layer_engine::TiltCurve` decodes projected sensor angles to physical radial tilt,
applies the response and direction, then lets the camera map direction into the
document. Each stroke captures its calibration at Down, including input held
before painting becomes ready. Queued samples, native predictions, estimated
corrections and contact cursors retain that calibration. Hover and later contacts
preview new settings immediately. Compiled curves are runtime data only; direction
changes reuse their response tables, and curve edits leave other profiles’ tables
intact.

## Validation

Run the core tilt tests, engine tests and UI tilt, pressure calibration, held
contact and localization tests. Run `native_tilt_calibration` through
`tools/performance/workspace-motion.sh gtk --native-test=native_tilt_calibration --tablet`
in both themes. It covers Preferences, mouse/touch/pen controls, native tilt
metadata, editing before first pen input, hover with a hidden cursor, retained
window geometry on first input with Lasso, Pencil and Hand, cancelled unbound
curve gestures, deferred identification during a curve drag, preview-only input
over the dialog and circle, native measurement recovery, retained
drafts, independent icon toggles, tooltips, matched square plots, header alignment,
footer geometry, clear-canvas placement, stacking and opaque pixels above an
expanded column and connector, live arrows, graph and
window dragging, canvas painting, Apply, Reset,
Cancel, Close and Escape. Run the pressure and tonal Curves journeys after changes
to their shared widgets. Set `LAYER_TILT_MOTION=1` and `LAYER_NATIVE_EVENT_MS=8`
for three five-second frame and curve drags with native presentation timestamps.
These desktop diagnostics do not qualify reference-tablet painting performance.
