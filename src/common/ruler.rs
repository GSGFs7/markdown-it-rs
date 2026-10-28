//! Plugin manager with dependency resolution.

use std::cmp::Reverse;
use std::collections::{BinaryHeap, HashMap, HashSet};
use std::fmt::{Debug, Formatter};
use std::hash::Hash;
use std::slice::Iter;
use std::sync::OnceLock;

///
/// Ruler allows you to implement a plugin system with dependency management and ensure that
/// your dependencies are called in the correct order.
///
/// You can use it like this:
/// ```
/// use markdown_it::common::ruler::Ruler;
///
/// // this example prints "[ hello, world! ]",
/// // where each token is printed by separate closure
/// let mut chain = Ruler::<&str, fn (&mut String)>::new();
///
/// // define rules printing "hello" and "world"
/// chain.add("hello", |s| s.push_str("hello"));
/// chain.add("world", |s| s.push_str("world"));
///
/// // open bracket should be before "hello", and closing one after "world"
/// chain.add("open_bracket", |s| s.push_str("[ ")).before("hello");
/// chain.add("close_bracket", |s| s.push_str(" ]")).after("world");
///
/// // between "hello" and "world" we shall have a comma
/// chain.add("comma", |s| s.push_str(", ")).after("hello").before("world");
///
/// // after "world" we should have "!" as a first rule, but ensure "world" exists first
/// chain.add("bang", |s| s.push_str("!")).require("world").after("world").before_all();
///
/// // now we run this chain
/// let mut result = String::new();
/// for f in chain.iter() { f(&mut result); }
/// assert_eq!(result, "[ hello, world! ]");
/// ```
///
/// This data structure contains any number of elements (M, T), where T is any type and
/// M (mark) is its identifier.
///
///  - `M` is used for ordering and dependency checking,
///    it must implement `Eq + Clone + Hash + Debug`.
///    Common choices for `M` are `u32`, `&'static str`,
///    or a special `Symbol` type designed for this purpose.
///
///  - `T` is any user-defined type. It's usually a function or boxed trait.
///
pub struct Ruler<M, T> {
    deps: Vec<RuleItem<M, T>>,
    compiled: OnceLock<Vec<T>>,
}

impl<M, T> Ruler<M, T> {
    pub fn new() -> Self {
        Self::default()
    }

    pub(crate) fn len(&self) -> usize {
        self.deps.len()
    }
}

impl<M: Eq + Hash + Clone + Debug, T: Clone> Ruler<M, T> {
    /// Add a new rule identified by `mark` with payload `value`.
    pub fn add(&mut self, mark: M, value: T) -> &mut RuleItem<M, T> {
        self.compiled = OnceLock::new();
        let dep = RuleItem::new(mark, value);
        self.deps.push(dep);
        self.deps.last_mut().unwrap()
    }

    /// Remove all rules identified by `mark`.
    pub fn remove(&mut self, mark: M) {
        self.compiled = OnceLock::new();
        self.deps.retain(|dep| !dep.marks.contains(&mark));
    }

    /// Check if there are any rules identified by `mark`.
    pub fn contains(&self, mark: M) -> bool {
        self.deps.iter().any(|dep| dep.marks.contains(&mark))
    }

