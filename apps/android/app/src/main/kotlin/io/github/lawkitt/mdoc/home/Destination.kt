package io.github.lawkitt.mdoc.home

import androidx.compose.runtime.saveable.Saver
import io.github.lawkitt.mdoc.core.ProjectId

/**
 * What the main area shows when no document is open (#1091 Decision 33):
 * Home, a sidebar row's page, or a Project's page. Each is a top-level
 * destination, as a navigation drawer's are on Android.
 */
sealed interface Destination {
    data object Home : Destination

    data object AllProjects : Destination

    data object Unfiled : Destination

    data object AllDocuments : Destination

    data class Project(val id: ProjectId) : Destination

    /** The Project whose page shows, if one does: what is added there lands in it (Decision 35). */
    val project: ProjectId? get() = (this as? Project)?.id

    /** Where Back goes from here: Home, Material's start destination; null from Home, which leaves mdoc. */
    val back: Destination? get() = if (this == Home) null else Home

    companion object {
        /** Kept across a recreated activity as one string. */
        val Saver: Saver<Destination, String> = Saver(save = { encode(it) }, restore = { decode(it) })

        fun encode(destination: Destination): String = when (destination) {
            Home -> "home"
            AllProjects -> "projects"
            Unfiled -> "unfiled"
            AllDocuments -> "documents"
            is Project -> "project:${destination.id.raw}"
        }

        fun decode(saved: String): Destination = when {
            saved == "projects" -> AllProjects
            saved == "unfiled" -> Unfiled
            saved == "documents" -> AllDocuments
            saved.startsWith("project:") -> Project(ProjectId(saved.removePrefix("project:")))
            else -> Home
        }
    }
}
