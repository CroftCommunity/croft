package ing.croft.call.net

/**
 * Where the endpoint's secret key lives between launches. The key IS the
 * identity the account publishes (`ing.croft.iroh.endpoint`), so losing it
 * silently strands the listing — §16's rig-state note is what that costs.
 * `IdentityStore` is the phone's (encrypted prefs); a JVM test keeps one in
 * memory.
 */
interface SecretKeyStore {
    fun loadSecretKey(): ByteArray?
    fun saveSecretKey(bytes: ByteArray)
}
