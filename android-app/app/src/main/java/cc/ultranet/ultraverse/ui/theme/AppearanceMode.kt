package cc.ultranet.ultraverse.ui.theme

import android.content.Context

/** How the app resolves its colours. Persisted so a choice survives restarts. */
internal enum class AppearanceMode(val label: String) {
    System("Follow system"),
    Light("Light"),
    Dark("Dark"),
}

internal class AppearanceStore(context: Context) {
    private val preferences = context.applicationContext
        .getSharedPreferences(PREFERENCES_NAME, Context.MODE_PRIVATE)

    fun load(): AppearanceMode {
        val stored = preferences.getString(KEY_MODE, null)
        return AppearanceMode.entries.firstOrNull { it.name == stored } ?: AppearanceMode.System
    }

    fun save(mode: AppearanceMode) {
        preferences.edit().putString(KEY_MODE, mode.name).apply()
    }

    private companion object {
        const val PREFERENCES_NAME = "ultraverse.appearance"
        const val KEY_MODE = "mode"
    }
}
