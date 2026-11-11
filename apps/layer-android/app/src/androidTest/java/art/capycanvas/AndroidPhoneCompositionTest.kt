package art.capycanvas

import androidx.compose.ui.test.junit4.createAndroidComposeRule
import androidx.compose.ui.test.onNodeWithTag
import org.junit.Assert.*
import org.junit.Assume.assumeTrue
import org.junit.Rule
import org.junit.Test

class AndroidPhoneCompositionTest {
    @get:Rule(order=0) val device = CapyDeviceRule()
    @get:Rule(order=1) val compose = createAndroidComposeRule<MainActivity>()

    @Test fun sizeChangesRetainUnchangedPhoneTileCompositions() {
        val host=compose.activity.host
        host.awaitReady(compose=compose)
        assumeTrue("Requires the native phone layout",host.snapshot!!.getJSONObject("layout").objectOrNull("phone") != null)
        host.awaitMain("launch reveal removed",30_000,{ "${host.snapshot?.opt("launch")}" },compose) { findTag("launch-splash") == null }
        compose.onNodeWithTag("phone-frame").assertExists()
        compose.runOnIdle { CompositionCounts { info ->
            when {
                info.startsWith("art.capycanvas.PhoneTile") -> "PhoneTile"
                info.startsWith("art.capycanvas.HeaderButton") -> "HeaderButton"
                else -> null
            }
        } }.use { counts ->
            for (command in listOf("eraser","drawing_brush")) {
                host.drain(obj("type" to "invoke","command" to command))
                compose.waitForIdle()
            }
            host.awaitReady(compose=compose)
            compose.waitForIdle()
            val tools=compose.runOnIdle { jsonValue(host.snapshot!!.getJSONArray("phone_tools")) }
            compose.runOnIdle {
                assertTrue("Changing the selected tool composes its real phone tiles",counts["PhoneTile"]>0)
                assertTrue("Changing the selected tool composes its real buttons",counts["HeaderButton"]>0)
                counts.clear()
            }
            for (size in listOf(40,48,64)) {
                host.drain(obj("type" to "set_brush_size","value" to size))
                compose.waitForIdle()
                compose.runOnIdle {
                    assertEquals("Size does not change the tool tile models",tools,jsonValue(host.snapshot!!.getJSONArray("phone_tools")))
                    assertEquals(size.toDouble(),host.snapshot!!.getJSONArray("phone_sliders").getJSONObject(0)
                        .getJSONObject("numeric").getDouble("value"),0.0)
                }
            }
            compose.runOnIdle {
                assertEquals("Size publications retain unchanged phone tiles",0,counts["PhoneTile"])
                assertEquals("Size publications retain unchanged phone buttons",0,counts["HeaderButton"])
            }
        }
    }
}
