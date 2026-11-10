package art.capycanvas

import android.graphics.Rect
import android.graphics.drawable.ColorDrawable
import android.graphics.drawable.LayerDrawable
import android.os.Build
import android.os.SystemClock
import android.view.Gravity
import android.view.View
import android.view.ViewTreeObserver
import android.widget.FrameLayout
import android.widget.TextView
import android.window.SplashScreenView
import androidx.activity.compose.LocalActivity
import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.animation.core.tween
import androidx.compose.foundation.Image
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.offset
import androidx.compose.foundation.layout.size
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.SideEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.ColorFilter
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import kotlinx.coroutines.delay

internal class LaunchReveal {
    var covered by mutableStateOf(false)
        private set
    var started by mutableStateOf(false)
        private set
    var visible = true
        private set
    var finished = false
        private set
    var closed = false
        private set
    private var paint by mutableStateOf<Any?>(null)
    private var submitted: Any? = null

    fun holding(visible: Boolean, finished: Boolean) = !closed && (visible || finished && covered && !started)

    fun update(visible: Boolean, finished: Boolean) {
        if (closed) return
        this.visible = visible
        this.finished = finished
        if (visible || !finished) {
            covered = visible
            started = false
            paint = null
            submitted = null
        } else if (covered && !started && paint == null) paint = Any()
    }
    fun native() {
        if (closed || !visible && !finished) return
        covered = true
        started = false
        paint = if (finished) Any() else null
        submitted = null
    }
    fun drawn(): (() -> Unit)? {
        val current = paint ?: return null
        if (closed || submitted === current) return null
        submitted = current
        return {
            if (!closed && paint === current && submitted === current) {
                paint = null
                submitted = null
                started = true
            }
        }
    }
    fun close() {
        closed = true
        covered = false
        started = false
        paint = null
        submitted = null
    }
}

internal class LaunchSplashWindow(private val activity: MainActivity) {
    val shownAt = SystemClock.uptimeMillis()
    val reveal = LaunchReveal()
    var nativeActive by mutableStateOf(false)
        private set
    var nativeCaptured by mutableStateOf(false)
        private set
    var hadNativeVisual by mutableStateOf(false)
        private set
    var background by mutableStateOf(activity.getColor(R.color.canvas_launch_background))
        private set
    val foreground = activity.getColor(R.color.capy_splash_foreground)
    val legacyMarkSize = if (Build.VERSION.SDK_INT < 31)
        (activity.getDrawable(R.drawable.capy_launch_background) as? LayerDrawable)?.let { it.getLayerHeight(it.numberOfLayers - 1).takeIf { size -> size > 0 } } else null
    private var view: View? = null
    private var caption: TextView? = null
    private var frameCommit: Pair<ViewTreeObserver, Runnable>? = null
    private var fading = false
    private var duration: Long? = null
    private var gap = 0f
    private var text: String? = null

    fun install() {
        if (Build.VERSION.SDK_INT >= 31) activity.splashScreen.setOnExitAnimationListener { splash ->
            if (reveal.closed) { splash.remove(); return@setOnExitAnimationListener }
            background = (splash.background as? ColorDrawable)?.color ?: background
            view = splash
            splash.post {
                if (view !== splash) return@post
                val icon = splash.iconView
                nativeActive = icon != null && icon.width > 0 && icon.height > 0
                nativeCaptured = true
                hadNativeVisual = nativeActive
                if (nativeActive) {
                    cancelFrame()
                    reveal.native()
                    caption = TextView(activity).apply {
                        textSize = 12f
                        gravity = Gravity.CENTER
                        alpha = 0f
                        splash.addView(this, FrameLayout.LayoutParams(FrameLayout.LayoutParams.WRAP_CONTENT,
                            FrameLayout.LayoutParams.WRAP_CONTENT))
                    }
                }
                refresh()
            }
        }
    }
    fun update(visible: Boolean, finished: Boolean, text: String?, reveal: Long?, gap: Float) {
        if (this.reveal.closed) return
        this.reveal.update(visible, finished)
        if (visible || !finished) cancelFrame()
        this.text = text
        this.duration = reveal
        this.gap = gap
        refresh()
    }
    fun readyDrawn(content: View) {
        val observer = content.viewTreeObserver
        if (!observer.isAlive) return
        val complete = reveal.drawn() ?: return
        lateinit var callback: Runnable
        callback = Runnable {
            content.post {
                if (frameCommit?.second === callback) {
                    frameCommit = null
                    complete()
                    refresh()
                }
            }
        }
        frameCommit = observer to callback
        observer.registerFrameCommitCallback(callback)
        content.postInvalidateOnAnimation()
    }
    private fun cancelFrame() {
        frameCommit?.let { (observer, callback) ->
            if (observer.isAlive) observer.unregisterFrameCommitCallback(callback)
        }
        frameCommit = null
    }
    private fun refresh() {
        if (Build.VERSION.SDK_INT < 31) return
        val splash = view as? SplashScreenView ?: return
        if (!nativeCaptured) return
        if (!nativeActive) {
            splash.post { if (view === splash) { view = null; splash.remove() } }
            return
        }
        caption?.apply {
            val showing = this.text.isNotEmpty()
            this.text = this@LaunchSplashWindow.text.orEmpty()
            setTextColor(this@LaunchSplashWindow.foreground)
            val icon = splash.iconView
            if (icon != null) {
                val bounds = Rect(0, 0, icon.width, icon.height)
                splash.offsetDescendantRectToMyCoords(icon, bounds)
                measure(View.MeasureSpec.makeMeasureSpec(splash.width, View.MeasureSpec.AT_MOST),
                    View.MeasureSpec.makeMeasureSpec(splash.height, View.MeasureSpec.AT_MOST))
                x = bounds.exactCenterX() - measuredWidth / 2f
                y = bounds.bottom + gap
            }
            if (this.text.isNotEmpty() && !showing) animate().alpha(.5f).setDuration(180).start()
            else if (this.text.isEmpty()) { animate().cancel(); alpha = 0f }
        }
        if (!reveal.visible) {
            if (!reveal.finished) {
                splash.animate().cancel()
                view = null
                nativeActive = false
                fading = false
                splash.remove()
            } else if (reveal.started && !fading && duration != null) {
                fading = true
                splash.animate().alpha(0f).setDuration(duration!!).withEndAction {
                    if (view === splash) { view = null; nativeActive = false }
                    fading = false
                    splash.remove()
                }.start()
            }
        }
    }
    fun close() {
        reveal.close()
        cancelFrame()
        if (Build.VERSION.SDK_INT >= 31) (view as? SplashScreenView)?.let { it.animate().cancel(); it.remove() }
        view = null
        caption = null
        nativeActive = false
    }
}

