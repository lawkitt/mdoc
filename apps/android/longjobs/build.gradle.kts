// The long-job device test (ADR 0089): UiAutomator drives the debug app as
// a person would, presses Home and kills mdoc's process mid-job. It is
// self-instrumenting, so it runs in its own process and outlives the kill.
plugins {
    alias(libs.plugins.android.test)
}

val repo = rootDir.resolve("../..")
val assetsDir = layout.buildDirectory.dir("generated/long-job-assets").get().asFile
val copyAssets = tasks.register<Copy>("copyLongJobAssets") {
    from(repo.resolve("tests/fixtures/ocr-qualification")) { include("english.pdf") }
    into(assetsDir)
}
tasks.matching { it.name.endsWith("Assets") && it.name != "copyLongJobAssets" }.configureEach { dependsOn(copyAssets) }

android {
    namespace = "io.github.lawkitt.mdoc.longjobs"
    compileSdk = 37
    buildToolsVersion = "36.1.0"

    defaultConfig {
        minSdk = 31
        targetSdk = 36
        testInstrumentationRunner = "androidx.test.runner.AndroidJUnitRunner"
    }

    targetProjectPath = ":app"
    experimentalProperties["android.experimental.self-instrumenting"] = true

    sourceSets {
        getByName("main") {
            assets.directories += assetsDir.path
        }
    }

    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }
}

dependencies {
    implementation(libs.androidx.test.runner)
    implementation(libs.androidx.test.junit)
    implementation(libs.uiautomator)
    implementation(libs.junit)
}
