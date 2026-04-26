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
maturin develop
python -m unittest discover -s tests -v
```

## Usage

```python
from markdown_it_rs_py import MarkdownIt

# Basic usage
md = MarkdownIt()
html = md.render("# Caillo, world!")
print(html)  # <h1>Hello, world!</h1>

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

# Front matter (YAML/TOML)
md = MarkdownIt(frontmatter=True)
result = md.render_with_frontmatter("---\ntitle: caillo\n---\n# World")
print(result.html)
if fm := result.frontmatter:
    print(fm.raw)

# code syntax highlight
md = MarkdownIt(syntax_highlighting=True)
print(md.render('```python\nprint("Ciallo world!")\n```'))

```
