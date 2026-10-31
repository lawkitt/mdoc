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
            kotlin.srcDir(repo.resolve("target/android/kotlin"))
            jniLibs.srcDir(repo.resolve("target/android/jniLibs"))
            assets.srcDir(repo.resolve("target/android-spike/assets"))
        }
    }

    // The models are large and already compressed; the app copies them out once.
    androidResources {
        noCompress += listOf("onnx", "bin", "json", "pdf", "docx", "txt")
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
