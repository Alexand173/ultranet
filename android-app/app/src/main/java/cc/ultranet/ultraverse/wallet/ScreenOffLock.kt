package cc.ultranet.ultraverse.wallet

import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent
import android.content.IntentFilter
import android.os.Build

/**
 * Locks the wallet when the screen turns off.
 *
 * `ON_STOP` alone is not enough for this. The screen can go off with the activity still started, and
 * that is the exact moment the device stops being in the owner's hands — the seed should not stay in
 * memory until the activity happens to stop. The idle timer is a bound on abandonment, not a
 * substitute for this.
 *
 * One filter, one callback, and the receiver flag the platform requires from API 33 for a protected
 * system broadcast. Nothing else is observed and no other permission is asked for.
 */
internal class ScreenOffReceiver(private val onScreenOff: () -> Unit) : BroadcastReceiver() {
    override fun onReceive(context: Context?, intent: Intent?) {
        if (intent?.action == Intent.ACTION_SCREEN_OFF) onScreenOff()
    }
}

internal object ScreenOffLock {
    fun register(context: Context, onScreenOff: () -> Unit): BroadcastReceiver {
        val receiver = ScreenOffReceiver(onScreenOff)
        val filter = IntentFilter(Intent.ACTION_SCREEN_OFF)
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU) {
            // A protected system broadcast, so it is registered not-exported: nothing else can reach it.
            context.registerReceiver(receiver, filter, Context.RECEIVER_NOT_EXPORTED)
        } else {
            @Suppress("UnspecifiedRegisterReceiverFlag")
            context.registerReceiver(receiver, filter)
        }
        return receiver
    }

    fun unregister(context: Context, receiver: BroadcastReceiver) {
        // A receiver that was already torn down by the platform must not crash the dispose path.
        runCatching { context.unregisterReceiver(receiver) }
    }
}
