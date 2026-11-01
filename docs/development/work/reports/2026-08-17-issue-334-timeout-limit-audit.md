# Lean timeout-limit audit report

## Result

The audit confirms the issue's implementation mismatch: configuration
validation previously accepted direct timeout values that the CLI later used
in unchecked `Instant + Duration` expressions. It also makes explicit a second
edge of the same bug family: an individually acceptable baseline can derive an
oversized automatic mutant timeout.

The repaired validator establishes one invariant for raw CLI configuration and
deserialized normalized plans. Every accepted effective deadline is nonzero
and no greater than `MAX_TIMEOUT`, including `max(5s, 2 × baseline + 1s)` in
auto mode. Internal shutdown grace is derived with checked addition and falls
back to the original deadline at the `Instant` representation boundary. All 15
strict Lean-generated cases match both public Rust configuration paths.

Lean proves properties of an independent natural-number model. It does not
prove Rust's `Duration`, `Instant`, serde, or implementation. The generated
corpus and Rust adapter exercise that correspondence.

## Claim and boundary

The audited claim is:

> An accepted run configuration has analyzer, baseline, effective mutant, and
> total timeouts in the inclusive interval `(0, MAX_TIMEOUT]`. In auto mode,
> validation bounds the derived mutant timeout rather than only its baseline
> input. A rejection identifies the user-controlled flag that violates the
> invariant.

The maximum is 100 × 365 days. The exact value is a production policy based on
Rust's documented cross-platform guidance that durations around one hundred
years can be used comfortably with `Instant`. That platform claim is an
external assumption, not a Lean theorem.

Excluded from the Lean numeric model are actual timer resolution, absolute
`Instant` representation, execution for the entire accepted duration,
unrelated internal timeouts, system clock behavior, and resource cleanup after
an already-started run. The total-timeout finalization grace is instead covered
by a Rust checked-add boundary test and a successful maximum-timeout CLI run.

## Correspondence worksheet

| Premise or observation | Lean representation | Public Rust configuration | Evidence | Mode | Result |
| --- | --- | --- | --- | --- | --- |
| analyzer timeout | `Input.analyzer : Nat` nanoseconds | `RawRunLimits` and normalized `RunLimits` | `RunConfig::try_from`, `PlanConfig::validate` | strict | match |
| baseline timeout | `Input.baseline : Nat` | same two public paths | exact `ConfigError` | strict | match |
| fixed mutant timeout | `MutantTimeout.fixed` | raw `Some(Duration)` / normalized `Fixed` | validation and effective value | strict | match |
| auto mutant timeout | `max(5s, 2b + 1s)` | raw `None` / normalized `Auto` | `auto_mutant_timeout` plus validation | strict | match |
| total timeout | `Input.total : Nat` | same two public paths | exact `ConfigError` | strict | match |
| inclusive maximum | `maximum = 3153600000000000000` ns | `MAX_TIMEOUT` | adapter schema check | strict | match |
| invalid flag | `invalidField : Option String` | `ConfigError::InvalidLimit` | exact enum comparison | strict | match |

The adapter validates schema version, strict mode, unique case IDs, mutant-mode
shape, maximum identity, expected acceptance consistency, and allowed error
fields before comparison. It does not recompute expected validation in Rust.

## Lean evidence

Kernel-checked theorems establish:

- accepted analyzer, baseline, and total values are positive and bounded;
- the accepted effective mutant timeout is positive and bounded for both fixed
  and auto modes;
- the combined accepted configuration bounds all four effective deadlines.

The executable contains 15 named boundary cases: defaults; zero, exact
maximum, and maximum-plus-one for each direct timeout; and both sides of the
largest baseline that derives an accepted auto timeout.

## Refutation sensitivity

| Risk family | Deliberately broken behavior | Witness | Detected |
| --- | --- | --- | --- |
| zero boundary | allow zero | analyzer/fixed/total zero rows | yes |
| inclusive maximum | reject maximum or accept maximum-plus-one | paired direct rows | yes |
| derived value | validate baseline input but omit auto result | `maximumAutoBaseline + 1ns` | yes |
| attribution | report the wrong user flag | analyzer, auto baseline, fixed mutant, total rows | yes |

Atomicity and idempotency do not apply: validation is a pure observation with
no durable state transition. Error precedence is represented by the ordered
`invalidField` function, while the fixed cases vary one invalid dimension at a
time to keep each expected flag unambiguous.

## Counterexample record

```text
claim:
  every accepted effective timeout can be used inside the supported deadline range
minimal direct witness:
  total_timeout = MAX_TIMEOUT + 1ns
old behavior:
  configuration accepted; later unchecked Instant addition may panic
new behavior:
  ConfigError::InvalidLimit("total_timeout")
derived witness:
  baseline = 1,576,799,999.500000001s, mutant timeout = auto
intermediate value:
  2 × baseline + 1s = MAX_TIMEOUT + 2ns
old behavior:
  baseline accepted because Duration arithmetic does not overflow
new behavior:
  ConfigError::InvalidLimit("baseline_timeout")
classification:
  confirmed bug family, repaired
```

## Verification record

| Command | Result |
| --- | --- |
| `lake build HoiminOracle.TimeoutLimitProofs generate_timeout_limit` | pass |
| `generate_timeout_limit -- --check corpus/timeout-limit.jsonl` | pass/fresh |
| `generate_timeout_limit -- --stats` | 15 fixed / 15 strict |
| `generate_timeout_limit -- --sensitivity` | all four families detected |
| `generate_timeout_limit -- --cases` | 15/15 true |
| `cargo test -p hoimin-core --test lean_timeout_limit_oracle` | 2/2 pass |
| focused raw/normalized boundary tests | pass |
| focused CLI total/fixed-mutant regression | pass |
| maximum total-timeout successful CLI run | pass |
| forced `Instant`-edge shutdown-grace fallback | pass |
| `lake build` | pass, 81 targets |
| focused `cargo mutants` | 6/6 caught; 0 missed/timeout/unviable |
| `cargo test --workspace --all-features --quiet` | pass |
| `cargo clippy --workspace --all-targets --all-features -- -D warnings` | pass |
| `cargo +1.88 check --workspace --all-targets --all-features --locked` | pass |
| Python unittest discovery | 134 run, 4 skipped, no failures |
| release wheel build and `tests/wheel_smoke.py` | pass |
| independent review after correction | no findings; ready to merge |

The first mutation baseline exposed that the pre-existing top-budget saturation
fixture constructed a fixed `Duration::MAX` through public configuration. The
new limit correctly made that premise unreachable. Its strict case now uses 10
billion waves at `MAX_TIMEOUT`, preserving the same `Duration::MAX` capacity
saturation observation while remaining configurable on 64-bit targets. The
updated top-budget corpus and Rust adapter pass before the final mutation run.
