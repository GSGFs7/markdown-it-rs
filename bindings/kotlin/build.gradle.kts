import org.gradle.api.tasks.Exec
import org.gradle.api.tasks.testing.Test

plugins {
    alias(libs.plugins.kotlin.jvm)
}

group = "moe.gsgfs"
version = "0.1.0-SNAPSHOT"

repositories {
    mavenCentral()
}

dependencies {
    implementation(libs.jna)
    testImplementation(kotlin("test"))
}

kotlin {
    jvmToolchain(17)
}

val repositoryRoot = rootDir.resolve("../..").canonicalFile
val cargoTargetDirectory = repositoryRoot.resolve("target")
val generatedKotlinDirectory = layout.buildDirectory.dir("generated/uniffi") // in "./build" dir
val rustFeatures = providers.gradleProperty("rustFeatures").orElse("")

fun nativeLibraryName(): String {
    val os = System.getProperty("os.name").lowercase()
    return when {
        os.contains("win") -> "markdown_it_rs_kt.dll"
        os.contains("mac") || os.contains("darwin") -> "libmarkdown_it_rs_kt.dylib"
        else -> "libmarkdown_it_rs_kt.so"
    }
}

val nativeLibrary = cargoTargetDirectory.resolve("debug").resolve(nativeLibraryName())

fun cargoFeatures(includeBindgen: Boolean): String = buildList {
    if (includeBindgen) add("bindgen")
    addAll(
        rustFeatures.get().split(',').map(String::trim).filter(String::isNotEmpty),
    )
}.distinct().joinToString(",")

// task 1, build rust lib
val buildRust = tasks.register<Exec>("buildRust") {
    group = "rust"
    description = "Build the native Rust library for the current host."
    workingDir(repositoryRoot)
    environment("CARGO_TARGET_DIR", cargoTargetDirectory)

    // pass features paranaters
    val features = cargoFeatures(includeBindgen = false)
    commandLine(
        buildList {
            addAll(listOf("cargo", "build", "-p", "markdown-it-rs-kt"))
            if (features.isNotEmpty()) addAll(listOf("--features", features))
        },
    )

    // include rust sourse code
    inputs.files(
        repositoryRoot.resolve("Cargo.toml"),
        rootDir.resolve("Cargo.toml"),
        fileTree(repositoryRoot.resolve("src")) { include("**/*.rs") },
        fileTree(repositoryRoot.resolve("crates/markdown-it-url/src")) { include("**/*.rs") },
        fileTree(rootDir.resolve("src")) { include("**/*.rs") },
    )
    outputs.file(nativeLibrary)
}

// task 2, generate UniFFI binding
val generateUniFfiBindings = tasks.register<Exec>("generateUniFfiBindings") {
    group = "rust"
    description = "Generate Kotlin sources from the compiled UniFFI component."
    dependsOn(buildRust)
    workingDir(repositoryRoot)
    environment("CARGO_TARGET_DIR", cargoTargetDirectory)

    val features = cargoFeatures(includeBindgen = true)
    commandLine(
        "cargo",
        "run",
        "-p",
        "markdown-it-rs-kt",
        "--features",
        features,
        "--bin",
        "uniffi-bindgen",
        "--",
        "generate",
        "--library",
        nativeLibrary.absolutePath,
        "--language",
        "kotlin",
        "--out-dir",
        generatedKotlinDirectory.get().asFile.absolutePath,
        "--config",
        rootDir.resolve("uniffi.toml").absolutePath,
        "--no-format",
    )

    inputs.files(
        nativeLibrary,
        rootDir.resolve("uniffi.toml"),
    )
    outputs.dir(generatedKotlinDirectory)

    doFirst {
        delete(generatedKotlinDirectory)
    }
}

// tesk 3, run test
val cargoTest = tasks.register<Exec>("cargoTest") {
    group = "verification"
    description = "Run tests for the Rust binding facade."
    workingDir(repositoryRoot)
    environment("CARGO_TARGET_DIR", cargoTargetDirectory)

    val features = cargoFeatures(includeBindgen = false)
    commandLine(
        buildList {
            addAll(listOf("cargo", "test", "-p", "markdown-it-rs-kt"))
            if (features.isNotEmpty()) addAll(listOf("--features", features))
        },
    )
}

kotlin.sourceSets.named("main") {
    kotlin.srcDir(generatedKotlinDirectory)
    kotlin.srcDir("kotlin/src/main/kotlin")
}

kotlin.sourceSets.named("test") {
    kotlin.srcDir("kotlin/src/test/kotlin")
}

tasks.named("compileKotlin") {
    dependsOn(generateUniFfiBindings)
}

tasks.named<Test>("test") {
    dependsOn(buildRust)
    useJUnitPlatform()
    systemProperty("jna.library.path", nativeLibrary.parentFile.absolutePath)
}

tasks.named("check") {
    dependsOn(cargoTest)
}
