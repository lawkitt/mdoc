package io.github.lawkitt.mdoc

import java.io.File
import java.nio.file.Files
import org.json.JSONArray
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import uniffi.mdoc_ffi.Candidate
import uniffi.mdoc_ffi.Category
import uniffi.mdoc_ffi.DocumentSession
import uniffi.mdoc_ffi.ImportOptions
import uniffi.mdoc_ffi.MdocLibrary
import uniffi.mdoc_ffi.MentionState
import uniffi.mdoc_ffi.ReviewSnapshot
import uniffi.mdoc_ffi.TextRange

/**
 * ADR 0085's parity tests on the JVM (ADR 0089): the core called through
 * the Kotlin bindings gives the same bytes as the desktop's GUI-free path,
 * checked against the goldens `crates/mdoc-ffi/tests/parity.rs` writes in
 * `tests/fixtures/mobile-parity/`. The twin of the iOS `ParityTests`; the
 * core is the desktop build of `mdoc-ffi` (`target/android/host`).
 */
class ParityTest {
    private val repo = File(System.getProperty("mdoc.repo") ?: error("mdoc.repo is not set"))

    private fun read(path: String) = File(repo, path).readText()

    private data class Identity(val category: String, val originals: List<String>)

    private fun identities(): List<Identity> {
        val array = JSONArray(read("tests/fixtures/restore/claude-en/identities.json"))
        return (0 until array.length()).map { i ->
            val item = array.getJSONObject(i)
            val originals = item.getJSONArray("originals")
            Identity(item.getString("category"), (0 until originals.length()).map(originals::getString))
        }
    }

    private fun category(token: String) = when (token) {
        "PERSON" -> Category.PERSON
        "ORG" -> Category.ORGANIZATION
        "EMAIL" -> Category.EMAIL
        "PHONE" -> Category.PHONE
        "ADDRESS" -> Category.ADDRESS
        "IDENTITY" -> Category.IDENTITY
        "TAX" -> Category.TAX
        "BANK" -> Category.BANK
        "DATE" -> Category.DATE
        else -> Category.OTHER
    }

    /**
     * Every whole occurrence of each original, as Rust's `match_indices`
     * finds them (left to right, not overlapping, in UTF-8 bytes), sorted
     * by range.
     */
    private fun candidates(text: String, identities: List<Identity>): List<Candidate> {
        val haystack = text.toByteArray(Charsets.UTF_8)
        val found = mutableListOf<Candidate>()
        for (identity in identities) {
            for (original in identity.originals) {
                val needle = original.toByteArray(Charsets.UTF_8)
                var start = 0
                while (start + needle.size <= haystack.size) {
                    if (haystack.copyOfRange(start, start + needle.size).contentEquals(needle)) {
                        val range = TextRange(start.toULong(), (start + needle.size).toULong())
                        found += Candidate(range, category(identity.category), 0.9f)
                        start += needle.size
                    } else {
                        start += 1
                    }
                }
            }
        }
        return found.sortedWith(compareBy({ it.range.start }, { it.range.end }))
    }

    /** Mentions as the goldens list them: range, state, alias, original. */
    private fun mentions(snapshot: ReviewSnapshot) = snapshot.mentions.joinToString("") { mention ->
        val state = when (mention.state) {
            MentionState.PROPOSED -> "Proposed"
            MentionState.REPLACED -> "Replaced"
            MentionState.KEPT -> "Kept"
        }
        "${mention.range.start}..${mention.range.end}\t$state\t${mention.alias ?: "-"}\t${mention.original}\n"
    }

    @Test
    fun settlementDocxMatchesTheGoldens() {
        val docx = File(repo, "docs/product-media/Settlement agreement.docx")
        val goldens = "tests/fixtures/mobile-parity/settlement-docx/"

        DocumentSession().use { session ->
            val opened = session.importFile(docx.path, ImportOptions(ocr = null, nativeTextOnly = false), null)
            assertEquals("Settlement agreement", opened.title)
            assertEquals("conversion differs from desktop's", read(goldens + "markdown.md"), opened.text)

            val found = candidates(opened.text, identities())
            assertTrue(found.size > 20)
            val proposed = session.addCandidates(found)
            assertTrue(proposed.mentions.all { it.state == MentionState.PROPOSED })
            assertEquals(read(goldens + "candidates.tsv"), mentions(proposed))

            val replaced = session.replaceAll()
            assertEquals(read(goldens + "replaced.tsv"), mentions(replaced))
            val copied = session.copyForAi()
            assertEquals(read(goldens + "copy-for-ai.md"), copied)
            assertEquals(copied, session.shareText())

            // An AI that answers with the document itself gets every original back.
            val body = copied.indexOf("\n\n---\n\n").also { assertTrue(it >= 0) } + "\n\n---\n\n".length
            val restored = session.restoreReply("Summary of the agreement:\n\n" + copied.substring(body))
            assertEquals(read(goldens + "restored.md"), restored.text)
            assertEquals(0uL, restored.unrecognised)
        }
    }

    /**
     * The Restore with mdoc path: a saved document's case restores a reply
     * exactly as the document's session does.
     */
    @Test
    fun libraryRestoreMatchesTheSession() {
        val root = Files.createTempDirectory("mdoc-parity").toFile()
        try {
            MdocLibrary(root.path, ByteArray(32) { 42 }).use { library ->
                val markdown = read("docs/product-media/Settlement agreement.md")
                DocumentSession().use { session ->
                    session.importText(markdown, "Settlement agreement")
                    session.addCandidates(candidates(markdown, identities()))
                    session.replaceAll()
                    val saved = session.save(library)
                    val copied = session.copyForAi()
                    val body = copied.indexOf("\n\n---\n\n") + "\n\n---\n\n".length
                    val reply = "Summary:\n\n" + copied.substring(body)
                    val inSession = session.restoreReply(reply)
                    val inShareTarget = library.restore(saved.case.id, reply)
                    assertEquals(inSession.text, inShareTarget.text)
                    assertEquals("Summary:\n\n$markdown", inShareTarget.text)
                }
            }
        } finally {
            root.deleteRecursively()
        }
    }
}
