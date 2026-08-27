// The social surface: a DEV-ONLY app, in its own module.
//
// P7 S1. Q1 chose build-level separation over a runtime flag, and the owner
// chose a separate module over a product flavor (2026-08-27) for a specific
// reason: flavors rename every variant task in `:app`
// (`assembleDebug` -> `assembleCallingDebug`), and those names are written into
// `ops/RUNBOOK-two-device-call-test.md`, `ops/RELEASING.md`,
// `docs/ENFORCEMENT-SCENARIOS.md` and the Makefile. croftcall is LIVE and
// baking on two phones; churning the runbook the owner is currently following
// is exactly the ambient change the P7 standing constraint exists to prevent.
//
// A separate module gives a STRONGER guarantee than a flavor, not a weaker
// one. With a flavor, the social code is in `:app` and excluded from the
// release variant by configuration. Here it is not in the calling app's
// dependency graph at all — the release APK cannot contain code from a module
// it never depends on, and no configuration mistake can change that. The one
// line the calling build reads is `include(":social")` in settings.gradle.kts.
plugins {
    id("com.android.application")
    id("org.jetbrains.kotlin.android")
    id("org.jetbrains.kotlin.plugin.compose")
}

android {
    namespace = "ing.croft.social"
    compileSdk = 35

    defaultConfig {
        // A DIFFERENT applicationId from the calling app, deliberately: both
        // can sit on the same device without one replacing the other, which is
        // what makes a two-device session possible while croftcall is baking.
        applicationId = "ing.croft.social"
        minSdk = 26
        targetSdk = 35
        versionCode = 1
        versionName = "0.1.0"
    }

    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }
    kotlinOptions { jvmTarget = "17" }
    buildFeatures {
        compose = true
        buildConfig = true
    }

    // The uniffi bindings are generated Kotlin SOURCE, not a jar, so they join
    // the source set rather than the classpath. Regenerated from the built
    // cdylib by `env/gen-kotlin-bindings.sh`, and never committed — a generated
    // file in the tree is a file that can disagree with its generator.
    sourceSets.getByName("main") {
        java.srcDir(rootProject.file("../ffi/kotlin/build/generated/uniffi"))
    }

    testOptions {
        unitTests.isIncludeAndroidResources = true
        unitTests.isReturnDefaultValues = true
        unitTests.all {
            // Where the DESKTOP cdylib lands, so JNA can find it when the unit
            // tests drive the real bindings on the JVM.
            it.systemProperty(
                "jna.library.path",
                rootProject.file("../target/debug").absolutePath,
            )

            // The two real inputs gradle cannot see, declared so it stops
            // reporting a stale pass as a fresh one.
            //
            // This is the same defect P7 S0 fixed for `ffi/kotlin` and it
            // arrived again the moment a second module drove the bindings —
            // which is the argument for writing it down rather than
            // remembering it. Gradle's up-to-date check knows the Kotlin
            // sources and nothing else, so a rebuilt cdylib with unchanged
            // test code looks like "nothing happened": the task is SKIPPED and
            // the build reports success. A gate that passes without running is
            // worse than one that fails, because nothing in the output says so.
            it.inputs.files(
                fileTree(rootProject.file("../target/debug")) {
                    include("libcroft_ffi.*")
                },
            ).withPropertyName("croftFfiLibrary").withPathSensitivity(PathSensitivity.NONE)
            it.inputs.dir(rootProject.file("../ffi/kotlin/build/generated/uniffi"))
                .withPropertyName("generatedBindings")
                .withPathSensitivity(PathSensitivity.RELATIVE)
        }
    }
}

dependencies {
    // The PLAIN jar, not the `@aar`. Phase 0's D1 finding, and the reason it is
    // repeated here rather than assumed: the android artifact carries only
    // `jni/<abi>/libjnidispatch.so`, while a desktop JVM test needs
    // `darwin-aarch64/libjnidispatch.jnilib`. Get it wrong and the unit tests
    // fail with `UnsatisfiedLinkError` while the on-device path is perfectly
    // healthy — so it reads as a binding bug and is not one.
    //
    // 5.14.0 matches the version on the calling app's classpath. One JNA across
    // both is the point.
    implementation("net.java.dev.jna:jna:5.14.0@aar")
    testImplementation("net.java.dev.jna:jna:5.14.0")

    implementation("androidx.activity:activity-compose:1.9.3")
    implementation("androidx.lifecycle:lifecycle-viewmodel-compose:2.8.7")
    implementation(platform("androidx.compose:compose-bom:2024.12.01"))
    implementation("androidx.compose.material3:material3")
    implementation("androidx.compose.ui:ui")
    implementation("androidx.compose.ui:ui-tooling-preview")
    debugImplementation("androidx.compose.ui:ui-tooling")
    implementation("org.jetbrains.kotlinx:kotlinx-coroutines-android:1.9.0")

    testImplementation("junit:junit:4.13.2")
    // kotlin.test's assertions take the message LAST, which is the order these
    // tests are written in and the order that reads as a sentence. JUnit 4's
    // own `assertTrue(message, condition)` puts it first, and a test that gets
    // that backwards still compiles when both arguments are Strings — a silent
    // way to assert on a message instead of a condition.
    testImplementation(kotlin("test"))
}
