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
