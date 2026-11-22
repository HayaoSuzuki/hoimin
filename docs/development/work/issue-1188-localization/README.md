# Mutation-based fault-localization evaluation

This record helps hoimin maintainers assess and reproduce the initial evaluation
for [#1188](https://github.com/HayaoSuzuki/hoimin/issues/1188). It measures whether
mutating already-failing Python code adds useful location information. The
experiment is separate from the product's passing-baseline `run`/`verify` contract.

Read the [results and adoption decision](report.md) before interpreting the
rankings. The [design](design.md), [implementation plan](plan.md), and
[review record](review.md) explain the experiment's boundaries and checks.

## Evidence

The retained records contain two historical faults from
[pytest-httpserver](https://github.com/csernazs/pytest-httpserver), using unchanged
upstream regression tests. [Case metadata](cases.json) pins the fixed and buggy
source revisions, tests, and repair locations. Each repair location uses the buggy
file's line numbers and is frozen before mutant selection.

The evidence files retain source/test hashes, generated candidates, selected
candidate IDs, both baseline repetitions, per-test mutation observations, failure
digests, process outcomes, and costs:

- [Empty-query observations](evidence/empty-query.json.gz).
- [Repeated-start observations](evidence/readiness.json.gz).
- [Recomputed summary](evidence/summary.json).
- [Upstream license](evidence/pytest-httpserver-LICENSE.txt) for source fragments
  in candidate records and test failure output.

Harness hashes identify the executed file bytes. The checker accepts Git's
LF/CRLF conversion when comparing those hashes with another checkout; other
content changes fail verification.

## Reproduce on Windows

Use Python 3.14.9, Git, uv, and a hoimin debug binary built from this checkout's
Rust sources. The retained run uses hoimin 0.15.0; the record includes its SHA256.
Build the binary with `cargo build -p hoimin-cli` if it isn't available. Run the
following PowerShell commands from the hoimin repository root. The selected server
tests require permission to bind to localhost; no external service is involved.

1. Set paths for the scripts and a fresh, ignored output directory:

   ```powershell
   $study = "docs/development/work/issue-1188-localization"
   $work = ".superpowers/issue-1188-reproduction"
   ```

2. Create the experiment environment and install the declared dependencies:

   ```powershell
   uv venv --python 3.14.9 "$work/venv"
   $python = "$work/venv/Scripts/python.exe"
   uv pip install --python $python -r "$study/requirements.txt"
   ```

   Direct dependencies are pinned. The evidence also records observed package
   versions; transitive dependency resolution and machine speed can differ.

3. Obtain the public source history:

   ```powershell
   git clone https://github.com/csernazs/pytest-httpserver "$work/upstream"
   ```

4. Export the pinned inputs without changing the upstream checkout:

   ```powershell
   & $python "$study/prepare_public.py" `
     --repository "$work/upstream" --projects "$work/projects"
   ```

   Expect two preparation receipts. This command refuses existing case folders;
   use a fresh output directory when preparing another run.

5. Run the bounded experiment:

   ```powershell
   & $python "$study/evaluate.py" --manifest "$study/cases.json" `
     --projects "$work/projects" --binary target/debug/hoimin.exe `
     --output "$work/results"
   ```

   Each case has a 600-second budget, at most 96 selected mutants, and two
   observations per mutant. Test processes have a 20-second limit. Windows Job
   Objects bound each invocation to 16 processes and 1 GiB of committed memory.
   The 30-second candidate-generation invocation also receives the remaining case
   budget. Cleanup can extend the wall time by a small amount after the deadline.

6. Validate the new records and write their summary:

   ```powershell
   & $python "$study/check_evidence.py" --records "$work/results" `
     --write-summary "$work/summary.json"
   ```

   Expect `Verified 2 public cases, selections, observations and rankings.`
   Review each record's `complete`, `stop_reason`, and `error` fields: ordinary
   case rejection or an exhausted budget can still leave the driver exit code at
   zero. The checker rejects errors and recomputes the accepted or partial data.
   Changing case metadata or harness code intentionally invalidates the retained
   evidence checks; prepare and run a new evaluation for such changes.

## Verify the retained evidence and harness

To recompute the committed summary without rerunning upstream tests, use the
environment from the reproduction procedure:

```powershell
& $python "$study/check_evidence.py"
```

To check the analysis and process boundaries, run:

```powershell
& $python -m pytest -q $study
```

The harness tests exercise real subprocesses, Windows tree termination, invalid
test observations, source restoration, and stale bytecode. They supplement
hand-calculated examples with parametrization and Hypothesis properties.

The driver uses owned temporary copies. On Windows, unconfirmed process-tree
termination aborts the experiment and retains the copy for inspection. The POSIX
fallback kills a process group; this evaluation doesn't establish equivalent
descendant containment or memory limits there. These scripts aren't a sandbox for
untrusted code or a supported hoimin execution API.

See [remaining evaluation work](report.md#remaining-evaluation-work) for the
evidence needed before a product adoption decision.
