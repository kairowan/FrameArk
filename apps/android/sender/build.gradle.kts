plugins {
    id("com.android.application")
    id("org.jetbrains.kotlin.android")
}

val playbackProbe = providers.gradleProperty("framearkPlaybackProbe").orNull == "true"
android {
    namespace = "dev.frameark.sender"
    compileSdk = 36
    defaultConfig {
        applicationId = "dev.frameark.sender"
        minSdk = 26
        targetSdk = 36
        versionCode = 1
        versionName = "0.1.0"
        testInstrumentationRunner = "android.test.InstrumentationTestRunner"
        ndk { abiFilters += if (playbackProbe) listOf("x86_64") else listOf("arm64-v8a") }
    }
    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }
    kotlin { compilerOptions { jvmTarget.set(org.jetbrains.kotlin.gradle.dsl.JvmTarget.JVM_17) } }
    // Both apps consume the same Rust bridge output, never a second protocol implementation.
    sourceSets["main"].jniLibs.srcDir(rootProject.file("apps/android/receiver/src/main/jniLibs"))
    if (playbackProbe) {
        sourceSets["main"].jniLibs.srcDir(rootProject.file("build/probe-jni"))
        sourceSets["debug"].java.srcDir("src/playbackProbe/java")
        sourceSets["debug"].manifest.srcFile("src/playbackProbe/AndroidManifest.xml")
    }
    useLibrary("android.test.runner", false)
    useLibrary("android.test.base", false)
}
dependencies { testImplementation("junit:junit:4.13.2") }