internal fun CanvasHost.launchCovered(): Boolean = failure == null &&
    !(snapshot?.objectOrNull("state") == null && notice?.id == null && notice != null) && snapshot?.objectOrNull("launch")?.optBoolean("visible", true) != false

internal fun CanvasHost.launchFinished(): Boolean = snapshot?.objectOrNull("launch")?.optBoolean("finished") == true && failure == null

internal fun CanvasHost.launchHolding(reveal: LaunchReveal?): Boolean {
    val visible = launchCovered()
    return reveal?.holding(visible, launchFinished()) ?: visible
}

@Composable internal fun LaunchSplash(host: CanvasHost) {
    val activity = LocalActivity.current as? MainActivity ?: return
    val window = activity.launchSplash
    val launch = host.snapshot?.objectOrNull("launch")
    val visible = host.launchCovered()
    val finished = host.launchFinished()
    val bootstrap = host.bootstrap
    val density = LocalDensity.current
    val reveal = bootstrap?.getLong("launch_reveal_ms")
    val gap = bootstrap?.getDouble("launch_caption_gap")?.toFloat() ?: 0f
    val captionDelay = bootstrap?.getLong("launch_caption_delay_ms")
    var captionVisible by remember { mutableStateOf(false) }
    LaunchedEffect(window.shownAt, captionDelay) {
        captionVisible = false
        if (captionDelay != null) {
            delay((window.shownAt + captionDelay - SystemClock.uptimeMillis()).coerceAtLeast(0))
            captionVisible = true
        }
    }
    val captionAlpha by animateFloatAsState(if (captionVisible) .5f else 0f,
        tween(if (Build.VERSION.SDK_INT >= 31) 180 else 0), label = "launch-caption")
    val caption = if (captionVisible) bootstrap?.optString("preparing_${launch?.optString("notice", "canvas") ?: "canvas"}") else null
    SideEffect { window.update(visible, finished, caption, reveal, gap * density.density) }
    val holding = host.launchHolding(window.reveal)
    val alpha by animateFloatAsState(if (holding) 1f else 0f,
        tween(if (finished) reveal?.toInt() ?: 0 else 0), label = "launch-reveal")
    val native = window.nativeActive || (window.hadNativeVisual && finished)
    if (alpha > 0f || window.nativeActive) BoxWithConstraints(Modifier.fillMaxSize().pointerInput(Unit) {
        awaitPointerEventScope { while (true) awaitPointerEvent().changes.forEach { it.consume() } }
    }) {
        if (!native && alpha > 0f) {
            val mark = window.legacyMarkSize?.let { with(density) { it.toDp() } }
                ?: if (bootstrap != null) Native.launchMarkSize(maxWidth.value, maxHeight.value).dp else 0.dp
            val foreground = Color(window.foreground)
            val captionTop = maxHeight / 2 + mark / 2 + gap.dp
            Box(Modifier.fillMaxSize().graphicsLayer { this.alpha = alpha }.background(Color(window.background))
                .testTag("launch-splash"), contentAlignment = Alignment.Center) {
                Image(painterResource(if (window.legacyMarkSize != null) R.drawable.capy_splash_mark else R.drawable.capy_full_mark),
                    null, Modifier.size(mark), colorFilter = ColorFilter.tint(foreground))
                if (!caption.isNullOrEmpty()) Box(Modifier.fillMaxSize().offset(y = captionTop),
                    contentAlignment = Alignment.TopCenter) {
                    Text(caption, color = foreground.copy(alpha = captionAlpha), fontSize = 12.sp,
                        modifier = Modifier.testTag("launch-splash-caption"))
                }
            }
        }
    }
}
