package art.capycanvas

import android.view.ViewTreeObserver
import androidx.compose.ui.geometry.Rect
import androidx.compose.ui.layout.LayoutInfo
import androidx.compose.ui.test.junit4.createAndroidComposeRule
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.unit.IntSize
import org.junit.Assert.*
import org.junit.Assume.assumeTrue
import org.junit.Rule
import org.junit.Test
import java.util.concurrent.ConcurrentLinkedQueue
import java.util.concurrent.atomic.AtomicInteger

class AndroidDrawerCompositionTest {
    @get:Rule(order=0) val device = CapyDeviceRule()
    @get:Rule(order=1) val compose = createAndroidComposeRule<MainActivity>()

    @Test fun closingColorDrawerRetainsPaletteCompositionWhileBoundsMove() {
        val host=compose.activity.host
        host.awaitReady(compose=compose)
        assumeTrue("Requires the native phone layout",host.snapshot!!.getJSONObject("layout").objectOrNull("phone") != null)
        host.awaitMain("launch reveal removed",30_000,{ "${host.snapshot?.opt("launch")}" },compose) { findTag("launch-splash") == null }
        val root=compose.activity.window.decorView
        assertTrue("The private window owns the drawer",root.isAttachedToWindow && root.hasWindowFocus())
        val dock=checkNotNull(host.workspaceInteraction)
        val initialChrome=compose.runOnIdle { dock.chromeRegions.keys.toSet() }
        val initialSources=compose.runOnIdle { dock.regions.keys.toSet() }
        val counts=compose.runOnIdle { CompositionCounts { info ->
            if(info.substringBefore(" (")=="art.capycanvas.ContentDrawer")"DrawerContent"
            else "PalettePanel".takeIf { info.startsWith("art.capycanvas.PalettePanel") }
        } }
        data class Frame(val order:Int,val size:IntSize?,val placed:Boolean,val outer:Rect?,val published:Boolean,val contentCompositions:Int,
            val chrome:List<Rect>,val sources:List<Rect>)
        data class Draw(val order:Int,val size:IntSize)
        val sequence=AtomicInteger()
        val frames=mutableListOf<Frame>()
        val draws=ConcurrentLinkedQueue<Draw>()
        val callbacks=mutableListOf<Runnable>()
        fun content(): LayoutInfo? = findTag("tool-drawer")?.second?.layoutInfo?.let {
            (it.javaClass.getMethod("getChildrenInfo").invoke(it) as List<*>).singleOrNull() as? LayoutInfo
        }
        val observer=ViewTreeObserver.OnPreDrawListener {
            val outer=findTag("tool-drawer")?.second
            val child=content()
            val placed=outer?.layoutInfo?.isPlaced == true && child?.isPlaced == true && child.coordinates.isAttached
            val size=child?.coordinates?.takeIf { it.isAttached }?.size
            val bounds=dock.drawer?.objectOrNull("placement")?.objectOrNull("bounds")
            frames.add(Frame(sequence.incrementAndGet(),size,placed,outer?.boundsInRoot?.translate(-dock.origin),
                bounds != null && bounds.number("width")>0f && bounds.number("height")>0f,counts["DrawerContent"],
                dock.chromeRegions.filterKeys { it !in initialChrome }.values.map { it.bounds },
                dock.regions.filterKeys { it !in initialSources }.values.map { it.bounds }))
            if(placed && size != null && size.width>0 && size.height>0) {
                val callback=Runnable { draws.add(Draw(sequence.incrementAndGet(),size)) }
                callbacks.add(callback)
                root.viewTreeObserver.registerFrameCommitCallback(callback)
            }
            true
        }
        val color=host.snapshot!!.getJSONObject("state").getJSONObject("workspace").getJSONObject("layout")
            .array("phone").objects().first { it.getJSONObject("control").getString("kind")=="color" }.getInt("id")
        counts.use {
            compose.runOnIdle { root.viewTreeObserver.addOnPreDrawListener(observer) }
            try {
                compose.mainClock.autoAdvance=false
                host.drain(obj("type" to "phone","action" to obj("kind" to "tool","tile" to color)))
                repeat(30) {
                    host.drain()
                    compose.mainClock.advanceTimeByFrame()
                    compose.waitForIdle()
                }
                host.awaitMain("Color drawer composed and placed",30_000,{
                    "drawer=${dock.drawer}; frame=${frames.lastOrNull()}; draws=${draws.toList().takeLast(2)}; model=${host.snapshot?.getJSONObject("state")?.getJSONObject("customization")?.opt("drawer")}" },compose) {
                    val bounds=dock.drawer?.objectOrNull("placement")?.objectOrNull("bounds")
                    findTag("drawer-panel-palettes") != null && bounds != null && bounds.number("height")>0f
                }
                compose.onNodeWithTag("drawer-panel-palettes").assertExists()
                compose.waitForIdle()
                val target=compose.runOnIdle { checkNotNull(content()).coordinates.size }
                compose.runOnIdle {
                    assertTrue("The tracer observes the real palette",counts["PalettePanel"]>0)
                    assertTrue("The tracer observes the drawer content body",counts["DrawerContent"]>0)
                    val moving=frames.zipWithNext().filter { (before,after) ->
                        before.published && after.published && before.outer?.isEmpty==false && after.outer?.isEmpty==false && before.outer!=after.outer
                    }.map { it.second }
                    assertTrue("The opening has multiple actual moving frames",moving.size>=2)
                    assertEquals("Moving frames retain the drawer content body",0,moving.last().contentCompositions-moving.first().contentCompositions)
                }
                host.drain(obj("type" to "customize","action" to obj("type" to "close_expanded")))
                compose.mainClock.advanceTimeByFrame()
                compose.waitForIdle()
                val before=compose.onNodeWithTag("tool-drawer").fetchSemanticsNode().boundsInRoot
                compose.runOnIdle { counts.clear() }
                repeat(4) {
                    compose.mainClock.advanceTimeByFrame()
                    compose.waitForIdle()
                }
                val after=compose.onNodeWithTag("tool-drawer").fetchSemanticsNode().boundsInRoot
                assertNotEquals("The retained drawer actually moves",before,after)
                compose.runOnIdle {
                    val visible=frames.filter { it.published && it.outer?.isEmpty == false }
                    assertTrue("Native draws observe the moving drawer",visible.map { it.outer }.distinct().size>1)
                    assertTrue("Visible drawer content stays at $target: ${visible.map { it.size }}",visible.all { it.placed && it.size==target })
                    val first=frames.first { it.published }
                    assertTrue("A full-target content frame commits before geometry ${first.order}: target=$target; draws=$draws",
                        draws.any { it.order<first.order && it.size==target })
                    assertTrue("Hidden drawer input bounds are empty",frames.filter { !it.published }.all { frame ->
                        (frame.chrome+frame.sources).all { it.isEmpty }
                    })
                    assertTrue("Content input bounds stay inside the animated viewport",visible.all { frame ->
                        val bounds=checkNotNull(frame.outer)
                        frame.sources.all { it.isEmpty || it.left>=bounds.left-1 && it.top>=bounds.top-1 && it.right<=bounds.right+1 && it.bottom<=bounds.bottom+1 }
                    })
                    assertEquals("Geometry-only frames retain palette content",0,counts["PalettePanel"])
                }
            } finally {
                instrumentation.runOnMainSync {
                    root.viewTreeObserver.removeOnPreDrawListener(observer)
                    callbacks.forEach { root.viewTreeObserver.unregisterFrameCommitCallback(it) }
                }
                compose.mainClock.autoAdvance=true
                compose.waitForIdle()
            }
        }
    }
}
