import unittest

from markdown_it_rs_py import MarkdownIt


class MarkdownItTests(unittest.TestCase):
    def test_heading(self):
        md = MarkdownIt()
        self.assertEqual(md.render("# heading"), "<h1>heading</h1>\n")

    def test_strikethrough(self):
        md = MarkdownIt()
        self.assertEqual(md.render("~~114~~514"), "<p><s>114</s>514</p>\n")

    def test_mark(self):
        md = MarkdownIt()
        self.assertEqual(
            md.render("==highlighted=="), "<p><mark>highlighted</mark></p>\n"
        )

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

    def test_parse_ast(self):
        ast = MarkdownIt().parse("# heading")
        root = ast.root

        self.assertEqual(len(root.children), 1)
        self.assertTrue(root.type_name.endswith("Root"))
        self.assertEqual(root.children[0].render(), "<h1>heading</h1>\n")


if __name__ == "__main__":
    unittest.main()
