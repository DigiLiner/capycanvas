package art.capycanvas

import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.input.key.Key
import androidx.compose.ui.semantics.SemanticsActions
import androidx.compose.ui.semantics.SemanticsProperties
import androidx.compose.ui.test.*
import androidx.compose.ui.test.junit4.createAndroidComposeRule
import org.json.JSONObject
import org.junit.Before
import org.junit.Rule
import org.junit.Test
import org.junit.Assert.*
import java.util.concurrent.CountDownLatch
import java.util.concurrent.TimeUnit

class AndroidPortParityTest {
    @get:Rule(order = 0) val device = CapyDeviceRule()
    @get:Rule(order = 1) val compose = createAndroidComposeRule<MainActivity>()
    private val host get() = compose.activity.host
    private fun state() = host.snapshot!!.getJSONObject("state")
    @Before fun ready() { wakeDevice(); host.awaitReady(compose = compose); action(obj("type" to "close_settings")) }
    private fun action(value: JSONObject) {
        val done = CountDownLatch(1)
        compose.runOnIdle { host.dispatch(value); host.query(obj("type" to "catalog")) { done.countDown() } }
        assertTrue(done.await(15, TimeUnit.SECONDS)); compose.waitForIdle()
        assertNull(host.actionError); assertNull(host.failure)
    }

    @Test fun colorReadoutModifierKeysUseSharedFastAndFineScrubs() {
        action(obj("type" to "color", "action" to obj("op" to "definition", "color" to obj("space" to "Srgb", "rgba" to org.json.JSONArray(listOf(.4, .5, .6, 1))))))
        for (theme in listOf("light", "dark")) {
            action(obj("type" to "set_theme", "theme" to theme))
            for ((key, speed) in listOf(Key.ShiftLeft to "fast", Key.CtrlLeft to "fine", Key.AltLeft to "fine")) {
                var reference = JSONObject(Native.colorUi(obj("type" to "editor_open", "colors" to state().displayColors(), "slot" to "foreground", "display_space" to "Srgb").toString(), host.languageTag))
                fun edit(action: JSONObject) { reference = JSONObject(Native.colorUi(obj("type" to "editor", "editor" to reference.getJSONObject("editor"), "action" to action, "display_space" to "Srgb").toString(), host.languageTag)) }
                edit(obj("op" to "scrub", "target" to obj("kind" to "value", "row" to 0, "index" to 0), "pixels" to 2, "speed" to speed))
                edit(obj("op" to "end_scrub", "cancel" to false))
                compose.runOnIdle { host.colorEditor = ColorEditorRequest("foreground", null, false, false) { _, _ -> } }
                compose.waitUntil(10_000) { compose.onAllNodesWithTag("color-value-0-0").fetchSemanticsNodes().isNotEmpty() }
                compose.onNodeWithTag("color-value-0-0").performSemanticsAction(SemanticsActions.RequestFocus) { it() }
                compose.onNodeWithTag("color-value-0-0").performKeyInput { keyDown(key); pressKey(Key.DirectionUp); keyUp(key) }
                compose.onNodeWithTag("color-value-hex").assert(SemanticsMatcher("shared $speed result") {
                    it.config[SemanticsProperties.ContentDescription].single().endsWith(reference.getJSONObject("view").getString("hex"))
                })
                compose.onNodeWithTag("color-cancel").performClick()
                compose.waitUntil(5_000) { host.colorEditor == null }
            }
        }
    }

    @Test fun standaloneZoomReadoutAcceptsWheelAndNormalizedTouchScrub() {
        val workspace = JSONObject(state().getJSONObject("workspace").toString())
        val fixture = JSONObject(workspace.toString()).apply {
            getJSONObject("layout").put("canvas_info", obj("visible" to true))
            put("zen_mode", false)
        }
        action(obj("type" to "restore_workspace", "workspace" to fixture))
        compose.waitUntil(5_000) { compose.onAllNodesWithTag("camera-readout").fetchSemanticsNodes().isNotEmpty() }
        try { for (theme in listOf("light", "dark")) {
            action(obj("type" to "set_theme", "theme" to theme))
            action(obj("type" to "set_zoom", "zoom" to 1))
            compose.onNodeWithTag("camera-readout").performClick()
            val label = host.catalog.getJSONObject("native_copy").getJSONObject("header").getString("zoom")
            val readout = compose.onNodeWithTag("number-value-$label")
            val control = host.catalog.getJSONObject("zoom")
            fun number(value: Double, operation: JSONObject) = JSONObject(Native.number(obj("control" to control, "value" to value, "operation" to operation).toString(), host.languageTag))
            val before = host.cameraZoom.toDouble()
            val wheel = number(before, obj("type" to "step", "steps" to -1)).getDouble("value")
            readout.performMouseInput { moveTo(center); scroll(1f) }
            compose.waitUntil(5_000) { kotlin.math.abs(host.cameraZoom - wheel) < .0001 }
            val fill = number(wheel, obj("type" to "format")).getDouble("fill")
            val expected = number(wheel, obj("type" to "position", "position" to fill + .2)).getDouble("value")
            val pixels = 40 * compose.activity.resources.displayMetrics.density
            readout.performTouchInput { down(center); moveBy(Offset(0f, -pixels)); up() }
            compose.waitUntil(5_000) { kotlin.math.abs(host.cameraZoom - expected) < .0001 }
            compose.onNodeWithTag("zoom-menu").performKeyInput { pressKey(Key.Escape) }
        } } finally { action(obj("type" to "restore_workspace", "workspace" to workspace)) }
    }

