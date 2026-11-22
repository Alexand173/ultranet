package cc.ultranet.ultraverse

import cc.ultranet.ultraverse.data.AppChainLedger
import cc.ultranet.ultraverse.data.AppChainLedgerChain
import cc.ultranet.ultraverse.data.AppChainTerms
import cc.ultranet.ultraverse.wallet.PreparedAnchorIntake
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * A-2's threat model, executed. A backend supplies the payload; therefore a payload must be able to
 * prove nothing, and every refusal below is one of the abuses the intake exists to survive.
 *
 * Two chains share the display name "UltraGrid" throughout: the naming decision (O-3) means the name
 * cannot identify a chain, so every case is resolved by id.
 */
class PreparedAnchorIntakeTest {

    private val alice = "aa".repeat(32)
    private val bob = "bb".repeat(32)
    private val root = "1f".repeat(32)

    private fun chain(id: Int, owner: String, name: String = "UltraGrid") = AppChainLedgerChain(
        id = id,
        name = name,
        owner = owner,
        registeredTx = "00".repeat(32),
        registeredBlock = 7,
        anchorCount = 0,
        latestStateRoot = null,
        latestAnchorBlock = null,
    )

    private fun ledger(
        chains: List<AppChainLedgerChain> = listOf(chain(1, alice), chain(2, bob)),
        activationHeight: Long? = 10,
        anchorFeeBaseUnits: Long = 1_000,
    ) = AppChainLedger(
        terms = AppChainTerms(
            version = 5,
            recipient = "appchain",
            registerFeeBaseUnits = 10_000,
            anchorFeeBaseUnits = anchorFeeBaseUnits,
            transferFeeBaseUnits = 1_000,
            activationHeight = activationHeight,
        ),
        chains = chains,
    )

    private fun payload(chainId: Int, stateRoot: String = root): String =
        """{"type":"ultranet/appchain-anchor","version":1,"chain_id":$chainId,""" +
            """"state_root":"$stateRoot","origin":"https://grid.example"}"""

    private fun parse(text: String) = PreparedAnchorIntake.parse(text)

    @Test
    fun `a request for a chain another wallet owns is refused, even with an identical name`() {
        val parsed = parse(payload(chainId = 2)) as PreparedAnchorIntake.Parse.Parsed

        val verdict = PreparedAnchorIntake.evaluate(parsed.payload, ledger(), walletAddress = alice)

        assertTrue(verdict is PreparedAnchorIntake.Verdict.Refused)
        val reason = (verdict as PreparedAnchorIntake.Verdict.Refused).reason
        assertTrue("must name the owner rule: $reason", reason.contains("owned by"))
        assertTrue("must show the chain id, not just the name: $reason", reason.contains("#2"))
    }

    @Test
    fun `the chain is resolved by id when two chains share a display name`() {
        val shared = ledger()

        val first = parse(payload(chainId = 1)) as PreparedAnchorIntake.Parse.Parsed
        val second = parse(payload(chainId = 2)) as PreparedAnchorIntake.Parse.Parsed

        val aliceReady = PreparedAnchorIntake.evaluate(first.payload, shared, alice)
        val bobReady = PreparedAnchorIntake.evaluate(second.payload, shared, bob)

        assertEquals(1, (aliceReady as PreparedAnchorIntake.Verdict.Ready).chain.id)
        assertEquals(2, (bobReady as PreparedAnchorIntake.Verdict.Ready).chain.id)
        assertEquals(shared.chains[0].name, shared.chains[1].name)
    }

    @Test
    fun `the owner comparison is not defeated by letter case`() {
        val parsed = parse(payload(chainId = 1)) as PreparedAnchorIntake.Parse.Parsed

        val verdict = PreparedAnchorIntake.evaluate(parsed.payload, ledger(), walletAddress = alice.uppercase())

        assertTrue(verdict is PreparedAnchorIntake.Verdict.Ready)
    }

    @Test
    fun `the fee comes from the node's terms, not from the client's constants`() {
        val parsed = parse(payload(chainId = 1)) as PreparedAnchorIntake.Parse.Parsed

        val verdict = PreparedAnchorIntake.evaluate(parsed.payload, ledger(anchorFeeBaseUnits = 2_500), alice)

        assertEquals(2_500, (verdict as PreparedAnchorIntake.Verdict.Ready).feeBaseUnits)
    }

    @Test
    fun `the root displayed is the root signed, normalised once`() {
        val parsed = parse(payload(chainId = 1, stateRoot = "0X${root.uppercase()}"))

        val payload = (parsed as PreparedAnchorIntake.Parse.Parsed).payload

        assertEquals(root, payload.stateRootHex)
        assertTrue(payload.stateRootHex.none { it.isUpperCase() })
    }

    @Test
    fun `a root that is not 32 bytes is refused before any chain is consulted`() {
        val parsed = parse(payload(chainId = 1, stateRoot = "abcd"))

        assertTrue(parsed is PreparedAnchorIntake.Parse.Refused)
        assertTrue((parsed as PreparedAnchorIntake.Parse.Refused).reason.contains("64 hexadecimal"))
    }

    @Test
    fun `a payload that is not JSON is refused`() {
        assertTrue(parse("3:deadbeef") is PreparedAnchorIntake.Parse.Refused)
    }

    @Test
    fun `a payload for a different product is refused`() {
        val text = """{"type":"ultranet/game-result","chain_id":3,"state_root":"$root"}"""

        val parsed = parse(text)

        assertTrue(parsed is PreparedAnchorIntake.Parse.Refused)
        assertTrue((parsed as PreparedAnchorIntake.Parse.Refused).reason.contains("game-result"))
    }

    @Test
    fun `a payload without a numeric chain id is refused`() {
        val text = """{"chain_id":"UltraGrid","state_root":"$root"}"""

        assertTrue(parse(text) is PreparedAnchorIntake.Parse.Refused)
    }

    @Test
    fun `an unregistered chain cannot be anchored`() {
        val parsed = parse(payload(chainId = 9)) as PreparedAnchorIntake.Parse.Parsed

        val verdict = PreparedAnchorIntake.evaluate(parsed.payload, ledger(), alice)

        assertTrue(verdict is PreparedAnchorIntake.Verdict.Refused)
        assertTrue((verdict as PreparedAnchorIntake.Verdict.Refused).reason.contains("not registered"))
    }

    @Test
    fun `a node with no activation height accepts nothing`() {
        val parsed = parse(payload(chainId = 1)) as PreparedAnchorIntake.Parse.Parsed

        val verdict = PreparedAnchorIntake.evaluate(parsed.payload, ledger(activationHeight = null), alice)

        assertTrue(verdict is PreparedAnchorIntake.Verdict.Refused)
        assertTrue((verdict as PreparedAnchorIntake.Verdict.Refused).reason.contains("activation height"))
    }

    @Test
    fun `a locked wallet is offered no signature`() {
        val parsed = parse(payload(chainId = 1)) as PreparedAnchorIntake.Parse.Parsed

        val verdict = PreparedAnchorIntake.evaluate(parsed.payload, ledger(), walletAddress = null)

        assertTrue(verdict is PreparedAnchorIntake.Verdict.Refused)
    }
}
