// Root build file. Version alignment note: if Gradle sync fights you on plugin
// versions, mirror the versions used in n0's reference app
// (github.com/n0-computer/hello-iroh-ffi/tree/main/kotlin-android), which is the
// known-good combination for the computer.iroh artifact. Kotlin must be 2.2+:
// the published iroh artifact carries Kotlin 2.2 metadata.
plugins {
    id("com.android.application") version "8.7.3" apply false
    id("org.jetbrains.kotlin.android") version "2.2.0" apply false
    id("org.jetbrains.kotlin.plugin.compose") version "2.2.0" apply false
}
