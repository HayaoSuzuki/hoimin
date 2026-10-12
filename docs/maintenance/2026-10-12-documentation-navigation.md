# Documentation navigation and first-use workflow

This record covers the documentation changes for #742 and #818 on one branch.
The baseline is `4f60a24`, with no CLI, resource-policy, schema or packaging
change. Current guides and maintained indexes remain English; historical source
documents retain their language and recorded evidence.

## Concrete problems and scope

The initial inspection found these examples before editing:

| Location at baseline | Problem | Intended correction |
| --- | --- | --- |
| `docs/usage.md`, Run it | First execution omits OS-policy prerequisites; README instead includes unconditional best-effort permission | Put policy choice before execution and align both entry points |
| `docs/usage.md`, Plan and verify with an agent | First plan requires Git changes; alternatives interrupt the before/after loop | Use an explicit-file example, numbered improvement steps and separate selection options |
| `docs/usage.md`, Live progress and debugging / Human review | POSIX environment syntax and multiline commands lack a shell label or PowerShell equivalent | Label shell-specific examples and supply runnable PowerShell paths for the main procedures |
| `docs/usage.md`, contents | Agent workflow and human review are absent | Link the main workflows directly and put diagnosis after the improvement loop |
| `docs/knowledge/index.md` | Appended design, audit and playbook entries sit under the source-catalog heading | Group existing links by purpose and add task-based routes to canonical guides |
| `docs/knowledge/overview.md` | Historical organization details dominate; guide ownership and current versus historical authority are unclear | Identify canonical guides and scope evidence independently of document status |
| `docs/knowledge/cli-reference.md` and `docs/development.md` | Regeneration commands are maintained twice | Keep commands in development and link to that procedure from the concept |
| `docs/knowledge/design/index.md` and `audits/index.md` | Maintained navigation remains Japanese and mixes related entries with the wrong section | Use English navigation and distinguish contracts from historical evidence |

The correction uses existing pages and indexes. It adds no requirements ledger,
new document-management tool, format standard or historical translation project.
The standalone distributed skills remain self-contained.

## Design self-reviews

1. Compared both Issues and assigned first-use procedures to #818 and maintainer
   source/evidence navigation to #742; one branch handles their shared entry points.
2. Read README, Usage, Development, OKF workflow, catalog overview and indexes;
   recorded concrete duplication and navigation failures before making edits.
3. Checked resource defaults, backend consent and plan/verify flags against
   `crates/hoimin-cli/src/cli.rs` and the existing usage resource contract.
4. Chose a standalone addition example and the existing `binary_add_sub`
   operator so a passing but weak test can be improved without Git changes.
5. Kept stable section anchors and historical sources; current procedures own
   commands, concepts own rationale and scope, and dated records own past results.

## Implementation plan self-reviews

1. Put prerequisites and shell conventions before first execution; keep optional
   best-effort consent explicit rather than recommending it unconditionally.
2. Prepare source and tests, create report directories, plan one candidate,
   verify, add a specification-backed test and reverify the same candidate.
3. Move logging after the primary workflow, keep selection alternatives separate,
   and preserve existing public section anchors when reorganizing Usage.
4. Reuse the catalog and overview for task routes, canonical ownership and evidence
   distinctions; deduplicate only the identified CLI regeneration procedure.
5. Check commands on current Rust code and Python 3.14, strict site and changed
   repository links, OKF metadata and representative task routes. Use two build
   jobs and an owned target, clean it, and finish with independent review.

## Verification scope

The installed `hoimin` on this host reports 0.1.0 and cannot validate current
examples. Build the current baseline CLI in a task-owned target instead. The
example exercises a small local project and a test-only improvement; it does not
establish mutation-test performance or native resource enforcement on other OSes.
No general E2E suite or permanent test helper is added for document organization.

## Findings from executing and rendering the examples

The first verification correctly returned exit 1 with a complete report and one
survivor. The tutorial now explains that expected result next to the command.
After the test-only improvement, verification returned 0 with the same candidate
killed and `progress` reported `improving`. Both baselines passed. Empty-plan
verification returned 2 before a baseline.

