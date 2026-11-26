package cc.ultranet.ultraverse

import cc.ultranet.ultraverse.data.StoreGame
import cc.ultranet.ultraverse.ui.games.StoreFilter
import cc.ultranet.ultraverse.ui.games.filterStore
import cc.ultranet.ultraverse.ui.games.newResultId
import cc.ultranet.ultraverse.ui.games.notRewardedText
import cc.ultranet.ultraverse.ui.games.parseUltraUnits
import cc.ultranet.ultraverse.ui.games.payloadHexError
import cc.ultranet.ultraverse.ui.games.payoutStateText
import cc.ultranet.ultraverse.ui.games.playableReasonText
import cc.ultranet.ultraverse.ui.games.rewardsReasonText
import cc.ultranet.ultraverse.ui.games.stageLabel
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotEquals
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

/** The words and rules behind the Games Store and Awards screens: what is called verified, reserved or paid. */
class GameTextTest {
    private val me = "ab".repeat(32)
    private val other = "cd".repeat(32)

    private fun game(id: String, owner: String?, playable: Boolean, rewards: String) = StoreGame(
        gameId = id, chainId = 7, name = "Same Name", description = "", status = "active", validationScheme = "verified_adapter", adapterHash = null, owner = owner,
        playable = playable, playableReason = if (playable) null else "record_only_no_approved_adapter",
        rewards = rewards, rewardUnits = null, rewardsReason = null,
    )

    @Test
    fun onlyTheNetworkConfirmedStageSaysPaid() {
        val labels = listOf("verified", "reward_reserved", "reward_paid", "reward_released").associateWith(::stageLabel)
        assertTrue(labels.getValue("reward_paid").startsWith("Paid"))
        listOf("verified", "reward_reserved", "reward_released").forEach { assertFalse(labels.getValue(it), labels.getValue(it).startsWith("Paid")) }
        assertEquals("an unknown stage is never shown as paid", "Unknown State", stageLabel("something_else"))
        assertFalse(payoutStateText("submitted")!!.contains("confirmed by the network"))
        assertTrue(payoutStateText("confirmed")!!.contains("confirmed"))
        assertNull(payoutStateText(null))
    }

    @Test
    fun everyReasonTheServiceCanGiveHasWordsAndNoneIsEmpty() {
        listOf("game_not_active", "record_only_no_approved_adapter", "adapter_module_not_installed", "unexpected").forEach {
            assertTrue(playableReasonText(it).isNotBlank())
        }
        listOf("not_playable", "rewards_not_enabled", "no_approved_reward_policy", "pool_cannot_cover_the_reward", "unexpected").forEach {
            assertTrue(rewardsReasonText(it).isNotBlank())
        }
        assertNull(notRewardedText(null))
        listOf("policy_does_not_pay_this_outcome", "daily_limit_reached", "rewards_not_enabled", "unexpected").forEach {
            assertTrue(notRewardedText(it)!!.startsWith("Verified"))
        }
    }

    @Test
    fun filtersUseTheServiceFactsAndTheOwnerIsTheL1Address() {
        val games = listOf(
            game("g1", me, playable = true, rewards = "available"),
            game("g2", other, playable = true, rewards = "none"),
            game("g3", me, playable = false, rewards = "none"),
            game("g4", null, playable = false, rewards = "none"),
        )
        assertEquals(4, filterStore(games, StoreFilter.All, me).size)
        assertEquals(listOf("g1", "g2"), filterStore(games, StoreFilter.Playable, me).map { it.gameId })
        assertEquals(listOf("g1"), filterStore(games, StoreFilter.Rewarded, me).map { it.gameId })
        assertEquals(listOf("g1", "g3"), filterStore(games, StoreFilter.Mine, me).map { it.gameId })
        assertTrue("without a wallet nothing is mine", filterStore(games, StoreFilter.Mine, null).isEmpty())
    }

    @Test
    fun theInputMustBeEvenLengthHexWithinTheHostLimit() {
        assertNull(payloadHexError("01"))
        assertNull(payloadHexError("0aFf"))
        assertNotNull(payloadHexError(""))
        assertNotNull(payloadHexError("abc"))
        assertNotNull(payloadHexError("zz"))
        assertNull(payloadHexError("00".repeat(4_096)))
        assertNotNull(payloadHexError("00".repeat(4_097)))
    }

    @Test
    fun amountsParseToExactBaseUnitsAndRefuseWhatIsNotAPositiveAmount() {
        assertEquals(1_000_000L, parseUltraUnits("1"))
        assertEquals(100L, parseUltraUnits("0.0001"))
        assertEquals(1_500_000L, parseUltraUnits(" 1.5 "))
        assertEquals(1L, parseUltraUnits("0.000001"))
        assertNull(parseUltraUnits("0"))
        assertNull(parseUltraUnits("0.0000001"))
        assertNull(parseUltraUnits("-1"))
        assertNull(parseUltraUnits("1e3"))
        assertNull(parseUltraUnits(""))
        assertNull(parseUltraUnits("1,5"))
    }

    @Test
    fun everyPlayGetsAFreshResultIdOfAnAcceptableLength() {
        val ids = (1..50).map { newResultId() }
        assertEquals(50, ids.toSet().size)
        assertTrue(ids.all { it.length in 1..128 })
        assertNotEquals(ids[0], ids[1])
    }
}
