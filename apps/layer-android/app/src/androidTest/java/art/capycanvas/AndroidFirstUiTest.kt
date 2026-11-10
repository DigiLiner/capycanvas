package art.capycanvas

import android.graphics.Bitmap
import android.os.SystemClock
import android.provider.Settings
import androidx.compose.ui.graphics.toArgb
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.semantics.SemanticsProperties
import androidx.compose.ui.platform.WindowRecomposerFactory
import androidx.compose.ui.platform.WindowRecomposerPolicy
import androidx.compose.ui.test.*
import androidx.compose.ui.test.junit4.createEmptyComposeRule
import androidx.test.core.app.ActivityScenario
import androidx.test.platform.app.InstrumentationRegistry
import org.json.JSONObject
import org.junit.Assert.*
import org.junit.Assume.assumeTrue
import org.junit.Rule
import org.junit.Test
import java.io.File
import java.util.concurrent.CountDownLatch
import java.util.concurrent.TimeUnit

/** Hold the real renderer before device creation, with the real Compose UI running. */
class AndroidFirstUiTest {
    @get:Rule(order = 0) val device = CapyDeviceRule()
    @get:Rule(order = 1) val compose = createEmptyComposeRule()

    @OptIn(androidx.compose.ui.InternalComposeUiApi::class)
    @Test fun removedSystemAnimationsStillReachTheFirstCanvas() {
        val theme = startupTheme()
        val key = Settings.Global.ANIMATOR_DURATION_SCALE
        val previous = Settings.Global.getString(instrumentation.targetContext.contentResolver, key)
        check(previous == null || previous.toFloatOrNull() != null)
        try {
            shell("settings put global $key 0")
            WindowRecomposerPolicy.withFactory(WindowRecomposerFactory.LifecycleAware) {
                ActivityScenario.launch(MainActivity::class.java).use { scenario ->
                    val host = scenario.activity().host
                    host.awaitMain("first canvas with system animations removed", 60_000, { host.snapshot.toString() }, compose) {
                        host.surfaceReady && host.snapshot?.objectOrNull("launch")?.optBoolean("finished") == true
                    }
                    instrumentation.runOnMainSync {
                        assertNull(host.failure)
                        assertTrue(host.surfaceReady)
                        assertEquals(theme, host.snapshot!!.getJSONObject("state").getString("theme"))
                    }
                }
            }
        } finally {
            shell(if (previous == null) "settings delete global $key" else "settings put global $key $previous")
        }
    }

    @Test fun workspaceFailureBeforeFirstCanvasAllowsRetry() = workspaceFailureBeforeFirstCanvas(false)

    @Test fun workspaceFailureBeforeFirstCanvasAllowsSaveAsNew() = workspaceFailureBeforeFirstCanvas(true)

