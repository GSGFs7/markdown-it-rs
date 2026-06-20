import unittest

from markdown_it_rs_py import MarkdownIt


YAML_FRONTMATTER_INPUT = "---\ntitle: Test\n---\n# heading"
TOML_FRONTMATTER_INPUT = "+++\ntitle = 'Test'\n+++\n# heading"
UNCLOSED_FRONTMATTER_INPUT = "---\ntitle: Test\n# heading"


class FrontmatterTests(unittest.TestCase):
    def test_disable_frontmatter(self):
        md = MarkdownIt()
        html = md.render(YAML_FRONTMATTER_INPUT)

        self.assertIn("<hr>", html)
        self.assertIn("<h2>title: Test</h2>", html)
        self.assertIn("<h1>heading</h1>", html)

    def test_enable_yaml_frontmatter(self):
        md = MarkdownIt(frontmatter=True)
        html = md.render(YAML_FRONTMATTER_INPUT)

        self.assertIn("<h1>heading</h1>", html)
        self.assertNotIn("title", html)
        self.assertNotIn("Test", html)
        self.assertNotIn("<hr>", html)

    def test_parse_yaml_frontmatter(self):
        md = MarkdownIt(frontmatter=True)
        frontmatter = md.parse_frontmatter(YAML_FRONTMATTER_INPUT)

        self.assertIsNotNone(frontmatter)
        self.assertEqual(frontmatter.kind, "yaml")
        self.assertEqual(frontmatter.raw, "title: Test")
        self.assertEqual(frontmatter.start_line, 0)
        self.assertEqual(frontmatter.end_line, 2)

    def test_render_with_yaml_frontmatter(self):
        md = MarkdownIt(frontmatter=True)
        result = md.render_with_frontmatter(YAML_FRONTMATTER_INPUT)

        self.assertEqual(result.html, "<h1>heading</h1>\n")
        self.assertIsNotNone(result.frontmatter)
        self.assertEqual(result.frontmatter.kind, "yaml")
        self.assertEqual(result.frontmatter.raw, "title: Test")

    def test_enable_toml_frontmatter(self):
        md = MarkdownIt(frontmatter=True)
        html = md.render(TOML_FRONTMATTER_INPUT)

        self.assertIn("<h1>heading</h1>", html)
        self.assertNotIn("title", html)
        self.assertNotIn("+++", html)

    def test_parse_toml_frontmatter(self):
        md = MarkdownIt(frontmatter=True)
        frontmatter = md.parse_frontmatter(TOML_FRONTMATTER_INPUT)

        self.assertIsNotNone(frontmatter)
        self.assertEqual(frontmatter.kind, "toml")
        self.assertEqual(frontmatter.raw, "title = 'Test'")

    def test_parse_frontmatter_disabled(self):
        md = MarkdownIt()
        self.assertIsNone(md.parse_frontmatter(YAML_FRONTMATTER_INPUT))

    def test_render_with_frontmatter_disabled(self):
        md = MarkdownIt()
        result = md.render_with_frontmatter(YAML_FRONTMATTER_INPUT)

        self.assertIn("<h1>heading</h1>", result.html)
        self.assertIsNone(result.frontmatter)

    def test_unclosed_frontmatter(self):
        md = MarkdownIt(frontmatter=True)
        self.assertEqual(
            md.render(UNCLOSED_FRONTMATTER_INPUT),
            "<hr>\n<p>title: Test</p>\n<h1>heading</h1>\n",
        )


if __name__ == "__main__":
    unittest.main()
