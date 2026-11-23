package cc.ultranet.ultraverse.data

import java.net.URI

/**
 * The node an install talks to.
 *
 * An AppChain developer runs their own backend and may run their own node or gateway, so the host
 * cannot be a compile-time constant that only UltraNet's own deployment satisfies. Validation is
 * strict because this value is concatenated with absolute API paths: a base URL that carried a path
 * of its own would silently address a different route.
 *
 * Pure Kotlin on purpose — the rules are unit-tested without a device.
 */
internal object NodeEndpoint {
    const val DEFAULT = "https://api.ultranetwork.cc"

    private const val LOOPBACK_IPV4 = "127.0.0.1"
    private const val LOOPBACK_IPV6 = "::1"
    private const val LOOPBACK_NAME = "localhost"

    sealed interface Verdict {
        data class Accepted(val baseUrl: String) : Verdict
        data class Rejected(val reason: String) : Verdict
    }

    fun check(input: String): Verdict {
        val trimmed = input.trim()
        if (trimmed.isEmpty()) {
            return Verdict.Rejected("Enter the node's address, for example $DEFAULT")
        }
        val uri = try {
            URI(trimmed)
        } catch (_: Exception) {
            return Verdict.Rejected("That is not a valid address.")
        }
        val scheme = uri.scheme?.lowercase()
        if (scheme != "https" && scheme != "http") {
            return Verdict.Rejected("The address must start with https://")
        }
        val host = uri.host ?: return Verdict.Rejected("The address must include a host name.")
        if (uri.userInfo != null) {
            return Verdict.Rejected("The address must not contain a user name or password.")
        }
        if (uri.query != null || uri.fragment != null) {
            return Verdict.Rejected("Enter the node's base address, with no query or fragment.")
        }
        val path = uri.path.orEmpty()
        if (path.isNotEmpty() && path != "/") {
            return Verdict.Rejected("Enter the base address only, without a path.")
        }
        // Cleartext to anywhere but this device would expose every request; the release manifest
        // forbids it too, so accepting it here would only produce an opaque socket failure later.
        if (scheme == "http" && !isLoopback(host)) {
            return Verdict.Rejected("Use https://: an http node would send your requests in the clear.")
        }
        val port = if (uri.port == -1) "" else ":${uri.port}"
        val authority = if (host.contains(':')) "[$host]$port" else "$host$port"
        return Verdict.Accepted("$scheme://$authority")
    }

    /**
     * Whether this base URL may carry an administrator bearer token.
     *
     * [check] refuses cleartext when an address is *saved*, but a token has to be refused at the
     * moment it would leave the device: that is the only place a call site cannot forget, and it
     * also covers a base URL that arrived from a stored preference, a test seam or a constructor
     * argument instead of through the settings sheet. Loopback stays allowed, because a node on the
     * same device has no network to cross.
     */
    fun admitsAdminToken(baseUrl: String): Verdict {
        val verdict = check(baseUrl)
        val accepted = verdict as? Verdict.Accepted ?: return verdict
        val uri = runCatching { URI(accepted.baseUrl) }.getOrNull()
            ?: return Verdict.Rejected("That is not a valid address.")
        val scheme = uri.scheme?.lowercase()
        val host = uri.host
        if (scheme != "https" && (host == null || !isLoopback(host))) {
            return Verdict.Rejected(
                "Refusing to send the node's admin token over ${scheme ?: "an unknown scheme"}://: use https://.",
            )
        }
        return accepted
    }

    private fun isLoopback(host: String): Boolean =
        host.equals(LOOPBACK_NAME, ignoreCase = true) ||
            host == LOOPBACK_IPV4 ||
            host == LOOPBACK_IPV6

    /** A short label for the UI: the host, with a port when it is not the scheme's default. */
    fun label(baseUrl: String): String = runCatching { URI(baseUrl) }.getOrNull()
        ?.let { uri ->
            val host = uri.host ?: baseUrl
            if (uri.port == -1) host else "$host:${uri.port}"
        }
        ?: baseUrl
}
