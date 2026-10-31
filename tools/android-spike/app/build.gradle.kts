plugins {
    id("com.android.application")
}

// What scripts/build-android-core.sh and scripts/android-spike-data.sh made.
val repo = rootDir.resolve("../..")

android {
    namespace = "io.github.lawkitt.mdoc.spike"
    compileSdk = 36
    // The build-tools scripts/install-android-sdk.sh pins; AGP would
    // otherwise fetch its own default.
    buildToolsVersion = "36.1.0"

    defaultConfig {
        applicationId = "io.github.lawkitt.mdoc.spike"
        minSdk = 31
        targetSdk = 36
        versionCode = 1
        versionName = "0.1"
        testInstrumentationRunner = "androidx.test.runner.AndroidJUnitRunner"
    }

    sourceSets {
        getByName("main") {
            kotlin.directories += repo.resolve("target/android/kotlin").path
            jniLibs.directories += repo.resolve("target/android/jniLibs").path
            assets.directories += repo.resolve("target/android-spike/assets").path
        }
    }

    // The fixtures are copied out of the APK once.
    androidResources {
        noCompress += listOf("pdf", "docx", "txt")
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
}

dependencies {
    implementation("net.java.dev.jna:jna:5.19.1@aar")
    androidTestImplementation("androidx.test:runner:1.7.0")
    androidTestImplementation("androidx.test.ext:junit:1.3.0")
}
