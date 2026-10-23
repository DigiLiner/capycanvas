package art.capycanvas

import android.os.Handler
import android.os.HandlerThread
import android.os.PowerManager
import android.os.SystemClock
import android.os.Trace
import android.view.Choreographer
import android.view.FrameMetrics
import android.view.MotionEvent
import android.view.ViewTreeObserver
import android.view.Window
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.platform.ViewRootForTest
import androidx.compose.ui.semantics.SemanticsNode
import androidx.test.platform.app.InstrumentationRegistry
import org.json.JSONArray
import org.json.JSONObject
import org.junit.Assert.*
import org.junit.Assume.assumeTrue
import org.junit.Rule
import org.junit.Test
import java.io.File
import java.util.Collections
import java.util.concurrent.CountDownLatch
import java.util.concurrent.TimeUnit
import java.util.concurrent.atomic.AtomicBoolean

class AndroidBubbleBenchmarkTest {
    @get:Rule val device = CapyDeviceRule(nativeFileJobs = true)

    @Test fun fiveBubbleMotion() {
        val args = InstrumentationRegistry.getArguments()
        assumeTrue(args.getString("bubbleBenchmark") == "true")
        assertFalse("Bubble qualification uses the optimized benchmark build", BuildConfig.DEBUG)
        val duration = args.getString("durationMs", "6000")!!.toLong()
        val repeats = args.getString("repeats", "3")!!.toInt()
        val expectedRefresh = args.getString("expectedRefreshHz", "0")!!.toFloat()
        require(duration in 5000L..10000L && repeats >= 3)
        val width = args.getString("width", "6000")!!.toInt()
        val height = args.getString("height", "4000")!!.toInt()
        val photo = File(requireNotNull(args.getString("photo")))
        val size = android.graphics.BitmapFactory.Options().apply { inJustDecodeBounds = true }
        android.graphics.BitmapFactory.decodeFile(photo.path, size)
        assertEquals(width, size.outWidth); assertEquals(height, size.outHeight)
        launchCapy(180_000).use { scenario ->
            device.landscape(scenario)
            val activity = scenario.activity()
            val host = activity.host
            fun action(value: JSONObject) = host.drain(value)
            fun invoke(command: String) = action(obj("type" to "invoke", "command" to command))
            fun state() = host.snapshot!!.getJSONObject("state")
            fun query(value: JSONObject): Any? {
                val ready = CountDownLatch(1)
                var answer: Any? = null
                instrumentation.runOnMainSync { host.query(value) { answer = it; ready.countDown() } }
                assertTrue("$value completes", ready.await(20, TimeUnit.SECONDS))
                return answer
            }
            fun packets() = (query(obj("type" to "bubbles")) as JSONArray).objects()
            host.openDocument(photo)
            host.awaitMain("Tier photo opens", 180_000) {
                state().array("tabs").objects().any { it.optBoolean("active") && it.optInt("width") == width && it.optInt("height") == height }
            }
            val photoLayer = state().array("layers").objects().single { it.optBoolean("editing") }.getLong("id")
            state().array("layers").objects().filter { it.getLong("id") != photoLayer }.forEach {
                action(obj("type" to "set_layer_visibility", "id" to it.getLong("id"), "visible" to false))
            }
            invoke("add_layer"); invoke("fit_canvas")
            host.awaitMain("All photo pipelines warm", 180_000) { host.snapshot?.optBoolean("shaders_ready") == true }
            val saved = JSONObject(state().getJSONObject("workspace").toString())
            val fixture = JSONObject(saved.toString()).apply {
                getJSONObject("layout").put("bubbles", obj("bubbles" to JSONArray(), "rings" to JSONArray(), "kept" to JSONArray()))
            }
            val controls = listOf(obj("kind" to "command", "command" to "drawing_brush"), obj("kind" to "color"),
                obj("kind" to "color_picker"), obj("kind" to "command", "command" to "undo"), obj("kind" to "panel", "panel" to "layers"))
            fixture.getJSONObject("layout").put("header", obj("size" to "medium", "next_id" to 7,
                "zones" to JSONArray(listOf(JSONArray(listOf(obj("id" to 1, "item" to obj("kind" to "capy")))),
                    JSONArray(controls.mapIndexed { i, control -> obj("id" to i + 2, "item" to obj("kind" to "tool", "control" to control)) }), JSONArray()))))
            val output = File(instrumentation.targetContext.getExternalFilesDir(null), "bubble-benchmark").apply { mkdirs() }
            val contacts = JSONArray()
            val frameThread = HandlerThread("bubble-frame-metrics").apply { start() }
            val measuring = AtomicBoolean(false)
            val drawn = Collections.synchronizedList(mutableListOf<LongArray>())
            val frameRows = Collections.synchronizedList(mutableListOf<LongArray>())
            val frameListener = Window.OnFrameMetricsAvailableListener { _, metric, lost ->
                if (measuring.get()) frameRows.add(longArrayOf(metric.getMetric(FrameMetrics.VSYNC_TIMESTAMP),
                    metric.getMetric(FrameMetrics.TOTAL_DURATION), metric.getMetric(FrameMetrics.DEADLINE), lost.toLong(),
                    metric.getMetric(FrameMetrics.INPUT_HANDLING_DURATION), metric.getMetric(FrameMetrics.ANIMATION_DURATION),
                    metric.getMetric(FrameMetrics.LAYOUT_MEASURE_DURATION), metric.getMetric(FrameMetrics.DRAW_DURATION),
                    metric.getMetric(FrameMetrics.SYNC_DURATION), metric.getMetric(FrameMetrics.COMMAND_ISSUE_DURATION),
                    metric.getMetric(FrameMetrics.SWAP_BUFFERS_DURATION), metric.getMetric(FrameMetrics.GPU_DURATION)))
            }
            var owner: ViewRootForTest? = null
            var nodes = emptyList<SemanticsNode>()
            val drawListener = ViewTreeObserver.OnDrawListener {
                if (measuring.get()) drawn.add(longArrayOf(System.nanoTime(), *nodes.flatMap {
                    val center = it.boundsInRoot.center
                    listOf((center.x * 1000).toLong(), (center.y * 1000).toLong())
                }.toLongArray()))
            }
            try {
                action(obj("type" to "restore_workspace", "workspace" to fixture))
                if (packets().isNotEmpty()) action(obj("type" to "bubble", "action" to obj("type" to "put_back_all")))
                lateinit var workspace: androidx.compose.ui.geometry.Rect
                var density = 1f
                instrumentation.runOnMainSync {
                    val root = checkNotNull(findTag("workspace")); owner = root.first; workspace = root.second.boundsInRoot
                    density = root.first.view.resources.displayMetrics.density
                    root.first.view.viewTreeObserver.addOnDrawListener(drawListener)
                    activity.window.addOnFrameMetricsAvailableListener(frameListener, Handler(frameThread.looper))
                }
                val ids = mutableListOf<Int>()
                fun place(grab: JSONObject, index: Int) {
                    val position = JSONArray(listOf(workspace.width / density * (.22f + .56f * index / 4), workspace.height / density * .55f))
                    val press = if (grab.getString("kind") == "bubble") {
                        (query(obj("type" to "bubble_motion", "now_ns" to System.nanoTime())) as JSONObject)
                            .array("placements").objects().single { it.getInt("id") == grab.getInt("id") }.getJSONArray("center")
                    } else position
                    for ((phase, point) in listOf("down" to press, "move" to position, "up" to position)) action(obj("type" to "bubble", "action" to obj("type" to "drag",
                        "grab" to grab, "phase" to phase, "position" to point,
                        "viewport" to JSONArray(listOf(workspace.width / density, workspace.height / density)))))
                }
                for (i in controls.indices) {
                    val before = packets().map { it.getJSONObject("view").getInt("id") }.toSet()
                    place(obj("kind" to "header", "id" to i + 2), i)
                    ids.add(packets().single { it.getJSONObject("view").getInt("id") !in before }.getJSONObject("view").getInt("id"))
                }
                SystemClock.sleep(1000)
                output.resolve("fixture.json").writeText(obj("width" to width, "height" to height, "photo" to photo.path,
                    "bubbles" to JSONArray(packets()), "workspace" to state().getJSONObject("workspace"), "transparency" to state().getJSONObject("settings").getString("transparency")).toString())
                if (args.getString("waitForGo") == "true") {
                    val ready = output.resolve("ready")
                    val go = output.resolve("go")
                    go.delete(); ready.writeText("ready")
                    val deadline = SystemClock.uptimeMillis() + 180_000
                    while (!go.exists()) {
                        check(SystemClock.uptimeMillis() < deadline) { "Timed out waiting for benchmark refresh-policy setup" }
                        SystemClock.sleep(50)
                    }
                    scenario.onActivity { assertSame("Policy setup retains the private Activity", activity, it) }
                    host.awaitMain("Canvas ready after refresh-policy setup", 180_000) {
                        host.snapshot?.optBoolean("canvas_ready") == true && host.snapshot?.optBoolean("shaders_ready") == true
                    }
                }
                for (run in 0..repeats) {
                    val resetBegin = System.nanoTime()
                    ids.forEachIndexed { index, id -> place(obj("kind" to "bubble", "id" to id), index) }
                    val resetEnd = System.nanoTime()
                    SystemClock.sleep(600)
                    lateinit var points: List<Offset>
                    var downAt = 0L
                    var refresh = 0f
                    instrumentation.runOnMainSync {
                        nodes = ids.map { id -> checkNotNull(findTag("tool-bubble-$id") ?: findTag("bubble-card-$id")).second }
                        points = nodes.map { it.boundsInRoot.center }.let { it + it.first() }
                        refresh = owner!!.view.display.refreshRate
                    }
                    var lastCenters = points.dropLast(1)
                    var stableAt = SystemClock.uptimeMillis()
                    host.awaitMain("Five bubble placements settle") {
                        val next = nodes.map { it.boundsInRoot.center }
                        if (next.zip(lastCenters).any { (a, b) -> (a - b).getDistance() > .1f }) stableAt = SystemClock.uptimeMillis()
                        lastCenters = next
                        SystemClock.uptimeMillis() - stableAt >= 200
                    }
                    points = lastCenters + lastCenters.first()
                    val power = instrumentation.targetContext.getSystemService(PowerManager::class.java)
                    val thermalBefore = power.currentThermalStatus
                    val inputRows = mutableListOf<LongArray>()
                    fun event(action: Int, point: Offset) {
                        val event = motion(MotionEvent.TOOL_TYPE_FINGER, action, point, downAt)
                        try { owner!!.view.dispatchTouchEvent(event) } finally { event.recycle() }
                    }
                    instrumentation.runOnMainSync {
                        downAt = SystemClock.uptimeMillis(); event(MotionEvent.ACTION_DOWN, points.first())
                        event(MotionEvent.ACTION_MOVE, points.first() + Offset(12 * density, 0f))
                    }
                    if (expectedRefresh > 0) host.awaitMain("Reference display reaches $expectedRefresh Hz during native drag", 10_000) {
                        owner!!.view.display.refreshRate >= expectedRefresh * .95f
                    }
                    instrumentation.runOnMainSync { refresh = owner!!.view.display.refreshRate }
                    drawn.clear(); frameRows.clear(); host.measurementReport(true)
                    val done = CountDownLatch(1)
                    var begin = 0L
                    var end = 0L
                    lateinit var callback: Choreographer.FrameCallback
                    val name = "bubble-motion-$run"
                    instrumentation.runOnMainSync {
                        val clock = Choreographer.getInstance()
                        begin = System.nanoTime(); measuring.set(true); Trace.beginAsyncSection(name, run)
                        callback = Choreographer.FrameCallback {
                            val now = System.nanoTime()
                            val elapsed = (now - begin) / 1_000_000.0
                            if (elapsed >= duration) { end = now; measuring.set(false); done.countDown() }
                            else {
                                val progress = elapsed / duration * (points.size - 1)
                                val segment = progress.toInt().coerceAtMost(points.size - 2)
                                val point = points[segment] + (points[segment + 1] - points[segment]) * (progress - segment).toFloat()
                                event(MotionEvent.ACTION_MOVE, point)
                                inputRows.add(longArrayOf(now, (point.x * 1000).toLong(), (point.y * 1000).toLong()))
                                clock.postFrameCallback(callback)
                            }
                        }
                        clock.postFrameCallback(callback)
                    }
                    try { assertTrue("Native motion producer completes", done.await(duration / 1000 + 20, TimeUnit.SECONDS)) }
                    finally {
                        instrumentation.runOnMainSync {
                            Choreographer.getInstance().removeFrameCallback(callback); measuring.set(false)
                            Trace.endAsyncSection(name, run); event(MotionEvent.ACTION_UP, points.first())
                        }
                    }
                    val positions = synchronized(drawn) { drawn.toList() }
                    val distinct = ids.indices.map { index -> positions.map { it[index * 2 + 1] to it[index * 2 + 2] }.toSet().size }
                    contacts.put(obj("run" to run, "warmup" to (run == 0), "reset_begin_ns" to resetBegin, "reset_end_ns" to resetEnd,
                        "path_px" to JSONArray(points.map { JSONArray(listOf(it.x, it.y)) }), "begin_ns" to begin, "end_ns" to end,
                        "clock" to "MONOTONIC", "display_hz" to refresh, "thermal_before" to thermalBefore,
                        "thermal_after" to power.currentThermalStatus, "inputs" to JSONArray(inputRows.map { JSONArray(it.toList()) }),
                        "drawn_centers" to JSONArray(positions.map { JSONArray(it.toList()) }), "distinct_centers" to JSONArray(distinct),
                        "frame_metrics_proxy" to JSONArray(synchronized(frameRows) { frameRows.map { JSONArray(it.toList()) } }),
                        "publication_metrics" to host.measurementReport()))
                    output.resolve("result.json").writeText(obj("debuggable" to BuildConfig.DEBUG, "duration_ms" to duration,
                        "width" to width, "height" to height,
                        "frame_metrics_fields" to JSONArray(listOf("vsync_ns", "total_ns", "deadline_ns", "dropped_reports",
                            "input_ns", "animation_ns", "layout_measure_ns", "draw_ns", "sync_ns", "command_issue_ns", "swap_buffers_ns", "gpu_ns")),
                        "contacts" to contacts).toString())
                    assertTrue("Dragged bubble visibly moves: $distinct", distinct.first() > 10)
                    assertTrue("All four neighbors visibly move: $distinct", distinct.drop(1).all { it > 1 })
                }
            } finally {
                instrumentation.runOnMainSync {
                    measuring.set(false)
                    owner?.view?.viewTreeObserver?.removeOnDrawListener(drawListener)
                    activity.window.removeOnFrameMetricsAvailableListener(frameListener)
                }
                frameThread.quitSafely()
                action(obj("type" to "restore_workspace", "workspace" to saved))
            }
        }
    }
}