    private fun workspaceFailureBeforeFirstCanvas(saveAsNew: Boolean) {
        val entered = CountDownLatch(1)
        val resume = CountDownLatch(1)
        lateinit var host: CanvasHost
        CanvasHost.beforeGpuAttachForTest = {
            entered.countDown()
            check(resume.await(20, TimeUnit.SECONDS))
            val handle = CanvasHost::class.java.getDeclaredField("handle").apply { isAccessible = true }.getLong(host)
            val deadline = SystemClock.uptimeMillis() + 20_000
            do {
                val view = JSONObject(Native.workspace(handle, obj("type" to "tick").toString())).getJSONObject("view")
                if (view.optBoolean("ready") && !view.optBoolean("busy")) break
                check(SystemClock.uptimeMillis() < deadline)
                SystemClock.sleep(10)
            } while (true)
            val failed = JSONObject(Native.workspace(handle, obj("type" to "submit", "name" to "Unopened").toString())).getJSONObject("view")
            check(!failed.isNull("error"))
        }
        var scenario: ActivityScenario<MainActivity>? = null
        try {
            val theme = startupTheme()
            scenario = ActivityScenario.launch(MainActivity::class.java)
            scenario.onActivity { host = it.host }
            compose.waitUntil(20_000) { entered.count == 0L }
            resume.countDown()
            compose.waitUntil(30_000) { host.workspaceManager?.isNull("error") == false && host.snapshot?.objectOrNull("launch")?.optBoolean("visible") == false }
            compose.runOnIdle {
                assertFalse(host.snapshot!!.getJSONObject("launch").getBoolean("finished"))
                assertNull(host.failure)
            }
            compose.onNodeWithTag("launch-splash").assertDoesNotExist()
            val phone = host.snapshot!!.getJSONObject("layout").objectOrNull("phone") != null
            val action = if (saveAsNew) {
                if (phone) "workspace-status-action" else "workspace-error-save"
            } else if (phone) "workspace-status-retry" else "workspace-error-retry"
            compose.onNodeWithTag(action, useUnmergedTree = true).assertIsDisplayed().assertIsEnabled()
            screenshot("startup-workspace-error-$theme-${if(saveAsNew) "save" else "retry"}.png")
            compose.onNodeWithTag(action, useUnmergedTree = true).performTouchInput { click() }
            if (saveAsNew) {
                compose.onNode(hasSetTextAction() and hasAnyAncestor(hasTestTag("workspace-name")), useUnmergedTree = true)
                    .assertIsDisplayed().performTextReplacement("Launch recovery")
                compose.runOnIdle {
                    assertFalse(host.snapshot!!.getJSONObject("launch").getBoolean("visible"))
                    assertFalse(host.snapshot!!.getJSONObject("launch").getBoolean("finished"))
                }
                screenshot("startup-workspace-form-$theme.png")
                compose.onNodeWithTag("workspace-submit", useUnmergedTree = true).assertIsEnabled().performTouchInput { click() }
            }
            compose.waitUntil(60_000) { host.workspaceManager?.isNull("error") == true && host.snapshot?.objectOrNull("launch")?.optBoolean("finished") == true }
            compose.onNodeWithTag("launch-splash").assertDoesNotExist()
            screenshot("startup-workspace-recovered-$theme-${if(saveAsNew) "save" else "retry"}.png")
        } finally {
            resume.countDown()
            CanvasHost.beforeGpuAttachForTest = null
            scenario?.close()
        }
    }

