# Issue #988 — bounded plan schema routing

Date: 2026-10-19 (Asia/Tokyo). Base: `ac554aa9fe4d3907ed4a42f6c89486e6f599c897`.

## Requirements and design reviews

1. **Acceptance boundary:** Both review and verify call `decode_manifest`.
   Retain their original bytes for strict `PlanManifest` decoding; routing
   evidence must never become the accepted document. This preserves duplicate
   known-field, unknown-field, nested-shape and trailing-content rejection.
2. **Version policy:** Valid obsolete JSON needs regeneration guidance before
   old field shapes obscure its version. No old decoder, adapter or schema bump
   is needed. Unsupported routes check JSON structure with `IgnoredAny` and `end()`
   before returning guidance, so malformed/trailing input remains a JSON error.
3. **Cost boundary:** A map visitor can stop at the first top-level unsigned
   version; schema-last must skip preceding values with `IgnoredAny`. Neither
   path owns candidate structures. Ordinary schema-first output avoids body
   prescanning; schema-last still costs a prefix scan. This makes no RSS or I/O
   deadline claim.
4. **Mixed-invalid priority:** The frozen ruling permits removing the DOM's
   last-duplicate-wins routing. First unsupported evidence routes to syntax
   validation and regeneration; first current evidence routes to strict typed
   duplicate rejection. Missing/wrong-type evidence falls back to typed errors.
   Every such document is rejected, regardless of the error selected.
5. **Module ownership:** A private `plan/schema.rs` owns routing and decoding
   behind the existing one-argument interface. It does not share report parsing
   or export probe policy. Task #985's admitted descriptor and read-only review
   byte limit stay at their existing I/O boundaries. Initial design score 8.75/10:
   the existing implementation hides decisions but its interface comment omits
   the distinction between routing and final validation; document that promise
   as part of the replacement. No other scoped diagnostic fails.

## Plan reviews

1. **Test before implementation:** Add independent generated decode cases first,
   including the selected unsupported-first duplicate priority. Run them against
   the existing DOM decoder; retain its expected behavioral RED and the passing
   unchanged-behavior cases. Avoid a compile-error-only RED.
2. **Finite matrix:** Each generated test gets one literal input and one error
   expectation. Valid fixtures derive only input bytes from the existing typed
   fixture; expected decoded fields are literal. Cover first/last/escaped/nested
   keys, duplicate schema and other fields, missing/wrong type, obsolete/future,
   malformed prefix/body and trailing content independently.
3. **Varying domain:** Use bounded properties for candidate-body sizes and
   field positions; assert schema routing separately from strict full-read
   validation. A counted reader records consumed bytes, without adding test
   counters to production. A malformed body after the header must still fail
   final decoding.
4. **Allocation sensitivity:** Include the actual private schema source in a
   dedicated allocator integration binary, following existing `ranking_heap`
   practice. Generate valid typed plan bytes before measurement, compare decoder
   peak against direct typed decoding, and retain an actual DOM control which
   exceeds the same ceiling. Serialize allocator observations with a mutex.
5. **Verification and integration:** Sole Cargo slot uses the owned target,
   jobs=2, debug=0, incremental=false, offline and owned TMPDIR. Run plan library,
   public plan integration and the allocator binary, then formatting and normal
   serialized commit hooks. Root owns final workspace checks and shared English
   contracts/OKF updates; preserve its five dirty files and the `.venv` symlink.

## Implementation reviews

1. **Ownership:** Inspected the complete production diff: only the private
   schema module and its import replace the DOM decoder. The probe owns one
   `Option<u64>`; a derived field identifier handles plain and escaped keys,
   and preceding values use `IgnoredAny`. No candidate collection or generic
   JSON value is retained by production routing.
2. **Error semantics:** Current evidence always reaches original-byte typed
   decoding, including duplicate schema/kind/nested `files` fields and unknown
   fields. Unsupported evidence checks the whole JSON and its end before
   guidance. Missing, wrong-type and malformed-prefix evidence falls back to
   strict decoding; an earlier unknown-field error can precede malformed JSON.
   The initial malformed-prefix fixture used an unknown `before` field, so its
   expected syntax error was wrong. Changed that fixture to the known
   `candidates` field; decoder validation was not weakened.
3. **Regression scope:** Read both caller paths and compared `read_manifest`
   with the task base. Descriptor admission, requested/canonical protection,
   read-only byte cap, source indexing, candidate semantics, schema number and
   compatibility floor are untouched. The public verify tests exercise escaped
   schema-last acceptance and strict duplicate/body/trailing rejection.
