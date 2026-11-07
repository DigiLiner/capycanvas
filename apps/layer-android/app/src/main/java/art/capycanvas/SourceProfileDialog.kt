package art.capycanvas

import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.foundation.layout.*
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.unit.dp
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import org.json.JSONObject
import org.json.JSONArray

@Composable internal fun ProfileFileButton(host:CanvasHost,label:String?=null,onBusy: (Boolean) -> Unit = {}, enabled: Boolean = true, onProfile: (JSONObject) -> Unit) {
    val copy = host.catalog.getJSONObject("profile_copy")
    val context = LocalContext.current
    val scope = rememberCoroutineScope()
    var error by remember { mutableStateOf<Exception?>(null) }
    var busy by remember { mutableStateOf(false) }
    LaunchedEffect(busy) { onBusy(busy) }
    val picker = rememberLauncherForActivityResult(ActivityResultContracts.OpenDocument()) { uri ->
        if (uri != null) scope.launch {
            busy = true
            try {
                val profile = withContext(Dispatchers.IO) {
                    context.contentResolver.openInputStream(uri)?.use { input ->
                        val bytes = ByteArray(copy.getInt("read_bytes") + 1); var length = 0
                        while (length < bytes.size) { val n = input.read(bytes, length, bytes.size-length); if (n < 0) break; length += n }
                        if (length == bytes.size) throw ColorFeatureFailure("ProfileReadLimit")
                        ProfileStore.import(context,bytes.copyOf(length))
                    } ?: error(host.bootstrap!!.getString("action_failed"))
                }
                onProfile(profile); error = null
            } catch (e: Exception) { error = e }
            finally { busy = false }
        }
    }
    Column {
        TextButton(enabled = enabled && !busy, onClick = { picker.launch(arrayOf("*/*")) }) { Text(if (busy) copy.getString("reading") else label ?: copy.getString("import")) }
        ColorFailureText(host, error, true)
    }
}

@Composable internal fun SourceProfileDialog(host:CanvasHost,interpretation: JSONObject, onChoose: (JSONObject?) -> Unit) {
    val copy = host.catalog.getJSONObject("profile_copy")
    var selected by remember(interpretation) { mutableStateOf<JSONObject?>(null) }
    var originalName by remember(interpretation) { mutableStateOf(copy.getString("choose")) }
    var rawName by remember(interpretation) { mutableStateOf<String?>(null) }
    var failure by remember(interpretation) { mutableStateOf<Exception?>(null) }
    val original = interpretation.getJSONObject("profile")
    LaunchedEffect(interpretation) {
        try {
            val raw = withContext(Dispatchers.IO) { Native.profileLibrary(obj("type" to "describe", "profile" to original).toString(),byteArrayOf()) }
            rawName = org.json.JSONTokener(raw).nextValue() as? String
        } catch(e:CancellationException){throw e} catch(e:Exception){failure=e}
    }
    LaunchedEffect(rawName,host.languageTag) { originalName = profileNameCopy(host,rawName) }
    val cancel={onChoose(null)}
    val choose={onChoose(selected?.getJSONObject("profile") ?: original)}
    DialogShell(host,copy.getJSONObject("dialog"),copy.getString("interpret_title"),onCancel=cancel,onPrimary=choose,
        primaryLabel=copy.getString("use_profile"),primaryEnabled=failure==null || selected!=null,tag="source-profile",primaryTag="source-profile-use",
        responses=listOf(DialogResponse(copy.getJSONObject("common").getString("cancel"),"source-profile-cancel",action=cancel),
            DialogResponse(copy.getString("use_profile"),"source-profile-use",accent=true,enabled=failure==null || selected!=null,action=choose)),
        body={padding -> Column(Modifier.padding(padding),verticalArrangement=Arrangement.spacedBy(8.dp)) {
            Text(copy.getString("interpret_help"))
            SourceProfileChoice(host,copy.getString("interpret_as"),obj("kind" to "source","channels" to interpretation.getString("channels")),
                originalName,original,selected) { selected=it }
            ColorFailureText(host,failure,true)
        } })
}

@Composable internal fun SourceProfileChoice(host:CanvasHost,label:String,purpose:JSONObject,originalName:String,original:JSONObject,
    selected:JSONObject?,enabled:Boolean=true,onChoose:(JSONObject?)->Unit) {
    var open by remember {mutableStateOf(false)}
    var library by remember {mutableStateOf(false)}
    val initial=remember(originalName,original){obj("name" to originalName,"profile" to original)}
    val current=selected ?: initial
    val accept by rememberUpdatedState<(JSONObject?)->Unit>({if(enabled)onChoose(it)})
    Column {
        Text(label,style=MaterialTheme.typography.labelMedium)
        Box {
            OutlinedButton({open=true},enabled=enabled,modifier=Modifier.fillMaxWidth().testTag("color-choice-$label")){Text(if(selected==null)originalName else profileCaption(host,selected))}
            DropdownMenu(open&&enabled,{open=false}) {
                ProfileMenu(host,purpose,null,current,{accept(it);open=false},{open=false;library=true})
            }
        }
    }
    if(library)ProfileLibraryDialog(host,{library=false}){accept(it);library=false}
}

