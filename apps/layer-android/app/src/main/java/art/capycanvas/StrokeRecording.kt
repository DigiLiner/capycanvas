package art.capycanvas

import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.runtime.*
import androidx.compose.ui.platform.LocalContext
import androidx.lifecycle.viewModelScope
import kotlinx.coroutines.*
import org.json.JSONObject
import java.util.zip.GZIPOutputStream

internal class StrokeRecording(private val host: CanvasHost) {
    var status by mutableStateOf<JSONObject?>(null)
        private set
    var busy by mutableStateOf(false)
    var saveRequested by mutableStateOf(false)

    fun reportFailure(error:Exception,message:String=host.bootstrap!!.getString("action_failed")) {
        android.util.Log.e("CapyCanvas.StrokeRecording","Stroke recording failed",error)
        host.reportActionError(message,host.catalog.getJSONObject("native_copy").getJSONObject("color").getString("stroke_recording"))
    }
    fun reportSaveFailure(error:Exception)=reportFailure(error,host.catalog.getJSONObject("native_copy").getJSONObject("color").getString("stroke_recording_save_failed"))

    suspend fun update(action: String? = null) {
        val next = host.withNative { JSONObject(Native.query(it, obj("type" to "stroke_recording", "action" to action).toString())) }
        if (status?.optBoolean("recording") == true && next.optBoolean("ready")) saveRequested = true
        status = next
    }
    fun click() {
        if (busy) return
        busy = true
        host.viewModelScope.launch {
            try {
                when {
                    status?.optBoolean("recording") == true -> update("stop")
                    status?.optBoolean("ready") == true -> saveRequested = true
                    else -> update("start")
                }
            } catch(e:CancellationException){throw e}
            catch (e: Exception) { reportFailure(e) }
            finally { busy = false }
        }
    }
}

@Composable internal fun StrokeRecordingSave(host: CanvasHost) {
    val recording = host.strokeRecording
    val resolver = LocalContext.current.contentResolver
    val launcher = rememberLauncherForActivityResult(ActivityResultContracts.CreateDocument("application/octet-stream")) { uri ->
        if (uri == null) recording.busy = false
        else host.viewModelScope.launch {
            try {
                val raw = host.withNative { Native.strokeRecordingData(it) }
                val header = recording.status!!.getString("header")
                withContext(Dispatchers.IO) {
                    val stream = resolver.openOutputStream(uri, "wt") ?: throw java.io.IOException()
                    stream.use { output ->
                        output.write(header.toByteArray(Charsets.US_ASCII))
                        GZIPOutputStream(output).use { it.write(raw) }
                    }
                }
                recording.update("saved")
            } catch(e:CancellationException){throw e}
            catch (e: Exception) { recording.reportSaveFailure(e) }
            finally { recording.busy = false }
        }
    }
    LaunchedEffect(host) {
        while (isActive) {
            if (host.snapshot != null && (recording.status == null || recording.status?.optBoolean("recording") == true)) {
                try { recording.update() }
                catch (e: CancellationException) { throw e }
                catch (e: Exception) { recording.reportFailure(e); break }
            }
            delay(200)
        }
    }
    LaunchedEffect(recording.saveRequested) {
        if (recording.saveRequested) {
            recording.saveRequested = false
            recording.busy = true
            try { launcher.launch("stroke-recording.capystrokes") }
            catch (e: Exception) { recording.busy = false; recording.reportSaveFailure(e) }
        }
    }
}