4. **Test independence:** Generated named tests replace multi-scenario loops.
   Literal version/diagnostic expectations are separate from fixture generation;
   bounded properties vary payload, position and escaping. Reviewed fixture
   validity and corrected its initial zero-based sequence and absent ranking
   reason: it now uses sequences 1..=N and the literal behavioral score 80.
   Library fixture setup additionally validates the header before serialization.
   This correction justifies rerunning only its consumers and resource control.
5. **Complexity:** The existing one-argument decode interface hides the routing
   decision and syntax-only unsupported path. No report abstraction, configurable
   parser, adapter, production test counters or new dependency was introduced.
   The interface comment now states routing versus acceptance. Final scoped
   software-design score 10/10: all eight diagnostics pass; the initial missing
   interface promise is resolved. Test fixture sharing avoids duplicating
   candidate construction across the library and allocator binary.

## Executed evidence

All Cargo commands use `TMPDIR=/tmp/hoimin-issues-984-989/tmp`,
`CARGO_TARGET_DIR=/tmp/hoimin-issues-984-989/target`, `CARGO_BUILD_JOBS=2`,
`CARGO_PROFILE_DEV_DEBUG=0`, `CARGO_PROFILE_TEST_DEBUG=0`,
`CARGO_INCREMENTAL=false`, `CARGO_NET_OFFLINE=true`. Cargo processes are
serialized; `.venv` was read only. Logs below live in
`/tmp/hoimin-issues-984-989`.

| Command or stage | Observed result | Log |
| --- | --- | --- |
| Old production: `cargo test --locked -p hoimin-cli --lib plan::schema_tests -- --test-threads=2` | Exit 101; 11 passed, 5 failed on selected duplicate/type diagnostic changes | `task-4-red.log` |
| First `cargo test --locked -p hoimin-cli --lib plan:: -- --test-threads=2` | Exit 101; 78 passed, 5 failed, 3 ignored; four fixture index failures plus unknown-field precedence | `task-4-green-library.log` |
| Same command after fixture corrections | Exit 0; 83 passed, 3 ignored | `task-4-green-library-corrected.log` |
| Initial `cargo test --locked -p hoimin-cli --test plan_schema_heap -- --test-threads=2 --nocapture` | Exit 0; 5 passed | `task-4-green-heap.log` |
| Temporary DOM parse/drop restored, exact small schema-first allocator case | Exit 101; 1 expected resource failure: 244,632 B direct versus 1,547,957 B routed | `task-4-old-dom-sensitivity.log` |
| Restored production: `cargo test --locked -p hoimin-cli --test plan --test plan_schema_heap -- --test-threads=2 --nocapture` | Exit 0; plan 113 passed/1 ignored; allocator 5 passed | `task-4-green-cli-heap.log` |
| Header-valid fixture: `cargo test --locked -p hoimin-cli --lib plan::schema_tests -- --test-threads=2` | Exit 0; 25 passed | `task-4-valid-fixture-library.log` |
| Header-valid fixture, temporary DOM restored, exact small schema-first allocator case | Exit 101; 1 expected resource failure: 261,016 B direct versus 1,946,805 B routed | `task-4-old-dom-valid-fixture-sensitivity.log` |
| Header-valid fixture, restored production: `cargo test --locked -p hoimin-cli --test plan_schema_heap -- --test-threads=2 --nocapture` | Exit 0; 5 passed | `task-4-valid-fixture-heap.log` |
| Final scalar-priority cases: `cargo test --locked -p hoimin-cli --lib plan::schema_tests -- --test-threads=2` | Exit 0; 29 passed | `task-4-final-scalar-cases.log` |

The first valid-position fixtures mistakenly treated the eight-field manifest as
nine fields. Corrected last position to 7 and generated positions to 0..8; removed
only the local proptest seed file generated by that fixture panic. The temporary
DOM controls restore the former allocation behavior (parse a complete `Value`,
then drop it before decoding); they are not present in final production.

Final header-valid allocator observations use 512 candidates. For 32-byte
payloads, input is 244,769 B and direct/routed peak additional live requested
allocation is 261,016 B. For 4,096-byte payloads, input is 4,408,381 B and
direct/routed peak is 4,422,454 B. Both first and last schema positions produce
these same peaks. The separate DOM control is 1,946,805 B for small payloads;
temporarily restoring its parse/drop step fails the production decoder gate
against the same direct-plus-128-KiB ceiling. Source bytes and fixture
construction are outside each allocation window. These are requested allocator
bytes, not RSS, allocator overhead, wall-time or whole-CLI measurements.