    @Test fun splashCoversTheWorkspaceWithoutBlockingCanvasAttachment() {
        val entered = CountDownLatch(1)
        val resume = CountDownLatch(1)
        CanvasHost.beforeGpuAttachForTest = {
            entered.countDown()
            check(resume.await(30, TimeUnit.SECONDS)) { "Test did not release GPU initialization" }
        }
        var scenario: ActivityScenario<MainActivity>? = null
        try {
            val theme = startupTheme()
            scenario = ActivityScenario.launch(MainActivity::class.java)
            lateinit var host: CanvasHost
            lateinit var activity: MainActivity
            scenario.onActivity { activity = it; host = it.host }
            // Advance Compose's test clock while the first surface is mounted.
            compose.waitUntil(20_000) { entered.count == 0L }
            scenario.onActivity {
                val canvas = it.window.decorView.descendant<CanvasSurfaceView>()!!
                assertTrue(canvas.isAttachedToWindow)
                assertTrue(canvas.width > 0 && canvas.height > 0)
            }
            compose.onNodeWithTag("launch-splash").assertIsDisplayed()
            compose.runOnIdle {
                assertFalse(host.snapshot!!.getBoolean("gpu_ready"))
                assertFalse(host.surfaceReady)
                assertNull(host.failure)
            }
            val placeholder = compose.onNodeWithTag("launch-splash").fetchSemanticsNode().boundsInRoot
            val point = Offset(placeholder.left + placeholder.width * .15f, placeholder.top + placeholder.height * .15f)
            compose.onNodeWithContentDescription("Main Menu").assertDoesNotExist()
            val captionDelay = host.bootstrap!!.getLong("launch_caption_delay_ms")
            val captionAt = activity.launchSplash.shownAt + captionDelay
            fun beforeCaption(): Boolean {
                val captions = compose.onAllNodesWithTag("launch-splash-caption").fetchSemanticsNodes()
                val before = SystemClock.uptimeMillis() < captionAt
                if (before) assertTrue("The shared delay has not elapsed", captions.isEmpty())
                return before
            }
            fun splashScreenshot(name: String, captionShown: Boolean) {
                val bitmap = instrumentation.uiAutomation.takeScreenshot()!!
                try {
                    instrumentation.targetContext.getExternalFilesDir(null)!!.resolve(name).outputStream().use {
                        bitmap.compress(Bitmap.CompressFormat.PNG, 100, it)
                    }
                    val state = host.snapshot!!.getJSONObject("state")
                    val expected = Palette(state.getString("theme") != "light", state.getJSONObject("palette")).surround.toArgb()
                    assertEquals("Splash covers the native canvas before Vulkan exists", expected, bitmap.getPixel(point.x.toInt(), point.y.toInt()))
                    val center = placeholder.center
                    val radius = (48 * instrumentation.targetContext.resources.displayMetrics.density).toInt()
                    val markPixels = (center.y.toInt() - radius until center.y.toInt() + radius).sumOf { y ->
                        (center.x.toInt() - radius until center.x.toInt() + radius).count { x -> bitmap.getPixel(x, y) != expected }
                    }
                    assertTrue("The centered capy mark is visible", markPixels > 100)
                    val density = instrumentation.targetContext.resources.displayMetrics.density
                    val captionPixels = ((center.y + 64 * density).toInt() until (center.y + 96 * density).toInt()).sumOf { y ->
                        ((center.x - 120 * density).toInt() until (center.x + 120 * density).toInt()).count { x -> bitmap.getPixel(x, y) != expected }
                    }
                    if (captionShown) assertTrue("The delayed caption is visible", captionPixels > 0)
                    else assertEquals("Only the capy appears before the shared caption delay", 0, captionPixels)
                } finally { bitmap.recycle() }
            }
            assertTrue(beforeCaption())
            compose.waitUntil(captionDelay) {
                beforeCaption()
                SystemClock.uptimeMillis() - activity.launchSplash.shownAt >= captionDelay * 3 / 4
            }
            splashScreenshot("startup-splash-mark-only-$theme.png", false)
            assertTrue(beforeCaption())
            compose.waitUntil(captionDelay) { !beforeCaption() }
            compose.mainClock.advanceTimeBy(captionDelay)
            compose.waitUntil(captionDelay) { compose.onAllNodesWithTag("launch-splash-caption").fetchSemanticsNodes().isNotEmpty() }
            compose.onNodeWithTag("launch-splash-caption").assertIsDisplayed()
            splashScreenshot("startup-splash-without-gpu-$theme.png", true)
            resume.countDown()
            compose.waitUntil(60_000) { host.snapshot?.objectOrNull("launch")?.optBoolean("finished") == true }
            compose.waitUntil(5_000) { compose.onAllNodesWithTag("launch-splash").fetchSemanticsNodes().isEmpty() }
            compose.runOnIdle {
                assertTrue(host.snapshot!!.getJSONObject("state").getBoolean("workspace_ready"))
                assertTrue(host.snapshot!!.getJSONObject("layout").array("groups").length() > 0)
                assertEquals("builtin:workspace:painter", host.workspaceManager!!.getString("id"))
                assertEquals("Sketch", host.workspaceManager!!.getString("name"))
            }
            compose.onNodeWithContentDescription("Main Menu").assertIsDisplayed().performClick()
            host.drain();compose.waitForIdle()
            compose.onNodeWithText("View", useUnmergedTree = true).performScrollTo().assertIsDisplayed().performClick()
            host.drain();compose.waitForIdle()
            compose.onNodeWithText("Fit canvas").assertIsDisplayed()
            android.util.Log.i("CapyStartupTest", "splash_ui_without_gpu boot_ns=${SystemClock.elapsedRealtimeNanos()}")
            compose.waitUntil(60_000) { host.surfaceReady || host.failure != null }
            compose.runOnIdle { assertNull(host.failure) }
            compose.onNodeWithTag("canvas-placeholder").assertDoesNotExist()
            compose.waitUntil(60_000) { host.snapshot?.optBoolean("brush_ready") == true }
            // The retained GPU must not make a replacement SurfaceView reveal
            // an empty buffer on Activity recreation.
            scenario.recreate()
            compose.waitUntil(30_000) { host.surfaceReady || host.failure != null }
            compose.runOnIdle { assertNull(host.failure) }
            compose.onNodeWithTag("canvas-placeholder").assertDoesNotExist()
        } finally {
            resume.countDown()
            CanvasHost.beforeGpuAttachForTest = null
            scenario?.close()
        }
    }

