use markdown_it::MarkdownIt as RustMarkdownIt;
use wasm_bindgen::prelude::*;

#[derive(Default)]
#[wasm_bindgen]
pub struct MarkdownIt {
    inner: RustMarkdownIt,
}

#[derive(Default, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct Options {
    #[serde(default)]
    allow_html: bool,
}

#[wasm_bindgen]
impl MarkdownIt {
    #[wasm_bindgen(constructor)]
    pub fn new(options: Option<JsValue>) -> Result<Self, JsValue> {
        let options: Options = options
            .map(serde_wasm_bindgen::from_value)
            .transpose()
            .map_err(|err| JsValue::from_str(&err.to_string()))?
            .unwrap_or_default();

        let mut inner = RustMarkdownIt::new();
        markdown_it::plugins::cmark::add(&mut inner);

        #[cfg(feature = "extras")]
        markdown_it::plugins::extra::add(&mut inner);

        if options.allow_html {
            markdown_it::plugins::html::add(&mut inner);
        }

        Ok(Self { inner })
    }

    pub fn render(&self, source: &str) -> String {
        self.inner.parse(source).render()
    }
}