@Composable internal fun ProfileLibraryDialog(host:CanvasHost,onDismiss:()->Unit,onProfile:((JSONObject)->Unit)?=null) {
    val copy = host.catalog.getJSONObject("profile_copy")
    val context=LocalContext.current
    val scope=rememberCoroutineScope()
    var entries by remember {mutableStateOf<List<JSONObject>>(emptyList())}
    var rawEntries by remember {mutableStateOf<List<JSONObject>>(emptyList())}
    var error by remember {mutableStateOf<Exception?>(null)}
    var busy by remember {mutableStateOf(true)}
    var importBusy by remember { mutableStateOf(false) }
    var refresh by remember {mutableIntStateOf(0)}
    var query by remember { mutableStateOf("") }
    val matching = remember(rawEntries, query, host.languageTag) {
        JSONArray(Native.profileMatches(JSONArray(rawEntries.map(::profileMetadata)).toString(), query, host.languageTag)).values().toSet()
    }
    LaunchedEffect(refresh){busy=true;try{rawEntries=ProfileStore.list(context);error=null}catch(e:Exception){error=e}finally{busy=false}}
    LaunchedEffect(rawEntries, host.languageTag) {
        val language=host.languageTag
        val retained=rawEntries
        val projected=profileEntriesCopy(host,retained)
        if(language==host.languageTag && retained===rawEntries)entries=projected
    }
    val close={if(!busy&&!importBusy)onDismiss()}
    val phone=host.snapshot?.getJSONObject("layout")?.objectOrNull("phone")!=null
    DialogShell(host,copy.getJSONObject("library_dialog"),copy.getString("library_title"),onCancel=close,primaryEnabled=!busy&&!importBusy,
        naturalWidth=copy.getJSONArray("library_size").getInt(0).dp,tag="profile-library",
        responses=listOf(DialogResponse(copy.getJSONObject("common").getString("done"),"profile-library-done",enabled=!busy&&!importBusy,action=close)),body={padding ->
        Column(Modifier.fillMaxWidth().padding(padding),verticalArrangement=Arrangement.spacedBy(8.dp)) {
            Text(copy.getString("library_help"))
            if (entries.size > 7 || query.isNotEmpty()) CoreTextField(query, { query = it }, Modifier.testTag("profile-library-search"), placeholder = { Text(copy.getString("search")) }, background = LocalPalette.current.dialogField)
            if(busy)CircularProgressIndicator()
            for(entry in entries.filter { it.getString("id") in matching }){
                Text(entry.getString("name"),style=MaterialTheme.typography.titleSmall)
                Text(entry.optString("issue").ifEmpty { entry.getString("details") })
                val remove={busy=true;scope.launch{try{ProfileStore.remove(context,entry.getString("id"));refresh++}catch(e:Exception){error=e}finally{busy=false}};Unit}
                val show={busy=true;scope.launch{try{ProfileStore.show(context,entry.getString("id"),!entry.optBoolean("visible",true));refresh++}catch(e:Exception){error=e}finally{busy=false}};Unit}
                Row {
                    if(onProfile!=null)TextButton(enabled=!busy&&!importBusy&&!entry.has("issue"),onClick={busy=true;scope.launch{try{onProfile(ProfileStore.get(context,entry.getString("id")))}catch(e:Exception){error=e}finally{busy=false}}}){Text(copy.getString("use_profile"))}
                    if(phone)Box {
                        var menu by remember(entry.getString("id")){mutableStateOf(false)}
                        IconButton({menu=true},enabled=!busy&&!importBusy){SharedIcon("more",copy.getString("library_title"))}
                        DropdownMenu(menu,{menu=false}) {
                            DropdownMenuItem(text={Text(copy.getString("remove"))},onClick={menu=false;remove()})
                            DropdownMenuItem(text={Text(if(entry.optBoolean("visible",true))copy.getString("hide") else copy.getString("show"))},onClick={menu=false;show()})
                        }
                    } else TextButton(remove,enabled=!busy&&!importBusy){Text(copy.getString("remove"))}
                }
                if(!phone)TextButton(show,enabled=!busy&&!importBusy){Text(if(entry.optBoolean("visible",true))copy.getString("hide") else copy.getString("show"))}
            }
            if(entries.isEmpty()&&!busy)Text(copy.getString("empty"))
            ProfileFileButton(host, onBusy = { importBusy = it }, enabled = !busy) {if(onProfile!=null)onProfile(it)else refresh++}
            ColorFailureText(host, error, true)
        }
    })
}
