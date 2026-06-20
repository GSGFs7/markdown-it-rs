import unittest

from markdown_it_rs_py import MarkdownIt


YAML_FRONTMATTER_INPUT = "---\ntitle: Test\n---\n# heading"


class ExtensionTests(unittest.TestCase):
    def test_python_core_rule_can_mutate_ast(self):
        def plugin(md):
            md.add_core_rule(
                "footer",
                lambda root: root.append_html("<footer>generated</footer>\n"),
            )

        md = MarkdownIt().use(plugin)

        self.assertEqual(
            md.render("# heading"),
            "<h1>heading</h1>\n<footer>generated</footer>\n",
        )

    def test_python_core_rule_can_replace_children(self):
        def plugin(md):
            def replace(root):
                root.clear_children()
                root.append_text("plain <text>")

            md.add_core_rule("replace", replace)

        self.assertEqual(
            MarkdownIt().use(plugin).render("# heading"), "plain &lt;text&gt;"
        )

    def test_python_core_rule_applies_to_render_with_frontmatter(self):
        def plugin(md):
            md.add_core_rule(
                "footer", lambda root: root.append_html("<footer>generated</footer>\n")
            )

        result = (
            MarkdownIt(frontmatter=True)
            .use(plugin)
            .render_with_frontmatter(YAML_FRONTMATTER_INPUT)
        )

        self.assertEqual(result.html, "<h1>heading</h1>\n<footer>generated</footer>\n")
        self.assertIsNotNone(result.frontmatter)
        self.assertEqual(result.frontmatter.raw, "title: Test")

    def test_python_core_rule_requires_callable(self):
        with self.assertRaisesRegex(TypeError, "core rule must be callable"):
            MarkdownIt().add_core_rule("invalid", None)

    def test_python_postprocessor_failed_add_rolls_back(self):
        md = MarkdownIt()
        md.add_postprocessor("first", lambda html: html + "<!--first-->")

        with self.assertRaisesRegex(ValueError, "unknown postprocessor dependency"):
            md.add_postprocessor(
                "bad", lambda html: html + "<!--bad-->", after="missing"
            )

        md.add_postprocessor("bad", lambda html: html + "<!--bad-->")

        self.assertEqual(
            md.render("# heading"),
            "<h1>heading</h1>\n<!--first--><!--bad-->",
        )


if __name__ == "__main__":
    unittest.main()
