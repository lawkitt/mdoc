// Kotlin is built by AGP 9 itself; the Kotlin Gradle plugin on the build
// classpath sets its version, which the Compose compiler plugin must match.
buildscript {
    dependencies {
        classpath(libs.kotlin.gradle.plugin)
    }
}

plugins {
    alias(libs.plugins.android.application) apply false
    alias(libs.plugins.android.test) apply false
    alias(libs.plugins.kotlin.compose) apply false
}
