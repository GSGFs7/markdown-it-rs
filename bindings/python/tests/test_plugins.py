import unittest

from markdown_it_rs_py import MarkdownIt


class PluginTests(unittest.TestCase):
    def test_enable_mark_plugin(self):
        md = MarkdownIt().use("mark")
        self.assertEqual(
            md.render("==highlighted=="), "<p><mark>highlighted</mark></p>\n"
        )

    def test_enable_tasklist_plugin(self):
        md = MarkdownIt().use("tasklist")
        html = md.render("- [x] done")

        self.assertIn('class="contains-task-list"', html)
        self.assertIn('class="task-list-item"', html)
        self.assertIn('type="checkbox" checked=""', html)
        self.assertNotIn("[x]", html)

    def test_enable_footnote_plugin(self):
        md = MarkdownIt().use("footnote")
        html = md.render("Here is a footnote.[^a]\n\n[^a]: Footnote text.")

        self.assertIn('class="footnote-ref"', html)
        self.assertIn('href="#fn1"', html)
        self.assertIn('id="fn1"', html)
        self.assertIn("Footnote text.", html)
        self.assertNotIn("[^a]:", html)

    def test_enable_directives_plugin(self):
        md = MarkdownIt().use("directives")

        self.assertEqual(
            md.render('hello :name{a="b"} world'),
            '<p>hello <span class="directive name" a="b"></span> world</p>\n',
        )
        self.assertEqual(
            md.render('::name{cia="llo"}'),
            '<div class="directive name" cia="llo"></div>\n',
        )
        self.assertEqual(
            md.render(':::name{cia="llo"}\nworld\n:::'),
            '<div class="directive name" cia="llo">\n<p>world</p>\n</div>\n',
        )

    def test_directive_attributes_are_not_sanitized(self):
        md = MarkdownIt().use("directives")

        self.assertEqual(
            md.render(':name{onclick="alert(1)"}'),
            '<p><span class="directive name" onclick="alert(1)"></span></p>\n',
        )

    def test_enable_directives_from_constructor(self):
        md = MarkdownIt(directives=True)

        self.assertEqual(
            md.render('hello :name{a="b"} world'),
            '<p>hello <span class="directive name" a="b"></span> world</p>\n',
        )

    def test_enable_tasklist_and_footnote_from_constructor(self):
        md = MarkdownIt(tasklist=True, footnote=True)

        tasklist_html = md.render("- [x] done")
        footnote_html = md.render("Here is a footnote.[^a]\n\n[^a]: Footnote text.")

        self.assertIn('class="contains-task-list"', tasklist_html)
        self.assertIn('type="checkbox" checked=""', tasklist_html)
        self.assertIn('class="footnote-ref"', footnote_html)
        self.assertIn('href="#fn1"', footnote_html)


if __name__ == "__main__":
    unittest.main()