    /// Ordered iteration through rules.
    #[inline]
    pub fn iter(&self) -> Iter<'_, T> {
        self.as_slice().iter()
    }

    /// Borrow the cached ordered payloads without copying the rule table.
    pub(crate) fn as_slice(&self) -> &[T] {
        self.compiled.get_or_init(|| self.compile())
    }

    /// Kahn Topological Sort (earlier joined at the front)
    ///
    /// it runs in expected O(E + V log V).
    fn compile(&self) -> Vec<T> {
        // Mark (including aliases) -> matching RuleItem indices
        let mut rules_by_mark = HashMap::<M, Vec<usize>>::new();

        // RuleItem index -> its prerequisites (must run before this rule)
        let mut deps_graph = vec![HashSet::new(); self.deps.len()];

        // stable partition by priority
        let mut before_all = Vec::new();
        let mut normal = Vec::new();
        let mut after_all = Vec::new();
        for (idx, dep) in self.deps.iter().enumerate() {
            match dep.prio {
                RuleItemPriority::BeforeAll => before_all.push(idx),
                RuleItemPriority::Normal => normal.push(idx),
                RuleItemPriority::AfterAll => after_all.push(idx),
            }
            for mark in &dep.marks {
                rules_by_mark.entry(mark.clone()).or_default().push(idx);
            }
        }
        let deps_order: Vec<usize> = before_all
            .into_iter()
            .chain(normal)
            .chain(after_all)
            .collect();

        // build dependency graph, replacing all after's with before's,
        // i.e. B.after(A) -> A.before(B)
        for idx in deps_order.iter().copied() {
            let dep = &self.deps[idx];
            for constraint in &dep.cons {
                match constraint {
                    RuleItemConstraint::Before(v) => {
                        for depidx in rules_by_mark.get(v).into_iter().flatten() {
                            deps_graph[*depidx].insert(idx);
                        }
                    }
                    RuleItemConstraint::After(v) => {
                        for depidx in rules_by_mark.get(v).into_iter().flatten() {
                            deps_graph[idx].insert(*depidx);
                        }
                    }
                    RuleItemConstraint::Require(v) => {
                        assert!(
                            rules_by_mark.contains_key(v),
                            "missing dependency: {:?} requires {:?}",
                            dep.marks.first().unwrap(),
                            v
                        );
                    }
                }
            }
        }

        let mut indegree = vec![0usize; self.deps.len()];
        let mut dependents = vec![Vec::new(); self.deps.len()];
        for (idx, prereqs) in deps_graph.iter().enumerate() {
            indegree[idx] = prereqs.len();
            for &prereq in prereqs {
                dependents[prereq].push(idx);
            }
        }

        let mut position = vec![0usize; self.deps.len()];
        for (pos, idx) in deps_order.iter().copied().enumerate() {
            position[idx] = pos;
        }

        let mut available: BinaryHeap<_> = (0..self.deps.len())
            .filter(|&idx| indegree[idx] == 0)
            .map(|idx| Reverse((position[idx], idx)))
            .collect();

        // compiled result
        let mut result = Vec::with_capacity(self.deps.len());
        while let Some(Reverse((_, idx))) = available.pop() {
            result.push(self.deps[idx].value.clone());

            for &dependent in &dependents[idx] {
                indegree[dependent] -= 1;
                if indegree[dependent] == 0 {
                    // BFS
                    available.push(Reverse((position[dependent], dependent)));
                }
            }
        }

        if result.len() != self.deps.len() {
            #[cfg(debug_assertions)]
            if let Some(path) = self.describe_cycle(&deps_order, &deps_graph, &indegree) {
                panic!("cyclic dependency: {path}");
            }

            // if you see this in debug mode, report it as a bug
            panic!("cyclic dependency: (use debug mode for more details)");
        }

        result
    }

    // --- debug ---

    #[cfg(debug_assertions)]
    fn describe_cycle(
        &self,
        deps_order: &[usize],
        deps_graph: &[HashSet<usize>],
        indegree: &[usize],
    ) -> Option<String> {
        for &start in deps_order {
            if indegree[start] == 0 {
                continue;
            }

            let Some(path) = Self::find_cycle_from(start, deps_graph, indegree) else {
                continue;
            };

            return Some(
                path.iter()
                    .map(|&idx| format!("{:?}", self.deps[idx].marks.first().unwrap()))
                    .collect::<Vec<_>>()
                    .join(" < "),
            );
        }

        None
    }

    #[cfg(debug_assertions)]
    fn find_cycle_from(
        start: usize,
        deps_graph: &[HashSet<usize>],
        indegree: &[usize],
    ) -> Option<Vec<usize>> {
        let mut seen = HashMap::new();
        let mut stack = vec![start];
        while let Some(node) = stack.pop() {
            for &prereq in &deps_graph[node] {
                if indegree[prereq] == 0 || seen.contains_key(&prereq) {
                    continue;
                }

                seen.insert(prereq, node);
                if prereq == start {
                    return Some(Self::reconstruct_cycle(start, &seen));
                }

                stack.push(prereq);
            }
        }

        None
    }

    #[cfg(debug_assertions)]
    fn reconstruct_cycle(start: usize, seen: &HashMap<usize, usize>) -> Vec<usize> {
        let mut path = Vec::new();
        let mut current = start;
        while !path.contains(&current) {
            path.push(current);
            current = seen[&current];
        }
        path.push(current);
        path.reverse();
        path
    }
}

impl<M: Debug, T> Debug for Ruler<M, T> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Ruler")
            .field("deps", &self.deps)
            .field("compiled", &self.compiled.get().is_some())
            .finish()
    }
}

