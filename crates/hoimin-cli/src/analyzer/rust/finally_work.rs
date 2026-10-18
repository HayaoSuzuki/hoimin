//! Work sharing belongs to one `apply_finally` invocation, never a file-wide memo.
use super::{AnnotationCollector, ControlFlowExits, KnownImports, Stmt};

#[cfg(test)]
use std::cell::Cell;

#[cfg(test)]
thread_local! {
    pub(super) static DISABLED: Cell<bool> = const { Cell::new(false) };
    pub(super) static MISSES: Cell<usize> = const { Cell::new(0) };
    pub(super) static HITS: Cell<usize> = const { Cell::new(0) };
    pub(super) static MAX_VECTOR: Cell<usize> = const { Cell::new(0) };
}

struct CompletedTransfer {
    entry: KnownImports,
    exits: ControlFlowExits,
    residual_imports: KnownImports,
}

/// One slot per live finally frame: O(depth) slots, not a byte or RSS bound.
/// Its lifetime fixes body, scope, qualname, implicit-exception tracking and
/// transfer-only observation mode. Class context and observation hooks disable
/// sharing. Only exact imports vary within that fixed context. Nested scopes
/// restore their context before returning; cached transfers invoke no callbacks.
pub(super) struct FinallyTransfer<'ast> {
    body: &'ast [Stmt],
    enabled: bool,
    previous: Option<CompletedTransfer>,
}

impl<'ast> FinallyTransfer<'ast> {
    pub(super) fn new(body: &'ast [Stmt], enabled: bool) -> Self {
        Self {
            body,
            enabled,
            previous: None,
        }
    }

    pub(super) fn evaluate(
        &mut self,
        collector: &mut AnnotationCollector<'ast, '_>,
        imports: KnownImports,
    ) -> ControlFlowExits {
        // Poll even hits, and never admit or replay a partial cancelled result.
        if collector.is_cancelled() {
            return ControlFlowExits::default();
        }
        if let Some(previous) = &self.previous
            && previous.entry == imports
        {
            #[cfg(test)]
            HITS.set(HITS.get() + 1);
            collector.imports.clone_from(&previous.residual_imports);
            return previous.exits.clone();
        }
        // Evict before recursion, so misses do not retain an old result per frame.
        self.previous = None;
        #[cfg(test)]
        MISSES.set(MISSES.get() + 1);
        let entry = self.enabled.then(|| imports.clone());
        let record_annotations = std::mem::replace(&mut collector.record_annotations, false);
        let mut exits = collector.visit_suite_from(imports, self.body);
        collector.record_annotations = record_annotations;
        if collector.is_cancelled() {
            return ControlFlowExits::default();
        }
        if let Some(entry) = entry {
            exits.deduplicate_abrupt();
            self.previous = Some(CompletedTransfer {
                entry,
                exits: exits.clone(),
                residual_imports: collector.imports.clone(),
            });
        }
        exits
    }
}

impl AnnotationCollector<'_, '_> {
    pub(super) fn can_share_finally_transfer(&self) -> bool {
        // Repeated equal imports in class scope may observe different mutable
        // fallbacks. This gate applies equally to reuse AND deleting duplicates.
        if self.class_context.is_some() {
            return false;
        }
        #[cfg(test)]
        if DISABLED.get()
            || self.marker_projection.is_some()
            || self.handler_exit_projection.is_some()
            || self.try_exit_projection.is_some()
            || self.test_mutation.is_some()
        {
            return false;
        }
        true
    }
}

impl ControlFlowExits {
    pub(super) fn deduplicate_abrupt(&mut self) {
        // Keep LAST occurrences in their original order: a later transfer's
        // residual imports can be observed by unreachable annotation scans.
        // Deleting an earlier identical transfer preserves the last contributor
        // to every output category; keeping first occurrences does not (A/B/A).
        // Compare complete typed bindings, compacting in place in O(n*k) equality
        // comparisons for n entries and k distinct states (quadratic if distinct).
        // No state caps or cross-category joins; fallthrough/implicit rules stay.
        for states in [
            &mut self.breaks,
            &mut self.continues,
            &mut self.terminates,
            &mut self.raises,
        ] {
            #[cfg(test)]
            MAX_VECTOR.set(MAX_VECTOR.get().max(states.len()));
            states.reverse();
            let mut unique = 0;
            for index in 0..states.len() {
                if !states[..unique].contains(&states[index]) {
                    states.swap(unique, index);
                    unique += 1;
                }
            }
            states.truncate(unique);
            states.reverse();
        }
    }
}