    @Test fun toolbarPromptProjectsChoicesAndRejectsBlankNameBeforeSubmit() {
        compose.runOnIdle { host.workspaceInput(obj("type" to "open", "page" to "this_workspace")) }
        compose.waitUntil(10_000) { host.workspaceManager?.optString("page") == "this_workspace" }
        compose.onNodeWithTag("new-toolbar").performClick()
        compose.waitUntil(5_000) { host.workspaceManager?.objectOrNull("prompt") != null }
        val prompt = host.workspaceManager!!.getJSONObject("prompt")
        assertTrue(prompt.array("choices").length() > 0)
        compose.onNode(hasSetTextAction() and hasAnyAncestor(hasTestTag("workspace-name")), useUnmergedTree = true).performTextReplacement("")
        compose.onNodeWithTag("workspace-submit").assertIsNotEnabled()
        compose.onNode(hasSetTextAction() and hasAnyAncestor(hasTestTag("workspace-name")), useUnmergedTree = true).performTextReplacement("Parity toolbar")
        compose.onNodeWithTag("workspace-submit").assertIsEnabled()
        compose.runOnIdle { host.workspaceInput(obj("type" to "cancel")) }
    }
    @Test fun convertedCopyOpensNewDrawingAfterAutomaticComparison() {
        val original = host.drawingTabs.selected
        val count = host.drawingTabs.rows.size
        val revision = state().getJSONObject("document_file").getLong("revision")
        val modified = state().getJSONObject("document_file").getBoolean("modified")
        action(obj("type" to "invoke", "command" to "convert_color_space"))
        val copy = host.catalog.getJSONObject("document_color_copy")
        compose.waitUntil(10_000) { compose.onAllNodesWithTag("color-choice-${copy.getString("result")}").fetchSemanticsNodes().isNotEmpty() }
        compose.onNodeWithTag("color-choice-${copy.getString("result")}").performScrollTo().performClick()
        compose.onNodeWithText(copy.getString("flattened_copy")).performClick()
        compose.waitUntil(60_000) { compose.onAllNodesWithContentDescription(copy.getString("prepared_composition")).fetchSemanticsNodes().isNotEmpty() }
        compose.onNodeWithText(copy.getJSONObject("common").getString("apply")).performClick()
        compose.waitUntil(60_000) { host.drawingTabs.rows.size == count + 1 && host.drawingTabs.selected != original && !host.drawingTabs.blocked }
        assertTrue(host.drawingTabs.rows.any { it.getLong("id") == original })
        compose.runOnIdle { host.drawingTabs.select(original) }
        compose.waitUntil(30_000) { host.drawingTabs.selected == original && !host.drawingTabs.blocked }
        assertEquals(revision, state().getJSONObject("document_file").getLong("revision"))
        assertEquals(modified, state().getJSONObject("document_file").getBoolean("modified"))
        assertNull(host.actionError); assertNull(host.failure)
    }

    @Test fun savedProfileLibraryFiltersThroughSharedMetadataSearch() {
        val context = compose.activity
        val root = AppStorage.of(context).colorProfiles.apply { mkdirs() }
        val created = (0 until 8).map { index ->
            val bytes = "parity-profile-${java.util.UUID.randomUUID()}-$index".toByteArray()
            val id = java.security.MessageDigest.getInstance("SHA-256").digest(bytes).joinToString("") { "%02x".format(it.toInt() and 255) }
            java.io.File(root, "$id.icc").also { check(!it.exists()); it.writeBytes(bytes) }
        }
        try {
            for (theme in listOf("light", "dark")) {
                action(obj("type" to "set_theme", "theme" to theme))
                action(obj("type" to "open_settings", "page" to "color"))
                val copy = host.catalog.getJSONObject("profile_copy")
                compose.onNodeWithText(copy.getString("manage")).performScrollTo().performClick()
                compose.waitUntil(10_000) { compose.onAllNodesWithTag("profile-library-search").fetchSemanticsNodes().isNotEmpty() }
                val query = created.first().nameWithoutExtension
                compose.onNode(hasSetTextAction() and hasAnyAncestor(hasTestTag("profile-library-search")), useUnmergedTree = true).performTextReplacement(query)
                compose.waitUntil(5_000) { compose.onAllNodesWithText(copy.getString("remove")).fetchSemanticsNodes().size == 1 }
                compose.onNodeWithTag("profile-library-done").performClick()
                action(obj("type" to "close_settings"))
            }
        } finally { created.forEach { check(it.delete()) } }
    }

