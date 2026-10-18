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

/** G-Pen lines on a vector layer, dragged with Move and restored by Undo, with stylus contacts on the device. */
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
    private fun canvasEvent(phase: Int, point: Offset) {
        fun find(view: android.view.View): CanvasSurfaceView? {
            if (view is CanvasSurfaceView) return view
            if (view is android.view.ViewGroup) for (i in 0 until view.childCount) find(view.getChildAt(i))?.let { return it }
            return null
        }
        instrumentation.runOnMainSync {
            val canvas = find(compose.activity.window.decorView)!!
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
        val bitmap = instrumentation.uiAutomation.takeScreenshot()!!
        val readable = bitmap.copy(Bitmap.Config.ARGB_8888, false)
        val color = readable.getPixel((point.x * readable.width).toInt(), (point.y * readable.height).toInt())
        readable.recycle(); bitmap.recycle(); return color
    }
    private fun green(color: Int) = (color shr 8 and 255) - maxOf(color shr 16 and 255, color and 255) > 25
    private fun awaitPixel(name: String, point: Offset, inked: Boolean) {
        try { compose.waitUntil(20_000) { green(pixel(point)) == inked || host.failure != null || host.actionError != null } }
        catch (e: Throwable) { screenshot("vector-$name-failure.png"); throw e }
        assertNull(host.failure); assertNull(host.actionError)
        assertEquals(name, inked, green(pixel(point)))
    }
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
}
