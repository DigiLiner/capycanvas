package art.capycanvas

import androidx.compose.foundation.layout.*
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.unit.dp
import org.json.JSONObject

@Composable internal fun SourceEditDialog(host: CanvasHost, request: JSONObject) {
    val id=request.getInt("id")
    val rasterize=request.getJSONObject("kind").getJSONObject("request").getString("type")=="rasterize_source"
    val job=remember(id) {DocumentColorJob(host,id,source=true)}
    var selected by remember(id) {mutableStateOf<JSONObject?>(null)}
    var context by remember(id) {mutableStateOf<JSONObject?>(null)}
    var details by remember(id) {mutableStateOf("")}
    LaunchedEffect(id) {
        try {
            val current=JSONObject(host.withNative {Native.query(it,obj("type" to "source_dialog_context","id" to id).toString())})
            job.describeSource(current);context=current
        } catch(e:Exception){job.error=e}
    }
    LaunchedEffect(id,host.languageTag) {
        job.refreshCopy()
        if(rasterize)details=org.json.JSONTokener(host.withNative {Native.query(it,obj("type" to "rasterize_details").toString())}).nextValue() as String
    }
    val choice=selected?.getJSONObject("profile")
    LaunchedEffect(context,choice?.toString()) {if(context!=null)job.prepare(choice)}
    DisposableEffect(job) {onDispose {job.close()}}
    val copy=host.catalog.getJSONObject("document_color_copy")
    val common=copy.getJSONObject("common")
    val baked=context?.getBoolean("baked")==true
    val label=copy.getString(if(rasterize)"rasterize" else if(baked)"add_source" else "apply_profile")
    if(job.applying) {
        ProgressDialog(host,copy.getString(if(rasterize)"rasterizing" else "applying"),"source-edit",onCancel=job::close)
        return
    }
    DialogShell(host,copy.getJSONObject("dialog"),copy.getString(if(rasterize)"rasterize_title" else "repair_title"),
        onCancel=job::close,onPrimary=job::apply,primaryLabel=label,primaryEnabled=job.ready&&!job.busy,tag="source-edit",primaryTag="source-edit-apply",
        responses=listOf(DialogResponse(common.getString("cancel"),"source-edit-cancel",action=job::close),
            DialogResponse(label,"source-edit-apply",accent=true,enabled=job.ready&&!job.busy,action=job::apply)),
        body={padding -> Column(Modifier.fillMaxWidth().padding(padding),verticalArrangement=Arrangement.spacedBy(8.dp)) {
            Text(if(rasterize)details else copy.getString(if(baked)"source_baked_help" else "source_native_help"))
            if(!rasterize) {
                Text(copy.getString("current_source"));Text(job.sourceDescription,Modifier.testTag("source-current-description"))
                context?.let { current -> SourceProfileChoice(host,copy.getString("correct_profile"),current.getJSONObject("purpose"),
                    job.sourceProfile,current.nullableString("profile_builtin"),selected,enabled=!job.applying){if(choice?.toString()!=it?.getJSONObject("profile")?.toString()){selected=it;job.invalidate()}} }
            }
            if(job.busy)CircularProgressIndicator()
            if(rasterize)Text(copy.getString(if(job.busy)"rasterize_comparison" else if(job.clipped>0)"source_clipped" else "rasterize_compare"))
            ColorComparison(job.previews,copy)
            ColorFailureText(host,job.error)
        }})
}
