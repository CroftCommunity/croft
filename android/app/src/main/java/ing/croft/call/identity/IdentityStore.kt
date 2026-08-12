package ing.croft.call.identity

import android.content.Context
import androidx.security.crypto.EncryptedSharedPreferences
import androidx.security.crypto.MasterKey
import android.util.Base64

/**
 * Persists the endpoint's secret key so the app keeps the same EndpointId
 * across launches. Stability matters more here than in most apps: this id is
 * what the user publishes to their PDS record, so losing it silently would
 * strand their published listing. iroh's Kotlin guide recommends exactly this
 * pattern: persist ep.secretKey().toBytes() somewhere durable (it names
 * EncryptedSharedPreferences) and re-bind with EndpointOptions(secretKey = ...).
 */
class IdentityStore(context: Context) {
    private val prefs = EncryptedSharedPreferences.create(
        context,
        "croftcall.identity",
        MasterKey.Builder(context).setKeyScheme(MasterKey.KeyScheme.AES256_GCM).build(),
        EncryptedSharedPreferences.PrefKeyEncryptionScheme.AES256_SIV,
        EncryptedSharedPreferences.PrefValueEncryptionScheme.AES256_GCM,
    )

    fun loadSecretKey(): ByteArray? =
        prefs.getString(KEY, null)?.let { Base64.decode(it, Base64.NO_WRAP) }

    fun saveSecretKey(bytes: ByteArray) {
        prefs.edit().putString(KEY, Base64.encodeToString(bytes, Base64.NO_WRAP)).apply()
    }

    private companion object { const val KEY = "endpoint_secret_key" }
}
