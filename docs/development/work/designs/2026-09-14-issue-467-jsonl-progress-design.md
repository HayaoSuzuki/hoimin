# JSONL progress input design (Issue 467)

## Contract and approach

Progress accepts current-schema JSONL lifecycle streams alongside existing JSON
v2/v3 documents, with automatic detection independent of filename. A first
nonblank line that is a JSON object with `kind` selects JSONL. Other input uses
the existing document decoder. JSONL requires one complete event per nonblank
line; CRLF, surrounding blank lines and a final complete line without newline
are accepted. Legacy v2 JSON documents retain their existing decoder; v2 JSONL
is explicitly unsupported.

Read JSONL through BufReader and reuse one line buffer. Retain the run header,
baseline, finished mutants and summary needed by existing progress validation;
discard diagnostic and mutant_started event payloads after checking them.
Memory is proportional to retained candidates plus the largest event, not the
whole event history. Existing progress keeps only adjacent reports (#432).

Expose a side-effect-free core ReportSequence::validate method before
observe: invalid external events must return errors even in contracts builds.
The producer observe method retains its existing contract assertions.
Use core ReportSequence for run ID, monotonic sequence, start/finish identity,
active-mutant and terminal-order rules. The initial run header is decoded by the
existing progress-specific type so historical/future normalized-config objects
remain opaque; seed the sequence validator with its validated identity and
metadata only. Baseline may occur at most once and before mutant starts.
Require run_finished, reject records after it, duplicate runs/mutants, unknown
kinds, malformed/truncated lines and unsupported event schemas. Error paths
identify the input report. Blank lines carry no event.

Project validated finished events into the existing RunReportDocument. Reuse
its schema, duplicate identity, result, count and completion/exit validation
before classifying usable or missing-baseline/baseline-failed/incomplete.
Lifecycle starts are mandatory for JSONL finished mutants; projected JSON
documents intentionally omit them and retain their looser sequence contract.

## Alternatives and limits

Whole-file Value parsing would preserve too much transient history. A new
independent result validator would duplicate the fixes for #460/#483. Extending
legacy JSONL is deferred until a concrete historical producer contract needs
it; this change supports the CLI's current output without weakening v2 JSON.
No fixed per-line size limit is introduced because existing output permits
large candidate/config payloads. Memory bounds are relative to largest event.

## Verification

Add failing current JSONL ingestion and mixed-format progress parity tests;
exercise actual run JSON and JSONL outputs, including failed/incomplete inputs.
Reject lifecycle/order/schema/result corruption at the public reader and CLI.
Use a diagnostic-history heap test to catch retaining every parsed event.
Keep all existing progress, heap and Lean progress-input adapter checks.

## Self-review changes before implementation

1. Preserve opaque normalized_config rather than parsing through RunConfig.
2. Baseline order/uniqueness is not covered by ReportSequence; check locally.
3. Separate projected JSON from full lifecycle JSONL; require starts only for
   JSONL and explicitly state current schema support and largest-line memory.
