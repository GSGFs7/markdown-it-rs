use crate::MarkdownIt;
use crate::common::extset::RootExtSet;
use crate::document::{Document, Root};

/// Shared state passed to core rules.
pub struct CoreState<'a> {
    /// Source after input normalization.
    pub source: &'a str,
    /// Parser configuration and rules.
    pub md: &'a MarkdownIt,
    /// Document built by the preceding rules.
    pub document: &'a mut Document,
}

impl<'a> CoreState<'a> {
    pub(crate) fn new(source: &'a str, md: &'a MarkdownIt, document: &'a mut Document) -> Self {
        Self {
            source,
            md,
            document,
        }
    }

    /// Cross-block storage shared with inline parsing.
    pub fn root_ext_mut(&mut self) -> &mut RootExtSet {
        &mut self
            .document
            .node_mut(self.document.root())
            .cast_mut::<Root>()
            .expect("core parsing requires a Root payload")
            .ext
    }

    /// Run a transformation with mutable access to the document and root extensions.
    ///
    /// The extensions are moved out of `Root.ext` during the callback and restored afterward.
    pub fn with_root_ext<R>(&mut self, run: impl FnOnce(&mut Document, &mut RootExtSet) -> R) -> R {
        let ext = std::mem::take(self.root_ext_mut());
        let mut guard = RootExtGuard {
            document: self.document,
            ext,
        };
        run(guard.document, &mut guard.ext)
    }
}

struct RootExtGuard<'a> {
    document: &'a mut Document,
    ext: RootExtSet,
}

impl Drop for RootExtGuard<'_> {
    fn drop(&mut self) {
        if let Some(root) = self
            .document
            .node_mut(self.document.root())
            .cast_mut::<Root>()
        {
            root.ext = std::mem::take(&mut self.ext);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detached_shared_state_is_restored_when_a_transformation_unwinds() {
        let md = MarkdownIt::empty();
        let mut document = Document::new("", Root::new(""));
        let mut state = CoreState::new("", &md, &mut document);
        state.root_ext_mut().insert(vec![1_u8]);
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            state.with_root_ext(|document, ext| {
                assert!(
                    document
                        .node(document.root())
                        .cast::<Root>()
                        .unwrap()
                        .ext
                        .is_empty()
                );
                ext.get_mut::<Vec<u8>>().unwrap().push(2);
                panic!("interrupted transformation");
            });
        }));
        assert!(result.is_err());
        assert_eq!(state.root_ext_mut().get::<Vec<u8>>().unwrap(), &[1, 2]);
    }
}
