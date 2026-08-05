# Python binding for markdown-it-rs

## Install

```bash
uv add markdown-it-rs-py
# or use pip
pip install markdown-it-rs-py
```

## Development

For development, use [maturin](https://www.maturin.rs/):

```bash
cd bindings/python
uv tool install maturin # or `pip install maturin`
uv run maturin develop
uv run python -m unittest discover -s tests -v
```

## Usage

```python
from markdown_it_rs_py import MarkdownIt

# Basic usage
md = MarkdownIt()
html = md.render("# Caillo, world!")
print(html)  # <h1>Caillo, world!</h1>

# Enable HTML tags
md = MarkdownIt(html=True)
print(md.render("ciallo<br>world"))  # <p>ciallo<br>world</p>

# Auto-linkify URLs
md = MarkdownIt(linkify=True)
print(md.render("https://example.com"))  # <a href="..">...</a>

# Math (inline and block)
md = MarkdownIt(math=True)
print(md.render("$E=mc^2$"))
print(md.render("$$\nE=mc^2\n$$"))

# Typographic replacements
md = MarkdownIt(typographer=True)
print(md.render("Something(TM)..."))  # <p>Something™…</p>

# Heading anchors (strategy: "simple" or "github")
md = MarkdownIt().use(
    "heading-anchors",
    strategy="github",
    existing_id="keep",       # or "override"
    empty_slug="section",     # None skips headings with an empty slug
    prefix="doc-",
)
print(md.render("# Hello, world!"))  # <h1 id="doc-hello-world">...</h1>

# Front matter (YAML/TOML)
md = MarkdownIt(frontmatter=True)
result = md.render_with_frontmatter("---\ntitle: caillo\n---\n# World")
print(result.html)
if fm := result.frontmatter:
    print(fm.raw)

# code syntax highlight
md = MarkdownIt(syntax_highlighting=True)
print(md.render('```python\nprint("Ciallo world!")\n```'))

# Directives
md = MarkdownIt().use("directives")
print(md.render('Ciallo :badge{label="Beta"} world'))
# default output:
# <p>Ciallo <span class="directive badge" label="Beta"></span> world</p>
```

## Security

The Python binding renders Markdown to HTML, but it does **not** sanitize the
result.
