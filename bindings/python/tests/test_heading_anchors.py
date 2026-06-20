import unittest

from markdown_it_rs_py import MarkdownIt


class HeadingAnchorTests(unittest.TestCase):
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

    def test_heading_anchors_github_strategy(self):
        md = MarkdownIt().use("heading-anchors", strategy="github")

        self.assertEqual(
            md.render("# Hello,  World!"),
            '<h1 id="hello--world">Hello,  World!</h1>\n',
        )

    def test_heading_anchors_options_update_existing_plugin(self):
        md = MarkdownIt(heading_anchors=True).use("heading-anchors", strategy="github")

        self.assertEqual(
            md.render("# Hello,  world!"),
            '<h1 id="hello--world">Hello,  world!</h1>\n',
        )

    def test_heading_anchors_empty_slug_and_prefix(self):
        md = MarkdownIt().use(
            "heading-anchors",
            empty_slug="section",
            prefix="doc-",
        )

        self.assertEqual(
            md.render("# !!!\n# ???"),
            '<h1 id="doc-section">!!!</h1>\n<h1 id="doc-section-1">???</h1>\n',
        )

    def test_heading_anchors_accept_optional_none(self):
        md = MarkdownIt().use(
            "heading-anchors",
            existing_id="override",
            empty_slug=None,
            prefix=None,
        )

        self.assertEqual(md.render("# !!!"), "<h1>!!!</h1>\n")

    def test_heading_anchors_reject_invalid_options(self):
        with self.assertRaisesRegex(ValueError, "unknown heading anchor strategy"):
            MarkdownIt().use("heading-anchors", strategy="invalid")
        with self.assertRaisesRegex(ValueError, "unknown existing heading ID policy"):
            MarkdownIt().use("heading-anchors", existing_id="invalid")
        with self.assertRaisesRegex(ValueError, "unknown option for heading-anchors"):
            MarkdownIt().use("heading-anchors", invalid=True)

    def test_heading_anchors_rule_update_with_callable(self):
        """Second .use() with a callable should override the previous builtin strategy."""

        def my_slugify(s: str) -> str:
            return "".join(
                char for char in s.replace(" ", "-") if char.isalnum() or char == "-"
            )

        md = (
            MarkdownIt()
            .use("heading-anchors", strategy="github")
            .use(
                "heading-anchors",
                strategy=my_slugify,
                existing_id="override",
                empty_slug="empty-",
            )
        )

        # my_slugify keeps case and uses raw hyphen replacement
        self.assertEqual(
            md.render("# Hello World"),
            '<h1 id="Hello-World">Hello World</h1>\n',
        )

        # empty_slug fallback should be applied
        self.assertEqual(
            md.render("# !!!"),
            '<h1 id="empty-">!!!</h1>\n',
        )

    def test_heading_anchors_rule_update_with_builtin(self):
        """Second .use() with a builtin strategy should replace the previous one."""

        md = (
            MarkdownIt()
            .use("heading-anchors", strategy="github")
            .use(
                "heading-anchors",
                strategy="simple",
                existing_id="override",
                empty_slug="empty-",
            )
        )

        # "simple" collapses punctuation runs into a single hyphen (unlike "github")
        self.assertEqual(
            md.render("# Hello,  World!"),
            '<h1 id="hello-world">Hello,  World!</h1>\n',
        )

        # empty_slug fallback with simple strategy
        self.assertEqual(
            md.render("# !!!"),
            '<h1 id="empty-">!!!</h1>\n',
        )


if __name__ == "__main__":
    unittest.main()
