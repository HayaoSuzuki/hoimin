# Separate FIFO watchdogs from large regular-input validation

Follow-up to PR #1058 at `9b4e76693e1b1b20cd9b70444ec661007fc80227`.
The original implementation and measurement record remains in
[README.md](README.md); this follow-up changes only the artifact-input tests.

## Failure and diagnosis

Three Linux CI jobs failed on the same test,
`progress_does_not_apply_review_size_limit`:

- [Contracts](https://github.com/HayaoSuzuki/hoimin/actions/runs/39576770437/job/119144930328?pr=1058).
- [Rust workspace](https://github.com/HayaoSuzuki/hoimin/actions/runs/39576770437/job/119144930596?pr=1058).
- [Nightly randomized order](https://github.com/HayaoSuzuki/hoimin/actions/runs/39576770437/job/119144930346?pr=1058).

Each job passed the other 24 artifact-input cases, then the helper's five-second
watchdog killed the large-report command. The fixture contains a valid current
report followed by 65 MiB of whitespace, and progress reads it twice. Streaming
serde decoding in debug builds processes that whitespace byte by byte. The test
was applying the watchdog intended to detect a blocked FIFO open to finite CPU
work on a large regular file. The CLI has no five-second artifact-read contract.

The same 68,162,282-byte input completed locally with exit 0 in 4.846 seconds,
producing valid progress output. This explains why the Linux shared-runner limit
was fragile; it does not establish a general throughput guarantee. Nightly's
failed artifact suite used shuffle seed `1792575186426381072`.

## Change and verification

Only the two >64 MiB regular-file fixtures use a separate 30-second budget. Small
inputs and all FIFO rejection cases retain the five-second watchdog. Success,
error, report usability and comparison-state assertions are unchanged. The child
owner still kills and reaps on panic, including a genuine timeout. Both fixture
size and work remain finite. No production reader, input acceptance, memory
limit or CLI setting changes.

The full 25-test artifact-input suite passed locally in the default configuration,
with contracts enabled, and on nightly-2026-10-08 in randomized order. Nightly was
checked with both seed `20261021` and the failed CI seed. Workspace all-target,
all-feature Clippy passed. Exact results are in
[ci-followup.json](ci-followup.json).

A fresh-context read-only review found no Critical or Important issues and
confirmed the large-input budget does not relax FIFO checks. Cargo ran serially
with two build jobs and the existing shared target; no release build or large
benchmark was added. Validation ran on macOS arm64; the updated Linux jobs are
verified separately after push.

Existing OKF selection and session/report contracts were consulted. Their
regular-file admission, current-format validation, finite-read policy and lack
of a universal filesystem deadline are unchanged. This test-only watchdog policy
therefore needs no contract or index update. The original source hashes and
measurements remain attached to their original recorded state.
