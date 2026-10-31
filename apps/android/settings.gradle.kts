// The mdoc Android app (ADR 0089): Compose over the Rust core's Kotlin bindings.
pluginManagement {
    repositories {
        google()
        mavenCentral()
        gradlePluginPortal()
    }
}
dependencyResolutionManagement {
    repositoriesMode.set(RepositoriesMode.FAIL_ON_PROJECT_REPOS)
    repositories {
        google()
        mavenCentral()
    }
}
rootProject.name = "mdoc"
include(":app")
// The long-job device test: its own process, so it can kill mdoc mid-job.
include(":longjobs")
