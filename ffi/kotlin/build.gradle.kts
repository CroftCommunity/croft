// The JVM harness for S0's wiring test.
//
// Deliberately a plain Kotlin/JVM project, not an android one. The android app
// already ships and loads a uniffi cdylib (iroh), so the android packaging path
// is proven; what this harness answers is whether OUR bindings are correct,
// and that question is cleanest without an emulator, a manifest, or an android
// gradle plugin in the way. The arm64 emulator load is a separate, and also
// required, rung — see `env/build-croft-ffi-android.sh`.
//
// The generated Kotlin lands in `build/generated/uniffi` (see
// `env/gen-kotlin-bindings.sh`); it is not committed, because a generated file
// in the tree is a file that can disagree with its generator.

plugins {
    kotlin("jvm") version "2.2.0"
}

repositories {
    mavenCentral()
}

kotlin {
    jvmToolchain(17)
}

sourceSets {
    test {
        java.srcDir(layout.buildDirectory.dir("generated/uniffi"))
    }
}

dependencies {
    // The PLAIN jar, not the `@aar`. This is the trap Phase 0 (D1) hit and the
    // reason it is written down here: JNA ships its dispatch library as a
    // per-platform artifact, and the android `.aar` in the gradle cache carries
    // only `jni/<abi>/libjnidispatch.so`. A desktop JVM test needs
    // `darwin-aarch64/libjnidispatch.jnilib`, which lives in the plain jar. The
    // failure mode if you get this wrong is an `UnsatisfiedLinkError` here
    // while the android path stays perfectly healthy — so it looks like a
    // binding bug and is not one.
    //
    // 5.14.0 is the version already on the android classpath
    // (`android/app/build.gradle.kts`), and uniffi 0.31.1's Kotlin templates
    // need >= 5.12 for `com.sun.jna.internal.Cleaner`. One JNA version across
    // both is the point: two on a classpath is a packaging bug waiting to
    // happen.
    testImplementation("net.java.dev.jna:jna:5.14.0")
    testImplementation(kotlin("test"))
    testImplementation("org.jetbrains.kotlinx:kotlinx-coroutines-core:1.9.0")
}

tasks.test {
    useJUnitPlatform()
    // Where the cdylib built by `env/gen-kotlin-bindings.sh` lands. JNA reads
    // this to find `libcroft_ffi.dylib`.
    systemProperty("jna.library.path", System.getProperty("croft.ffi.libdir") ?: "")
    testLogging {
        events("passed", "failed", "skipped")
        showStandardStreams = true
    }
}
