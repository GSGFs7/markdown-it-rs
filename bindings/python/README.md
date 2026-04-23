Python binding for markdown-it-rs

For development, use [maturin](https://www.maturin.rs/):

```bash
cd bindings/python
pip install maturin
maturin develop
python -m unittest discover -s tests -v
```

Install:

```bash
uv add "markdown-it-rs-py @ git+<This repo git url>#subdirectory=bindings/python"
# or pip
pip install "git+<This repo git url>#subdirectory=bindings/python"
```

replace `<This repo git url>` to actual git url.
