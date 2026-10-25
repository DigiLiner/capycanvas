package art.capycanvas

import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.ExperimentalLayoutApi
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.rememberUpdatedState
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.layout.Layout
import androidx.compose.ui.layout.layout
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.semantics.LiveRegionMode
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.liveRegion
import androidx.compose.ui.semantics.paneTitle
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.semantics.stateDescription
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.Constraints
import androidx.compose.ui.unit.dp
import androidx.compose.ui.zIndex
import kotlinx.coroutines.delay
import org.json.JSONObject
import kotlin.math.roundToInt

private const val NoticeTimeoutMs = 4_000L
private val NoticeMargin = 12.dp
private val NoticeGap = 8.dp

/** One of the core's ordered notice choices, identified by its stable token. */
internal data class CanvasNoticeAction(val id: String, val label: String, val enabled: Boolean, val reason: String?)

/** The core's notice, or a refused command or action, which has no id. */
internal data class CanvasNotice(val id: Long?, val name: String, val text: String, val actions: List<CanvasNoticeAction>)

/** Projects the shared canvas notice at its shared placement: centred on the
 * placement's anchor, no wider than its maximum width, and above a canvas action
 * bar along the bottom edge. It is not a popup, so it never takes window focus.
 * It hides after a timeout, answering the core, and the host hides it at the
 * next canvas contact. */
@OptIn(ExperimentalLayoutApi::class)
@Composable internal fun NoticeBubble(host: CanvasHost, dock: DockInteraction, placement: JSONObject) {
    val notice = host.notice ?: return
    val current by rememberUpdatedState(notice)
    LaunchedEffect(notice.id ?: notice) {
        delay(NoticeTimeoutMs)
        host.answerNotice(current, accept = false)
    }
    val colors = LocalPalette.current
    Surface(Modifier.zIndex(250f).layout { measurable, constraints ->
        val x = placement.number("x") * density
        val anchor = placement.number("y") * density
        val bar = dock.canvasBarSlot?.takeIf { it.bottom > placement.number("bar_floor") * density }
        val bottom = minOf(anchor, bar?.let { it.top - NoticeMargin.toPx() } ?: anchor)
        val bubble = measurable.measure(Constraints(maxWidth = (placement.number("max_width") * density).roundToInt().coerceAtLeast(0)))
        layout(constraints.maxWidth, constraints.maxHeight) {
            bubble.place((x - bubble.width / 2f).roundToInt(), (bottom - bubble.height).roundToInt())
        }
    }.chromeRegion(dock).testTag("canvas-notice").semantics { liveRegion = LiveRegionMode.Polite; paneTitle = notice.name },
        shape = ControlShape, color = colors.panel, contentColor = colors.text, shadowElevation = 6.dp) {
        Layout({
            Text(notice.text, Modifier.testTag("canvas-notice-text"))
            FlowRow(horizontalArrangement = Arrangement.spacedBy(NoticeGap), verticalArrangement = Arrangement.Center) {
                notice.actions.forEach { action ->
                    Box(Modifier.heightIn(min = 32.dp).clip(ControlShape).testTag("canvas-notice-action-${action.id}")
                        .semantics { action.reason?.let { stateDescription = it } }
                        .clickable(enabled = action.enabled, role = Role.Button) { host.answerNotice(notice, accept = true, action = action.id) }.padding(horizontal = 10.dp),
                        contentAlignment = Alignment.Center) {
                        Text(action.label, color = if (action.enabled) colors.accent else colors.secondary, fontWeight = FontWeight.Bold, maxLines = 1)
                    }
                }
            }
        }, Modifier.heightIn(min = 40.dp).padding(start = 14.dp, end = if (notice.actions.isEmpty()) 14.dp else 4.dp, top = 4.dp, bottom = 4.dp)) { (text, actions), constraints ->
            val gap = NoticeGap.roundToPx()
            val offers = if (notice.actions.isEmpty()) 0 else actions.maxIntrinsicWidth(Constraints.Infinity).coerceAtMost(constraints.maxWidth)
            val words = text.maxIntrinsicWidth(Constraints.Infinity)
            val inline = offers == 0 || words + gap + offers <= constraints.maxWidth
                || constraints.maxWidth - offers - gap >= placement.number("min_text_width") * density
            val row = actions.measure(Constraints(maxWidth = if (inline) offers else constraints.maxWidth))
            val label = text.measure(Constraints(maxWidth = if (inline) (constraints.maxWidth - row.width - if (offers == 0) 0 else gap).coerceAtLeast(0) else constraints.maxWidth))
            val width = if (inline) label.width + row.width + if (offers == 0) 0 else gap else maxOf(label.width, row.width)
            val height = maxOf(constraints.minHeight, if (inline) maxOf(label.height, row.height) else label.height + row.height)
            layout(width, height) {
                if (inline) {
                    label.place(0, (height - label.height) / 2)
                    row.place(label.width + gap, (height - row.height) / 2)
                } else {
                    label.place(0, 0)
                    row.place(0, label.height)
                }
            }
        }
    }
}
