import unittest

from markdown_it_rs_py import MarkdownIt


class MarkdownItTests(unittest.TestCase):
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


if __name__ == "__main__":
    unittest.main()
