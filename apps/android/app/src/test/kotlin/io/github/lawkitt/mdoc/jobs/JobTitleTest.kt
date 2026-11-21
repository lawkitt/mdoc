package io.github.lawkitt.mdoc.jobs

import io.github.lawkitt.mdoc.R
import java.io.File
import javax.xml.parsers.DocumentBuilderFactory
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Test
import org.w3c.dom.Element

/**
 * A job's notification shows outside the app, so its title never names the
 * document: iOS's continued task shows the same words (DocumentViewModel).
 */
class JobTitleTest {
    private val strings: Map<String, String> = run {
        val repo = File(System.getProperty("mdoc.repo") ?: error("mdoc.repo is not set"))
        val xml = DocumentBuilderFactory.newInstance().newDocumentBuilder()
            .parse(File(repo, "apps/android/app/src/main/res/values/strings.xml"))
        val nodes = xml.getElementsByTagName("string")
        (0 until nodes.length).map { nodes.item(it) as Element }.associate { it.getAttribute("name") to it.textContent }
    }

    private fun text(id: Int): String {
        val name = R.string::class.java.fields.first { it.getInt(null) == id }.name
        return strings.getValue(name)
    }

    @Test
    fun notificationTitlesNeverNameTheDocument() {
        assertEquals("Recognizing a document", text(JobKind.Recognize.title))
        assertEquals("Finding names", text(JobKind.Scan.title))
        for (kind in JobKind.entries) {
            assertFalse("$kind's title takes an argument", "%" in text(kind.title))
        }
    }
}
