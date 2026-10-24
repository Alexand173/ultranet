package cc.ultranet.ultraverse

import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import cc.ultranet.ultraverse.data.derivedAddress
import cc.ultranet.ultraverse.data.validatorRecordMatches
import org.json.JSONArray
import org.json.JSONObject
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith
import java.nio.charset.StandardCharsets

/**
 * Pins the mining gate against the live approval journal captured from production on
 * 2026-10-24T16:33Z (`validator_count 6`, one activated entry, no pending proposals).
 *
 * The fixture is the real row trimmed to the fields the gate reads: `activated_validator.public_key`
 * stays verbatim as its 2592-byte array because the derived address depends on every byte, while
 * `approval_transaction` keeps only its `sender` and the `proposal` keeps no key material.
 *
 * The case that matters is [matchesTheValidatorKeysOwnAddress]: before this gate derived the address,
 * the approved validator key `0944…` was refused while its applicant `2859…` was accepted, because a
 * 2592-byte key array never contains a hex address as text.
 */
@RunWith(AndroidJUnit4::class)
class ValidatorRecordMatchTest {
    private val journal: JSONObject by lazy {
        val assets = InstrumentationRegistry.getInstrumentation().context.assets
        val text = assets.open("validator-journal.json").bufferedReader(StandardCharsets.UTF_8).use { it.readText() }
        JSONObject(text)
    }
    private val approvals: JSONArray get() = journal.getJSONArray("approvals")

    @Test
    fun derivesTheValidatorsAddressFromItsLivePublicKey() {
        val derived = derivedAddress(approvals.getJSONObject(0).getJSONObject("activated_validator"))
        assertEquals(VALIDATOR_ADDRESS, derived)
    }

    @Test
    fun matchesTheValidatorKeysOwnAddress() {
        assertTrue(validatorRecordMatches(VALIDATOR_ADDRESS, null, approvals))
    }

    @Test
    fun normalisesHexPrefixAndCase() {
        assertTrue(validatorRecordMatches("0x" + VALIDATOR_ADDRESS.uppercase(), null, approvals))
    }

    @Test
    fun stillMatchesTheApplicantThroughTheProposal() {
        assertTrue(validatorRecordMatches(APPLICANT_ADDRESS, null, approvals))
    }

    @Test
    fun rejectsAnUnrelatedOrEmptyAddress() {
        assertFalse(validatorRecordMatches("ab".repeat(32), null, approvals))
        assertFalse(validatorRecordMatches("", null, approvals))
        assertFalse(validatorRecordMatches("not-an-address", null, approvals))
    }

    @Test
    fun matchesAPendingProposalProposer() {
        val proposals = JSONArray().put(
            JSONObject().put("hash", "00").put("proposer", APPLICANT_ADDRESS).put("metadata", "pending"),
        )
        assertTrue(validatorRecordMatches(APPLICANT_ADDRESS, proposals, null))
        assertFalse(validatorRecordMatches(VALIDATOR_ADDRESS, proposals, null))
    }

    @Test
    fun treatsAMalformedKeyAsNoRecordAndKeepsTheApplicantFallback() {
        val truncated = JSONArray().put(
            JSONObject()
                .put("proposal", JSONObject().put("proposer", APPLICANT_ADDRESS))
                .put(
                    "activated_validator",
                    JSONObject().put("is_active", true).put("public_key", JSONArray().put(1).put(2).put(3)),
                ),
        )
        assertFalse(validatorRecordMatches(VALIDATOR_ADDRESS, null, truncated))
        assertTrue(validatorRecordMatches(APPLICANT_ADDRESS, null, truncated))
    }

    private companion object {
        const val VALIDATOR_ADDRESS = "094420342ad33eda3ed61841d09317a4d9a6a903a0a4c95b61d14b8ce2c55d78"
        const val APPLICANT_ADDRESS = "2859eb671f8e03129932efd184f4707352a9770e61fc5e6b8215911c8bd377ed"
    }
}
