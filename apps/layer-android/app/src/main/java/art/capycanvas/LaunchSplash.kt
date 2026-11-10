package art.capycanvas

import android.os.Build
import android.os.SystemClock
import android.view.Gravity
import android.widget.FrameLayout
import android.widget.ImageView
import android.widget.TextView
import android.window.SplashScreenView
import androidx.activity.compose.LocalActivity
import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.animation.core.tween
import androidx.compose.foundation.Image
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Box
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
import androidx.compose.ui.graphics.ColorFilter
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.graphics.toArgb
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.colorResource
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import kotlinx.coroutines.delay

internal class LaunchSplashWindow(private val activity: MainActivity) {
    val shownAt = SystemClock.uptimeMillis()
    private var view: android.view.View? = null
    private var caption: TextView? = null
    private var mark: ImageView? = null
    private var visible = true
    private var finished = false
    private var fading = false
    private var background = activity.getColor(R.color.canvas_launch_background)
    private var foreground = activity.getColor(R.color.capy_splash_foreground)
    private var text: String? = null

    fun install() {
        if (Build.VERSION.SDK_INT >= 31) activity.splashScreen.setOnExitAnimationListener { splash ->
            view = splash
            splash.iconView?.let { (it.parent as? android.view.ViewGroup)?.removeView(it) }
            mark = ImageView(activity).apply {
                setImageResource(R.drawable.capy_splash_mark)
                splash.addView(this, FrameLayout.LayoutParams(dp(96), dp(96), Gravity.CENTER))
            }
            caption = TextView(activity).apply {
                textSize = 12f
                gravity = Gravity.CENTER
                translationY = dp(80).toFloat()
                alpha = 0f
                splash.addView(this, FrameLayout.LayoutParams(FrameLayout.LayoutParams.MATCH_PARENT,
                    FrameLayout.LayoutParams.WRAP_CONTENT, Gravity.CENTER))
            }
            refresh()
        }
    }
    private fun dp(value: Int) = (value * activity.resources.displayMetrics.density).toInt()
    fun update(visible: Boolean, finished: Boolean, text: String?, background: Int, foreground: Int) {
        this.visible = visible
        this.finished = finished
        this.text = text
        this.background = background
        this.foreground = foreground
        refresh()
    }
    private fun refresh() {
        if (Build.VERSION.SDK_INT < 31) return
        val splash = view as? SplashScreenView ?: return
        splash.setBackgroundColor(background)
        mark?.setColorFilter(foreground)
        caption?.apply {
            val showing = text.isNotEmpty()
            text = this@LaunchSplashWindow.text.orEmpty()
            setTextColor(this@LaunchSplashWindow.foreground)
            if (text.isNotEmpty() && !showing) animate().alpha(.5f).setDuration(180).start()
            else if (text.isEmpty()) alpha = 0f
        }
        if (!visible) {
            if (!finished) {
                splash.animate().cancel()
                view = null
                fading = false
                splash.remove()
            } else if (!fading) {
                fading = true
                splash.animate().alpha(0f).setDuration(180).withEndAction {
                    if (view === splash) view = null
                    fading = false
                    splash.remove()
                }.start()
            }
        }
    }
    fun close() {
        if (Build.VERSION.SDK_INT >= 31) (view as? SplashScreenView)?.let { it.animate().cancel(); it.remove() }
        view = null
        caption = null
        mark = null
    }
}

internal fun CanvasHost.launchCovered(): Boolean = failure == null &&
    !(snapshot?.objectOrNull("state") == null && notice?.id == null && notice != null) && snapshot?.objectOrNull("launch")?.optBoolean("visible", true) != false

@Composable internal fun LaunchSplash(host: CanvasHost) {
    val activity = LocalActivity.current as? MainActivity
    val launch = host.snapshot?.objectOrNull("launch")
    val visible = host.launchCovered()
    val finished = launch?.optBoolean("finished") == true && host.failure == null
    val state = host.snapshot?.objectOrNull("state")
    val palette = state?.let { Palette(it.getString("theme") != "light", it.getJSONObject("palette")) }
    val surround = palette?.surround ?: colorResource(R.color.canvas_launch_background)
    val foreground = palette?.text ?: colorResource(R.color.capy_splash_foreground)
    val shownAt = remember { activity?.launchSplash?.shownAt ?: SystemClock.uptimeMillis() }
    val captionDelay = host.bootstrap?.getLong("launch_caption_delay_ms")
    var captionVisible by remember { mutableStateOf(false) }
    LaunchedEffect(shownAt, captionDelay) {
        captionVisible = false
        if (captionDelay != null) {
            delay((shownAt + captionDelay - SystemClock.uptimeMillis()).coerceAtLeast(0))
            captionVisible = true
        }
    }
    val captionAlpha by animateFloatAsState(if (captionVisible) .5f else 0f,
        tween(if (Build.VERSION.SDK_INT >= 31) 180 else 0), label = "launch-caption")
    val caption = if (captionVisible) host.bootstrap?.optString("preparing_${launch?.optString("notice", "canvas") ?: "canvas"}") else null
    SideEffect { activity?.launchSplash?.update(visible, finished, caption, surround.toArgb(), foreground.toArgb()) }
    val alpha by animateFloatAsState(if (visible) 1f else 0f, tween(if (finished) 180 else 0), label = "launch-reveal")
    if (alpha > 0f) Box(Modifier.fillMaxSize().graphicsLayer { this.alpha = alpha }.background(surround)
        .testTag("launch-splash").pointerInput(Unit) {
            awaitPointerEventScope { while (true) awaitPointerEvent().changes.forEach { it.consume() } }
        }, contentAlignment = Alignment.Center) {
        Image(painterResource(R.drawable.capy_splash_mark), null, Modifier.size(96.dp), colorFilter = ColorFilter.tint(foreground))
        if (!caption.isNullOrEmpty()) Text(caption, color = foreground.copy(alpha = captionAlpha), fontSize = 12.sp,
            modifier = Modifier.offset(y = 80.dp).testTag("launch-splash-caption"))
    }
}
