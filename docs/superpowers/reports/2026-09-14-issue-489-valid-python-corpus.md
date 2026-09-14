# Issue #489: Valid Python corpus review and evidence

Base: 8b33167a049e3cae0fc05e96ccf2253c660b7023. New evidence will be labeled by execution layer and host.

## OKF review: three passes

1. Read analyzer and Lean-evidence concepts with BindingFlow, AnnotationScope and CandidateSpan sources. Found scope models describe different consumers; reuse only explicit premises and do not upgrade historical internal-fixture modes.
2. Compared CandidateSpan.locationAt to current newline/BOM contracts. It is LF-only; limit reuse to one-byte-span replacement and check all physical newline/Unicode coordinates independently in the adapter.
3. Checked knowledge routing and source rules. Extend the existing analyzer concept and source inventories; keep draft status, source hashes and a separate worksheet so future reviewers can see uncovered consumer combinations.

## Design review: three passes

1. Mapped issue axes to current tests. Direct discovery alone already calls validation and cannot expose the raw-analyzer difference; use the established test-only direct rust.rs adapter plus public plan.
2. Checked for vacuous positives and shared expected calculations. Define explicit eligible and ineligible anchors once in Lean, require nonempty positive controls by producer, and compare exact candidates before compilation.
3. Checked model scope and inventory claims. Use finite integral key semantics only; preserve real CPython compilation and runtime. Register all canonical operators and report uncovered pairs instead of implying complete language coverage.

## Plan review: three passes

1. Traced every acceptance item to a task. Added both compile-first source validity and mutation-by-mutation compile, shared validator and real plan identity comparison.
2. Reviewed tests that could mirror implementation. Expected offsets come from independent source anchors; expected eligibility comes from Lean source rules. Added missing/excess/stale-hash/bad-span sensitivity, not just success counts.
3. Reviewed resources and mode promotion. Serial guarded Lean commands and dedicated two-job Cargo builds are mandatory. Worksheet source premises/public observations are recorded before execution; no strict label relies solely on an internal seam.

## Model and implementation review: three passes

1. Read all normalized facts against each consumer. Found that an operator module attribute replacement edits only the member token, unlike the full callable source string; corrected its declared original/replacement while retaining the full unique source anchor. Bound the model to trusted-import/shadow premises instead of inferring universal builtin flow rules for imports.
2. Compared deliberate broken rules with generated case coverage. Independent model sensitivity alone could pass even if the relevant public case disappeared; attached broken eligibility to the same generated walrus/generic/key sites and required the actual observation to reject that broken prediction. Generator gating requires all three fault families to remain represented.
3. Reviewed span and candidate transport. Raw candidates are validated individually and matched to independent byte anchors/physical locations; plan candidates are separately validated using their saved file_hash, and duplicate plan IDs/count changes are rejected. Extracted public plan construction/validation from the cross-product loop and switched every plan check to the actual hoimin binary with a subprocess deadline.

## Test review: three passes

1. The first compiled adapter exposed incorrect fixture scope: selecting collection_list_tuple also targets an inner tuple literal, and exception_type_pair accepts a simple handler name, not a tuple. The existing operator contract and code confirmed these were fixture errors. Isolated callable fixtures with grouped range input; retained an explicit trailing-comma tuple-negative exception fixture alongside grouped-name positives.
2. Public selector runs rejected an empty include-minus-exclude selection and the initially assumed symbol syntax. Corrected exclusion to retain a harmless break_continue control; symbol selection supplies a source root and module:qualname. These were harness configuration errors, not missing candidates; they were not treated as successful empty tests.
3. Reviewed independence and coverage accounting. Added explicit required syntax positions and eight known-risk producer/position/binding triples so deletion of a row cannot silently remove a required risk. Added a two-site selector fixture for below/equal/above cap checks, Unicode before the token for nontrivial columns, and original runtime probes for generic-source, global and nonlocal binding. Operator registrations include deferred reasons; missing pairs are output as uncovered rather than labeled supported.

## Resource and sensitivity observations so far

The sandbox blocked ps, so the first guarded Lean attempt returned infrastructure-error/monitor_error (exit126) before semantic execution. Re-ran with permitted process monitoring and the unchanged 30s/2GiB/250ms guard. Each dependency/model/generator command ran alone. A deliberate inverted generic-visibility definition failed both generic shadow theorems and the aggregate sensitivity theorem; restoring the model passed. No heartbeat/timeout limits were increased.

## Final correspondence evidence

