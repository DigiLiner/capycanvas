package art.capycanvas

import android.graphics.Bitmap
import android.view.WindowManager
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.test.junit4.createAndroidComposeRule
import org.json.JSONObject
import org.junit.*
import org.junit.Assert.*
import java.util.concurrent.CountDownLatch
import java.util.concurrent.TimeUnit

/** Vector layer journeys with stylus contacts on the device: G-Pen lines, Figure shapes, Path, Edit Points, Properties, the Move frame and the eraser. */
class AndroidVectorLayerTest {
    @get:Rule(order = 0) val device = CapyDeviceRule()
    @get:Rule(order = 1) val compose = createAndroidComposeRule<MainActivity>()
    private val host get() = compose.activity.host
    @Before fun ready() {
        wakeDevice()
        compose.activity.runOnUiThread { compose.activity.window.addFlags(WindowManager.LayoutParams.FLAG_KEEP_SCREEN_ON) }
        host.awaitReady(compose = compose)
    }
    private fun action(value: JSONObject) {
        val done = CountDownLatch(1)
        compose.runOnIdle { host.dispatch(value); host.query(obj("type" to "catalog")) { done.countDown() } }
        assertTrue(done.await(15, TimeUnit.SECONDS))
        compose.waitForIdle()
        assertNull(host.actionError)
        assertNull(host.failure)
    }
    private fun canvasView(): CanvasSurfaceView {
        fun find(view: android.view.View): CanvasSurfaceView? {
            if (view is CanvasSurfaceView) return view
            if (view is android.view.ViewGroup) for (i in 0 until view.childCount) find(view.getChildAt(i))?.let { return it }
            return null
        }
        return find(compose.activity.window.decorView)!!
    }
    private fun canvasEvent(phase: Int, point: Offset) {
        instrumentation.runOnMainSync {
            val canvas = canvasView()
            val properties = android.view.MotionEvent.PointerProperties().apply { id = 0; toolType = android.view.MotionEvent.TOOL_TYPE_STYLUS }
            val coords = android.view.MotionEvent.PointerCoords().apply { x = point.x * canvas.width; y = point.y * canvas.height; pressure = .8f }
            val now = android.os.SystemClock.uptimeMillis()
            val event = android.view.MotionEvent.obtain(now - 30, now, phase, 1, arrayOf(properties), arrayOf(coords), 0, 0, 1f, 1f, 1, 0, android.view.InputDevice.SOURCE_STYLUS, 0)
            assertTrue(canvas.dispatchTouchEvent(event)); event.recycle()
        }
        android.os.SystemClock.sleep(8)
    }
    private fun drag(from: Offset, to: Offset) {
        canvasEvent(android.view.MotionEvent.ACTION_DOWN, from)
        for (i in 1..24) canvasEvent(android.view.MotionEvent.ACTION_MOVE, from + (to - from) * (i / 24f))
        canvasEvent(android.view.MotionEvent.ACTION_UP, to)
    }
    private fun pixel(point: Offset): Int {
        var origin = IntArray(2); var size = IntArray(2)
        instrumentation.runOnMainSync { val canvas = canvasView(); canvas.getLocationOnScreen(origin); size = intArrayOf(canvas.width, canvas.height) }
        val bitmap = instrumentation.uiAutomation.takeScreenshot()!!
        val readable = bitmap.copy(Bitmap.Config.ARGB_8888, false)
        val color = readable.getPixel(origin[0] + (point.x * size[0]).toInt(), origin[1] + (point.y * size[1]).toInt())
        readable.recycle(); bitmap.recycle(); return color
    }
    private fun green(color: Int) = (color shr 8 and 255) - maxOf(color shr 16 and 255, color and 255) > 25
    private fun orange(color: Int) = (color shr 16 and 255) - (color and 255) > 90
    private fun awaitPixel(name: String, point: Offset, inked: Boolean, ink: (Int) -> Boolean = ::green) {
        try { compose.waitUntil(20_000) { ink(pixel(point)) == inked || host.failure != null || host.actionError != null } }
        catch (e: Throwable) { screenshot("vector-$name-failure.png"); throw e }
        assertNull(host.failure); assertNull(host.actionError)
        assertEquals(name, inked, ink(pixel(point)))
    }
    private fun tap(point: Offset) {
        canvasEvent(android.view.MotionEvent.ACTION_DOWN, point)
        canvasEvent(android.view.MotionEvent.ACTION_UP, point)
    }
    private fun tool(tool: JSONObject) = action(obj("type" to "layer", "action" to obj("op" to "tool", "tool" to tool)))
    private fun figure(shape: String, paint: String) = tool(obj("figure" to obj("shape" to shape, "paint" to paint)))
    private fun selectedLayer() = host.snapshot!!.getJSONObject("state").getJSONArray("layers").objects().first { it.getBoolean("selected") }.getLong("id")
    @Test fun gPenLinesDrawMoveAndUndoOnVectorLayers() {
        for (theme in listOf("light", "dark")) {
            action(obj("type" to "set_theme", "theme" to theme))
            action(obj("type" to "layer", "action" to obj("op" to "new_vector")))
            action(obj("type" to "select_brush", "id" to 1))
            action(obj("type" to "set_brush_size", "value" to 24))
            val y = .5f
            val left = Offset(.38f, y)
            val press = Offset(.48f, y)
            val probe = Offset(.43f, y)
            val below = Offset(.43f, y + .1f)
            drag(left, Offset(.6f, y))
            awaitPixel("$theme-drawn", probe, true)
            awaitPixel("$theme-empty-below", below, false)
            screenshot("vector-$theme-drawn.png")
            action(obj("type" to "invoke", "command" to "move"))
            drag(press, press + (below - probe))
            awaitPixel("$theme-moved-away", probe, false)
            awaitPixel("$theme-moved-to", below, true)
            screenshot("vector-$theme-moved.png")
            action(obj("type" to "invoke", "command" to "undo"))
            awaitPixel("$theme-undone", probe, true)
            awaitPixel("$theme-undone-below", below, false)
            action(obj("type" to "invoke", "command" to "undo"))
            action(obj("type" to "invoke", "command" to "pen"))
        }
    }
    @Test fun vectorToolsDrawEditRestyleScaleAndErase() {
        for (theme in listOf("light", "dark")) {
            action(obj("type" to "set_theme", "theme" to theme))
            action(obj("type" to "layer", "action" to obj("op" to "new_vector")))
            action(obj("type" to "invoke", "command" to "fit_canvas"))
            action(obj("type" to "set_color", "rgba" to org.json.JSONArray(listOf(.85, .35, .1, 1.0))))
            action(obj("type" to "set_brush_size", "value" to 24))
            figure("rectangle", "fill")
            drag(Offset(.34f, .34f), Offset(.44f, .46f))
            awaitPixel("$theme-rectangle", Offset(.39f, .4f), true, ::orange)
            awaitPixel("$theme-beside-rectangle", Offset(.49f, .4f), false, ::orange)
            screenshot("vector-$theme-figure.png")

            figure("path", "outline")
            for (at in listOf(Offset(.55f, .34f), Offset(.65f, .34f))) tap(at)
            drag(Offset(.65f, .46f), Offset(.7f, .5f))
            screenshot("vector-$theme-path-drafting.png")
            instrumentation.sendKeyDownUpSync(android.view.KeyEvent.KEYCODE_ENTER)
            awaitPixel("$theme-path", Offset(.6f, .34f), true, ::orange)
            screenshot("vector-$theme-path.png")

            action(obj("type" to "invoke", "command" to "edit_points"))
            awaitPixel("$theme-before-edit", Offset(.6f, .41f), false, ::orange)
            drag(Offset(.55f, .34f), Offset(.55f, .48f))
            awaitPixel("$theme-edited", Offset(.6f, .41f), true, ::orange)
            screenshot("vector-$theme-edit-points.png")

            val wide = Offset(.635f, .4f)
            awaitPixel("$theme-before-restyle", wide, false, ::orange)
            action(obj("type" to "effect", "action" to obj("op" to "set", "layer" to selectedLayer(), "key" to "path_width",
                "value" to obj("kind" to "number", "value" to 160))))
            awaitPixel("$theme-restyled", wide, true, ::orange)
            screenshot("vector-$theme-properties.png")

            action(obj("type" to "invoke", "command" to "move"))
            drag(Offset(.33f, .31f), Offset(.47f, .5f))
            screenshot("vector-$theme-frame.png")
            awaitPixel("$theme-before-scale", Offset(.47f, .5f), false, ::orange)
            drag(Offset(.44f, .46f), Offset(.5f, .54f))
            awaitPixel("$theme-scaled", Offset(.47f, .5f), true, ::orange)
            screenshot("vector-$theme-scaled.png")

            action(obj("type" to "select_brush", "id" to 1))
            action(obj("type" to "set_color", "rgba" to org.json.JSONArray(listOf(.1, .7, .2, 1.0))))
            drag(Offset(.34f, .68f), Offset(.7f, .68f))
            awaitPixel("$theme-inked", Offset(.5f, .68f), true)
            action(obj("type" to "invoke", "command" to "eraser"))
            action(obj("type" to "set_brush_size", "value" to 40))
            drag(Offset(.5f, .62f), Offset(.5f, .74f))
            awaitPixel("$theme-erased", Offset(.5f, .68f), false)
            awaitPixel("$theme-kept", Offset(.4f, .68f), true)
            screenshot("vector-$theme-erased.png")
            action(obj("type" to "layer", "action" to obj("op" to "delete_selected")))
            action(obj("type" to "invoke", "command" to "pen"))
        }
    }
}
