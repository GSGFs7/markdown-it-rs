use super::cache::{LabelScanKey, cache_label_scan};
use crate::parser::inline::InlineState;

// Scan a link label with independent check sessions.
//
// This function assumes that the first character ("[") already matches;
// it returns the byte length of the label including the closing "]".
pub(super) fn scan_link_label(mut context: InlineState<'_>, enable_nested: bool) -> Option<usize> {
    // Empty labels close immediately, even at the nesting limit; no cache needed.
    if context.remaining().starts_with(']') {
        return Some(0);
    }
    let start = context.pos;
    let memoize = context
        .scan_limit_cache
        .borrow()
        .link_bounds()
        .is_some_and(|bounds| bounds.can_memoize_labels(start, context.pos_max));
    let initial_key = LabelScanKey {
        start,
        end: context.pos_max,
        depth: context.depth,
        link_level: context.link_level,
        enable_nested,
    };
    if memoize {
        let shared = context.scan_limit_cache.borrow();
        if shared.link_bounds()?.failed_labels.contains(&(
            initial_key.start,
            initial_key.end,
            initial_key.link_level,
            initial_key.enable_nested,
        )) {
            return None;
        }
        if let Some(&(epoch, close)) = shared.link_bounds()?.labels.get(&initial_key) {
            // Additional nesting cutoffs can invalidate a successful boundary.
            if epoch == shared.epoch() {
                return Some(close - start);
            }
        }
    }

    let mut open = vec![initial_key];
    while let Some(ch) = context.remaining().chars().next() {
        let before = context.remaining().len();
        if ch == ']' {
            let matched = open.pop().unwrap();
            if memoize {
                // Every raw opener traversed by this scan has the same fresh
                // label boundary at this depth. Record its result as well, so
                // later candidates do not walk the overlapping label again.
                cache_label_scan(&context, matched, Some(context.pos));
            }
            if open.is_empty() {
                return Some(context.pos - start);
            }
        }

        if context.skip_token().is_none() {
            break;
        }

        let consumed = before - context.remaining().len();
        if ch == '[' {
            if consumed == 1 {
                if memoize
                    && context
                        .scan_limit_cache
                        .borrow()
                        .link_bounds()
                        .is_some_and(|bounds| {
                            bounds.failed_labels.contains(&(
                                context.pos,
                                context.pos_max,
                                context.link_level,
                                enable_nested,
                            ))
                        })
                {
                    // An uncloseable nested opener also blocks this label; its
                    // probe already recorded any nesting cutoff.
                    break;
                }
                open.push(LabelScanKey {
                    start: context.pos,
                    end: context.pos_max,
                    depth: context.depth,
                    link_level: context.link_level,
                    enable_nested,
                });
            } else if !enable_nested {
                break;
            }
        }
    }

    if memoize {
        for key in open {
            cache_label_scan(&context, key, None);
        }
    }
    None
}
