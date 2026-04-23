import unittest

from markdown_it_rs_py import MarkdownIt, available_syntax_themes


class MarkdownItTests(unittest.TestCase):
    def __init__(self, *args, **kwargs):
        super().__init__(*args, **kwargs)

        self.YAML_FRONTMATTER_INPUT = "---\ntitle: Test\n---\n# heading"
        self.TOML_FRONTMATTER_INPUT = "+++\ntitle = 'Test'\n+++\n# heading"
        self.UNCLOSED_FRONTMATTER_INPUT = "---\ntitle: Test\n# heading"

    def test_heading(self):
        md = MarkdownIt()
        self.assertEqual(md.render("# heading"), "<h1>heading</h1>\n")

    def test_strikethrough(self):
        md = MarkdownIt()
        self.assertEqual(md.render("~~114~~514"), "<p><s>114</s>514</p>\n")

    def test_disable_html(self):
        md = MarkdownIt()
        self.assertEqual(md.render("hello<br>world"), "<p>hello&lt;br&gt;world</p>\n")

    def test_enable_html(self):
        md = MarkdownIt(html=True)
        self.assertEqual(md.render("hello<br>world"), "<p>hello<br>world</p>\n")

    def test_disable_linkify(self):
        md = MarkdownIt()
        self.assertEqual(
            md.render("https://youtu.be/dQw4w9WgXcQ"),
            "<p>https://youtu.be/dQw4w9WgXcQ</p>\n",
        )

    def test_enable_linkify(self):
        md = MarkdownIt(linkify=True)
        self.assertEqual(
            md.render("https://youtu.be/dQw4w9WgXcQ"),
            '<p><a href="https://youtu.be/dQw4w9WgXcQ">youtu.be/dQw4w9WgXcQ</a></p>\n',
        )

    def test_disable_math(self):
        md = MarkdownIt()
        self.assertEqual(md.render("$E=mc^2$"), "<p>$E=mc^2$</p>\n")

    def test_enable_inline_math(self):
        md = MarkdownIt(math=True)
        self.assertIn('<span class="math-inline">', md.render("$E=mc^2$"))

    def test_enable_block_math(self):
        md = MarkdownIt(math=True)
        self.assertIn('<div class="math-block">', md.render("$$\nE=mc^2\n$$"))

    def test_disable_frontmatter(self):
        md = MarkdownIt()
        html = md.render(self.YAML_FRONTMATTER_INPUT)

        self.assertIn("<hr>", html)
        self.assertIn("<h2>title: Test</h2>", html)
        self.assertIn("<h1>heading</h1>", html)

    def test_enable_yaml_frontmatter(self):
        md = MarkdownIt(frontmatter=True)
        html = md.render(self.YAML_FRONTMATTER_INPUT)

        self.assertIn("<h1>heading</h1>", html)
        self.assertNotIn("title", html)
        self.assertNotIn("Test", html)
        self.assertNotIn("<hr>", html)

    def test_parse_yaml_frontmatter(self):
        md = MarkdownIt(frontmatter=True)
        frontmatter = md.parse_frontmatter(self.YAML_FRONTMATTER_INPUT)

        self.assertIsNotNone(frontmatter)
        self.assertEqual(frontmatter.kind, "yaml")
        self.assertEqual(frontmatter.raw, "title: Test")
        self.assertEqual(frontmatter.start_line, 0)
        self.assertEqual(frontmatter.end_line, 2)

    def test_render_with_yaml_frontmatter(self):
        md = MarkdownIt(frontmatter=True)
        result = md.render_with_frontmatter(self.YAML_FRONTMATTER_INPUT)

        self.assertEqual(result.html, "<h1>heading</h1>\n")
        self.assertIsNotNone(result.frontmatter)
        self.assertEqual(result.frontmatter.kind, "yaml")
        self.assertEqual(result.frontmatter.raw, "title: Test")

    def test_enable_toml_frontmatter(self):
        md = MarkdownIt(frontmatter=True)
        html = md.render(self.TOML_FRONTMATTER_INPUT)

        self.assertIn("<h1>heading</h1>", html)
        self.assertNotIn("title", html)
        self.assertNotIn("+++", html)

    def test_parse_toml_frontmatter(self):
        md = MarkdownIt(frontmatter=True)
        frontmatter = md.parse_frontmatter(self.TOML_FRONTMATTER_INPUT)

        self.assertIsNotNone(frontmatter)
        self.assertEqual(frontmatter.kind, "toml")
        self.assertEqual(frontmatter.raw, "title = 'Test'")

    def test_parse_frontmatter_disabled(self):
        md = MarkdownIt()
        self.assertIsNone(md.parse_frontmatter(self.YAML_FRONTMATTER_INPUT))

    def test_render_with_frontmatter_disabled(self):
        md = MarkdownIt()
        result = md.render_with_frontmatter(self.YAML_FRONTMATTER_INPUT)

        self.assertIn("<h1>heading</h1>", result.html)
        self.assertIsNone(result.frontmatter)

    def test_unclosed_frontmatter(self):
        md = MarkdownIt(frontmatter=True)
        self.assertEqual(
            md.render(self.UNCLOSED_FRONTMATTER_INPUT),
            "<hr>\n<p>title: Test</p>\n<h1>heading</h1>\n",
        )

    def test_disable_typographer(self):
        md = MarkdownIt()
        self.assertEqual(
            md.render("Something(TM)..."),
            "<p>Something(TM)...</p>\n",
        )

    def test_enable_typographer(self):
        md = MarkdownIt(typographer=True)
        self.assertEqual(
            md.render("Something(TM)..."),
            "<p>Something™…</p>\n",
        )

    def test_disable_sourcepos(self):
        md = MarkdownIt()
        self.assertEqual(md.render("# hello"), "<h1>hello</h1>\n")

    def test_enable_sourcepos(self):
        md = MarkdownIt(sourcepos=True)
        self.assertEqual(
            md.render("# hello"),
            '<h1 data-sourcepos="1:1-1:7">hello</h1>\n',
        )

    def test_disable_heading_anchors(self):
        md = MarkdownIt()
        self.assertEqual(
            md.render("## Ciallo ～(∠・ω< )⌒★!"),
            "<h2>Ciallo ～(∠・ω&lt; )⌒★!</h2>\n",
        )

    def test_enable_heading_anchors(self):
        md = MarkdownIt(heading_anchors=True)
        html = md.render("## Ciallo ～(∠・ω< )⌒★!")

        self.assertIn('<h2 id="ciallo', html)
        self.assertIn("Ciallo ～(∠・ω&lt; )⌒★!", html)

    def test_disable_syntax_highlighting(self):
        md = MarkdownIt()
        self.assertEqual(
            md.render("```rust\nfn main() {}\n```"),
            '<pre><code class="language-rust">fn main() {}\n</code></pre>\n',
        )

    def test_enable_syntax_highlighting(self):
        md = MarkdownIt(syntax_highlighting=True)
        html = md.render("```rust\nfn main() {}\n```")

        self.assertIn('<code class="language-rust">', html)
        self.assertIn('class="syntect-line"', html)
        self.assertIn("<span", html)

    def test_syntax_highlighting_classed_mode(self):
        md = MarkdownIt(syntax_highlighting=True, syntax_classed=True)
        html = md.render("```rust\nfn main() {}\n```")

        self.assertIn('<code class="syntect-code language-rust">', html)
        self.assertIn('class="syntect-line"', html)
        self.assertIsNotNone(md.syntax_theme_css())
        self.assertIn(".syntect-code", md.syntax_theme_css())

    def test_syntax_theme_css_inline_mode(self):
        md = MarkdownIt(syntax_highlighting=True)
        self.assertIsNone(md.syntax_theme_css())

    def test_available_syntax_themes(self):
        themes = available_syntax_themes()

        self.assertIn("InspiredGitHub", themes)
        self.assertEqual(themes, sorted(themes))

    def test_syntax_theme(self):
        md = MarkdownIt(
            syntax_highlighting=True,
            syntax_theme="base16-ocean.dark",
            syntax_classed=True,
        )

        self.assertIn(".syntect-code", md.syntax_theme_css())

    def test_unknown_syntax_theme(self):
        with self.assertRaisesRegex(ValueError, "unknown syntect theme"):
            MarkdownIt(syntax_highlighting=True, syntax_theme="a invalid theme")


if __name__ == "__main__":
    unittest.main()
