package cc.ultranet.ultraverse

import android.os.Bundle
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.activity.enableEdgeToEdge
import cc.ultranet.ultraverse.ui.UltraVerseApp
import cc.ultranet.ultraverse.ui.theme.UltraVerseTheme

class MainActivity : ComponentActivity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        enableEdgeToEdge()
        setContent {
            UltraVerseTheme {
                UltraVerseApp()
            }
        }
    }
}
