pluginManagement {
    repositories {
        google()
        mavenCentral()
        gradlePluginPortal()
    }
}
plugins {
    // Lets Gradle download a pinned JDK itself (env/toolchain.yml `jdk:`) instead
    // of trusting whatever JAVA_HOME points at. The unit-test task needs a 21
    // launcher because computer.iroh:iroh ships Java-21 bytecode.
    id("org.gradle.toolchains.foojay-resolver-convention") version "0.8.0"
}
dependencyResolutionManagement {
    repositories {
        google()
        mavenCentral()
    }
}
rootProject.name = "croftcall"
include(":app")

// P7 S1. The one line the calling build reads — the social surface is a
// separate DEV-ONLY module, so `:app`'s variant names, its release APK and
// every command in ops/RUNBOOK-two-device-call-test.md stay exactly as they
// are while croftcall bakes. See android/social/build.gradle.kts for why this
// is a stronger guarantee than a product flavor rather than a weaker one.
include(":social")
