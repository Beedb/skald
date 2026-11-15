import java.util.Properties
import org.jetbrains.kotlin.gradle.dsl.JvmTarget

plugins {
    id("com.android.application")
    id("org.jetbrains.kotlin.android")
    id("rust")
}

val tauriProperties = Properties().apply {
    val propFile = file("tauri.properties")
    if (propFile.exists()) {
        propFile.inputStream().use { load(it) }
    }
}

// Chave de release fora do repositório: gen/android/keystore.properties (ignorado pelo git)
// aponta para o .jks (storeFile, storePassword, keyAlias, keyPassword).
val keystoreProperties = Properties().apply {
    val f = rootProject.file("keystore.properties")
    if (f.exists()) f.inputStream().use { load(it) }
}

android {
    signingConfigs {
        if (keystoreProperties.getProperty("storeFile") != null) {
            create("release") {
                storeFile = file(keystoreProperties.getProperty("storeFile"))
                storePassword = keystoreProperties.getProperty("storePassword")
                keyAlias = keystoreProperties.getProperty("keyAlias")
                keyPassword = keystoreProperties.getProperty("keyPassword")
            }
        }
    }
    compileSdk = 37
    ndkVersion = "27.3.13750724"
    namespace = "com.skald.app"
    defaultConfig {
        manifestPlaceholders["usesCleartextTraffic"] = "false"
        applicationId = "com.skald.app"
        minSdk = 24
        targetSdk = 37
        versionCode = tauriProperties.getProperty("tauri.android.versionCode", "1").toInt()
        versionName = tauriProperties.getProperty("tauri.android.versionName", "1.0")
    }
    buildTypes {
        getByName("debug") {
            manifestPlaceholders["usesCleartextTraffic"] = "true"
            isDebuggable = true
            isJniDebuggable = true
            isMinifyEnabled = false
            packaging {
                jniLibs.keepDebugSymbols.add("*/arm64-v8a/*.so")
                jniLibs.keepDebugSymbols.add("*/armeabi-v7a/*.so")
                jniLibs.keepDebugSymbols.add("*/x86/*.so")
                jniLibs.keepDebugSymbols.add("*/x86_64/*.so")
            }
        }
        getByName("release") {
            signingConfigs.findByName("release")?.let { signingConfig = it }
            optimization {
               enable = true
            }
            proguardFiles(
                *fileTree(".") {
                  include("**/*.pro")
                  exclude("build/**")
                }.files.toTypedArray()
            )
        }
    }
    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_1_8
        targetCompatibility = JavaVersion.VERSION_1_8
    }
    buildFeatures {
        buildConfig = true
    }
    // Saída do ndk-build do GStreamer (ver a tarefa gstreamerNdkBuild abaixo).
    sourceSets {
        getByName("main") {
            java.srcDir("build/gstreamer/java")
            assets.srcDir("build/gstreamer/assets")
            jniLibs.srcDir("build/gstreamer/libs")
        }
    }
}

// Raiz do SDK do GStreamer para Android: variável GSTREAMER_ROOT_ANDROID ou
// `gstreamer.dir` em local.properties.
val gstreamerRoot: String = System.getenv("GSTREAMER_ROOT_ANDROID")
    ?: Properties().apply {
        val f = rootProject.file("local.properties")
        if (f.exists()) f.inputStream().use { load(it) }
    }.getProperty("gstreamer.dir")
    ?: throw GradleException("Defina GSTREAMER_ROOT_ANDROID ou gstreamer.dir em local.properties")

// Gera libgstreamer_android.so (+ GStreamer.java e assets) antes do Rust, que se liga a ela.
val gstreamerNdkBuild = tasks.register<Exec>("gstreamerNdkBuild") {
    val ndkBuild = android.ndkDirectory.resolve("ndk-build")
    inputs.files("jni/Android.mk", "jni/Application.mk")
    inputs.property("gstreamerRoot", gstreamerRoot)
    outputs.dirs("build/gstreamer/libs", "build/gstreamer/java", "build/gstreamer/assets")
    // O gstreamer-1.0.mk grava gst-android-build/ na pasta atual.
    workingDir = layout.buildDirectory.dir("gstreamer").get().asFile.apply { mkdirs() }
    commandLine(
        ndkBuild.path,
        "NDK_PROJECT_PATH=${projectDir}",
        "APP_BUILD_SCRIPT=${projectDir}/jni/Android.mk",
        "NDK_APPLICATION_MK=${projectDir}/jni/Application.mk",
        "NDK_OUT=${projectDir}/build/gstreamer/obj",
        "NDK_LIBS_OUT=${projectDir}/build/gstreamer/libs",
        "GSTREAMER_ROOT_ANDROID=$gstreamerRoot",
        "-j${Runtime.getRuntime().availableProcessors()}",
    )
}

tasks.named("preBuild") { dependsOn(gstreamerNdkBuild) }
tasks.matching { it.name.startsWith("rustBuild") }.configureEach { dependsOn(gstreamerNdkBuild) }

kotlin {
    compilerOptions {
        jvmTarget = JvmTarget.JVM_1_8
    }
}

rust {
    rootDirRel = "../../../"
}

dependencies {
    implementation("androidx.webkit:webkit:1.14.0")
    implementation("androidx.appcompat:appcompat:1.7.1")
    implementation("androidx.activity:activity-ktx:1.10.1")
    implementation("com.google.android.material:material:1.12.0")
    implementation("androidx.lifecycle:lifecycle-process:2.10.0")
    // MediaSession e notificação MediaStyle (reprodução em segundo plano).
    implementation("androidx.media:media:1.7.0")
    testImplementation("junit:junit:4.13.2")
    androidTestImplementation("androidx.test.ext:junit:1.1.4")
    androidTestImplementation("androidx.test.espresso:espresso-core:3.5.0")
}

apply(from = file("tauri.build.gradle.kts"))
