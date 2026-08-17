package ing.croft.call.net

import computer.iroh.PathSnapshot
import computer.iroh.PathStatsRecord
import org.junit.Assert.assertEquals
import org.junit.Test

/**
 * PathSummary tests — the one line the UI and logcat get about which path a
 * connection is actually using. The two-device call test (2026-08-17) recorded
 * "direct or relayed: unknown" because nothing observable said; this is the
 * instrument that closes that gap, built on the real PathSnapshot type from
 * computer.iroh so the tests exercise the same shape the endpoint returns.
 */
class PathSummaryTest {

    private fun zeroStats() = PathStatsRecord(
        rttMs = 0u, udpTxDatagrams = 0u, udpTxBytes = 0u,
        udpRxDatagrams = 0u, udpRxBytes = 0u, cwnd = 0u,
        congestionEvents = 0u, lostPackets = 0u, lostBytes = 0u,
        currentMtu = 0u,
    )

    private fun path(
        selected: Boolean,
        ip: Boolean = false,
        relay: Boolean = false,
        addr: String = "203.0.113.7:4433",
    ) = PathSnapshot(
        id = "path-$addr",
        isSelected = selected,
        remoteAddr = addr,
        isIp = ip,
        isRelay = relay,
        rttMs = 12u,
        stats = zeroStats(),
    )

    @Test
    fun `selected ip path reads as direct with its address`() {
        val paths = listOf(path(selected = true, ip = true, addr = "192.0.2.5:7842"))
        assertEquals("direct 192.0.2.5:7842", PathSummary.describe(paths))
    }

    @Test
    fun `selected relay path reads as relayed with its address`() {
        val paths = listOf(
            path(selected = true, relay = true, addr = "https://use1-1.relay.n0.services./"),
        )
        assertEquals("relayed https://use1-1.relay.n0.services./", PathSummary.describe(paths))
    }

    @Test
    fun `only the selected path is reported`() {
        val paths = listOf(
            path(selected = false, relay = true, addr = "https://relay.example/"),
            path(selected = true, ip = true, addr = "198.51.100.9:1024"),
        )
        assertEquals("direct 198.51.100.9:1024", PathSummary.describe(paths))
    }

    @Test
    fun `no paths reads as unknown`() {
        assertEquals("path unknown", PathSummary.describe(emptyList()))
    }

    @Test
    fun `no selected path reads as unknown`() {
        val paths = listOf(path(selected = false, ip = true))
        assertEquals("path unknown", PathSummary.describe(paths))
    }

    @Test
    fun `a selected path that is neither ip nor relay is not guessed at`() {
        val paths = listOf(path(selected = true))
        assertEquals("path unknown", PathSummary.describe(paths))
    }
}