The original logging example writes `debug.jsonl` inside the project being
copied. Executing that form reproduced `workspace.original.changed` for the log;
the corrected examples write into a sibling diagnostics directory. Both shell
forms then completed, and the PowerShell form restored its original `RUST_LOG`.
Review list/show/record commands also ran successfully in both shells with a real
candidate ID and an assessment reason appropriate to the example.

The original Markdown configuration does not render fenced code inside numbered
steps. Enable the already locked `pymdownx.superfences` extension and use the
renderer-required indentation. A small regression verifies code placement and
text within a numbered item. It failed under the old configuration and passed
with the new configuration. This protects a usable first-run procedure; it is
not an E2E suite or an assertion about a particular Markdown configuration key.
The existing ranked-plan documentation assertion now follows the one-candidate
first loop rather than requiring the old ten-candidate example.

README remains an entry point rather than the tutorial: its net increase is two
lines. The worked example and OS/shell details live in Usage.

## Implementation self-reviews

1. Checked that the example requires no Git diff or package installation of the
   sample source, and compares one unchanged source/candidate before and after.
2. Kept best-effort consent before execution, explicit and inherited from the
   plan; default Windows and delegated Linux commands retain hard enforcement.
3. Checked shell-specific quoting, continuations, JSON encoding, expected exit
   codes and output placement; corrected the logging failure found by execution.
4. Preserved existing Usage section anchors and all existing catalog link targets;
   grouped misplaced entries without moving historical documents or translating
   their contents. Maintained child indexes are English.
5. Kept generation commands in Development, retained rationale in the concept,
   refreshed only reread source metadata and preserved historical evidence dates,
   hashes and status. README stays short; no CLI or resource guarantee changes.

## Testing self-reviews

1. Built the current baseline Rust CLI with two jobs and a dedicated target;
   the host's installed 0.1.0 executable was not used for verification.
2. Executed the full weak-test → plan → preview → verify → improved-test → verify
   → progress loop with Python 3.14.6 and pytest 9.1.1 in PowerShell 7.6.6 and Git
   Bash on Windows. Compared saved candidate IDs, passing baselines, completeness,
   survived/killed outcomes and expected exit codes. Native Linux/macOS resource
   enforcement is not established by these executions.
3. Exercised logging and review commands in both shells, environment restoration
   in PowerShell and empty-selection rejection. The initial inside-project log
   failure is distinguished from the successful corrected examples.
4. Verified the render regression fails without the extension, restored the
   configuration, and ran both documentation test modules: 26 passed. Full
   maintained-code Ruff lint, formatting (50 files) and ty checks passed with no
   added lint exception or dependency.
5. Built the strict site, checked changed local links and anchors, retained catalog
   targets and OKF structure/source metadata. These are documentation checks,
   not fresh proofs or evidence that historical audits pass on the current code.

## Representative maintainer lookups

The following reading routes were inspected against the linked pages. They use
existing guides and original sources rather than a new requirements ledger:

| Task | Reading route | What the route distinguishes |
| --- | --- | --- |
| Change CLI help | Catalog task table → Development, Generated CLI reference → generator and CLI definitions | Canonical regeneration/check commands versus reflection and runtime-validation scope |
| Change a resource guard | Development task table → Usage limits/disk safety → runtime-lifecycle concept → referenced implementation/model evidence | User-visible policy, ownership rationale, model assumptions and actual host execution |
| Change a saved plan or session | Catalog task table → Compatibility → selection and session/report contracts | Current reader/version policy versus older design proposals and audit outcomes |
| Interpret a Lean audit | Catalog/overview evidence guide → audit summaries → original report and comparison sources | Model proof, finite corpus comparison, static inspection and historical host execution |
| Diagnose publication | Catalog task table → Releases/PyPI guide → provenance concepts and original hosted records | Current recovery procedure versus historical successful run evidence |

Source catalogs still cover their original directories; this maintenance record
does not add a design/audit original under those directories. Standalone skill
packages and generated references retain the self-contained content they need.

## Final verification and independent review

A separate temporary project outside Git also completed the same loop: Git
reported no work tree, the first verification exited 1 and the second exited 0,
both baselines passed, both reports were complete and candidate IDs matched.
The built Usage HTML contains seven continuous numbered steps with rendered
code blocks rather than literal fences.

