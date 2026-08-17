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
