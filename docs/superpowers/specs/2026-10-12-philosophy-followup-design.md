# Documentation contracts and runtime ownership boundaries

This design implements #834–#837 for maintainers, based on main `437e19f`.
The user approved proceeding with all four issues and stacking where useful.
The objective is less change amplification and fewer independently mutable
states, without changing the documented CLI or filesystem safety contracts.

## Selected changes

- #834: remove duplicated prose-presence tests from Python and Rust. Keep the
  actual Usage run-command execution, schema validation, CLI behavior tests,
  rendered-code regression and strict site checks. Check the metrics example
  through its parsed configuration rather than a complete command string.
- #835: make `VerifiedPlan` fields private. Expose read-only configuration,
  resolved selection, selection metadata and preview for actual inspection
  needs. Permit only a consuming metrics-destination update. Transfer the
  execution payload as one internal value and offer a borrowed-writer execute
  operation so callers need not reconstruct its configuration and fingerprint
  inputs. Remove duplicate scope/truncation storage when metadata already owns
  those values. Preserve selection order and protected manifest inputs.
- #837: remove the six unused public workspace forwarding functions after
  checking repository consumers and the internal-Rust-API compatibility policy.
  Existing handler methods remain the canonical entry points.
- #836: replace each cleanup-attempted/cleaned boolean pair with an explicit
  three-state cleanup progress value. Preserve every assignment/observation
  point, errors, deadline, ownership transfer and DiskLifecycle transition.
  Do not merge execution and delivery lifetimes or add a second scheduler.

## Alternatives and boundaries

Keeping duplicate prose assertions preserves maintenance cost without proving
behavior. A general shell parser or documentation ledger costs more than the
examples require. Unrestricted setters would weaken the verified-plan promise;
an immutable view and a narrow metrics operation match existing callers.
Deprecation wrappers add another maintenance period without a promised stable
Rust API; removal follows `docs/compatibility.md` and needs an explicit Rust
source migration note. A large finalization rewrite carries more safety risk
than removing the representable cleaned-without-attempt state.

The version floor stays 0.4.0. There are no new dependencies, schemas, public
CLI options, default operators, package layouts or platform requirements.
README stays unchanged. Tests follow AAA without conditional branches in new
test bodies. Existing meaningful behavior tests take priority over new mirror
tests. Formal models are unchanged: cleanup ordering and their guarantees do
not change; host checks are not represented as new formal proofs.

## Review and resources

Each issue receives at least five distinct design, plan, implementation and
testing self-review checks, recorded with evidence in maintenance records.
One fresh independent reviewer checks the complete stack at the end.
Use Python 3.14, two Cargo build jobs and two Rust test threads. Reuse one
task-owned target across the stack, run the full workspace with
`--no-fail-fast` at the final head, and clean only its verified absolute path.
