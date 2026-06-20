import unittest

from markdown_it_rs_py import MarkdownIt, available_syntax_themes


class SyntaxHighlightingTests(unittest.TestCase):
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
