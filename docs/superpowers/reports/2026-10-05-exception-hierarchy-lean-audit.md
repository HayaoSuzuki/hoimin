# Exception hierarchy: formal audit and implementation correspondence

Branch: `investigate/user-defined-exception-mutations`. Implementation baseline:
`ab0356a`. This audit revises the earlier decision to omit Lean. A graph-only
proof would have been insufficient, but that did not justify omitting models of
visibility, identity, index bounds and prepared-input consistency.

## Claim and correspondence worksheet

A hierarchy candidate must name distinct, related, trusted exception identities
visible at its use site. Building an index must not authorize classes from changed
prepared inputs or from a lower-priority module exposed by a transient deletion.
Retained index entries must respect their configured bound.

The following worksheet is fixed before choosing the exploration domain.

| Premise / observation | Lean representation | Production configuration / observation | Evidence and mode |
| --- | --- | --- | --- |
| Exception ancestry and direct parent/sibling relation | Module-qualified class identifiers and parent lookup; bounded ancestry walk | Owned Python class definitions; public plan replacement pairs | `plan::create`, `ExceptionIndex::resolve_class/replacements`; `strict` for generated fixtures |
| Constructor restrictions | Constructor-transparent path | Plain/custom source or inherited constructor; public raise/handler pairs | `resolve_class`, Python fixtures; `strict` |
| Lexical shadowing, including private names | Canonical binding keys and local-before-global lookup | Parameter/private parameter fixtures; public plan pairs | `Bindings`, `mangled_name`, `Collector`; `strict` for fixtures; arbitrary Python canonicalization excluded from the theorem |
| Import position and module identity | Natural-number load/use order and module-qualified identifiers | Earlier/later import, relative module context, reserved module origin; public plan pairs | `Module.loaded`, `resolve_relative_imports`, `module_name`; `strict` for fixtures |
| Index entry limit | Guarded insertion counter, arbitrary natural-number bound | Actual 65536-entry visibility boundary; public plan success/error and pairs | `build_visible`; `strict` at the production bound; reduced-bound sensitivity is `model-only` |
| Change/delete/build/restore ordering | Prepared/current input state and immutable successful cache | Owned fixture invokes `ExceptionProject::load` between filesystem events; observes load result, candidate pairs and fingerprint recheck | `exception_project`; `internal-fixture`, not public concurrency correspondence |
| Parser, OS, custom import hooks, hash collisions | Outside model | Existing parser/CPython/filesystem tests remain necessary | Not claimed as proved |

## Design and implementation plan

1. Define a small executable model and kernel-checked theorems for ancestry,
   shadowing, visibility, bounded insertion and snapshot/cache preservation.
2. Enumerate snapshot traces shortest-first and retain witnesses for deliberately
   broken validation, origin selection, shadowing, visibility and limit rules.
3. Generate all expected observations in Lean. Compare public plan fixtures in
   `strict` mode and snapshot schedules in `internal-fixture` mode. Adapters may
   normalize observations but must not compute expected values.
4. Register model, generator, freshness and sensitivity gates in the existing serial
   Lean CI lane. Run correspondence tests and repository checks; commit the artifacts.

Design reviews: (1) narrowed the claim to model properties plus exercised Rust
correspondence, (2) separated internal snapshot scheduling from public plan cases,
(3) kept parser/import-runtime facts outside the proof instead of assuming they
were verified. Plan reviews: (1) Lean owns source fixtures and expected values,
(2) broken variants must be detected before trusting green checks, (3) every Lean
command uses a wall-time/RSS guard and CI checks corpus freshness.

## Evaluation placement and limits

Imported modules contain definitions and kernel-checked proofs only. Enumeration,
serialization and broken-model checks live in the generator executable. Start the
snapshot alphabet at change/delete/restore/build, depth zero, and measure each
increment through depth three; do not increase after unexplained cost growth.
Use a 20-second interactive deadline, 2 GiB aggregate RSS cap and 250 ms sampling.
The repository CI lane retains its 30-second deadline. Theorems use explicit local
heartbeat limits, never unlimited search.

## Results and counterexample ledger

Pending model construction and verification. No production-correctness claim is
made by this report while these sections are pending.
