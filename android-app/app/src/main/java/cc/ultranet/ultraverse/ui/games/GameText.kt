package cc.ultranet.ultraverse.ui.games

import cc.ultranet.ultraverse.data.GamesResult
import cc.ultranet.ultraverse.data.StoreGame
import java.security.SecureRandom

/** A remote read in one of three honest states. Nothing here shows stale or invented data as if it were fresh. */
internal sealed interface Load<out T> {
    data object Loading : Load<Nothing>
    data class Failed(val message: String) : Load<Nothing>
    data class Ready<T>(val value: T) : Load<T>
}

internal fun <T> GamesResult<T>.toLoad(): Load<T> = when (this) {
    is GamesResult.Ok -> Load.Ready(value)
    is GamesResult.Refused -> Load.Failed(message)
    is GamesResult.Unavailable -> Load.Failed(message)
}

/** Why a game cannot be played, in words. The service sends a code; the player reads this. */
internal fun playableReasonText(code: String?): String = when (code) {
    null -> "Ready to play."
    "game_not_active" -> "This game is not active right now."
    "record_only_no_approved_adapter" -> "Results are recorded only: no approved verifier exists for this game yet."
    "adapter_module_not_installed" -> "The platform has not installed this game's verifier yet."
    else -> "This game cannot be played right now."
}

/** Why no reward is on offer for a game that can be played. */
internal fun rewardsReasonText(code: String?): String = when (code) {
    null -> "A reward is on offer."
    "not_playable" -> "Rewards need a playable game."
    "rewards_not_enabled" -> "Rewards are not switched on yet."
    "no_approved_reward_policy" -> "This game has no approved reward policy."
    "pool_cannot_cover_the_reward" -> "The game's reward pool cannot cover a reward right now."
    else -> "No reward is on offer right now."
}

/** Why a verified result earned nothing. */
internal fun notRewardedText(code: String?): String? = when (code) {
    null -> null
    "policy_does_not_pay_this_outcome" -> "Verified, but this outcome does not earn a reward."
    "daily_limit_reached" -> "Verified, but a daily reward limit was reached."
    "rewards_not_enabled" -> "Verified, but rewards are not switched on yet."
    "no_approved_reward_policy" -> "Verified, but this game has no approved reward policy."
    "pool_cannot_cover_the_reward" -> "Verified, but the game's reward pool cannot cover the reward."
    else -> "Verified, but no reward was reserved."
}

/** The one line a player reads about a result. Only `reward_paid` says paid. */
internal fun stageLabel(stage: String): String = when (stage) {
    "verified" -> "Verified, no reward"
    "reward_reserved" -> "Reward reserved, not paid yet"
    "reward_paid" -> "Paid, confirmed by the network"
    "reward_released" -> "Reward released, not paid"
    else -> "Unknown state"
}

internal fun payoutStateText(state: String?): String? = when (state) {
    null -> null
    "queued" -> "Payment queued"
    "signed" -> "Payment signed, waiting to be sent"
    "submitted" -> "Payment sent, waiting for the network to confirm"
    "confirmed" -> "Payment confirmed by the network"
    "failed" -> "Payment could not be made"
    else -> state
}

internal enum class StoreFilter(val label: String) {
    All("All"),
    Playable("Playable"),
    Rewarded("Rewards"),
    Mine("My chains"),
}

/** Search is by name or by id; the chain filter is applied by the service. Identity is the id, never the name. */
internal fun filterStore(games: List<StoreGame>, filter: StoreFilter, wallet: String?): List<StoreGame> = games.filter { game ->
    when (filter) {
        StoreFilter.All -> true
        StoreFilter.Playable -> game.playable
        StoreFilter.Rewarded -> game.rewards == "available"
        StoreFilter.Mine -> wallet != null && game.owner == wallet
    }
}

/** An input for the game, as hex. Even length, at most 4,096 bytes (the host's limit). Null means acceptable. */
internal fun payloadHexError(text: String): String? = when {
    text.isEmpty() -> "Enter the move or answer as hexadecimal, for example 01."
    text.length % 2 != 0 -> "Hexadecimal comes in pairs of characters."
    text.length > 8_192 -> "That is longer than a game input may be (4,096 bytes)."
    !text.all { it in '0'..'9' || it in 'a'..'f' || it in 'A'..'F' } -> "Use only 0 to 9 and a to f."
    else -> null
}

/** ULTRA with up to six decimals into base units; null when it is not a positive amount that fits. */
internal fun parseUltraUnits(text: String): Long? {
    val trimmed = text.trim()
    if (!Regex("^[0-9]{1,12}(\\.[0-9]{1,6})?$").matches(trimmed)) return null
    val whole = trimmed.substringBefore('.').toLong()
    val fraction = trimmed.substringAfter('.', "").padEnd(6, '0').toLong()
    val units = whole * 1_000_000L + fraction
    return units.takeIf { it > 0 }
}

private val random = SecureRandom()

/** A fresh id for one play. The service treats the same id as the same result, so a retry never pays twice. */
internal fun newResultId(): String = "app-" + ByteArray(8).also(random::nextBytes).joinToString("") { "%02x".format(it) }
