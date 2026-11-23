package io.github.lawkitt.mdoc.core

import uniffi.mdoc_ffi.ReplyVault
import uniffi.mdoc_ffi.SharedFrom

/** A place Restore with mdoc restores with: a Project, or an Unfiled document. */
data class RestorePlace(val place: Place, val name: String, val colour: ProjectColour?)

/** The places to pick from, Projects first, and the one picked first. */
data class RestorePlaces(
    val projects: List<RestorePlace> = emptyList(),
    val unfiled: List<RestorePlace> = emptyList(),
    val default: Place? = null,
) {
    val all: List<RestorePlace> get() = projects + unfiled

    fun of(place: Place?): RestorePlace? = all.firstOrNull { it.place == place }
}

/**
 * Restore with mdoc over the Vault (#1091 Decisions 18, 20 and 37): it reads
 * the Projects and Unfiled documents and saves a reply, through the core's
 * `ReplyVault`, which writes nothing but new reply files and never the
 * manifest. The app takes a reply saved here in through `Vault::refresh`.
 */
class Replies(private val handle: ReplyVault, private val lastShared: LastShared) {
    /**
     * The places, as the app wrote them last: first the place text was
     * shared from last (Copy for AI or Share), while it is still there;
     * else the first Project, else the newest Unfiled document.
     */
    @Synchronized
    fun places(): RestorePlaces {
        runCatching { handle.refresh() }
        val projects = handle.projects().map {
            RestorePlace(Place.Project(ProjectId(it.id)), it.name, Documents.projectColour(it.hue))
        }
        val unfiled = handle.unfiled().map { RestorePlace(Place.Unfiled(it.id), it.title, null) }
        val places = RestorePlaces(projects, unfiled)
        val default = lastShared.get()?.takeIf { places.of(it) != null } ?: places.all.firstOrNull()?.place
        return places.copy(default = default)
    }

    /** `reply` restored with `place`'s names as they are now; nothing is saved. */
    fun restore(reply: String, place: Place): RestoreResult =
        restoreResult(core { handle.restoreText(sharedFrom(place), reply) })

    /** Save to mdoc: `reply`, as received, as an AI Reply of `place`. */
    fun save(reply: String, place: RestorePlace) {
        core { handle.saveReply(sharedFrom(place.place), Documents.replyTitle(place.name), null, reply) }
    }

    private fun sharedFrom(place: Place): SharedFrom = when (place) {
        is Place.Project -> SharedFrom.Project(place.id.raw)
        is Place.Unfiled -> SharedFrom.Document(place.document)
    }
}
