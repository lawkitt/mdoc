plugins {
    alias(libs.plugins.android.application)
    alias(libs.plugins.kotlin.compose)
}

// What scripts/build-android-core.sh made: the core for each ABI and its
// Kotlin bindings, plus a desktop build of the core for the JVM tests.
val repo = rootDir.resolve("../..")
val core = repo.resolve("target/android")
val testAssets = layout.buildDirectory.dir("generated/test-assets").get().asFile

// The OCR qualification fixtures, and the settlement agreement with its names
// and an AI reply for the review flow and the survey.
val copyTestAssets = tasks.register<Copy>("copyTestAssets") {
    from(repo.resolve("tests/fixtures/ocr-qualification")) { include("english.pdf", "english.txt", "russian.pdf", "russian.txt") }
    from(repo.resolve("docs/product-media")) { include("Settlement agreement.md") }
    from(repo.resolve("tests/fixtures/restore/claude-en")) { include("identities.json", "reply.md") }
    into(testAssets)
}
tasks.matching { it.name.endsWith("AndroidTestAssets") }.configureEach { dependsOn(copyTestAssets) }

android {
    namespace = "io.github.lawkitt.mdoc"
    compileSdk = 37
    // The build-tools scripts/install-android-sdk.sh pins; AGP would
    // otherwise fetch its own default.
    buildToolsVersion = "36.1.0"

    defaultConfig {
        applicationId = "io.github.lawkitt.mdoc"
        minSdk = 31
        targetSdk = 36
        versionCode = 1
        versionName = "0.1.0"
        testInstrumentationRunner = "androidx.test.runner.AndroidJUnitRunner"
        // The core is built for these two only (scripts/build-android-core.sh);
        // JNA's AAR would otherwise bring 32-bit libraries a phone could
        // install but not load the core with. Release drops x86_64 below.
        ndk {
            abiFilters += listOf("arm64-v8a", "x86_64")
        }
    }

    buildTypes {
        release {
            // UniFFI's bindings reach the core through JNA reflection.
            isMinifyEnabled = false
        }
    }

    buildFeatures {
        compose = true
        buildConfig = true
    }

    sourceSets {
        getByName("main") {
            kotlin.directories += core.resolve("kotlin").path
            jniLibs.directories += core.resolve("jniLibs").path
        }
        // The OCR qualification fixtures, for the instrumented setup test.
        // The fixtures the instrumented tests read, copied by copyTestAssets.
        getByName("androidTest") {
            assets.directories += testAssets.path
        }
    }

    // The core opens PDFium and ONNX Runtime by file path beside itself
    // (`linked_runtimes`), so the APK's libraries are extracted on install.
    packaging {
        jniLibs {
            useLegacyPackaging = true
        }
    }

    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }

    testOptions {
        unitTests.all {
            // The parity tests load the desktop build of the core.
            it.systemProperty("jna.library.path", core.resolve("host").path)
            it.systemProperty("mdoc.repo", repo.path)
        }
    }
}

dependencies {
    implementation(platform(libs.compose.bom))
    implementation(libs.compose.ui)
    implementation(libs.compose.ui.tooling.preview)
    implementation(libs.compose.material3)
    implementation(libs.compose.material.icons)
    implementation(libs.core.ktx)
    implementation(libs.activity.compose)
    implementation(libs.lifecycle.runtime.compose)
    implementation(libs.lifecycle.viewmodel.compose)
    implementation(libs.coroutines.android)
    implementation(variantOf(libs.jna.aar) { artifactType("aar") })
    debugImplementation(libs.compose.ui.tooling)
    debugImplementation(libs.compose.ui.test.manifest)

    testImplementation(libs.junit)
    testImplementation(libs.jna.jar)
    testImplementation(libs.org.json)

    androidTestImplementation(platform(libs.compose.bom))
    androidTestImplementation(libs.compose.ui.test.junit4)
    androidTestImplementation(libs.androidx.test.runner)
    androidTestImplementation(libs.androidx.test.junit)
    androidTestImplementation(libs.androidx.test.core)
}

// Release ships arm64-v8a only (ADR 0089); x86_64 is for the emulator.
androidComponents {
    onVariants(selector().withBuildType("release")) { variant ->
        variant.packaging.jniLibs.excludes.add("lib/x86_64/**")
    }
}
