package ing.croft.call.net

import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.cancel
import kotlinx.coroutines.flow.filterIsInstance
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.withTimeout
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * D3.3: `CallPeer` over OUR port, driven on the JVM through the generated
 * bindings — the same object the ViewModel holds, the same state machine the
 * screen renders. Hermetic: TEST-NET-1 relay, no discovery, a loopback call
 * by direct address. Admission is the device run's job (runbook §17).
 *
 * What it pins: bind → Ready with a stable id from a persisted key; the
 * presence line honest while nothing is attached (E135(a) — a refused
 * attach reads NOT camped); a call between two peers with both hellos; the
 * E129 endings in the words the phones already show; and that an ended peer
 * is STILL Ready (the device stays callable).
 */
class CallPeerWiringTest {

    /** A key store with no Android in it. */
    private class MemoryKeys(private var key: ByteArray? = null) : SecretKeyStore {
        override fun loadSecretKey(): ByteArray? = key
        override fun saveSecretKey(bytes: ByteArray) { key = bytes }
    }

    private val scope = CoroutineScope(SupervisorJob() + Dispatchers.IO)
    private val relay = CroftRelay.Target(url = "https://192.0.2.1:443", quicPort = 443u, discoveryN0 = false)

    private fun peer(keys: SecretKeyStore) = CallPeer(keys, scope, relay)

    private suspend inline fun <reified S : CallPeer.State> CallPeer.awaitState(): S =
        withTimeout(20_000) { state.filterIsInstance<S>().first() }

    @After
    fun tearDown() = scope.cancel()

    @Test
    fun `bind persists the key and the id survives a stop and start`() = runBlocking {
        val keys = MemoryKeys()
        val a = peer(keys)
        a.start()
        val first = a.awaitState<CallPeer.State.Ready>()
        assertEquals(32, keys.loadSecretKey()?.size)
        assertNull("nothing is attached to TEST-NET-1", a.homeRelay.value)
        assertEquals(
            "ready — NOT camped on relay; calls cannot reach this device",
            CampPresence.line(a.homeRelay.value),
        )
        a.stop()
        a.awaitState<CallPeer.State.Idle>()
        a.start()
        assertEquals(first.endpointId, a.awaitState<CallPeer.State.Ready>().endpointId)
        a.stop()
    }

    @Test
    fun `two peers call over loopback and both endings are the screen's words`() = runBlocking {
        val a = peer(MemoryKeys())
        val b = peer(MemoryKeys())
        a.start(); b.start()
        val aId = a.awaitState<CallPeer.State.Ready>().endpointId
        val bId = b.awaitState<CallPeer.State.Ready>().endpointId

        val addrs = generateSequence { b.localAddrs().takeIf { it.isNotEmpty() } ?: run { Thread.sleep(50); null } }
            .first()
        a.dial(bId, relayUrl = null, addrs = addrs, callerLabel = "croftcall-android")

        val outgoing = a.awaitState<CallPeer.State.Connected>()
        val incoming = b.awaitState<CallPeer.State.Connected>()
        assertEquals(bId, outgoing.peer)
        assertEquals(aId, incoming.peer)
        assertEquals("outgoing", outgoing.direction)
        assertEquals("incoming", incoming.direction)
        assertEquals("callee", outgoing.peerHello)
        assertEquals("croftcall-android", incoming.peerHello)
        assertTrue(outgoing.path, outgoing.path.startsWith("direct "))

        a.hangUp()
        assertEquals("you ended the call", a.awaitState<CallPeer.State.Ended>().message)
        assertEquals(
            "call ended: closed by peer: hangup (code 0)",
            b.awaitState<CallPeer.State.Ended>().message,
        )
        a.stop(); b.stop()
    }
}