Host: macOS arm64, CPython 3.14.5 (`/Users/hayao/.local/bin/python3.14`). Dedicated target, `CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 CARGO_INCREMENTAL=0 CARGO_BUILD_JOBS=2`; no shared target cache. `cargo test -p hoimin-cli --test valid_python_corpus -- --nocapture` passed 200 tests with three pre-existing ignored direct-analyzer tests. Its three new corpus tests passed: 41 source cases (39 strict, two model-only), 336 layout/selection checks, 253 validated and individually compiled candidates, 131 mutant runtime observations, 269 observed axis pairs and 385 explicitly uncovered pairs. Each plan uses the actual binary subprocess; every saved descriptor field is compared with the direct analyzer, not just the ID. An ID does not cover symbol metadata, so that stronger comparison was added in final review.

The final serial guarded output, freshness and sensitivity commands all exited zero. Generator reports `cases=41 seed=489 names=2 visibility_states=2 key_alias_pairs=3 max_frames=3 sensitivity=true`. Regeneration took 6.007 seconds and peaked at 782432 KiB RSS, below unchanged 30-second/2097152-KiB limits. This is a new model/import-dependency/generator check, not a local aggregate proof of all 126 repository modules.

Related integrations passed 54 tests: annotation scope 4, binding flow 4, candidate span 7, exception/match binding 8, mapping keys 5, operator function contracts 18 and type parameter bindings 8. The related tests used a temporary symlink to the existing Python environment, removed after execution. Workspace/all-target/all-feature Clippy with `-D warnings`, `cargo fmt --all -- --check`, all 28 CI workflow contract tests and `git diff --check` passed. Full OKF validation checked reserved-page/YAML structure, unique source IDs/footnotes, local links, root reachability, complete spec/report inventories and the new source hashes. Historical source hashes were not retrospectively revalidated.

## Independent and PR review: three passes

1. Parent traced compiler bytes, candidate transport, finite claims and real-plan selection. Found integration with #476 would reject the nonexistent `subject:missing` selector. Added a real `empty` function in Lean-generated variant sources and selected that function; regenerated byte expectations and reran all correspondence checks. No stack dependency is needed.
2. Re-read generated rows against the mode worksheet and current annotation resolver. Recorded exact builtin-source and generic-source annotation mismatches instead of relabeling reduced-model expectations as strict. The generic-source mismatch remains a potential PEP 695 contract gap; runtime original TypeError versus successful mutant is recorded separately from compile validity.
3. Compared final files with the issue acceptance list and PR claims. Kept all 55 operators registered (eight covered, 47 deferred with reasons), required syntax/risk triples and positive/ineligible controls, surfaced all uncovered pairs, and distinguished actual new runs from historical imported-model evidence. Parent independently found no further blocker in raw-byte compilation, validated real-plan comparison or coverage-gap reporting after the selector correction.

## Limits

This infrastructure change does not modify production analyzer behavior. Two annotation source fixtures are model-only because current public output disagrees with the normalized scope rule; they still undergo compile, validator and public/direct metadata checks. The linked worksheet identifies the mismatches. No existing internal-fixture scope evidence is promoted by analogy. Finite exact small key equality is not arbitrary Python numeric equality; the proof does not establish CPython grammar/compiler correctness. Representative cross-products leave 385 axis pairs uncovered, and 47 canonical operators deferred. The adapter reports those gaps instead of silently inferring support.



## Ordered integration: merge through issue464

Merged enhancement/issue-464 ata21d5a5 into the issue489 branch. Three actual merge reviews: (1) inspected all nine conflict blocks across development, analyzer concept and the two source indexes; additions were independent and source IDs disjoint, so retained both complete sides. (2) Checked the automatically combined workflow and its contract test: both issue490 boundary-contract Rust-job classification and issue489 valid-Python generator/freshness entry are retained. The package inventory remains126 modules,31 corpora and28 sensitivity generators, matching the development guide. The valid-Python model, generator source and generated corpus remain byte-identical to the previously validated issue489 head. (3) Ran28 CI workflow tests and10 boundary-runner contract unit tests, all passing, then verified formatting/diff and OKF20pages/828links with complete source indexes. Runner unit tests intentionally exercise mismatch/unexecuted report fixtures; this is not a fresh strict boundary replay.

No large Rust build or Lean regeneration was performed for this documentation-only conflict resolution; the parent owns the final combined Rust smoke and hosted CI. Historical corpus success is not relabeled as a new merged implementation run. Logs: `/private/tmp/issue489-merge464-ci.log` and `/private/tmp/issue489-merge464-boundary.log`.


### 統合CIのビルド資源

レビュー1:通常Rustのrunnerが、診断ログへの書き込み中にNo space left on deviceで終了したことをcheck-run annotationで確認した（103941334533）。取得できなかったjob logをテスト失敗の根拠にはしない。レビュー2:通常Rustとshuffleにdebug情報・incremental artifactの抑制とCargo並列数2を追加した。追加envを除いた全workflowが変更前と等しいことを構造比較し、test commands・features・assertionを維持した。レビュー3:CI構成28件が成功し、最終PR544と同じ設定であることを確認した。新しいheadのCIで効果を確認する。
