plugins {
    alias(libs.plugins.android.library)
}

android {
    namespace = "fr.frohub.weft.protocol"
    compileSdk = 36

    defaultConfig {
        minSdk = 26
        testInstrumentationRunner = "androidx.test.runner.AndroidJUnitRunner"
        consumerProguardFiles("consumer-rules.pro")
    }

    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }

    sourceSets {
        // The Rust reference test vectors, read in place (not copied into this tree).
        getByName("androidTest").assets.srcDir(rootProject.file("../crates/weft-protocol/vectors"))
    }
}

dependencies {
    implementation(libs.lazysodium.android) {
        // The plain JNA jar has no Android native libraries; the AAR below does.
        exclude(group = "net.java.dev.jna", module = "jna")
    }
    implementation(libs.jna) {
        artifact { type = "aar" }
    }

    testImplementation(libs.junit)
    androidTestImplementation(libs.junit)
    androidTestImplementation(libs.androidx.test.runner)
    androidTestImplementation(libs.androidx.test.ext.junit)
}
