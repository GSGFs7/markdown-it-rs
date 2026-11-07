//! Post-parse document rewrites configured by [`PluginSpec::transform`].
//!
//! [`PluginSpec::transform`]: crate::plugin::PluginSpec::transform

use std::collections::HashMap;

use crate::MarkdownIt;
use crate::common::RuleMark;
use crate::document::Document;
use crate::parser::core::CoreState;
use crate::parser::inline::InlineParserRule;
use crate::plugin::identity::rule_mark;
use crate::plugin::spec::{PluginInstallContext, PluginInstaller};

type TransformFn = Box<dyn Fn(&mut Document, &MarkdownIt) + Send + Sync + 'static>;

/// Per-parser transform state used to preserve declaration order.
#[derive(Debug, Default)]
struct TransformRegistry {
    last_by_plugin: HashMap<String, RuleMark>,
}

/// Return the core-rule mark for one transform.
///
/// Use it with [`before`](crate::common::ruler::RuleItem::before) or
/// [`after`](crate::common::ruler::RuleItem::after). Length prefixes keep the
/// encoding injective even when names contain the separator.
pub fn transform_rule_mark(plugin: &str, rule: &str) -> RuleMark {
    rule_mark("transform", plugin, rule)
}

/// Return the mark shared by every transform of a plugin or group.
///
/// A transform belongs to its plugin and all ancestor groups created by
/// [`PluginSpec::with`](crate::plugin::PluginSpec::with). Plugins without
/// transforms register a no-op anchor under this mark to preserve indirect
/// dependencies.
pub fn transform_plugin_mark(plugin: &str) -> RuleMark {
    RuleMark::named(format!("transform-plugin:{}:{plugin}", plugin.len()))
}

/// Return the primary mark of the ordering anchor for a plugin with no transforms.
fn transform_anchor_mark(plugin: &str) -> RuleMark {
    RuleMark::named(format!("transform-anchor:{}:{plugin}", plugin.len()))
}

/// Register a no-op core rule carrying a plugin's transform ordering marks.
///
/// Without it, dependencies on a plugin with no transforms would be dropped
/// from the core rule chain.
pub(super) fn install_ordering_anchor(md: &mut MarkdownIt, plugin: &PluginInstallContext<'_>) {
    if md
        .ext
        .get::<TransformRegistry>()
        .is_some_and(|registry| registry.last_by_plugin.contains_key(plugin.name))
    {
        return;
    }

    let mut builder = md
        .add_dynamic_core_rule(transform_anchor_mark(plugin.name), |_| {})
        .alias_mark(transform_plugin_mark(plugin.name));
    for group in plugin.groups {
        builder = builder.alias_mark(transform_plugin_mark(group));
    }
    for name in plugin.before {
        builder = builder.before_mark(transform_plugin_mark(name));
    }
    for name in plugin.after {
        builder = builder.after_mark(transform_plugin_mark(name));
    }
}

pub(super) struct TransformInstaller {
    rule: String,
    transform: TransformFn,
}

impl TransformInstaller {
    pub(super) fn new(
        rule: String,
        transform: impl Fn(&mut Document, &MarkdownIt) + Send + Sync + 'static,
    ) -> Self {
        Self {
            rule,
            transform: Box::new(transform),
        }
    }
}

impl PluginInstaller for TransformInstaller {
    fn name(&self) -> &str {
        &self.rule
    }

    fn validate(&self) {
        assert!(
            !self.rule.is_empty(),
            "plugin transform rule name must not be empty"
        );
    }

    fn install(self: Box<Self>, md: &mut MarkdownIt, plugin: &PluginInstallContext<'_>) {
        let Self { rule, transform } = *self;
        let mark = transform_rule_mark(plugin.name, &rule);

        let previous = md
            .ext
            .get::<TransformRegistry>()
            .and_then(|registry| registry.last_by_plugin.get(plugin.name))
            .cloned();

        let mut builder = md
            .add_dynamic_core_rule(mark.clone(), move |state: &mut CoreState<'_>| {
                transform(state.document, state.md);
            })
            .after::<InlineParserRule>()
            .alias_mark(transform_plugin_mark(plugin.name));

        for group in plugin.groups {
            builder = builder.alias_mark(transform_plugin_mark(group));
        }
        if let Some(previous) = previous {
            builder = builder.after_mark(previous);
        }

        for name in plugin.before {
            builder = builder.before_mark(transform_plugin_mark(name));
        }
        for name in plugin.after {
            builder = builder.after_mark(transform_plugin_mark(name));
        }

        md.ext
            .get_or_insert_default::<TransformRegistry>()
            .last_by_plugin
            .insert(plugin.name.to_owned(), mark);
    }
}
