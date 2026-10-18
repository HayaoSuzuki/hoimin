### Spec Compliance

- ❌ Issues found: the required stop-after-first-payload-failure behavior is incomplete in `crates/hoimin-cli/src/analyzer/rust.rs:3586`. One subsequent mapping replacement is still constructed after index-neighbor admission fails (Important finding below).
- The remaining reviewed Task 9 contract is implemented: exact 16,777,216-byte default and checked arithmetic (`analyzer/payload.rs:11`, `:61`); explicit diagnostic (`analyzer/mod.rs:838`); shared plan/run ownership (`analyzer/mod.rs:101`, `:131`, `:473`, `:630`); independent selected-record validation (`analyzer/mod.rs:498`, `analyzer/store.rs:124`); documented retained-population scope and exclusions (`docs/development.md:1135`). Paths abbreviated to `analyzer/` in this report mean `crates/hoimin-cli/src/analyzer/`.
- ⚠️ Cannot verify from this task diff: end-to-end native test argv behavior and all unchanged cancellation/deadline/error precedence contracts across Tasks 6–7. Their implementation is outside this task; the controller's final branch review should retain those acceptance checks. The reported plan/verify and bounded-discovery suites are supported by existing logs; no failing evidence was found there.
- ⚠️ No fuzz execution is established or claimed. The migrated harness (`fuzz/fuzz_targets/python_analyzer.rs:34`, `:82`) and recorded separate-workspace check/clippy results establish compilation/lint only. Django debug qualification remains the explicitly reported pre-existing limitation; release probes provide its relevant measurements.

### Strengths

- `analyzer/payload.rs:61`, `:141`, `:172`: budget synchronization, checked admission and non-Clone leases are compact and centralized. `Charged<T>` drops the value before its reservation and exposes only immutable dereferencing. The focused access-site inspection found no production mutation that enlarges a charged payload without reservation.
- `analyzer/rust.rs:252`, `:296`, `:579`: duplicate/nonwinner membership uses exact borrowed strings, winning eviction releases the old population before admission, and merged deduplication borrows keys rather than creating another owned replacement population. Existing rank and stable-order rules are retained.
- `analyzer/mod.rs:448`, `:493`, `:508`, `:630`: converted plan candidates remain charged across subsequent files; only the terminal plain-Discovery handoff sheds leases. Run work receives the handler's shared owner, so a detached job cannot be forgotten by handler reuse.
- `crates/hoimin-core/src/candidate.rs:446` and `analyzer/store.rs:124`: validation allocation knowledge stays adjacent to its owning implementation; the encoded-buffer reservation follows the existing record-size check and precedes allocation. Inspection of identity cloning, ID/path normalization and Latin-1 encoding supports the conservative scratch formula.
- `analyzer/exception_hierarchy.rs:44`, `analyzer/rust/enum_members.rs:283`, `analyzer/rust/container_elements.rs:44`, `analyzer/rust/optional_keywords.rs:209`: scalable replacement/fragment copies were removed through borrowing without a general edit-descriptor framework. Hierarchy and streamed emit callbacks propagate failure back to their loops.
- `analyzer/payload_tests.rs:1` and `analyzer/payload_probes.rs:1`: deterministic boundary, release, overlap, large-population and production-path measurement coverage is substantial. New arithmetic properties are bounded; finite large-source cases are independently registered. The shared large-population helper applies the same assertions to every count rather than selecting scenarios inside assertions.
- `.superpowers/sdd/2026-10-18-issues-971-979/task-9-report.md`: five concrete reviews per stage, failed intermediate checks, narrow final-change validation, measurement scope and limitations are recorded. Existing focused/core/neighbor/probe logs support the stated passing counts; commit hooks report all four gates passed. The archived public comparison records 27 exact normal-output matches and nine expected explicit large-input failures, with runtime/RSS distinguished from charged payload peaks.

### Issues

#### Critical (Must Fix)

- None found.

#### Important (Should Fix)

- **Stop the second subscript builder after the first admission failure.** `crates/hoimin-cli/src/analyzer/rust.rs:3586` calls `collect_index_neighbors` and then continues into the mapping-conversion gates at `:3590` and `subscript_to_mapping_get_replacement` at `:3600`, without checking `self.candidates.failure`. For `x = items[1]` with `structure_index_neighbor` and `structure_mapping_get_subscript` enabled and a tiny payload limit, the first numeric neighbor fails admission, but the mapping helper still allocates `items.get(1)` (`:4680`); only `add_candidate_in_scope` subsequently discards it. Numeric keys pass `is_supported_mapping_key` (`:4757`). This contradicts the explicit requirement to stop constructing replacements after the triggering failure. The extra replacement can be source-sized, for example when the receiver spelling or bracket trivia is large. Add an immediate stop check after the neighbor dispatch. Add one focused test using the existing mapping-helper/allocation counters, asserting `PayloadLimit`, release to zero, and zero mapping-builder entries after the index-neighbor failure; candidate-generation counts alone miss this because the discarded replacement never reaches `make_candidate`. The existing passing tests do not exercise this inter-builder failure boundary.

#### Minor (Nice to Have)

- None raised.

### Focused inspections and evidence checks

- Read the supplied diff once in seven bounded chunks, after the brief, resolved guidance, proposal, batch follow-up, approved issue and implementation report. No git commands, Cargo invocations, tests or benchmarks were run; repository/index/HEAD were left unchanged. Only this ignored review report was written.
- Named risk: conservative validation scratch could undercount unchanged allocation owners. Inspected only `CandidateIdentity::from`, `stable_mutant_id`, validation and canonical path normalization in `crates/hoimin-core/src/candidate.rs:78`, `:395`, `:475`, `:541`, plus `PythonSourceEncoding::encode` in `crates/hoimin-core/src/source_encoding.rs:27`. The counted buffers/strings cover these allocations; diagnostic/source/index exclusions remain explicit.
- Named risk: the diff cuts `collect_subscript` between successive builders, potentially concealing a stop guard. Inspected that complete function (`analyzer/rust.rs:3577`) and the specific mapping helper/key gates (`:4649`, `:4753`, `:4757`). This established the Important finding without running tests.
- Named risk: public charged-wrapper fields could allow payload mutation or premature release. Searched direct `.value`, `.lease`, `Charged` construction and `into_untracked` access sites in the four production accounting/transfer files. Production construction/destructuring is limited to admission and conversion; the plain projection occurs at terminal plan handoff. No additional current defect found.
- Read the archived public-comparison JSON and focused/core/neighbor/charged-peak/fuzz/commit log evidence at `/tmp/hoimin-design-audit-5d3aee95/`. The checked passing outputs contained no compiler warning/error matches. Measurements were inspected, not rerun; reproducibility beyond those archived executions is not independently asserted.

### Assessment

**Task quality: Needs fixes.**

**Reasoning:** Shared accounting, transfer lifetimes, borrowed batches and adjacent scratch reservations are well structured and supported by substantial deterministic and release evidence. The missing subscript continuation guard violates the task's explicit early-stop contract and needs a focused regression before this task gate can pass.
