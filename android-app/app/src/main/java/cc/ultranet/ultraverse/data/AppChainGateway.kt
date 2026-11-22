package cc.ultranet.ultraverse.data

import android.content.Context
import cc.ultranet.ultraverse.wallet.SignedAppChainTransaction

/**
 * The AppChain surface the wallet needs, as an interface.
 *
 * [UltraNetOperatorApi] is the real implementation. The interface exists so the signing path can be
 * driven by a fake — a wallet that can only be tested by talking to a live node is a wallet whose
 * write path is never tested.
 */
internal interface AppChainGateway {
    suspend fun appChainLedger(): AppChainLedger
    suspend fun appChainAnchors(chainId: Int, limit: Int = 50, from: Long = 1): List<AppChainAnchorRecord>
    suspend fun submitAppChain(tx: SignedAppChainTransaction): AppChainSubmission
}

/**
 * What the wallet uses by default. It reads the configured node on **every** call rather than
 * capturing it once, so pointing the app at another node takes effect immediately and the wallet
 * controller does not have to be rebuilt.
 */
internal class ConfiguredAppChainGateway(context: Context) : AppChainGateway {
    private val store = NodeEndpointStore(context)

    private fun api(): UltraNetOperatorApi = UltraNetOperatorApi(baseUrl = store.load())

    override suspend fun appChainLedger(): AppChainLedger = api().appChainLedger()

    override suspend fun appChainAnchors(chainId: Int, limit: Int, from: Long): List<AppChainAnchorRecord> =
        api().appChainAnchors(chainId, limit, from)

    override suspend fun submitAppChain(tx: SignedAppChainTransaction): AppChainSubmission =
        api().submitAppChain(tx)
}
