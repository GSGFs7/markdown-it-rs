use markdown_it::MarkdownIt as RustMarkdownIt;
use wasm_bindgen::prelude::*;

#[derive(Default)]
#[wasm_bindgen]
pub struct MarkdownIt {
    inner: RustMarkdownIt,
}

#[wasm_bindgen]
impl MarkdownIt {
    #[wasm_bindgen(constructor)]
    pub fn new() -> Self {
        let mut inner = RustMarkdownIt::new();
        markdown_it::plugins::cmark::add(&mut inner);

        #[cfg(feature = "extras")]
        markdown_it::plugins::extra::add(&mut inner);

        Self { inner }
    }

    pub fn render(&self, source: &str) -> String {
        self.inner.parse(source).render()
    }
}