    @Test fun customPreferenceSwatchSelectsTheWholeHexAndRetainsItsEntry() {
        val settings = JSONObject(state().getJSONObject("settings").toString())
        try {
            for (theme in listOf("light", "dark")) {
                action(obj("type" to "set_theme", "theme" to theme))
                action(obj("type" to "open_settings", "page" to "appearance"))
                compose.onNodeWithTag("preference-accent").performScrollTo()
                compose.onNodeWithTag("setting-accent-swatch-6").performClick()
                compose.waitUntil(5_000) { state().getJSONObject("settings").optString("accent") == "#e62d42" }
                compose.onNodeWithTag("setting-accent-swatch-10").performClick()
                val input = compose.onNode(hasSetTextAction() and hasAnyAncestor(hasTestTag("setting-text-accent")), useUnmergedTree = true)
                input.assert(SemanticsMatcher.expectValue(SemanticsProperties.TextSelectionRange, androidx.compose.ui.text.TextRange(0, 7)))
                input.performTextInput("#12ab56")
                input.performImeAction()
                compose.waitUntil(5_000) { state().getJSONObject("settings").optString("accent") == "#12ab56" }
                compose.onNodeWithTag("setting-text-accent").assertExists()
                action(obj("type" to "close_settings"))
                action(obj("type" to "open_settings", "page" to "appearance"))
                compose.onNodeWithTag("preference-accent").performScrollTo()
                compose.onNodeWithTag("setting-text-accent").assertExists()
                action(obj("type" to "close_settings"))
            }
        } finally { action(obj("type" to "restore_settings", "settings" to settings)) }
    }

    @Test fun numericDialHandlesShiftPageResetAndWheelOnItsOwnSurface() {
        val value = androidx.compose.runtime.mutableDoubleStateOf(0.0)
        val control = JSONObject(host.catalog.getJSONObject("rotation").toString()).put("kind", "dial")
        lateinit var view: androidx.compose.ui.platform.ComposeView
        compose.runOnIdle {
            view = androidx.compose.ui.platform.ComposeView(compose.activity)
            compose.activity.addContentView(view, android.widget.FrameLayout.LayoutParams(600, 180))
            view.setContent {
                androidx.compose.runtime.CompositionLocalProvider(LocalCanvasHost provides host,
                    LocalPalette provides Palette(state().getString("theme") == "dark", state().getJSONObject("palette"))) {
                    androidx.compose.material3.MaterialTheme {
                        NumericSetting("Direction", value.doubleValue, control, id = "parity-dial", onReset = { value.doubleValue = 0.0 }) { value.doubleValue = it }
                    }
                }
            }
        }
        fun expected(start: Double, steps: Int) = JSONObject(Native.number(obj("control" to control, "value" to start, "operation" to obj("type" to "step", "steps" to steps)).toString(), host.languageTag)).getDouble("value")
        try {
            val dial = compose.onNodeWithTag("number-dial-parity-dial")
            dial.performSemanticsAction(SemanticsActions.RequestFocus) { it() }
            dial.performKeyInput { keyDown(Key.ShiftLeft); pressKey(Key.DirectionRight); keyUp(Key.ShiftLeft) }
            val shifted = expected(0.0, 15)
            compose.runOnIdle { assertEquals(shifted, value.doubleValue, .00001) }
            dial.performKeyInput { pressKey(Key.PageUp) }
            compose.runOnIdle { assertEquals(expected(shifted, 45), value.doubleValue, .00001) }
            dial.performKeyInput { pressKey(Key.MoveHome) }
            compose.runOnIdle { assertEquals(0.0, value.doubleValue, .00001) }
            dial.performMouseInput { moveTo(center); scroll(1f) }
            compose.runOnIdle { assertEquals(expected(0.0, -1), value.doubleValue, .00001) }
        } finally { compose.runOnIdle { view.disposeComposition(); (view.parent as android.view.ViewGroup).removeView(view) } }
    }

}
