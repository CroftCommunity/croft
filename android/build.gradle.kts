// Root build file. The plugin versions were first aligned to n0's reference
// app (the known-good combination for the computer.iroh artifact); since D3.4
// (2026-09-21) the calling app carries no upstream iroh artifact — its iroh is
// ours, inside libcroft_ffi — and these versions are simply the pinned ones
// (env/toolchain.yml). Kotlin 2.2 is what uniffi 0.31's generated source needs.
plugins {
    id("com.android.application") version "8.7.3" apply false
    id("org.jetbrains.kotlin.android") version "2.2.0" apply false
    id("org.jetbrains.kotlin.plugin.compose") version "2.2.0" apply false
}

// Dependency locking: writes a `gradle.lockfile` per module recording the exact
// resolved dependency set. Without one there is no resolved set on disk, so no
// SCA scanner — osv-scanner, Dependabot, or otherwise — has anything to read,
// and the JVM half of the shipped client is unscannable by construction
// (workspace supply-chain sweep, 2026-08-29).
//
// Regenerate after any dependency change:  ./gradlew :app:dependencies --write-locks
subprojects {
    dependencyLocking {
        lockAllConfigurations()
    }
}