impl<M, T> Default for Ruler<M, T> {
    fn default() -> Self {
        Self {
            deps: Vec::new(),
            compiled: OnceLock::new(),
        }
    }
}

///
/// Result of [Ruler::add](Ruler::add), allows to customize position of each rule.
///
pub struct RuleItem<M, T> {
    marks: Vec<M>,
    value: T,
    prio: RuleItemPriority,
    cons: Vec<RuleItemConstraint<M>>,
}

impl<M: Clone, T> RuleItem<M, T> {
    /// Make sure this rule will be inserted before any rule defined by `mark` (if such rule exists).
    /// ```
    /// use markdown_it::common::ruler::Ruler;
    /// let mut chain = Ruler::<&str, fn (&mut String)>::new();
    ///
    /// chain.add("a", |s| s.push_str("bar"));
    /// chain.add("b", |s| s.push_str("foo")).before("a");
    ///
    /// let mut result = String::new();
    /// for f in chain.iter() { f(&mut result); }
    /// assert_eq!(result, "foobar");
    /// ```
    pub fn before(&mut self, mark: M) -> &mut Self {
        self.cons.push(RuleItemConstraint::Before(mark));
        self
    }

    /// Make sure this rule will be inserted after any rule defined by `mark` (if such rule exists).
    /// Similar to [RuleItem::before](RuleItem::before).
    pub fn after(&mut self, mark: M) -> &mut Self {
        self.cons.push(RuleItemConstraint::After(mark));
        self
    }

    /// This rule will be inserted as early as possible, while still taking into account dependencies,
    /// i.e. `.after(X).before_all()` causes this to be first rule after X.
    /// ```
    /// use markdown_it::common::ruler::Ruler;
    /// let mut chain = Ruler::<&str, fn (&mut String)>::new();
    ///
    /// chain.add("a", |s| s.push_str("A"));
    /// chain.add("c", |s| s.push_str("C")).after("a");
    /// chain.add("b", |s| s.push_str("B")).after("a").before_all();
    ///
    /// let mut result = String::new();
    /// for f in chain.iter() { f(&mut result); }
    /// // without before_all order will be ACB
    /// assert_eq!(result, "ABC");
    /// ```
    pub fn before_all(&mut self) -> &mut Self {
        self.prio = RuleItemPriority::BeforeAll;
        self
    }

    /// This rule will be inserted as late as possible, while still taking into account dependencies,
    /// i.e. `.before(X).after_all()` causes this to be last rule before X.
    /// Similar to [RuleItem::before_all](RuleItem::before_all).
    pub fn after_all(&mut self) -> &mut Self {
        self.prio = RuleItemPriority::AfterAll;
        self
    }

    /// Add another auxiliary identifier to this rule. It can be used to group together multiple
    /// rules with similar functionality.
    /// ```
    /// use markdown_it::common::ruler::Ruler;
    /// let mut chain = Ruler::<&str, fn (&mut String)>::new();
    ///
    /// chain.add("b", |s| s.push_str("B")).alias("BorC");
    /// chain.add("c", |s| s.push_str("C")).alias("BorC");
    /// chain.add("a", |s| s.push_str("A")).before("BorC");
    ///
    /// let mut result = String::new();
    /// for f in chain.iter() { f(&mut result); }
    /// assert_eq!(result, "ABC");
    /// ```
    pub fn alias(&mut self, mark: M) -> &mut Self {
        self.marks.push(mark);
        self
    }

    /// Require another rule identified by `mark`, panic if not found.
    pub fn require(&mut self, mark: M) -> &mut Self {
        self.cons.push(RuleItemConstraint::Require(mark));
        self
    }
}

impl<M: std::fmt::Debug, T> Debug for RuleItem<M, T> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RuleItem")
            .field("marks", &self.marks)
            .field("prio", &self.prio)
            .field("cons", &self.cons)
            .finish()
    }
}

impl<M, T> RuleItem<M, T> {
    fn new(mark: M, value: T) -> Self {
        Self {
            marks: vec![mark],
            value,
            prio: RuleItemPriority::Normal,
            cons: vec![],
        }
    }
}

#[derive(Debug)]
enum RuleItemConstraint<M> {
    Before(M),
    After(M),
    Require(M),
}

#[derive(Debug)]
enum RuleItemPriority {
    Normal,
    BeforeAll,
    AfterAll,
}

#[cfg(test)]
mod tests {
    use super::Ruler;
    use crate::common::RuleMark;

