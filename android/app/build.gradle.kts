plugins {
    alias(libs.plugins.android.application)
    alias(libs.plugins.kotlin.compose)
}

/** A non-blank environment variable, or null. */
fun env(name: String): String? = providers.environmentVariable(name).orNull?.takeIf { it.isNotBlank() }

/** Where the app looks for new versions: the asset of the latest GitHub release. */
val releaseUpdateUrl = "https://github.com/fraustiz/weft/releases/latest/download/android-update.json"

/**
 * Release signing (CI): all four variables, or none. With none the release APK stays unsigned,
 * as before; with only some the build stops, so a half-configured CI does not ship unsigned.
 */
val signingEnv = listOf("WEFT_KEYSTORE_FILE", "WEFT_KEYSTORE_PASSWORD", "WEFT_KEY_ALIAS", "WEFT_KEY_PASSWORD")
    .associateWith { env(it) }
val releaseSigned = signingEnv.values.all { it != null }
if (!releaseSigned && signingEnv.values.any { it != null }) {
    val missing = signingEnv.filterValues { it == null }.keys.joinToString()
    throw GradleException("Release signing needs all of WEFT_KEYSTORE_FILE, WEFT_KEYSTORE_PASSWORD, WEFT_KEY_ALIAS, WEFT_KEY_PASSWORD; missing: $missing")
}

android {
    namespace = "fr.frohub.weft"
    compileSdk = 36

    defaultConfig {
        applicationId = "fr.frohub.weft"
        minSdk = 26
        targetSdk = 36
        // CI sets both from the release tag (versionCode: the commit count); local builds keep these.
        versionCode = env("WEFT_VERSION_CODE")?.let {
            it.toIntOrNull()?.takeIf { code -> code > 0 } ?: throw GradleException("WEFT_VERSION_CODE must be a positive integer, got \"$it\"")
        } ?: 1
        versionName = env("WEFT_VERSION_NAME") ?: "0.1.0"
        testInstrumentationRunner = "androidx.test.runner.AndroidJUnitRunner"
    }

    signingConfigs {
        if (releaseSigned) {
            create("release") {
                storeFile = file(signingEnv.getValue("WEFT_KEYSTORE_FILE")!!)
                storePassword = signingEnv.getValue("WEFT_KEYSTORE_PASSWORD")
                keyAlias = signingEnv.getValue("WEFT_KEY_ALIAS")
                keyPassword = signingEnv.getValue("WEFT_KEY_PASSWORD")
            }
        }
    }

    buildTypes {
        release {
            isMinifyEnabled = true
            isShrinkResources = true
            proguardFiles(getDefaultProguardFile("proguard-android-optimize.txt"))
            if (releaseSigned) signingConfig = signingConfigs.getByName("release")
            buildConfigField("String", "UPDATE_MANIFEST_URL", "\"$releaseUpdateUrl\"")
        }
        debug {
            // Debug builds only: WEFT_UPDATE_URL points the updater at a test server (for example
            // http://10.0.2.2:8000/android-update.json on the emulator).
            val url = env("WEFT_UPDATE_URL") ?: releaseUpdateUrl
            buildConfigField("String", "UPDATE_MANIFEST_URL", "\"$url\"")
        }
    }

    /*
     * Where the app is distributed. Same applicationId everywhere: one app, three channels.
     * github: the release APK on GitHub Releases, with the in-app updater.
     * fdroid: F-Droid updates the app itself, so no updater (nor its install permission).
     * play: no updater (Play policy), and none of the permissions Play keeps for default SMS
     *       and dialer apps (SMS, call log) nor the full screen intent (calling and alarm apps).
     * Each flavor's AndroidManifest.xml removes what it drops; the fields below gate the code.
     */
    flavorDimensions += "distribution"
    productFlavors {
        create("github") {
            dimension = "distribution"
            buildConfigField("boolean", "UPDATER_ENABLED", "true")
            buildConfigField("boolean", "SMS_ENABLED", "true")
            buildConfigField("boolean", "CALL_LOG_ENABLED", "true")
            buildConfigField("boolean", "FULL_SCREEN_INTENT_ENABLED", "true")
        }
        create("fdroid") {
            dimension = "distribution"
            buildConfigField("boolean", "UPDATER_ENABLED", "false")
            buildConfigField("boolean", "SMS_ENABLED", "true")
            buildConfigField("boolean", "CALL_LOG_ENABLED", "true")
            buildConfigField("boolean", "FULL_SCREEN_INTENT_ENABLED", "true")
        }
        create("play") {
            dimension = "distribution"
            buildConfigField("boolean", "UPDATER_ENABLED", "false")
            buildConfigField("boolean", "SMS_ENABLED", "false")
            buildConfigField("boolean", "CALL_LOG_ENABLED", "false")
            buildConfigField("boolean", "FULL_SCREEN_INTENT_ENABLED", "false")
        }
    }

    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }

    buildFeatures {
        compose = true
        buildConfig = true
    }
}

dependencies {
    implementation(project(":protocol"))
    implementation(libs.okhttp)
    implementation(libs.kotlinx.coroutines.android)
    implementation(platform(libs.compose.bom))
    implementation(libs.compose.material3)
    implementation(libs.compose.ui.tooling.preview)
    implementation(libs.activity.compose)
    implementation(libs.lifecycle.runtime.compose)
    implementation(libs.camera.camera2)
    implementation(libs.camera.lifecycle)
    implementation(libs.camera.view)
    implementation(libs.zxing.core)
    implementation(libs.androidx.core.ktx)
    debugImplementation(libs.compose.ui.tooling)
    debugImplementation(libs.compose.ui.test.manifest)

    testImplementation(libs.junit)
    testImplementation(libs.json)

    androidTestImplementation(platform(libs.compose.bom))
    androidTestImplementation(libs.compose.ui.test.junit4)
    androidTestImplementation(libs.junit)
    androidTestImplementation(libs.androidx.test.runner)
    androidTestImplementation(libs.androidx.test.core)
    androidTestImplementation(libs.androidx.test.ext.junit)
}
