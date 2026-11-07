package art.capycanvas

import androidx.compose.foundation.layout.*
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag

@Composable internal fun ColorChoice(label: String, choices: List<Pair<String, String>>, value: String, enabled: Boolean = true, onChange: (String) -> Unit) {
    var open by remember { mutableStateOf(false) }
    Column {
        Text(label, style = MaterialTheme.typography.labelMedium)
        Box {
            OutlinedButton({ open = true }, enabled = enabled, modifier = Modifier.fillMaxWidth().testTag("color-choice-$label")) {
                Text(choices.firstOrNull { it.first == value }?.second ?: value)
            }
            DropdownMenu(open && enabled, { open = false }) {
                for ((id, title) in choices) DropdownMenuItem(text = { Text(title) }, onClick = { open = false; onChange(id) })
            }
        }
    }
}
