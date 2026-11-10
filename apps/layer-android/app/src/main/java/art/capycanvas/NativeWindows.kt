package art.capycanvas

import android.view.View
import androidx.compose.foundation.layout.ColumnScope
import androidx.compose.material3.MenuDefaults
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.Shape
import androidx.compose.ui.platform.LocalView
import androidx.compose.ui.unit.IntOffset
import androidx.compose.ui.window.DialogProperties
import androidx.compose.ui.window.PopupPositionProvider
import androidx.compose.ui.window.PopupProperties
import androidx.core.view.ViewCompat
import androidx.core.view.WindowCompat
import androidx.core.view.WindowInsetsCompat
import androidx.core.view.WindowInsetsControllerCompat

private fun WindowInsetsControllerCompat.enterCanvasFullscreen() {
    systemBarsBehavior = WindowInsetsControllerCompat.BEHAVIOR_SHOW_TRANSIENT_BARS_BY_SWIPE
    hide(WindowInsetsCompat.Type.systemBars())
}

internal fun android.view.Window.enterCanvasFullscreen() =
    WindowCompat.getInsetsController(this, decorView).enterCanvasFullscreen()

@Composable private fun CanvasWindow(content: @Composable () -> Unit) {
    val view = LocalView.current
    DisposableEffect(view) {
        fun hide() {
            if (android.os.Build.VERSION.SDK_INT >= 30) ViewCompat.getWindowInsetsController(view)?.enterCanvasFullscreen()
            else view.systemUiVisibility = view.systemUiVisibility or View.SYSTEM_UI_FLAG_FULLSCREEN or
                View.SYSTEM_UI_FLAG_HIDE_NAVIGATION or View.SYSTEM_UI_FLAG_IMMERSIVE_STICKY
        }
        val attached = object : View.OnAttachStateChangeListener {
            override fun onViewAttachedToWindow(view: View) = hide()
            override fun onViewDetachedFromWindow(view: View) {}
        }
        val focus = android.view.ViewTreeObserver.OnWindowFocusChangeListener { if (it) hide() }
        view.addOnAttachStateChangeListener(attached)
        view.viewTreeObserver.addOnWindowFocusChangeListener(focus)
        if (view.isAttachedToWindow) hide()
        onDispose {
            view.removeOnAttachStateChangeListener(attached)
            if (view.viewTreeObserver.isAlive) view.viewTreeObserver.removeOnWindowFocusChangeListener(focus)
        }
    }
    content()
}

@Composable internal fun DropdownMenu(expanded: Boolean, onDismissRequest: () -> Unit,
    modifier: Modifier = Modifier, properties: PopupProperties = PopupProperties(focusable = true),
    shape: Shape = MenuDefaults.shape, containerColor: Color = MenuDefaults.containerColor,
    content: @Composable ColumnScope.() -> Unit) {
    androidx.compose.material3.DropdownMenu(expanded, onDismissRequest, modifier = modifier,
        properties = properties, shape = shape, containerColor = containerColor) {
        CanvasWindow { content() }
    }
}

@Composable internal fun Popup(popupPositionProvider: PopupPositionProvider, onDismissRequest: (() -> Unit)? = null,
    properties: PopupProperties = PopupProperties(), content: @Composable () -> Unit) {
    androidx.compose.ui.window.Popup(popupPositionProvider, onDismissRequest, properties) { CanvasWindow(content) }
}

@Composable internal fun Popup(alignment: Alignment = Alignment.TopStart, offset: IntOffset = IntOffset.Zero,
    onDismissRequest: (() -> Unit)? = null, properties: PopupProperties = PopupProperties(), content: @Composable () -> Unit) {
    androidx.compose.ui.window.Popup(alignment, offset, onDismissRequest, properties) { CanvasWindow(content) }
}

@Composable internal fun Dialog(onDismissRequest: () -> Unit, properties: DialogProperties = DialogProperties(),
    content: @Composable () -> Unit) {
    androidx.compose.ui.window.Dialog(onDismissRequest, properties) { CanvasWindow(content) }
}
