package art.capycanvas

import android.app.Instrumentation
import android.graphics.Bitmap
import android.os.Bundle
import android.os.ParcelFileDescriptor
import android.os.SystemClock
import android.view.InputDevice
import android.view.MotionEvent
import kotlinx.coroutines.runBlocking
import org.json.JSONArray
import org.json.JSONObject
import java.io.File
import java.util.concurrent.CountDownLatch
import java.util.concurrent.TimeUnit
import java.util.concurrent.locks.LockSupport
import kotlin.math.*

internal fun runVectorBenchmark(instrumentation: Instrumentation, args: Bundle, host: CanvasHost,
    output: File, label: String, fixture: File) {
    fun <T> native(block: (Long) -> T): T = runBlocking { host.withNative(block) }
    fun healthy() { check(host.failure == null) { host.failure!! }; check(host.actionError == null) { host.actionError!! } }
    fun state(): JSONObject {
        native { Unit }
        var value = JSONObject()
        instrumentation.runOnMainSync { value = JSONObject(host.snapshot!!.toString()).getJSONObject("state") }
        return value
    }
    fun action(value: JSONObject) { instrumentation.runOnMainSync { host.dispatch(value) }; native { Unit }; healthy() }
    fun invoke(command: String) = action(obj("type" to "invoke", "command" to command))
    fun stats() = native { JSONObject(Native.query(it, obj("type" to "renderer_stats").toString())) }
    fun display() = native { JSONObject(Native.displayStatus(it)) }
    fun content() = state().array("layers").objects().joinToString(";") { "${it.optLong("id")}:${it.optLong("paint_revision")}:${it.optLong("mask_revision")}" }
    fun measurements(reset: Boolean): JSONObject {
        val done = CountDownLatch(1)
        var value = JSONObject()
        host.measurements(reset) { value = it; done.countDown() }
        check(done.await(30, TimeUnit.SECONDS))
        return value
    }
    fun waitFor(what: String, timeoutMs: Long = 120_000, condition: () -> Boolean) {
        val deadline = SystemClock.uptimeMillis() + timeoutMs
        while (!condition()) {
            healthy()
            check(SystemClock.uptimeMillis() < deadline) { "Timed out waiting for $what" }
            SystemClock.sleep(5)
        }
    }
    fun settle(): Long {
        waitFor("rendering to settle") { !native { Native.renderingPending(it) || JSONObject(Native.displayStatus(it)).optBoolean("pending_edits") } }
        return System.nanoTime()
    }
    fun ready() {
        native { Unit }
        waitFor("tool readiness") { host.snapshot?.optBoolean("brush_ready") == true }
        settle()
    }
    fun screenshot(name: String) = instrumentation.uiAutomation.takeScreenshot()?.let { bitmap ->
        File(output, "$label-$name.png").outputStream().use { bitmap.compress(Bitmap.CompressFormat.PNG, 100, it) }
        bitmap.recycle()
    }
    val plan = JSONObject(ParcelFileDescriptor.AutoCloseInputStream(instrumentation.uiAutomation.executeShellCommand(
        "cat ${args.getString("vectorPlan") ?: error("A vector plan is required")}")).use { it.readBytes().decodeToString() })
    val duration = args.getString("durationMs", "5000")!!.toInt()
    val repeats = args.getString("repeats", "3")!!.toInt()
    val waitForTrace = args.getString("waitForTrace") == "true"
    val density = instrumentation.targetContext.resources.displayMetrics.density.toDouble()
    val scenarios = args.getString("vectorScenarios", "move,scale,rotate,points,erase,width,pan,pinch,rotate_view,fill,commit,merge,open")!!.split(",")
    val eraserSize = args.getString("eraserSize", "64")!!.toDouble()
    val inkSize = args.getString("inkSize", "24")!!.toDouble()
    val fillBelow = args.getString("fillBelow", "true") == "true"
    val sizes = plan.getJSONObject("selections").keys().asSequence().toList().sortedBy { it.toInt() }

    fun layerId(name: String) = state().array("layers").objects().single { it.optString("label") == name }.getLong("id")
    fun selectLayer(name: String) = action(obj("type" to "select_layer", "id" to layerId(name)))
    for ((key, visible) in listOf("vectorShow" to true, "vectorHide" to false))
        args.getString(key)?.split(",")?.filter { it.isNotEmpty() }?.forEach { action(obj("type" to "set_layer_visibility", "id" to layerId(it), "visible" to visible)) }
    fun camera(): JSONObject {
        val c = state().getJSONObject("camera")
        check(c.getDouble("rotation") == 0.0 && !c.getJSONArray("flipped").getBoolean(0) && !c.getJSONArray("flipped").getBoolean(1)) { "The camera is rotated or flipped: $c" }
        return c
    }
    fun surface(c: JSONObject, x: Double, y: Double): DoubleArray {
        val zoom = c.getDouble("zoom")
        val t = c.getJSONArray("translation")
        return doubleArrayOf(x * zoom + t.getDouble(0), y * zoom + t.getDouble(1))
    }
    fun surface(c: JSONObject, p: JSONArray) = surface(c, p.getDouble(0), p.getDouble(1))
    fun inWorkArea(c: JSONObject, p: DoubleArray): Boolean {
        val a = c.getJSONArray("work_area")
        return p[0] >= a.getDouble(0) && p[1] >= a.getDouble(1) && p[0] <= a.getDouble(0) + a.getDouble(2) && p[1] <= a.getDouble(1) + a.getDouble(3)
    }
    fun workCenter(c: JSONObject): DoubleArray {
        val a = c.getJSONArray("work_area")
        return doubleArrayOf(a.getDouble(0) + a.getDouble(2) / 2, a.getDouble(1) + a.getDouble(3) / 2, a.getDouble(2), a.getDouble(3))
    }

    val stylus = arrayOf(MotionEvent.PointerProperties().apply { id = 7; toolType = MotionEvent.TOOL_TYPE_STYLUS })
    fun contact(milliseconds: Int, intervalNs: Long = 5_000_000L, at: (Double) -> DoubleArray): JSONObject {
        val coords = arrayOf(MotionEvent.PointerCoords())
        val count = max(1, (milliseconds * 1_000_000L / intervalNs).toInt())
        val begun = System.nanoTime()
        val boot = SystemClock.elapsedRealtimeNanos()
        val down = SystemClock.uptimeMillis()
        val injected = JSONArray()
        for (i in 0..count) {
            val delay = begun + i * intervalNs - System.nanoTime()
            if (delay > 0) LockSupport.parkNanos(delay)
            val phase = if (i == 0) MotionEvent.ACTION_DOWN else if (i == count) MotionEvent.ACTION_UP else MotionEvent.ACTION_MOVE
            val p = at(i * intervalNs / 1e9)
            coords[0].x = p[0].toFloat() + host.surfaceOrigin.x
            coords[0].y = p[1].toFloat() + host.surfaceOrigin.y
            coords[0].pressure = if (phase == MotionEvent.ACTION_UP) 0f else 1f
            val event = MotionEvent.obtain(down, SystemClock.uptimeMillis(), phase, 1, stylus, coords,
                0, 0, 1f, 1f, 0, 0, InputDevice.SOURCE_STYLUS, 0)
            val sent = System.nanoTime()
            try { check(instrumentation.uiAutomation.injectInputEvent(event, false)) } finally { event.recycle() }
            injected.put(JSONArray(listOf(sent, phase, p[0], p[1])))
        }
        return obj("begin_ns" to begun, "begin_boot_ns" to boot, "end_ns" to System.nanoTime(),
            "end_boot_ns" to SystemClock.elapsedRealtimeNanos(), "injected" to injected)
    }
    fun tap(p: DoubleArray) = contact(10) { p }
    fun twoFinger(milliseconds: Int, motion: String, c: JSONObject): JSONObject {
        val (cx, cy, width, height) = workCenter(c).toList()
        val radius = min(width, height) * .32
        val interval = 4_166_667L
        val properties = Array(2) { i -> MotionEvent.PointerProperties().apply { id = i + 11; toolType = MotionEvent.TOOL_TYPE_FINGER } }
        val coords = Array(2) { MotionEvent.PointerCoords().apply { pressure = 1f; size = .1f } }
        val down = SystemClock.uptimeMillis()
        fun send(action: Int, count: Int, t: Double) {
            val phase = t / 2 * 2 * PI
            val x = if (motion == "pan") cx + radius * .8 * sin(phase) else cx
            val y = if (motion == "pan") cy + radius * .5 * sin(2 * phase) else cy
            val spread = if (motion == "pinch") 150 * exp(.4 * sin(phase)) else 150.0
            val angle = if (motion == "rotate_view") .4 * sin(phase) else 0.0
            for (i in 0..1) {
                val direction = if (i == 0) -1 else 1
                coords[i].x = (x + direction * spread * cos(angle)).toFloat() + host.surfaceOrigin.x
                coords[i].y = (y + direction * spread * sin(angle)).toFloat() + host.surfaceOrigin.y
            }
            val event = MotionEvent.obtain(down, SystemClock.uptimeMillis(), action, count, properties, coords,
                0, 0, 1f, 1f, 0, 0, InputDevice.SOURCE_TOUCHSCREEN, 0)
            try { check(instrumentation.uiAutomation.injectInputEvent(event, false)) } finally { event.recycle() }
        }
        val shift = MotionEvent.ACTION_POINTER_INDEX_SHIFT
        val begun = System.nanoTime()
        val boot = SystemClock.elapsedRealtimeNanos()
        send(MotionEvent.ACTION_DOWN, 1, 0.0)
        send(MotionEvent.ACTION_POINTER_DOWN or (1 shl shift), 2, 0.0)
        val count = (milliseconds * 1_000_000L / interval).toInt()
        for (i in 1..count) {
            val delay = begun + i * interval - System.nanoTime()
            if (delay > 0) LockSupport.parkNanos(delay)
            send(MotionEvent.ACTION_MOVE, 2, i * interval / 1e9)
        }
        val end = count * interval / 1e9
        send(MotionEvent.ACTION_POINTER_UP or (1 shl shift), 2, end)
        send(MotionEvent.ACTION_UP, 1, end)
        return obj("begin_ns" to begun, "begin_boot_ns" to boot, "end_ns" to System.nanoTime(),
            "end_boot_ns" to SystemClock.elapsedRealtimeNanos(), "motion" to motion)
    }

    fun sync(unit: String) {
        File(output, "$label-$unit-ready").writeText("ready")
        instrumentation.sendStatus(0, Bundle().apply { putString("stream", "VECTOR_READY $label $unit\n") })
        if (!waitForTrace) return
        val deadline = SystemClock.uptimeMillis() + 300_000
        while (!File(output, "$label-$unit-go").isFile) {
            check(SystemClock.uptimeMillis() < deadline) { "Trace did not start for $unit" }
            SystemClock.sleep(20)
        }
    }
    fun done(unit: String, info: JSONObject) {
        File(output, "$label-$unit-info.json").writeText(info.toString(2))
        File(output, "$label-$unit-done").writeText("done")
        instrumentation.sendStatus(0, Bundle().apply { putString("stream", "VECTOR_DONE $label $unit\n") })
    }
    fun measured(unit: String, name: String, gesture: () -> JSONObject): JSONObject {
        settle()
        val before = stats()
        val displayBefore = display()
        val contentBefore = content()
        measurements(true)
        native { Native.completionTimings(it, true) }
        val motion = gesture()
        val displayAfterInput = display()
        val settled = settle()
        SystemClock.sleep(400)
        val data = measurements(false)
        val completions = native { JSONArray(Native.completionTimings(it, false)) }
        check(completions.length() < 32768) { "Completion observation capacity exceeded" }
        data.put("unit", unit).put("name", name).put("motion", motion).put("completions", completions)
            .put("renderer_before", before).put("renderer_after", stats())
            .put("display_before", displayBefore).put("display_after_input", displayAfterInput).put("display_after_drain", display())
            .put("settled_ns", settled).put("content_before", contentBefore).put("content_after", content())
            .put("camera", state().getJSONObject("camera"))
        File(output, "$label-$unit-$name.json").writeText(data.toString())
        healthy()
        return data
    }
    fun drawn(stats: JSONObject) = stats.array("rows").objects().single { it.getString("label") == "Frames" }.getString("value").toLong()
    fun changed(data: JSONObject) = data.getString("content_after") != data.getString("content_before") ||
        drawn(data.getJSONObject("renderer_after")) > drawn(data.getJSONObject("renderer_before"))
    fun undo() { invoke("undo"); settle() }
    fun pathsSelected() = state().getJSONObject("layer_properties").array("controls").objects().any { it.optString("key") == "path_width" }
    fun marquee(rect: JSONArray) {
        val c = camera()
        val a = surface(c, rect.getDouble(0), rect.getDouble(1))
        val b = surface(c, rect.getDouble(2), rect.getDouble(3))
        check(inWorkArea(c, a) && inWorkArea(c, b)) { "Marquee ${a.toList()} to ${b.toList()} leaves the work area ${c.getJSONArray("work_area")}" }
        contact(400) { t -> val f = min(1.0, t / .3); doubleArrayOf(a[0] + (b[0] - a[0]) * f, a[1] + (b[1] - a[1]) * f) }
        settle()
        waitFor("the marquee to select paths", 20_000) { pathsSelected() }
    }
    fun clearance(selection: JSONObject, key: String, least: Double) {
        val surfaceClearance = selection.getDouble(key) * camera().getDouble("zoom")
        check(surfaceClearance >= least) { "$key clearance is only $surfaceClearance surface px" }
    }
    fun deselect() { if (pathsSelected()) invoke("deselect"); waitFor("paths to be deselected", 20_000) { !pathsSelected() } }
    fun awayFromFrame(selection: JSONObject) {
        val b = selection.getJSONArray("bounds")
        val p = selection.getJSONArray("press")
        val reach = 12 * density / camera().getDouble("zoom")
        val handles = listOf(0.0 to 0.0, .5 to 0.0, 1.0 to 0.0, 1.0 to .5, 1.0 to 1.0, .5 to 1.0, 0.0 to 1.0, 0.0 to .5).map { (u, v) ->
            doubleArrayOf(b.getDouble(0) + u * (b.getDouble(2) - b.getDouble(0)), b.getDouble(1) + v * (b.getDouble(3) - b.getDouble(1)))
        } + listOf(doubleArrayOf((b.getDouble(0) + b.getDouble(2)) / 2, b.getDouble(1) - reach * 2.5))
        val nearest = handles.minOf { hypot(it[0] - p.getDouble(0), it[1] - p.getDouble(1)) }
        check(nearest > reach * 1.3) { "The press lies ${nearest / reach} handle reaches from the Move frame" }
    }
    fun variant(size: String): JSONObject {
        val c = camera()
        val variants = plan.getJSONObject("selections").getJSONArray(size).objects()
        return variants.firstOrNull { !it.has("rect") || inWorkArea(c, surface(c, it.getJSONArray("rect").getDouble(0), it.getJSONArray("rect").getDouble(1))) }
            ?: error("No marquee for $size starts inside the work area ${c.getJSONArray("work_area")}")
    }
    fun select(size: String): JSONObject {
        val selection = variant(size)
        invoke("move")
        ready()
        deselect()
        if (selection.has("rect")) marquee(selection.getJSONArray("rect"))
        else {
            tap(surface(camera(), selection.getJSONArray("press")))
            settle()
            waitFor("the press to select a path", 20_000) { pathsSelected() }
        }
        return selection
    }
    fun drag(press: DoubleArray, radius: Double, milliseconds: Int) = contact(milliseconds) { t ->
        val angle = t * PI
        doubleArrayOf(press[0] + radius * (cos(angle) - 1), press[1] + radius * sin(angle))
    }
    fun runUnit(unit: String, prepare: () -> JSONObject, measure: (String) -> JSONObject, restore: (JSONObject) -> Unit) {
        val info = prepare()
        sync(unit)
        restore(measure("prime"))
        for (run in 0 until repeats) restore(measure("$run"))
        screenshot(unit)
        done(unit, info.put("camera", state().getJSONObject("camera")).put("layers", state().getJSONArray("layers")))
    }
    val primeMs = 1500
    fun length(name: String) = if (name == "prime") primeMs else duration

    for (scenario in scenarios) when (scenario) {
        "move" -> for (size in sizes) runUnit("move-$size", {
            selectLayer("Ink"); JSONObject(select(size).toString()).also {
                clearance(it, "press_clearance", if (size == "1") 6.5 else 8.0)
                if (size != "1") awayFromFrame(it)
            }
        }, { name ->
            if (size == "1") deselect() else select(size)
            val press = surface(camera(), variant(size).getJSONArray("press"))
            measured("move-$size", name) { drag(press, 150.0, length(name)) }
        }) { data -> check(changed(data)) { "The drag did not move paths" }; undo() }
        "scale", "rotate" -> for (size in sizes.filter { it != "1" }) runUnit("$scenario-$size", {
            selectLayer("Ink"); JSONObject(select(size).toString())
        }, { name ->
            val selection = select(size)
            val c = camera()
            val b = selection.getJSONArray("bounds")
            val zoom = c.getDouble("zoom")
            val reach = 12 * density / zoom
            if (scenario == "scale") {
                val corner = surface(c, b.getDouble(2), b.getDouble(3))
                measured("$scenario-$size", name) { contact(length(name)) { t ->
                    val s = 90 * sin(t * PI)
                    doubleArrayOf(corner[0] + s, corner[1] + s)
                } }
            } else {
                val pivot = surface(c, (b.getDouble(0) + b.getDouble(2)) / 2, (b.getDouble(1) + b.getDouble(3)) / 2)
                val handle = surface(c, (b.getDouble(0) + b.getDouble(2)) / 2, b.getDouble(1) - reach * 2.5)
                val r = hypot(handle[0] - pivot[0], handle[1] - pivot[1])
                val start = atan2(handle[1] - pivot[1], handle[0] - pivot[0])
                measured("$scenario-$size", name) { contact(length(name)) { t ->
                    val angle = start + .6 * sin(t * PI)
                    doubleArrayOf(pivot[0] + r * cos(angle), pivot[1] + r * sin(angle))
                } }
            }
        }) { data -> check(changed(data)) { "The frame drag did not transform paths" }; undo() }
        "points" -> for (size in sizes.filter { it != "5000" }) runUnit("points-$size", {
            selectLayer("Ink"); JSONObject(select(size).toString()).also { clearance(it, "press_clearance", if (size == "1") 6.5 else 8.0); invoke("edit_points"); ready() }
        }, { name ->
            val anchor = surface(camera(), variant(size).getJSONArray("anchor"))
            measured("points-$size", name) { drag(anchor, 120.0, length(name)) }
        }) { data -> check(changed(data)) { "The anchor drag did not edit the path" }; undo() }
        "erase" -> for (mode in listOf("erase_touched_part", "erase_whole_paths", "erase_to_intersection")) runUnit(mode.replace('_', '-'), {
            selectLayer("Ink")
            invoke("eraser")
            action(obj("type" to "set_brush_size", "value" to eraserSize))
            invoke(mode)
            ready()
            check(state().array("commands").objects().single { it.getString("id") == mode }.getBoolean("selected")) { "$mode is not selected" }
            obj("mode" to mode, "eraser_size" to state().getJSONObject("brush").getDouble("diameter"))
        }, { name ->
            val (cx, cy, width, height) = workCenter(camera()).toList()
            val rx = min(width, height) * .32
            measured(mode.replace('_', '-'), name) { contact(length(name)) { t -> doubleArrayOf(cx + rx * sin(t * 3.2), cy + rx * .65 * sin(t * 4.7)) } }
        }) { data -> check(changed(data)) { "The eraser removed nothing" }; undo() }
        "width" -> for (size in sizes.filter { it != "1" }) runUnit("width-$size", {
            selectLayer("Ink"); JSONObject(select(size).toString())
        }, { name ->
            select(size)
            val layer = state().getJSONObject("layer_properties").getLong("layer")
            val request = obj("op" to "number", "layer" to layer, "key" to "path_width")
            fun valued(v: Double) = JSONObject(request.toString()).put("operation", obj("type" to "value", "value" to v))
            measured("width-$size", name) {
                val begun = System.nanoTime()
                val boot = SystemClock.elapsedRealtimeNanos()
                val sent = JSONArray()
                instrumentation.runOnMainSync { host.dispatch(obj("type" to "effect", "action" to obj("op" to "gesture", "phase" to "down", "action" to valued(4.0)))) }
                val count = length(name) / 5
                var value = 4.0
                for (i in 1..count) {
                    val delay = begun + i * 5_000_000L - System.nanoTime()
                    if (delay > 0) LockSupport.parkNanos(delay)
                    value = 4 + 3 * sin(i * .005 * PI)
                    val move = obj("type" to "effect", "action" to obj("op" to "gesture", "phase" to "move", "action" to valued(value)))
                    instrumentation.runOnMainSync { host.dispatch(move, moving = true) }
                    sent.put(JSONArray(listOf(System.nanoTime(), value)))
                }
                instrumentation.runOnMainSync { host.dispatch(obj("type" to "effect", "action" to obj("op" to "gesture", "phase" to "up", "action" to valued(value)))) }
                obj("begin_ns" to begun, "begin_boot_ns" to boot, "end_ns" to System.nanoTime(), "end_boot_ns" to SystemClock.elapsedRealtimeNanos(), "values" to sent)
            }
        }) { data -> check(changed(data)) { "The scrub did not restyle paths" }; undo() }
        "pan", "pinch", "rotate_view" -> runUnit(scenario.replace('_', '-'), {
            selectLayer("Ink"); deselect(); invoke("pen"); ready(); obj("motion" to scenario, "paths_selected" to pathsSelected())
        }, { name -> measured(scenario.replace('_', '-'), name) { twoFinger(length(name), scenario, camera()) } }) { data ->
            check(data.getString("content_after") == data.getString("content_before")) { "Navigation changed the drawing" }
            invoke("reset_rotation"); invoke("fit_canvas"); settle()
        }
        "commit" -> runUnit("commit", {
            selectLayer("Ink")
            invoke("pen")
            action(obj("type" to "select_brush", "id" to 1))
            action(obj("type" to "set_brush_size", "value" to inkSize))
            ready()
            obj("ink_size" to state().getJSONObject("brush").getDouble("diameter"))
        }, { name ->
            val (cx, cy, width, height) = workCenter(camera()).toList()
            val rx = min(width, height) * .32
            val stroke = measured("commit", "$name-stroke") { contact(3000) { t -> doubleArrayOf(cx + rx * sin(t * 3.2), cy + rx * .65 * sin(t * 4.7)) } }
            check(changed(stroke)) { "The stroke drew no path" }
            measured("commit", "$name-undo") { val begun = System.nanoTime(); val boot = SystemClock.elapsedRealtimeNanos()
                invoke("undo"); obj("begin_ns" to begun, "begin_boot_ns" to boot, "end_ns" to System.nanoTime(), "end_boot_ns" to SystemClock.elapsedRealtimeNanos()) }
        }) { }
        "fill" -> runUnit("fill", {
            selectLayer("Ink")
            action(obj("type" to "set_color", "rgba" to JSONArray(listOf(.85, .35, .1, 1.0))))
            action(obj("type" to "layer", "action" to obj("op" to "tool", "tool" to obj("figure" to obj("shape" to "ellipse", "paint" to "outline")))))
            action(obj("type" to "set_brush_size", "value" to 8))
            ready()
            val e = plan.getJSONArray("ellipse")
            val c = camera()
            val a = surface(c, e.getDouble(0), e.getDouble(1))
            val b = surface(c, e.getDouble(2), e.getDouble(3))
            val unfigured = content()
            contact(600) { t -> val f = min(1.0, t / .5); doubleArrayOf(a[0] + (b[0] - a[0]) * f, a[1] + (b[1] - a[1]) * f) }
            settle()
            check(content() != unfigured) { "The ellipse figure added no path" }
            action(obj("type" to "layer", "action" to obj("op" to "tool", "tool" to obj("region" to obj("fill" to true, "source" to "visible")))))
            ready()
            obj("ellipse" to e)
        }, { name ->
            val c = camera()
            if (fillBelow) {
                measured("fill", "$name-below") { tap(surface(c, plan.getJSONArray("fill_below"))) }
                waitFor("Fill outside the paths to add a paint layer below", 20_000) { state().array("layers").length() == 5 }
                settle()
                undo(); undo()
                waitFor("two undo steps to remove the filled layer", 20_000) { state().array("layers").length() == 4 }
                selectLayer("Ink")
            }
            val inside = measured("fill", "$name-inside") { tap(surface(c, plan.getJSONArray("fill_inside"))) }
            check(changed(inside)) { "Fill inside the path changed nothing" }
            check(state().array("layers").length() == 4) { "Fill inside a closed path added a layer" }
            undo()
            inside
        }) { }
        "merge" -> {
            val commands = listOf("merge_down_vector" to "Tones", "merge_down_paint" to "Ink", "rasterize" to "Ink")
            for ((unit, layer) in commands) runUnit(unit.replace('_', '-'), { obj("layer" to layer) }, { name ->
                selectLayer(layer)
                val layers = state().array("layers").length()
                val data = measured(unit.replace('_', '-'), name) {
                    val begun = System.nanoTime(); val boot = SystemClock.elapsedRealtimeNanos()
                    when (unit) {
                        "rasterize" -> invoke("rasterize_layer")
                        else -> invoke("merge_down")
                    }
                    val notice = state().optJSONObject("notice")
                    val accepted = if (notice != null && notice.array("actions").objects().any { it.getString("id") == "merge" }) {
                        val answered = System.nanoTime()
                        action(obj("type" to "notice", "id" to notice.getLong("id"), "accept" to true, "action" to "merge"))
                        answered
                    } else null
                    obj("begin_ns" to begun, "begin_boot_ns" to boot, "notice_answered_ns" to accepted, "end_ns" to System.nanoTime(), "end_boot_ns" to SystemClock.elapsedRealtimeNanos())
                }
                val after = state().array("layers")
                check(if (unit == "rasterize") after.length() == layers else after.length() == layers - 1) { "$unit left ${after.length()} layers" }
                data.put("layers_after", after)
                undo()
                waitFor("undo to restore $layers layers", 20_000) { state().array("layers").length() == layers }
                data
            }) { }
            selectLayer("Ink")
        }
        "open" -> runUnit("open", { obj("fixture" to fixture.name, "bytes" to fixture.length()) }, { name ->
            measured("open", name) {
                val begun = System.nanoTime(); val boot = SystemClock.elapsedRealtimeNanos()
                val task = native { h ->
                    Native.dispatch(h, obj("type" to "invoke", "command" to "open_document").toString())
                    val s = JSONObject(Native.snapshot(h)!!).getJSONObject("state")
                    val request = s.array("requests").objects().first { it.getJSONObject("kind").optString("type") == "document" }
                    val f = s.getJSONObject("document_file")
                    Native.projectTask(h, request.getInt("id"), "null", f.getLong("epoch"), f.getLong("revision"))
                }
                val parsed: Long
                val adopted: Long
                try {
                    Native.projectWork(task, ParcelFileDescriptor.open(fixture, ParcelFileDescriptor.MODE_READ_ONLY).detachFd(), "")
                    parsed = System.nanoTime()
                    native { Native.projectAdopt(it, task, "null") }
                    adopted = System.nanoTime()
                } finally { Native.projectFree(task) }
                instrumentation.runOnMainSync { host.documentChanged() }
                obj("begin_ns" to begun, "begin_boot_ns" to boot, "parsed_ns" to parsed, "adopted_ns" to adopted,
                    "end_ns" to System.nanoTime(), "end_boot_ns" to SystemClock.elapsedRealtimeNanos())
            }
        }) { }
        else -> error("Unknown vector scenario $scenario")
    }
}