    @Test fun startupFailureRetainsLongCauseAndRestartStaysReachable() {
        for (theme in listOf("light", "dark")) {
            val cause = IllegalStateException("Vulkan attach diagnostic\n" + (1..60).joinToString("\n") {
                "Driver detail $it: preserved native cause for renderer initialization"
            })
            val diagnostic = cause.message!!
            CanvasHost.beforeGpuAttachForTest = { throw cause }
            try {
                ActivityScenario.launch(MainActivity::class.java).use { scenario ->
                    val activity = scenario.activity()
                    val host = activity.host
                    compose.waitUntil(20_000) { host.failure != null && host.workspaceManager?.optBoolean("ready") == true }
                    compose.runOnIdle {
                        assertEquals(diagnostic, host.failure)
                        host.dispatch(obj("type" to "set_theme", "theme" to theme))
                    }
                    compose.waitUntil(10_000) { host.snapshot?.optJSONObject("state")?.optString("theme") == theme }
                    assertNull(host.actionError)
                    compose.onNodeWithTag("launch-splash").assertDoesNotExist()
                    assertFalse(host.snapshot!!.getJSONObject("launch").getBoolean("finished"))
                    compose.onNodeWithText(host.bootstrap!!.getString("canvas_init_failed")).assertIsDisplayed()
                    val detail = compose.onNodeWithText(diagnostic, useUnmergedTree = true)
                    detail.assertIsDisplayed().assertTextEquals(diagnostic)
                    val viewport = detail.fetchSemanticsNode().boundsInRoot
                    assertTrue("Long diagnostic keeps a bounded viewport", viewport.height <= 180f * activity.resources.displayMetrics.density + 1f)
                    val scroller = compose.onNode(hasScrollAction() and (hasText(diagnostic) or hasAnyDescendant(hasText(diagnostic))), useUnmergedTree = true)
                    assertTrue("The entire cause remains available by scrolling",
                        scroller.fetchSemanticsNode().config[SemanticsProperties.VerticalScrollAxisRange].maxValue() > 0f)
                    scroller.performTouchInput { swipeUp() }
                    val restart = compose.onNodeWithText(host.bootstrap!!.getString("restart_canvas"))
                    restart.assertIsDisplayed().assertHasClickAction()
                    val root = compose.onRoot().fetchSemanticsNode().boundsInRoot
                    val button = restart.fetchSemanticsNode().boundsInRoot
                    assertTrue("Restart stays within the editor", root.contains(button.topLeft) && root.contains(button.bottomRight))
                    screenshot("startup-failure-$theme.png")
                    val epoch = host.snapshot!!.getJSONObject("state").getJSONObject("document_file").getLong("epoch")
                    CanvasHost.beforeGpuAttachForTest = null
                    restart.performClick()
                    compose.waitUntil(60_000) {
                        host.failure == null && host.surfaceReady && host.snapshot?.optBoolean("brush_ready") == true &&
                            host.snapshot?.objectOrNull("launch")?.optBoolean("finished") == true
                    }
                    compose.runOnIdle {
                        assertEquals(epoch, host.snapshot!!.getJSONObject("state").getJSONObject("document_file").getLong("epoch"))
                    }
                    detail.assertDoesNotExist()
                    compose.onNodeWithTag("canvas-placeholder").assertDoesNotExist()
                }
            } finally { CanvasHost.beforeGpuAttachForTest = null }
        }
    }

    @Test fun unsupportedGpuExplainsTheDeviceInTheCanvasArea() {
        assumeTrue(InstrumentationRegistry.getArguments().getString("unsupportedGpu") == "true")
        for (theme in listOf("light", "dark")) {
            ActivityScenario.launch(MainActivity::class.java).use { scenario ->
                val activity = scenario.activity()
                val host = activity.host
                compose.waitUntil(60_000) { host.failure != null && host.workspaceManager?.optBoolean("ready") == true }
                compose.runOnIdle { host.dispatch(obj("type" to "set_theme", "theme" to theme)) }
                compose.waitUntil(10_000) { host.snapshot?.optJSONObject("state")?.optString("theme") == theme }
                assertNull(host.actionError)
                val failure = host.failure!!
                assertTrue(failure, failure.startsWith("This device’s graphics processor (GPU) can’t run Capy Canvas. Updating the system or graphics driver may help.\n\n"))
                assertFalse(failure, failure.contains("java.lang"))
                compose.onNodeWithText(host.bootstrap!!.getString("canvas_init_failed")).assertIsDisplayed()
                compose.onNodeWithText(failure, useUnmergedTree = true).assertIsDisplayed()
                compose.waitForIdle()
                val screen = InstrumentationRegistry.getInstrumentation().uiAutomation.takeScreenshot()
                File(activity.getExternalFilesDir(null), "unsupported-gpu-$theme.png").outputStream().use {
                    screen.compress(Bitmap.CompressFormat.PNG, 100, it)
                }
                screen.recycle()
            }
        }
    }
}
