package moe.gsgfs.markdownit

import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertNotNull
import kotlin.test.assertTrue

class MarkdownParserTest {
    @Test
    fun rendersCommonMark() {
        MarkdownParser().use { parser ->
            assertEquals(
                "<h1>Hello <strong>Kotlin</strong></h1>\n",
                parser.render("# Hello **Kotlin**"),
            )
        }
    }

    @Test
    fun returnsFrontMatterMetadata() {
        MarkdownParser(MarkdownOptions(frontMatter = true)).use { parser ->
            val result = parser.renderWithMetadata("---\ntitle: Kotlin\n---\n# Hello")
            val frontMatter = assertNotNull(result.frontMatter)

            assertEquals("<h1>Hello</h1>\n", result.html)
            assertEquals(FrontMatterKind.YAML, frontMatter.kind)
            assertEquals("title: Kotlin", frontMatter.raw)
            assertEquals(0UL, frontMatter.startLine)
            assertEquals(2UL, frontMatter.endLine)
        }
    }

    @Test
    fun enablesPluginsAtConstruction() {
        val options = MarkdownOptions(
            headingAnchors = true,
            taskList = true,
        )

        MarkdownParser(options).use { parser ->
            val html = parser.render("# Hello Kotlin\n\n- [x] bound")
            assertTrue(html.contains("<h1 id=\"hello-kotlin\">"))
            assertTrue(html.contains("task-list-item"))
        }
    }
}