    #[test]
    #[should_panic(expected = r#"cyclic dependency: "A" < "B" < "C" < "D" < "E" < "F" < "A""#)]
    #[cfg(debug_assertions)]
    fn cyclic_dependency_debug() {
        let mut r = Ruler::new();
        r.add("%", ()).after("D");
        r.add("A", ()).after("B");
        r.add("E", ()).after("F");
        r.add("C", ()).after("D");
        r.add("B", ()).after("C");
        r.add("D", ()).after("E");
        r.add("F", ()).after("A");
        r.compile();
    }

    #[test]
    #[should_panic(expected = r#"cyclic dependency"#)]
    fn cyclic_dependency() {
        let mut r = Ruler::new();
        r.add("A", ()).after("B");
        r.add("B", ()).after("C");
        r.add("C", ()).after("A");
        r.compile();
    }

    #[test]
    #[should_panic(expected = r#"missing dependency: "C" requires "Z"#)]
    fn missing_require() {
        let mut r = Ruler::new();
        r.add("A", ());
        r.add("B", ()).require("A");
        r.add("C", ()).require("Z");
        r.compile();
    }

    #[test]
    fn newly_available_before_all_rule_precedes_waiting_normal_rule() {
        let mut r = Ruler::new();
        r.add("prerequisite", "prerequisite");
        r.add("waiting", "waiting");
        r.add("unlocked", "unlocked")
            .after("prerequisite")
            .before_all();

        assert_eq!(
            r.iter().copied().collect::<Vec<_>>(),
            vec!["prerequisite", "unlocked", "waiting"]
        );
    }

    #[test]
    fn newly_available_rule_precedes_later_registered_waiting_rule() {
        let mut r = Ruler::new();
        r.add("unlocked", "unlocked").after("prerequisite");
        r.add("prerequisite", "prerequisite");
        r.add("waiting", "waiting");

        assert_eq!(
            r.iter().copied().collect::<Vec<_>>(),
            vec!["prerequisite", "unlocked", "waiting"]
        );
    }

    #[test]
    fn priority_groups_preserve_registration_order() {
        let mut r = Ruler::new();
        r.add("after1", "after1").after_all();
        r.add("normal1", "normal1");
        r.add("before1", "before1").before_all();
        r.add("after2", "after2").after_all();
        r.add("normal2", "normal2");
        r.add("before2", "before2").before_all();

        assert_eq!(
            r.iter().copied().collect::<Vec<_>>(),
            vec![
                "before1", "before2", "normal1", "normal2", "after1", "after2"
            ]
        );
    }

    #[test]
    fn before_and_after_shared_alias_apply_to_every_member() {
        let mut r = Ruler::new();
        r.add("end", "end").after("group");
        r.add("member1", "member1").alias("group");
        r.add("member2", "member2").alias("group");
        r.add("start", "start").before("group");

        assert_eq!(
            r.iter().copied().collect::<Vec<_>>(),
            vec!["start", "member1", "member2", "end"]
        );
    }

    #[test]
    fn overlapping_constraints_and_repeated_aliases_count_as_one_edge() {
        let mut r = Ruler::new();
        r.add("A", "A").after("B").after("B").after("group");
        r.add("B", "B")
            .alias("group")
            .alias("group")
            .before("A")
            .before("A");

        assert_eq!(r.iter().copied().collect::<Vec<_>>(), vec!["B", "A"]);
    }

    #[test]
    fn named_mark_can_target_typed_rule_alias() {
        struct TypedRule;

        let mut r = Ruler::new();
        r.add(RuleMark::of::<TypedRule>(), "typed")
            .alias(RuleMark::named("typed"));
        r.add(RuleMark::named("dynamic"), "dynamic")
            .before(RuleMark::named("typed"));

        let result = r.iter().copied().collect::<Vec<_>>();
        assert_eq!(result, vec!["dynamic", "typed"]);
    }

    #[test]
    fn remove_invalidates_compiled_cache() {
        let mut r = Ruler::new();
        r.add("A", "A");

        assert_eq!(r.iter().copied().collect::<Vec<_>>(), vec!["A"]);

        r.remove("A");
        assert!(r.iter().next().is_none());
    }

    #[test]
    fn contains_accepts_a_shared_reference() {
        let mut ruler = Ruler::new();
        ruler.add("present", ());

        let ruler = &ruler;
        assert!(ruler.contains("present"));
        assert!(!ruler.contains("missing"));
    }
}
