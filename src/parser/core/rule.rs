use std::fmt;
use std::sync::Arc;

use super::CoreState;

/// A rule in the core rule chain.
pub trait CoreRule: Send + Sync + 'static {
    const NAMES: &'static [&'static str] = &[];

    fn run(&self, state: &mut CoreState<'_>);
}

type RunFn = dyn Fn(&mut CoreState<'_>) + Send + Sync;

/// Type-erased core rule stored in the ruler.
#[derive(Clone)]
pub struct CoreRuleRunner {
    run: Arc<RunFn>,
}

impl CoreRuleRunner {
    pub(crate) fn new<T: CoreRule>(rule: T) -> Self {
        Self {
            run: Arc::new(move |state| rule.run(state)),
        }
    }

    pub(crate) fn run(&self, state: &mut CoreState<'_>) {
        (self.run)(state);
    }
}

impl fmt::Debug for CoreRuleRunner {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("CoreRuleRunner").finish_non_exhaustive()
    }
}

crate::parser::rule::rule_builder!(CoreRule);
