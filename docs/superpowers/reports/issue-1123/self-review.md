# Issue 1123 review and verification

Base main: `312b72d3e06e44145bbac28e998c97a8ca4f587d`.
Branch: `research/issue-1123`. No product code, operator defaults, dependency
manifests, compatibility paths or historical TOML documents are changed.

## Stage 1: scope and comparison (five passes)

1. Read the Issue's purpose and completion conditions: research and an adoption
   decision are required; unconditional implementation is not. Retained static
   planning and arbitrary test commands without target import for discovery.
2. Read the product principles: behavior assertions need independent requirements;
   default-on applies to accepted implementations, not unevaluated prototypes.
3. Inspected the real `statement_delete` collector and deletion guard, including
   expression binding/suspension exclusions. Confirmed that deleting an override
   body does not remove the method definition or restore parent lookup.
4. Inspected MutPy's IOD/IOP/SCD implementation at the referenced revision. Kept
   its operation meanings but rejected runtime MRO discovery for the proposal.
5. Compared nearest related operators plus all defaults, not just the favorable
   first two candidates. Defined equal 64-candidate ceilings and recorded every
   unexecuted count; all measured pools fit the ceiling.

## Stage 2: experiment and interpretation (five passes)

1. Checked each source/assertion pair against its stated requirement; every
   original weak and strong check passes. The tests distinguish child-provider
   selection and event ordering without asserting method dictionaries or MRO.
2. Checked candidate spans against original bytes before replacement and compiled
   every prototype source. The runner is explicitly ASCII/fixed-corpus, not a
   general-purpose static analyzer or source-preserving product producer.
3. Included identical-method and commuting-operation controls, constructor
   signature loss and initialization-dependency errors. Never count runtime
   errors as evidence of a missing assertion.
4. The initial direct-list ordering fixture overlaps `collection_append_insert`.
   Retained it and added a callback-sink fixture where those collection rewrites
   are unavailable. Report both results rather than claiming all ordering
   changes add independent coverage.
5. Checked exact-source overlap for deletion against both existing pools. Both
   deletion fixtures duplicate default `statement_delete`; recommend no SCD ID.

## Stage 3: boundaries and evidence (five passes)

1. Distinguished prototype matching from future product eligibility. Specified
   ordered class bindings, restricted single inheritance, same-name ordinary
   methods and shared cancellation-aware indexing; no universal MRO inference.
2. Excluded constructors/descriptors/async/generators and signature/default
   hazards from initial adoption. Kept residual effect-dependency risk explicit:
   a straight-line expression restriction does not prove reorderings safe.
3. Specified source-preservation and public selector/plan/verify tests as future
   implementation prerequisites, rather than claiming that the prototype tests
   cover new product behavior. Existing catalog remains 101 IDs / 82 defaults.
4. Checked process deadlines, disk reserve, one-job plans and temporary-directory
   cleanup. Recorded that direct fixed-source probes do not use worker resource
   management; no memory or throughput improvement is claimed.
5. Compared verdicts/descriptors across CPython 3.14.9 and 3.15.0b1, preserving
   the beta designation. Checked runner/helper hashes against the recorded
   receipts and avoided updating unrelated historical evidence.

## Verification

The following checks are recorded after their actual execution; the final PR
body reports their results. CI completion is a separate claim.

- Base CLI build with two Cargo jobs, incremental compilation disabled and debug
  information disabled.
- Fixed experiment on CPython 3.14.9 and 3.15.0b1: nine examples each, 18 original
  and 156 mutant checks each, identical verdict/descriptor results, no unexecuted
  candidates and both temporary roots removed.
- Independent read-only review reported no material findings. It checked the
  report against both receipts, verified descriptor/verdict agreement and
  runner/helper hashes, and independently replayed all 156 mutant checks with
  identical outcomes. It confirmed the source-level deletion overlap.
- Ruff `check --isolated` and `format --isolated --check` passed for the explicit
  research script. Isolation is necessary because repository configuration
  excludes historical research scripts from normal Python tooling.
- Safe-YAML validation passed for 59 concept headers; the knowledge reference
  inventory covered 63 pages with zero missing links/sources or unreachable
  pages. All nine newly recorded source hashes match their source bytes.
- `pytest tests/test_knowledge_references.py tests/test_docs_site.py
  tests/test_boundary_contracts.py -q`: **62 passed**. `git diff --check` passed.

The research adds no production implementation, so no workspace-wide Rust test
campaign or new formal proof is claimed. Existing commit hooks still apply.
