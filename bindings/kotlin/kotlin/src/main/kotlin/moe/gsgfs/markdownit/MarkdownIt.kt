package moe.gsgfs.markdownit

import moe.gsgfs.markdownit.internal.FrontMatter as NativeFrontMatter
import moe.gsgfs.markdownit.internal.FrontMatterKind as NativeFrontMatterKind
import moe.gsgfs.markdownit.internal.MarkdownException as NativeMarkdownException
import moe.gsgfs.markdownit.internal.MarkdownOptions as NativeMarkdownOptions
import moe.gsgfs.markdownit.internal.MarkdownParser as NativeMarkdownParser
import moe.gsgfs.markdownit.internal.availableSyntaxThemes as availableNativeSyntaxThemes

/** Options fixed when [MarkdownParser] is created. */
data class MarkdownOptions(
    val html: Boolean = false,
    val linkify: Boolean = false,
    val math: Boolean = false,
    val frontMatter: Boolean = false,
    val typographer: Boolean = false,
    val sourcePosition: Boolean = false,
    val headingAnchors: Boolean = false,
    /** Parses directives without sanitizing their rendered HTML attributes. */
    val directives: Boolean = false,
    val taskList: Boolean = false,
    val footnote: Boolean = false,
    val syntaxHighlighting: Boolean = false,
    val syntaxTheme: String? = null,
    val syntaxClassed: Boolean = false,
)

enum class FrontMatterKind {
    YAML, TOML,
}

data class FrontMatter(
    val kind: FrontMatterKind,
    val raw: String,
    val startLine: ULong,
    val endLine: ULong,
)

data class RenderResult(
    val html: String,
    val frontMatter: FrontMatter?,
)

class MarkdownConfigurationException internal constructor(
    message: String,
    cause: Throwable,
) : IllegalArgumentException(message, cause)

/** Reusable native Markdown parser. Call [close] when it is no longer needed. */
class MarkdownParser(
    options: MarkdownOptions = MarkdownOptions(),
) : AutoCloseable {
    // create the rust parser instance
    private val delegate = try {
        NativeMarkdownParser(options.toNative())
    } catch (error: NativeMarkdownException) {
        throw MarkdownConfigurationException(
            error.message ?: "Invalid Markdown parser configuration",
            error,
        )
    }

    fun render(source: String): String = delegate.render(source)

    fun renderWithMetadata(source: String): RenderResult = delegate.renderWithMetadata(source).let { result ->
        RenderResult(
            html = result.html,
            frontMatter = result.frontMatter?.toPublic(),
        )
    }

    fun syntaxThemeCss(): String? = delegate.syntaxThemeCss()

    override fun close() = delegate.close()

    companion object {
        fun availableSyntaxThemes(): List<String> = availableNativeSyntaxThemes()
    }
}

private fun MarkdownOptions.toNative() = NativeMarkdownOptions(
    html = html,
    linkify = linkify,
    math = math,
    frontMatter = frontMatter,
    typographer = typographer,
    sourcePosition = sourcePosition,
    headingAnchors = headingAnchors,
    directives = directives,
    taskList = taskList,
    footnote = footnote,
    syntaxHighlighting = syntaxHighlighting,
    syntaxTheme = syntaxTheme,
    syntaxClassed = syntaxClassed,
)

private fun NativeFrontMatter.toPublic() = FrontMatter(
    kind = when (kind) {
        NativeFrontMatterKind.YAML -> FrontMatterKind.YAML
        NativeFrontMatterKind.TOML -> FrontMatterKind.TOML
    },
    raw = raw,
    startLine = startLine,
    endLine = endLine,
)
