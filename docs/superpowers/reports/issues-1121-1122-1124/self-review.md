# Self-review record

These are reviews by the implementing agent, not independent review or proof.

## Step 1: requirements and protocol

1. Requirements: all three Issues ask for an adoption decision before product
   infrastructure. Kept production operators, defaults and saved formats out of scope.
2. Comparison validity: variable-bound slices can have no incumbent candidates.
   Protocol now distinguishes equal budget ceilings from equal actual run counts.
3. Semantics: omission can remove side effects and floor division changes types.
   Restricted the slice probe and included overload/error and domain-equivalence controls.
4. Resources/security: checked 165 GiB free and 28 GiB target; limited Cargo to
   two jobs with incremental/debug information disabled. Built the current CLI
   rather than attributing an old binary's results to current main.
5. Evidence: requires independent requirements, original controls and assertion
   failures; forbids treating a survivor or wider line coverage as a test defect.

## Step 2: adapter implementation

1. Requirements: reviewed every example against its stated requirement. Added
   literal-slice overlap and bounded-domain controls, rather than only positive examples.
2. Comparison validity: the first run rejected a mistaken selector spelling
   (`literal_integer`). Corrected it to the canonical `integer_literal_neighbor`.
   Read the public preview definition and used `candidates[].id`, not a guessed field.
3. Semantics: verified syntax after each proposed edit; inspected byte/character
   offsets and explicitly limited this adapter to ASCII. Added left-to-right,
   once-only operand evaluation and differing result-type controls for division.
4. Resources/security: target generation parses source only. Execution uses
   fixed, repository-owned examples in disposable directories, one child at a
   time, with subprocess and CLI timeouts. Cleanup is checked after the context exits.
5. Evidence: identified that offline policy replay alone understates agent
   orchestration cost. Added real adaptive one-candidate `verify` calls whose
   decisions use completed CLI statuses; retain fresh-baseline cost separately.
   Added assertions against CLI verdicts, completeness and exact selected IDs.

## Step 3: experiments and decisions

1. Requirements: compared the direct incumbent family and the full default
   profile. Variable-bound slice examples have no current default candidates;
   literal windows are redundant. The conversion control has a separate useful
   existing candidate, so it is not evidence for the division proposal.
2. Comparison validity: checked ceilings 4/8/12 separately from actual run
   counts. The line-stop queue exhausts at seven; it does not spend eight or
   twelve executions. Real adaptive execution includes seven or ten fresh
   baselines, compared with one for the existing batch; timings include that cost.
3. Semantics: positive-first assertions do not establish negative-input kills.
   Added and executed negative-only original/mutant controls. Kept unsupported
   overload TypeErrors out of useful kills and bounded equivalence out of proofs.
4. Resources/security: JSON CLI disk reports show small owned footprints with
   no resource stop or retained worker. Automatic temporary-directory deletion
   succeeded. Replaced whole-binary hashing with streaming hashing to avoid an
   unnecessary roughly 56 MiB allocation in the replay adapter.
5. Evidence: CPython 3.15 is specifically 3.15.0b1, not a stable release. A
   `/usr/bin/time -l` wrapper failed on a sandboxed sysctl after the experiment
   finished; reran Python directly with exit 0 and do not claim RSS measurement.
   The maintainer rejected an opt-in adoption assumption: adopted operators now
   default on with explicit exclusion; this evaluation still adds no product code.

## Step 4: documentation and final verification

1. Requirements: checked the three decision sections against the Issue
   completion criteria. Each has pool/budget evidence, an adoption decision and
   maintenance conditions. Applied opt-out to the product principles and OKF
   product-direction page, without claiming the proposed operators exist yet.
2. Comparison validity: recomputed report cost rows from both final JSON files.
   Verdicts, candidate pools and policy traces agree across Python versions.
   Distinct requirements remain separate from useful mutant counts; selected-ID
   completeness remains separate from unexecuted-pool coverage.
3. Semantics/provenance: reviewed the source catalog entries, original/report
   headings and links. Keep the audited base revision and untracked/modified
   states from the time of reference; do not label draft synthesis verified.
   The final metadata check resolves source revisions and compares SHA-256 hashes.
4. Resources/security: after both final replay runs, `cargo clean` removed
   144,977 build files (36.3 GiB logical size). Available host space increased
   from about 164 to 192 GiB. No Rust source changes require a further rebuild.
   Local `.idea/` and `.serena/` material is excluded from this change.
5. Evidence: both interpreter experiments exited 0. Explicit Ruff checking and
   formatting pass (ordinary discovery excludes historical docs), the existing
   reference-tool tests pass 5/5, and all 61 OKF pages parse with valid reserved
   file structure. Reference inventory finds no missing links/sources or
   unreachable pages. No independent reviewer, RSS measurement, Linux/Windows
   execution, real-project trial or production feature implementation is claimed.
