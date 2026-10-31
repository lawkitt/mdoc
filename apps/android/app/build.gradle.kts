plugins {
    alias(libs.plugins.android.application)
    alias(libs.plugins.kotlin.compose)
}

// What scripts/build-android-core.sh made: the core for each ABI and its
// Kotlin bindings, plus a desktop build of the core for the JVM tests.
val repo = rootDir.resolve("../..")
val core = repo.resolve("target/android")

android {
    namespace = "io.github.lawkitt.mdoc"
    compileSdk = 36

    defaultConfig {
        applicationId = "io.github.lawkitt.mdoc"
        minSdk = 31
        targetSdk = 36
        versionCode = 1
        versionName = "0.1.0"
        testInstrumentationRunner = "androidx.test.runner.AndroidJUnitRunner"
    }

    buildTypes {
        release {
            // UniFFI's bindings reach the core through JNA reflection.
            isMinifyEnabled = false
        }
    }

    buildFeatures {
        compose = true
    }

    sourceSets {
        getByName("main") {
            java.srcDir(core.resolve("kotlin"))
            jniLibs.srcDir(core.resolve("jniLibs"))
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
