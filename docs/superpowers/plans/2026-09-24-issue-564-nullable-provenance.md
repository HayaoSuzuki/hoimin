# Issue 564 implementation plan

Goal: suppress nullable additions for shadowed builtin type names without losing proven builtin candidates.
Spec: ../specs/2026-09-24-issue-564-nullable-provenance.md
Execution: inline in the independent issue-564 worktree, authorized through PR preparation.

## Constraints and review focus

No dependencies or public schema changes. Use annotation scope, preserve function-header/body distinctions and deferred module/class behavior. Keep #565 disallowed-syntax recursion separate from builtin provenance. Validate named imports that reuse builtin spellings, dynamic/wildcard uncertainty, nested scalar shadowing, type parameters, and nullable removal.

## Task 1: behavioral RED

- Add `nullable_builtin_provenance` unit tests in crates/hoimin-cli/src/analyzer/rust_tests.rs for all 8 names and scope boundaries; assert `type_nullable_add` counts and retained originals.
- Add crates/hoimin-cli/tests/nullable_builtin_provenance.rs public-plan fixtures that execute original annotations with CPython 3.14 and verify candidate counts/spans, including a custom metaclass whose union raises.
- Run `cargo test -p hoimin-cli --lib nullable_builtin_provenance` and the public test before production edits. Expected: shadowed-name candidates remain and violate zero-count expectations.

## Task 2: provenance gate

- Extend `MUTABLE_BUILTINS` with str/int/float/bool/bytes so every binding form and dynamic uncertainty uses existing resolver tracking.
- Change `nullable_add_allowed(annotation, imports)` to `nullable_add_allowed(annotation, facts, imports)` and add an independent recursive builtin-provenance predicate. `annotation_resolution(name.range().start(), name.id)` must equal DefinitelyBuiltin for scalar atoms and bare builtin collection constructors. Resolve imported constructor spellings first; recurse into subscript arguments and tuple/list/starred/union children only to check builtin provenance.
- Keep collection_replacements, nullable_removal, and contains_disallowed_annotation behavior unchanged. Run focused tests and the library suite.

## Task 3: verification and documentation

- Complete three separate implementation reviews and three separate test reviews, with findings recorded in the issue report.
- Run `RUST_TEST_THREADS=2 cargo test --workspace`, `cargo fmt --all -- --check`, and `cargo clippy --workspace --all-targets --all-features -- -D warnings` using the dedicated sequential Cargo target cache.
- Update analyzer and annotation-builtin OKF contracts, append repair evidence to the historical nullable audit, and register the new spec/report in source catalogs with actual revision/state/hashes. Validate YAML, links, source IDs and metadata.
- Commit implementation/tests/docs and report exact evidence to parent; parent pushes and opens PR.
