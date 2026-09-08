plugins {
    id("com.android.application")
    id("org.jetbrains.kotlin.android")
    id("org.jetbrains.kotlin.plugin.compose")
}

android {
    namespace = "ing.croft.call"
    compileSdk = 35

    defaultConfig {
        applicationId = "ing.croft.call"
        minSdk = 26            // matches iroh reference app floor (Android 8.0)
        targetSdk = 35
        versionCode = 6
        versionName = "0.5.0"
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

    // M4d test-rig overrides: point a DEBUG build's relay/admit at a local
    // enforce pair (-PcroftRelayUrl=... -PcroftAdmitBase=...). Defaults are
    // production; release builds always use them.
    defaultConfig.buildConfigField(
        "String", "CROFT_RELAY_URL",
        "\"" + (project.findProperty("croftRelayUrl") ?: "https://relay.croft.ing:8443") + "\"",
    )
    defaultConfig.buildConfigField(
        "String", "CROFT_RELAY_QUIC_PORT",
        "\"" + (project.findProperty("croftRelayQuicPort") ?: "7824") + "\"",
    )
    defaultConfig.buildConfigField(
        "String", "CROFT_ADMIT_BASE",
        "\"" + (project.findProperty("croftAdmitBase") ?: "https://admit.croft.ing") + "\"",
    )

    testOptions {
        // DeepLink.parse reads android.net.Uri, so its JVM unit tests run under
        // Robolectric; androidResources lets Robolectric load the resource table.
        unitTests.isIncludeAndroidResources = true
        unitTests.isReturnDefaultValues = true
    }
}

// computer.iroh:iroh ships Java-21 bytecode; the app is fine (D8 dexes it for
// Android) but JVM unit tests that load those classes need a 21 runtime. Only
// the test launcher moves to 21 — compile stays at 17 (compileOptions above) so
// the produced APK is unchanged. The JDK comes via Gradle toolchains + the
// foojay resolver (settings.gradle.kts), not from JAVA_HOME.
val javaToolchains = project.extensions.getByType<JavaToolchainService>()
tasks.withType<Test>().configureEach {
    javaLauncher.set(
        javaToolchains.launcherFor {
            languageVersion.set(JavaLanguageVersion.of(21))
        }
    )
    // EnforcementMatrixTest reads the repo-root matrix doc; without declaring
    // it an input, a doc-only edit leaves the test task up-to-date and the
    // gate silently passes stale (found live: a planted bogus PIN produced a
    // green run from cache).
    inputs.file(rootProject.file("../docs/ENFORCEMENT-SCENARIOS.md"))
        .withPathSensitivity(PathSensitivity.RELATIVE)
        .optional()
}

dependencies {
    // iroh Kotlin bindings from Maven Central. Per n0's reference Android app,
    // this artifact bundles libiroh_ffi.so for every Android ABI (no NDK).
    // If your resolved version lacks Android ABIs (older docs said the artifact
    // was single-platform), fall back to building iroh-ffi from source; see README.
    implementation("computer.iroh:iroh:1.0.0") {
        // Quirk from the reference app: the artifact declares plain-jar JNA
        // transitively, but Android needs the @aar variant which bundles
        // libjnidispatch.so per ABI. Keeping both duplicates classes at packaging.
        exclude(group = "net.java.dev.jna", module = "jna")
    }
    implementation("net.java.dev.jna:jna:5.14.0@aar")  // uniffi requires JNA >= 5.12

    implementation("androidx.security:security-crypto:1.1.0-alpha06") // EncryptedSharedPreferences
    implementation("androidx.activity:activity-compose:1.9.3")
    implementation("androidx.lifecycle:lifecycle-viewmodel-compose:2.8.7")
    implementation(platform("androidx.compose:compose-bom:2024.12.01"))
    implementation("androidx.compose.material3:material3")
    implementation("androidx.compose.ui:ui")
    implementation("org.jetbrains.kotlinx:kotlinx-coroutines-android:1.9.0")

    // Pure-JVM unit tests for the shared contract (DeepLink + WireFormat).
    // WireFormat is plain Kotlin; DeepLink needs android.net.Uri, so Robolectric
    // provides the framework classes on the JVM (no device, runs in `./gradlew test`).
    testImplementation("junit:junit:4.13.2")
    testImplementation("org.robolectric:robolectric:4.14.1")
    testImplementation("androidx.test:core:1.6.1")
    // The workflow harness (M4): FixtureExchange serves every backend the
    // client talks to from one in-JVM server, so journey tests drive the
    // REAL ports over real sockets. com.sun.net.httpserver is not on the
    // android unit-test compile classpath; MockWebServer is the standard
    // socket server for this layer.
    testImplementation("com.squareup.okhttp3:mockwebserver:4.12.0")
}