## Verification reviews

1. **Actual execution:** Read command exit codes and final summaries. The old
   decoder failed five of the first sixteen cases on intended diagnostic
   differences, not compilation. The first implementation run's fixture failures
   and their corrections are recorded separately. Plan neighbors passed 83
   library tests and 113 integration tests; the later fixture-only change passed
   its 25 library consumers and all five allocator tests.
2. **Acceptance coverage:** Separate generated cases cover first/last/escaped
   keys, current/obsolete/future versions, schema/kind/nested duplicates, missing
   and wrong-type evidence, malformed prefix/body, unknown fields and trailing
   content. Public verification additionally checks successful selected IDs
   after escaped/reordered schema and rejects the raw invalid documents. Existing
   review-reader neighbors ran in the full plan library selection.
3. **Resource sensitivity:** Counted-reader properties execute 24 bounded cases
   each. Schema-first consumes fewer than 32 bytes while payload varies through
   65,536 bytes; schema-last scans all but at most the closing byte. Strict
   `PlanManifest` reader decoding consumes the complete input across positions,
   escaped keys and payload sizes, while the actual decoder's trailing-content
   case guards its final full validation. Allocator gates exercise the actual
   private decoder source. The reinstated old DOM step fails that same gate;
   the restored implementation passes. This is an executed targeted resource
   mutation, not a claim of a broad mutation campaign or an RSS bound.
4. **Platform and scope:** All commands ran on this macOS worktree; no Linux or
   Windows execution is claimed. Three existing plan-library and one public-plan
   ignored cases remain unexecuted. Root owns final workspace, contracts,
   release/platform and OKF verification, avoiding redundant broad suite runs.
   Missing/wrong-type input may now report typed errors; unsupported-first
   duplicate input reports regeneration after syntax validation. No malformed,
   duplicate or unknown-field document becomes accepted.
5. **Artifacts and staging:** Reviewed the final diff and whitespace check.
   Only seven task files are staged: private decoder, its import, library tests,
   public plan tests, allocator binary, shared test fixture and this record.
   Six coordinator-owned dirty documentation files (including the implementation
   plan added to its ownership notice) are excluded. Temporary DOM edits are
   gone, `.venv` is preserved, and no dependencies were installed. Normal commit
   hooks use the owned Cargo environment plus `PREK_NO_CONCURRENCY=1` and
   `PREK_HOME=/tmp/hoimin-issues-984-989/prek`; their observed outcome is recorded
   in the coordinator report after execution.

## Shared contract/OKF proposal

Root owns shared English contracts and OKF maintenance. Suggested contract text:

> Review and verify route saved plans with a schema-only probe. The normal
> schema-first encoding stops before candidate bodies; schema-last input skips
> preceding values without building a generic JSON DOM. Current plans are still
> strictly decoded from the original bytes, preserving duplicate known-field,
> unknown-field and trailing-content rejection. Unsupported versions receive a
> JSON structure and end-of-input check before regeneration guidance; no old-schema decoder or
> migration is provided. Schema position and mixed-invalid diagnostics may affect
> which rejection is reported. This removes the extra DOM allocation, without
> imposing a general RSS, source/AST-memory or I/O-time bound.

Cite `crates/hoimin-cli/src/plan/schema.rs`, `plan/schema_tests.rs`,
`tests/plan.rs`, `tests/plan_schema_heap.rs` and this dated record. No schema or
version-floor increase is needed for #988; later tasks may change these for
their own independently documented reasons.


## Commit-hook and scalar-diagnostic review

The first sandboxed commit could not create the worktree Git `index.lock`; the
authorized retry used normal escalation. Its formatting and vendor Clippy hooks
passed, but workspace Clippy found unnecessary raw-string hashes in the empty
object fixture. Replaced it with the identical `b"{}"` bytes. No check was skipped.

Reviewing pinned Serde implementation exposed a limit in the phrase "complete
syntax check": `IgnoredAny` checks JSON structure, escape spelling and end of
input while skipping scalar UTF-8, surrogate and numeric-range semantics. Added
four independent cases: an invalid escape is a JSON error, whereas non-UTF-8
string bytes, an unpaired surrogate and an out-of-range number under an already
unsupported schema still receive version rejection. These inputs remain
rejected; current plans always undergo strict typed scalar validation. Updated
the interface/shared-contract wording to the actual structural check, rather
than introducing an obsolete-shape decoder or owning candidate strings merely
to choose another diagnostic for invalid unsupported input. This follows the
frozen mixed-invalid priority ruling and does not relax acceptance.
