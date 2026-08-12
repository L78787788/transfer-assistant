import java.io.FileInputStream
import java.util.Properties

plugins {
    id("com.android.application")
    // The Flutter Gradle Plugin must be applied after the Android and Kotlin Gradle plugins.
    id("dev.flutter.flutter-gradle-plugin")
}

val releaseKeyPropertiesFile = rootProject.file("key.properties")
val releaseKeyProperties = Properties().apply {
    if (releaseKeyPropertiesFile.exists()) {
        FileInputStream(releaseKeyPropertiesFile).use(::load)
    }
}
val releaseSigningConfigured = listOf(
    "storeFile",
    "storePassword",
    "keyAlias",
    "keyPassword",
).all { releaseKeyProperties.getProperty(it).isNullOrBlank().not() }

android {
    namespace = "com.transassist.transfer_assistant"
    compileSdk = flutter.compileSdkVersion
    ndkVersion = "29.0.14206865"

    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }

    defaultConfig {
        applicationId = "com.transassist.transfer_assistant"
        minSdk = 28
        targetSdk = flutter.targetSdkVersion
        versionCode = flutter.versionCode
        versionName = flutter.versionName
        ndk {
            abiFilters += "arm64-v8a"
        }
    }

    signingConfigs {
        if (releaseSigningConfigured) {
            create("release") {
                storeFile = rootProject.file(releaseKeyProperties.getProperty("storeFile"))
                storePassword = releaseKeyProperties.getProperty("storePassword")
                keyAlias = releaseKeyProperties.getProperty("keyAlias")
                keyPassword = releaseKeyProperties.getProperty("keyPassword")
            }
        }
    }

    buildTypes {
        release {
            if (releaseSigningConfigured) {
                signingConfig = signingConfigs.getByName("release")
            }
        }
    }

    sourceSets {
        getByName("main").jniLibs.srcDir("../../build/rustJniLibs")
    }
}

gradle.taskGraph.whenReady {
    val requestsReleaseArtifact = allTasks.any { task ->
        task.project == project &&
            task.name.contains("release", ignoreCase = true) &&
            (task.name.startsWith("assemble") || task.name.startsWith("package"))
    }
    if (requestsReleaseArtifact && !releaseSigningConfigured) {
        throw GradleException(
            "Android Release signing is not configured. Run scripts/ensure-android-signing.ps1.",
        )
    }
}

val buildRustCore by tasks.registering(Exec::class) {
    val repositoryRoot = rootProject.projectDir.resolve("../..").canonicalFile
    workingDir(repositoryRoot)
    commandLine(
        "powershell",
        "-NoProfile",
        "-ExecutionPolicy",
        "Bypass",
        "-File",
        repositoryRoot.resolve("scripts/build-android-rust-core.ps1"),
    )
}

tasks.configureEach {
    if (name.startsWith("merge") && name.endsWith("JniLibFolders")) {
        dependsOn(buildRustCore)
    }
}

kotlin {
    compilerOptions {
        jvmTarget = org.jetbrains.kotlin.gradle.dsl.JvmTarget.JVM_17
    }
}

flutter {
    source = "../.."
}
