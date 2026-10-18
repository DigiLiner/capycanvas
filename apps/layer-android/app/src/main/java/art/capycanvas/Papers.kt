package art.capycanvas

import android.app.Application
import android.net.Uri
import android.provider.OpenableColumns
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.runtime.*
import androidx.lifecycle.viewModelScope
import kotlinx.coroutines.*
import org.json.JSONObject

@Composable internal fun PaperRequests(host: CanvasHost, state: JSONObject) {
    val ready = host.snapshot?.optBoolean("canvas_ready") == true
    LaunchedEffect(ready) { if (ready) host.viewModelScope.launch { runPaper(host, -1, null) } }
    val request = state.array("requests").objects().firstOrNull { it.getJSONObject("kind").getString("type") == "paper" }
    var chosen by remember { mutableStateOf<CompletableDeferred<Uri?>?>(null) }
    val launcher = rememberLauncherForActivityResult(ActivityResultContracts.OpenDocument()) { uri -> chosen?.complete(uri); chosen = null }
    LaunchedEffect(request?.getInt("id")) {
        val id = request?.getInt("id") ?: return@LaunchedEffect
        val imports = request.getJSONObject("kind").getJSONObject("request").getString("type") == "import"
        val file = CompletableDeferred<Uri?>()
        if (imports) {
            chosen = file
            try { launcher.launch(arrayOf("image/*")) } catch (e: Exception) { file.complete(null); host.reportActionError(e.message ?: host.bootstrap!!.getString("action_failed")) }
        } else file.complete(null)
        host.viewModelScope.launch {
            val uri = file.await()
            if (imports && uri == null) host.dispatch(obj("type" to "complete_request", "id" to id, "error" to null))
            else runPaper(host, id, uri)
        }
    }
}

private suspend fun runPaper(host: CanvasHost, id: Int, uri: Uri?) {
    val failed = { e: Exception -> e.message ?: host.bootstrap!!.getString("action_failed") }
    val task = try { host.withNative { Native.paperTask(it, id, host.storage.papers.path) } }
        catch (e: Exception) { if (id >= 0) host.dispatch(obj("type" to "complete_request", "id" to id, "error" to failed(e))); return }
    try {
        withContext(Dispatchers.IO) {
            val resolver = host.getApplication<Application>().contentResolver
            val name = uri?.let { resolver.query(it, arrayOf(OpenableColumns.DISPLAY_NAME), null, null, null)?.use { c -> if (c.moveToFirst()) c.getString(0) else null } }
            val descriptor = uri?.let { resolver.openFileDescriptor(it, "r") ?: error(host.bootstrap!!.getString("action_failed")) }
            Native.paperWork(task, descriptor?.detachFd() ?: -1, name?.substringBeforeLast('.') ?: "")
        }
        host.withNative { Native.paperApply(it, task) }
        host.documentChanged()
    } catch (e: CancellationException) {
        Native.paperCancel(task); throw e
    } catch (e: Exception) {
        if (id >= 0) host.dispatch(obj("type" to "complete_request", "id" to id, "error" to failed(e))) else host.reportActionError(failed(e))
    } finally {
        withContext(NonCancellable + Dispatchers.IO) { Native.paperFree(task) }
    }
}
