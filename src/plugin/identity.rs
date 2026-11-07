//! Internal marks for independently scheduled high-level rules.
//!
//! A rule is identified by its owning plugin plus local name; shared
//! dispatchers and pair scanners get no per-rule aliases.

use crate::common::RuleMark;

/// Encode a rule's scope without collisions when names contain separators.
/// Keep this private: only transform marks are part of the public API.
pub(super) fn rule_mark(namespace: &str, plugin: &str, rule: &str) -> RuleMark {
    RuleMark::named(format!(
        "{namespace}:{}:{}:{}{}",
        plugin.len(),
        rule.len(),
        plugin,
        rule
    ))
}

#[cfg(test)]
mod tests {
    use super::rule_mark;

    #[test]
    fn marks_separate_plugins_rules_and_execution_namespaces() {
        assert_ne!(
            rule_mark("inline", "a", "run"),
            rule_mark("inline", "b", "run")
        );
        assert_ne!(
            rule_mark("inline", "a:b", "c"),
            rule_mark("inline", "a", "b:c")
        );
        assert_ne!(
            rule_mark("inline", "a", "run"),
            rule_mark("transform", "a", "run")
        );
        assert_ne!(
            rule_mark("inline", "雪:", "花"),
            rule_mark("inline", "雪", ":花")
        );
    }
}
