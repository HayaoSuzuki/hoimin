# #1005 import-depth facts and decision

Scope: read-only source/test inspection; no Cargo, Rust execution, product edits, or git mutation. Independent model: `/private/tmp/hoimin-priority-six-depth-facts/model.py` (`/tmp` aliases the same directory).

## Observed Rust source

- `crates/hoimin-python/src/analyzer/exception_hierarchy.rs:1080–1113`: each start gets a fresh DFS stack and visited set. The stack starts at `(start, 0)`. `depth >= MAX_STEPS` rejects the **start**, before checking whether the popped node was already visited. Other rejection is an edge returning exactly to the start. Rejections become `dynamic` after every start has been examined.
- `MAX_STEPS` is 256 (`:12`); depth counts edges. A pop at depth 255 passes; one at 256 rejects, including a duplicate pop that would otherwise be ignored. This is not a total visited-node/work budget.
- `Module::eager_imports` (`:644–649`) iterates normalized absolute eager dependency names in ascending BTreeMap order; pushes occur in that order, so the lexically greatest name pops first. Import statement/source-input ordering does not control it. `ImportEscapes::import` (`:459–468`) sets eager for nondeferred uses; relative dependencies are normalized before exclusion (`:872`, `:969–991`). Only mapped destinations present in `modules` are pushed; absent modules do not add depth.
- A cycle returning to some other reachable node does not by itself reject this start. Every actual cycle member is rejected when examined as its own start. Reaching a rejected/dynamic module does not propagate `dynamic` to its importers during this traversal.

## Concrete order-dependent DAG

Reduced bound **4**; edges: `r → a → b → x → leaf`, plus `r → x`.

- `r` pushes `a`, then `x`; `x@1` and `leaf@2` are visited first. Later `a@1 → b@2 → x@3` reaches an already visited node below the bound, so expansion stops. **r accepted**, despite an existing length-4 path.
- Rename only `a` to `z`. `z@1` pops first, followed by `b@2 → x@3 → leaf@4`. **r rejected**. These graphs are isomorphic; the depth predicate depends on import-name order.
- Real bound **256**: `r → n001 → … → n254 → x → leaf`, plus `r → x`, accepts `r`. Rename the first chain target `n001` to `z001` (or all chain nodes from `n*` to `z*`), and `r` rejects. Longest path is 256 edges in both.
- To expose this through candidates, give `r.py` its own unrelated `class Root(Exception): pass`, `class Child(Root): pass`, and `def f(): raise Child()`, alongside the two imports; other modules need only their imports. The depth decision controls whether the local `Child → Root` candidate is visible. This candidate consequence follows from inspected consumers; it was not executed in Rust.

## Noncycle prefix entering a cyclic SCC

For a simple prefix of **k ≥ 1 edges** into a simple cycle of **m nodes**, this start never gets the direct `next == start` rejection. With no other edges, the first cycle entry is popped at depth k and its duplicate is popped at depth k+m. Consequently the noncycle start rejects exactly when **k+m ≥ 256**. In particular:

- `p → cycle(254)` accepts p; duplicate entry depth 255.
- `p → cycle(255)` rejects p; duplicate entry depth 256, although every distinct reachable node is first visited by depth 255.
- A short prefix into a small cycle stays eligible. Marking every ancestor of any cyclic SCC dynamic would change that established behavior and remove unrelated local hierarchy candidates.

With branches or complicated SCCs, visited pruning makes depth order-dependent too; the k+m formula applies to the stated simple fixture only.

## Recommended deterministic conservative policy

Build the shared graph from exactly the existing mapped, present eager-import edges. Compute SCCs and their condensation DAG once. Mark only actual cyclic SCC members dynamic unconditionally. For depth, use a saturating upper bound on each SCC's traversable edge depth:

- SCC size `w(C)`; cyclic means size > 1 or a self-edge.
- `H(C) = max(terminal(C), max over C→D of [w(C) + H(D)])`, saturated at 256.
- `terminal(C) = w(C)` for cyclic SCCs, otherwise 0 (an acyclic SCC is one vertex).
- Mark an acyclic module dynamic for depth iff `H(component(module)) >= 256`.

On a DAG this is exact longest-path edge depth. The cyclic terminal includes the extra duplicate-entry edge that the old pre-visited depth check counted. SCC sizes safely bound distinct vertices a traversal could expand before exiting or revisiting, even when no Hamiltonian path exists. Therefore every old depth rejection remains rejected, regardless of dependency-name traversal order; the new policy can reject additional modules. It leaves a one-edge prefix into a 254-node cycle eligible and rejects one into a 255-node cycle. It does not propagate infinity from cycles to all prefixes. SCC construction plus reverse topological DP is O(V+E), with bounded arithmetic.

Conservatism here means candidate visibility cannot increase from this exclusion change alone: modules already dynamic remain dynamic, and new exclusions may remove candidates. It is deliberately an upper bound inside complicated SCCs, so a large highly branching SCC may cause extra prefix exclusions even when its longest simple path is short.

## Alternative and required user decision

An easier policy is to stop depth traversal on entering a cyclic SCC and measure the longest path through acyclic vertices only (optionally counting the entry edge). That preserves short prefixes but **revives** some modules the old code rejected, including `p → cycle(255)`. Treating every cyclic SCC as infinite depth instead removes candidates from every importer prefix, even `p → cycle(2)`; reject that blanket policy.

Exact compatibility with every old result and invariance under dependency-name renaming cannot both hold: the two isomorphic DAG fixtures have opposite old results. The decision is whether to accept extra exclusions from a deterministic conservative bound, or permit restored candidates under a deliberately narrower depth definition. Dynamic affects `binding_id` (`:1118`, `:1156`), `builtin_base` (`:1172`), `resolve_class` (`:1210`), visibility built through binding lookup (`:1235–1307`), and `replacements` (`:1321`, `untrusted_module` diagnostic); this is observable behavior, not just an internal performance refactor. Leave `resolve_class`'s separate 256-class ancestry cap (`:1205`) unchanged.

Proposed grilling question: **May we define the import-depth limit as the conservative SCC-weighted bound above, accepting that order-dependent DAGs and some complex-SCC prefixes can lose candidates, while guaranteeing that this change alone does not restore previously excluded candidates?** Recommended: yes. If rejected, explicitly choose which candidate increases are acceptable before implementing SCC depth semantics.

## Relevant tests and model evidence

- Observed existing test `crates/hoimin-python/src/analyzer/rust_tests.rs:9606`, `exception_hierarchy_rejects_import_cycles_and_unloaded_submodules`, checks a two-module cycle and an unloaded submodule. It does not establish depth boundaries, prefix-into-cycle behavior, or import-name order invariance.
- Existing cross-module candidate test at `:9491` demonstrates imported candidate visibility; the local candidate test at `:9575` demonstrates local hierarchy candidate emission. No directly relevant import-depth test was found in the inspected analyzer/tests.
- Ran **only Python model**, not Rust. Assertions passed for the two DAG orders at bounds 4 and 256; one-edge prefixes into cycles of size 254 and 255; and all 512 three-node directed graphs at reduced bound 2, where modeled old-rejected sets were subsets of the proposed policy. The latter supports the conservative relation on small graphs, not a formal proof or Rust integration validation.