The final reference check covered 374 local links/anchors and seven local OKF
sources across the changed pages, retaining every previous catalog link target.
The knowledge check parsed all 48 pages and verified the five refreshed source
hashes, IDs and retained draft status. Strict site generation, whitespace,
26 documentation tests, full Ruff lint/format and full ty checks passed.
External URL availability and every link inside historical originals were not
revalidated. At this initial documentation-only review, Rust, Lean and native
Linux/macOS suites were not rerun. The CI follow-up below records the subsequent
Rust verification.

The fresh independent reviewer read the complete change and saved examples and
found no actionable issue or missing Issue acceptance criterion. The scoped
technical-documentation diagnostic for the changed workflow and navigation
passages was 10/10, with no failed row or blocking finding. This is not a claim
to have audited the entire Usage reference or historical catalog contents.

After verification, `cargo clean` on the checked task-owned target removed
1,680 files (764.7 MiB). C: then had 181,938,376,704 bytes free (about 169 GiB).
No ordinary workspace target or unrelated cache was cleaned.

## CI follow-up: multiline command extraction

Run 38246773913 failed in the stable, randomized-order and contracts Rust jobs
at the same `documentation_contract` assertion: the test extracted only the
first physical line of a continued command, so it could not find `--`.
The previous verification scope missed this Rust consumer of the Usage page.

Five design checks inspected the three failure logs, traced the failing
separator assertion, compared old single-line and new shell examples, checked
the shared fixture paths, and confirmed that command extraction is test-only.
Five plan checks retained every existing command, limited joining to explicit
Bash/PowerShell continuations, kept shell redirection outside application argv,
required separate regression cases for both shells, and selected the complete
report-handler suite plus strict Rust lint and documentation checks.

The correction belongs to the existing documentation branch and changes no
production CLI behavior. The shared fixture substitutes its `src/calc.py` for
the tutorial's root-level `calc.py`, as it already substitutes the project root,
Python interpreter and metrics path. This fixture adaptation does not claim to
execute shell redirection or environment assignments; the earlier real-shell
smoke checks cover those operations.

Five implementation reviews checked fence-boundary buffer reset, buffer reset
after a complete logical line, preservation of the environment prefix, removal
of shell redirection before CLI execution, and restriction of the fixture path
substitution to the exact `--file calc.py` argument. Both new tests use separate
Arrange–Act–Assert blocks without runtime branches. The helper remains a narrow
whitespace tokenizer for these examples, not a general shell parser.

Both continuation regressions failed against the original extractor with only
the launch line returned. After the correction, all 33 report-handler tests,
including the existing real Usage execution contract, passed. The 26 Python
documentation tests and strict site build also passed. A fresh independent
read-only reviewer found no actionable issue; they did not rerun verification.

The first full workspace run then exposed a second stale documentation check:
`usage_documents_agent_plan_workflow` still expected the old ten-candidate
`PLAN.json` example. Its two expectations now distinguish the one-candidate
tutorial from the optional `CHANGED_PLAN.json` diverse-selection example.
This is the same expectation update already made in the Python documentation
tests, and does not relax the selection-policy contract assertions.

Five testing reviews checked the observed red/green continuation regressions,
the executable report-handler contract, the complete Rust workspace result,
the contracts-enabled report handler, and strict all-target/all-feature Clippy
plus formatting and documentation checks. The final full workspace run passed
2,800 tests with zero failures and 27 ignored tests across 151 test groups.
The contracts-enabled report handler passed all 33 tests. Clippy's string
allocation warning was addressed with `clone_into`, without a lint exclusion.
The additional stale assertions were also independently reviewed against Usage.

These runs used Windows, Python 3.14.6 and Rust 1.99.0 with two build jobs and
two test threads. Native Linux/macOS and the pinned nightly randomized-order
job remain hosted CI checks; the Rust corpus comparisons do not constitute a
fresh standalone Lean proof build.

The final default-feature report-handler rerun also passed all 33 tests after
the Clippy adjustment. The final independent diff review reported no actionable
finding. Cleanup verified the absolute task-owned target path, then removed
4,643 files (4.7 GiB) with `cargo clean`; C: had 184,049,463,296 bytes free.
