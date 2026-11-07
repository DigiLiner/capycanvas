package art.capycanvas

import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.*
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.unit.dp
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import org.json.JSONArray
import org.json.JSONObject

private data class ProfileMenuData(val entries:List<JSONObject>,val captions:List<JSONObject>,val standards:List<JSONObject>,val inventory:JSONObject)

@Composable internal fun ProfileMenu(host:CanvasHost,purpose:JSONObject,document:JSONObject?,current:JSONObject?,
    onChoose:(JSONObject)->Unit,onLibrary:()->Unit) {
    val context=LocalContext.current
    val scope=rememberCoroutineScope()
    val copy=host.catalog.getJSONObject("profile_copy")
    var entries by remember {mutableStateOf<List<JSONObject>?>(null)}
    var menu by remember {mutableStateOf<ProfileMenuData?>(null)}
    var error by remember {mutableStateOf<Exception?>(null)}
    var busy by remember {mutableStateOf(false)}
    val documentProfile=remember(document){document?.getJSONObject("profile")?.toString()}
    val currentProfile=remember(current){current?.getJSONObject("profile")?.toString()}
    LaunchedEffect(Unit) {
        try {entries=ProfileStore.list(context)} catch(e:CancellationException){throw e} catch(e:Exception){error=e}
    }
    LaunchedEffect(entries,purpose.toString(),documentProfile,currentProfile,host.languageTag) {
        val raw=entries ?: return@LaunchedEffect
        menu=null
        try {
            val (inventory,standards)=withContext(Dispatchers.IO) {
                val saved=JSONArray(raw.map{obj("id" to it.getString("id"),"visible" to it.optBoolean("visible",true),
                    "channels" to it.opt("channels"),"valid" to !it.has("issue"))})
                val view=JSONObject(Native.profileLibrary(obj("type" to "picker","purpose" to purpose,
                    "document" to documentProfile?.let(::JSONObject),"current" to currentProfile?.let(::JSONObject),"saved" to saved).toString(),byteArrayOf()))
                val standard=JSONArray(Native.profileLibrary(obj("type" to "standard","purpose" to purpose).toString(),byteArrayOf())).objects()
                view to standard
            }
            menu=ProfileMenuData(raw,profileEntriesCopy(host,raw),standards,inventory)
            error=null
        } catch(e:CancellationException){throw e} catch(e:Exception){error=e}
    }
    Column(Modifier.fillMaxWidth()) {
        val data=menu
        if(data==null && error==null)Text(copy.getString("loading"))
        @Composable fun heading(key:String) {Text(copy.getString(key),Modifier.padding(horizontal=12.dp).padding(top=4.dp).testTag("profile-menu-$key"),style=MaterialTheme.typography.labelMedium)}
        @Composable fun item(profile:JSONObject,tag:String) {
            DropdownMenuItem(text={Text(profileCaption(host,profile))},modifier=Modifier.testTag(tag),enabled=!busy,onClick={onChoose(profile)})
        }
        if(data!=null) {
            if(data.inventory.getBoolean("document")){heading("document");item(document!!,"profile-menu-document-item")}
            if(data.inventory.getBoolean("current")){heading("current");item(current!!,"profile-menu-current-item")}
            val saved=data.inventory.getJSONArray("saved")
            if(saved.length()>0) {
                heading("saved")
                saved.values().forEach { value ->
                    val index=(value as Number).toInt();val entry=data.entries[index]
                    DropdownMenuItem(text={Text(data.captions[index].getString("name"))},modifier=Modifier.testTag("profile-menu-saved-$index"),enabled=!busy,
                        onClick={busy=true;scope.launch{try{onChoose(ProfileStore.get(context,entry.getString("id")))}catch(e:CancellationException){throw e}catch(e:Exception){error=e}finally{busy=false}}})
                }
            }
            if(data.inventory.getBoolean("standard")){heading("standard");data.standards.forEachIndexed{i,profile->item(profile,"profile-menu-standard-$i")}}
        }
        ProfileFileButton(host,onBusy={busy=it},enabled=!busy,onProfile=onChoose)
        TextButton(onLibrary,enabled=!busy){Text(copy.getString("saved_dialog"))}
        ColorFailureText(host,error,true)
    }
}
