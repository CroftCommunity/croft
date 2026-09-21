package ing.croft.call.net

import android.content.Context

/**
 * The one JNI door into our library (D3.3): hands iroh's DNS resolver the
 * JVM and an application context, so it reads the phone's nameservers
 * instead of falling back to public ones. Upstream iroh-ffi called this
 * `IrohAndroid.installAndroidContext`; the Rust side is
 * `ffi/src/android.rs`. Call once, before the first `CallEndpoint.bind`.
 *
 * `System.loadLibrary`, not JNA: the generated bindings load the library
 * through JNA, and the JVM only resolves `Java_*` native methods in
 * libraries loaded this way — both loads are the same file.
 */
object CroftAndroid {
    init {
        System.loadLibrary("croft_ffi")
    }

    external fun installContext(context: Context)
}
