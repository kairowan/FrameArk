plugins {
    id("com.android.application")
    id("org.jetbrains.kotlin.android")
}

// Explicit local instrumentation opt-in; normal/release artifacts stay ARM64.
val playbackProbe = providers.gradleProperty("framearkPlaybackProbe").orNull == "true"

android {
    namespace = "dev.frameark.receiver"
    compileSdk = 36

    defaultConfig {
        applicationId = "dev.frameark.receiver"
        minSdk = 26
        targetSdk = 36
        versionCode = 1
        versionName = "0.1.0"
        testInstrumentationRunner = "android.test.InstrumentationTestRunner"
        ndk {
            abiFilters += if (playbackProbe) listOf("x86_64") else listOf("arm64-v8a")
        }
    }

    buildTypes {
        release {
            isMinifyEnabled = false
            proguardFiles(
                getDefaultProguardFile("proguard-android-optimize.txt"),
                "proguard-rules.pro",
            )
        }
    }

    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }

    kotlin {
        compilerOptions {
            jvmTarget.set(org.jetbrains.kotlin.gradle.dsl.JvmTarget.JVM_17)
        }
    }

    sourceSets["main"].jniLibs.srcDir("src/main/jniLibs")
    if (playbackProbe) sourceSets["main"].jniLibs.srcDir(rootProject.file("build/probe-jni"))
    useLibrary("android.test.runner", false)
    useLibrary("android.test.base", false)
}

dependencies {
    testImplementation("junit:junit:4.13.2")
}
