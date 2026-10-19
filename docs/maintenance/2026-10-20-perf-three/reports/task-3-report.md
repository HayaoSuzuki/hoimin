# Task 3 report
Base e5e33603a4ed6a269d4131fbb6dbe99a409de05c. Original release baseline 34e3349099ce7fd61838629ee8c0d0b814e65a5d remains immutable.

## Design reviews (five distinct passes)
1. **Ownership:** inspected private immutable raw borrow and decoded owner. A private OnceLock derives only from these fields. No independent public index argument; no source-owner migration. Task1 CandidateSelection remains unchanged.
2. **Error order:** constructor raw length → decode → decoded length → raw hash retained. Validator schema/path/hash/mutation/span/original/decode/boundaries/replacement/location/ID unchanged; only final location materializes. Cache Result using existing fallible factory, clone error, no new panic.
3. **Encoding:** Unicode index consumes decoded offsets; Latin1 line starts consume raw bytes. Preserve expansion mapping and BOM scalar exclusion. Functional matrix tests raw span, column, hash, IDs and actual worker bytes separately from bounded independent scalar oracle.
4. **Cancellation/budget:** both CLI callers drop analyzer-owned LineIndex before conversion. Existing cancellation checks precede conversion. Metadata/scratch reservations remain before validation; no new payload charges. Index scan remains synchronous (no stronger cancellation claim).
5. **Cost/evidence:** successful UTF8 still constructs twice sequentially; lazy access adds fixed overhead. Need context heap RED, actual CLI discovery slope RED, private real build boundary counter, full CLI heap/time and tiny/realistic ASCII matched samples. No claim total CLI halves or universal timing improvement.

## Implementation reviews (five distinct passes)
1. **Source coupling:** reviewed all field construction and lazy initializer sites. The closure takes only the context raw borrow and its retained successful decoded facts. No setter, public separate-source API or ownership migration. Production integration asserts Send+Sync.
2. **Error precedence:** compared validator diff line by line: only index access changed after replacement encoding. Decoding errors remain stored; raw/decoded checks precede hash and index deferral. Added independent early-error cases with deliberately bad later metadata. Cached fallible result is propagated by cloning errors; no new panic/expect.
3. **Boundary/encoding:** the shared length helper accepts u32::MAX and rejects a Latin1-sized decoded expansion beyond it without >4GiB allocation. Latin1 raw line-start branch remains separate; property oracle is a scalar walk, independent of index helpers. BOM and CRLF are not normalized.
4. **Call graph/resources:** both CLI paths construct validation before analysis and convert after owned analyzer results return. No metadata/scratch admission moved; no payload charges added. Task1 selection untouched, Task2 manifest untouched. Removed only ignored source argument from the private LineIndex lookup and its four sites.
5. **Instrumentation and measurement audit:** private cfg(test) counter is at actual CandidateSourceIndex constructor boundary. Cross-crate constructor/drop logs use a temporary documented patch, not exported test API. Found first probe revision counted a digest allocation in context peak: corrected argument formatting to occur after snapshot and added distinct index-build timing, then rebuilt both sides and reran the paired dataset. Earlier experimental measurement is superseded, not final evidence.

## Final paired measurement (ordinary product source, allocator probe)

Identical probe and inputs linked against frozen original release and candidate release. 3 paired samples, alternating order, serial, separate warmups. Each child bounded to 25s. Peak/live = requested heap delta, not RSS. Candidate includes Tasks1/2; especially run timings cannot isolate #1008 from #1007.

| Workload | Mode | Before peak bytes | After peak bytes | Before ms | After ms |
|---|---|---:|---:|---:|---:|
|262144|context|2097232|64|1.604250|0.815584|
|262144|index-build|2097168|2097168|0.582916|0.598250|
|262144|lookup|0|0|7.267583|6.799834|
|262144|pipeline|4196731|2099563|2.226833|2.632417|
|262144|discover|4722347|2625179|2.573208|2.254292|
|262144|plan|4754097|2656929|3.215916|4.129709|
|262144|run|4895100|3244793|59.616084|55.306291|
|524288|context|4194384|64|2.805541|1.506833|
|524288|index-build|4194320|4194320|1.161042|1.185333|
|524288|lookup|0|0|7.548750|7.581000|
|524288|pipeline|8391035|4196715|4.157291|4.122084|
|524288|discover|9440939|5246619|4.519292|4.689791|
|524288|plan|9472689|5278369|5.904750|5.768625|
|524288|run|9613756|6390585|70.164000|67.332334|
|1048576|context|8388688|64|5.452459|3.095917|
|1048576|index-build|8388624|8388624|2.561208|2.474834|
|1048576|lookup|0|0|10.363459|10.386416|
|1048576|pipeline|16779643|8391019|8.337459|8.514250|
|1048576|discover|18878125|10489501|8.663458|8.437292|
|1048576|plan|18909875|10521251|10.710542|10.335542|
|1048576|run|19050945|12682046|78.639625|80.397000|
|tiny|context|80|64|0.016416|0.016125|
|tiny|index-build|16|16|0.008208|0.007000|
|tiny|lookup|0|0|0.221416|0.209916|
|tiny|pipeline|2411|2395|0.196542|0.172833|
|tiny|discover|3733|3717|0.377292|0.421416|
|tiny|plan|165860|165860|1.008459|1.129833|
|tiny|run|301976|229615|54.644042|50.346125|
|small|context|576|64|0.025625|0.013958|
|small|index-build|512|512|0.011333|0.014917|
|small|lookup|0|0|0.582875|0.607500|
|small|pipeline|67510|66998|0.254916|0.284625|
|small|discover|70593|70081|0.527792|0.509667|
|small|plan|165862|165862|1.408750|1.326166|
|small|run|302044|229619|58.812458|52.839875|

### Separate uninstrumented ordinary CLI process medians

| Workload | Mode | Before ms | After ms | Before max RSS bytes | After max RSS bytes |
|---|---|---:|---:|---:|---:|
|262144|plan|7.959|7.864|12943360|12812288|
|262144|run|69.125|62.059|17498112|15908864|
|524288|plan|11.280|10.871|17563648|17498112|
|524288|run|69.222|70.890|26181632|25296896|
|1048576|plan|15.819|16.665|27082752|26804224|
|1048576|run|87.653|76.414|42172416|42074112|
|tiny|plan|4.855|4.825|8224768|7946240|
|tiny|run|62.172|62.673|11223040|11091968|
|small|plan|5.244|8.726|8372224|8372224|
|small|run|65.494|56.897|10403840|10518528|

No universal no-regression or total-CLI-half-memory claim: per-mode times and RSS are separate. `lookup` is 100000 queries with index constructed before timing. `index-build` times construction alone. Whole CLI heap includes real plan/run discovery and file read, configuration, preflight, rendering and parent execution; executor is `/usr/bin/true`, jobs1/max1. Runtime initialization and caller configuration happen before heap window; this is not allocator activity from process entry. Child process heap is not measured.
Context stack bytes before/after: {'before': 160, 'after': 168}. Input files and command/probe/binary/rlib hashes retained in raw JSON. Stable candidate/source/lookup digests match all paired samples; full CLI stdout SHA includes nonstable timings and is not required to match.

## Private construction/live evidence
Temporary `instrument.py` logged at real PythonSourceIndex construction and Drop boundaries, and CandidateSourceIndex materialization, with global atomics across CLI worker threads. Exact patches and source hashes are `instrument-eager.patch/.json` and `instrument-lazy.patch/.json`; rlib/probe hashes in `build-instrument-*.json`; full event traces in `counts-*.json`. Eager variant is task3-base candidate.rs at e5e33603, with Tasks1/2 and the private lookup argument removal elsewhere: it is explicitly **not pristine original**. Ordinary baseline executable/probes were never overwritten. Instrumentation was restored in a finally block; byte-for-byte comparison with the saved production candidate.rs succeeds.

Every N262144/524288/1048576, tiny, and realistic small workload, three observations per variant: context builds/max-live1/1→0/0; pipeline/discover/plan2/2→2/1; actual run3/2→3/1 (worker validation adds the third build). All builds have matching real drops. These observations count actual boundaries; they are not assumed callgraph phase counts. Count-instrumented timings are not ordinary timing evidence. No public test-hook API was added.

## Small-ASCII qualification
The initial3-pair set remains unchanged. It showed small pipeline0.255→0.285ms and ordinary small plan5.244→8.726ms, so a single prespecified bounded confirmation batch used existing unchanged binaries/fixtures:12 alternating pairs for tiny/small × pipeline/ordinary-plan (96 invocations, each≤25s), then stopped. No favorable-sample replacement or repeated batches.

Confirmation medians: small pipeline0.298958→0.299271ms; small ordinary plan5.510→5.850ms. Small plan paired after−before difference median+0.236521ms, mean+0.245913ms, positive8/12, negative4/12, range−0.634333…+1.049833ms. Small pipeline signs6/6, paired median+0.003125ms. Tiny plan signs6/6, paired median+0.032687ms; aggregate medians5.410→5.553ms. Context stack size160→168 bytes. These results do **not** establish whole-CLI no-regression; residual process-level overhead remains a qualified acceptance concern for review. API pipeline evidence isolates the affected flow more closely but does not negate whole-CLI observations or Task1/2 confounding. Full per-pair values are retained.

## TDD and preservation commands/results
All commands use the brief's bounded environment via `/tmp/hoimin-perf-three-task3/run.py`; adjacent `*.command.json` records exact argv, cwd, timeout and environment. No concurrent builds or measurements.

- **Initial RED (before product behavior edit):** `cargo test --locked -p hoimin-core --lib candidate::lazy_tests -- --nocapture`: 2 failed as intended. Real CandidateSourceIndex materialization counter reports unused context1 vs0; validating after context construction reports0 vs1. Only the private cfg(test) counter was present before this RED.
- **Initial CLI RED (before product behavior edit):** `cargo test --locked -p hoimin-cli --test source_index_heap -- --nocapture`: failed on actual discovery peak4722483>3407872 atN262144. Full recorded N262144/524288/1048576 context peaks2097232/4194384/8388688 and discovery peaks4722483/9440528/18877712. Deliberate extra-index eager controls6819104/13634848/27266336. Exact candidate count1, raw offset and column checks passed before heap assertion.
- **GREEN:** same CLI heap command plus `--test source_encoding_heap`:2 passed. Context64B; discovery2625315/5246208/10489088. Extra-index controls4721936/9440528/18877712 still violate12N+262144 bound. Latin1 test now performs real location validation inside window and stays within decode peak+1024 for all12 size/density cases.
- **Supplementary context-allocation RED/GREEN:** after initial implementation, `context_gate.py before` against frozen-original public API probe failed `<1024` with2097232B; `context_gate.py after` passed64B. This is explicitly a later baseline sensitivity check, not misdated as a pre-edit run.
- `cargo test --locked -p hoimin-core --lib candidate::`:21 passed (13 new lazy/error/boundary cases +8 existing). Final amended private assertion rerun:13 passed. An initial combined command also selected integration binaries but the candidate:: filter selected0 of their cases; that is not integration evidence.
- `cargo test --locked -p hoimin-core --test candidate_policy --test source_encoding --test source_encoding_properties`:25+8+2 passed. Includes production Send+Sync and96-case bounded independent scalar-walk property.
- `cargo test --locked -p hoimin-cli --test source_index_encoding_flow --test source_encoding`: initial12+6 passed. Final added malformed-encoding+invalid-syntax case and restored-product focused run:13 encoding-flow +2 heap passed.
- `cargo test --locked -p hoimin-cli --lib analyzer::`:39 passed,10 pre-existing manual release probes ignored. Includes395-byte success/394-byte failure, metadata admission, Latin1 conversion scratch, detached worker budget, cancellation, depth and timeout cases.
- `cargo test --locked -p hoimin-cli --lib workspace::mutation::`:12 passed,1 manual benchmark ignored. Strict hash/location/ID failures preserve worker bytes.
- `cargo test --locked -p hoimin-cli --test plan --test review_cli`:149+23 passed,1 subprocess-fixture test ignored. Includes stale hash/symbol/lone-CR/BOM coordinates, tampered candidate/manifest and review source checks.
- `python3 tools/performance_shapes.py check`:validated29 shapes. New deterministic Unicode peak gate registered; full performance gate execution remains controller Task4.

304 distinct focused test cases passed across these suites;12 explicitly ignored manual/fixture cases. Workspace/contracts/Python/full performance integration sweeps remain controller-owned and were not run here.

## Validation reviews (five distinct passes)
1. **Efficiency sensitivity:** original constructor-count and actual CLI allocation failures observed before the lazy behavior edit. Measured N/2N/4N fixed-one-candidate slope shrinks by8 bytes per Unicode scalar; eager-retention proxy fails the same ceiling. Later frozen-baseline context-allocation check independently shows exact bound violation. No ordinary test asserts elapsed time.
2. **Coordinate/writeback semantics:** reviewed independent scalar oracle (not PythonSourceIndex/helper-derived expectations), UTF8 interior-byte rejection, literal one-based line/zero-based column expectations, four encodings×three line endings, raw source hashes, plan/run/verify ID equivalence, actual worker-byte mutation and original workspace preservation. Existing published stable-ID vectors protect shared identity logic.
3. **Strict ordering/resources:** early candidate errors avoid actual materialization; invalid source preserves original-mismatch priority; CLI malformed encoding beats invalid syntax. External plan/review and worker strict-validation suites pass. Payload exact limits remain395/394 and exclusions unchanged; cancellation/timeout tests pass. Large-size boundary helper covers Latin1 decoded expansion without allocating>4GiB.
4. **Measurement provenance/limits:** all final paired inputs and binaries/probe/rlib hashes recorded; constructor logging source patches separately identified, original release immutable, counts not presented as ordinary timings. Heap/requested live, process RSS, elapsed, logical candidate budget and disk bytes explicitly distinguished. Successful paths retain2 sequential builds; run has3 including writeback. Small-control residual timing increases and context+8B stack remain qualified review concerns, not dismissed by overlapping ranges.
5. **Scope/final artifact hygiene:** compared restored candidate.rs byte-for-byte with the saved ordinary product source; no INDEX_EVENT/OBS_INDEX instrumentation survives in production. Task1 CandidateSelection and Task2 Arc manifest code untouched. Public constructors/validators and version0.14.0 unchanged. Added only private OnceLock, checked-length reuse, private unused-source-argument removal, focused tests and task evidence/registry gate. Normal commit hooks remain enabled; hook result and final commit recorded below after execution.

## Source and artifact restoration
`cmp crates/hoimin-core/src/candidate.rs /tmp/hoimin-perf-three-task3/candidate-production.rs` passes. The ordinary release was rebuilt after instrumentation removal; SHA256 matches the preserved measured candidate binary exactly:644f8e41c502b6bc6ddebb45cf9a00e1d85925c95585b8ddee0e281cc284274a. Frozen original release and rlibs unchanged. `verification.json` archives final source hashes and the exact product patch from task3 base. Raw artifacts remain `/tmp/hoimin-perf-three-task3`; reviewed evidence is tracked under `docs/maintenance/2026-10-20-issue-1008/`.

## Hook correction
The first normal commit hook run passed both fmt checks and vendored clippy, but workspace clippy rejected `format!`+collect in the new test's expected hexadecimal-byte formatter (`clippy::format_collect`). Replaced only that test formatter with `write!`+fold; no product/measurement changes. Re-ran the13 encoding-flow cases before retrying normal commit hooks. The original hook failure is retained in tracked `logs/commit-first.log`; no bypass or lint suppression was used. The initial unprivileged local Git index update was denied by filesystem sandbox; the authorized escalated local commit invocation was allowed.

The second hook run reached hoimin-python lib tests and found three existing private LineIndex test calls still passing the removed ignored source argument. Updated those calls to the private one-argument method and ran the existing `line_index` filtered suite. This was a test-compilation omission in the initial call-site review; CLI functional tests could not catch it because they compile hoimin-python without its cfg(test). Product sources and measured release are unchanged. Tracked `logs/commit-second.log` retains this failure.

`cargo test --locked -p hoimin-python --lib line_index`:4 passed; final total304 distinct focused cases. No product changes resulted from either hook correction.

## Final commit and hook result
Commit `5a1d6861c9f6fa662512472cb5cbdb18e606b649` — `perf(core): defer candidate validation location indexes`.
Normal commit hooks: workspace fmt PASS, vendored parser fmt PASS, workspace all-target/all-feature clippy PASS, vendored parser clippy PASS. Exact successful hook receipt is tracked at `docs/maintenance/2026-10-20-issue-1008/logs/commit-success.log`; verification.json points to that receipt. No hook disabled. Final source hashes match verification.json and working tree is clean. The product/evidence commit changed80 files,19385 insertions,22 deletions; most lines are raw per-sample/evidence receipts.

Changed product files: `crates/hoimin-core/src/candidate.rs` and private location argument plumbing in `crates/hoimin-python/src/analyzer/rust.rs`. Test files: new `candidate_lazy_tests.rs`, existing core `candidate_policy.rs`, existing Python `rust_tests.rs` private callers, CLI `source_encoding_heap.rs`, new `source_index_heap.rs` and `source_index_encoding_flow.rs`. Registry: `docs/performance/shapes.json`. Tracked evidence: `docs/maintenance/2026-10-20-issue-1008/`.

Status: **DONE_WITH_CONCERNS**. Functional/heap scope is implemented and verified. Concern for controller review: context stack+8B and residual small-plan process timing (+0.236521ms paired median,8/12 increases; first set and all confirmation samples preserved); no universal no-burden assertion. Successful UTF8 still builds two indexes sequentially, an accepted design limitation. Whole-CLI timing compares cumulative Tasks1/2/3. No push/PR/CI/full integration/cleanup performed.

Documentation-only receipt commit `952ab52780b181ae8de316e4b1f45988b5d0d1b0` — `docs: record source-index commit verification receipt` archives the successful product hook log and corrects its pointer; normal hooks reported no matching source files (Skipped). Product code/source hashes unchanged; product hooks already passed on5a1d6861. Final working tree is clean.

## Review fix round 1 — I1 / R6 (2026-10-20)

**Status: DONE_WITH_CONCERNS; I1 remains OPEN.** This evidence-only fix did not establish the prespecified small-ASCII practical non-regression bound. It neither changes the product nor relaxes the issue/spec. The controller was notified immediately after the single capped batch and instructed archival/hooks only while an explicit acceptance decision is pending. No response is assumed; no further measurement, profiling or redesign was performed.

Finding addressed: reviewer I1 correctly distinguished cumulative original→Tasks1/2/3 timing from Task3 causality and left ordinary small-ASCII acceptance unresolved. Existing three-pair and12-pair adverse observations remain unchanged. Per controller R6, the comparison now holds Tasks1/2 fixed using exact pre-Task3 production sources at `e5e33603a4ed6a269d4131fbb6dbe99a409de05c` versus fix-base candidate `952ab52780b181ae8de316e4b1f45988b5d0d1b0`.

`docs/maintenance/2026-10-20-issue-1008/r6/build_variants.py` temporarily wrote exactly the two changed production files from e5e33603, verified the production diff empty, and built `cargo build --locked --release -p hoimin-cli` and the unchanged public CLI allocator probe. A finally block restored both candidate files before rebuilding. Both variants used the same worktree/target/toolchain/profile/features/bounded env; no count instrumentation or public hook API was added. `build-variants.json` and `build-probe-*.json` retain source/binary/probe/rlib hashes and commands. Original frozen baseline/probe hashes remained unchanged; rebuilt candidate SHA256 equals prior ordinary candidate644f8e41c502b6bc6ddebb45cf9a00e1d85925c95585b8ddee0e281cc284274a. Tests/evidence from the current tree do not affect these production release builds.

Protocol was fixed before observation: exactly512 alternating base/lazy then lazy/base pairs per existing tiny/small fixture,2048 ordinary plan children; no discarded warmups/outliers, retries, optional stopping or repeated batch. Each child≤25s, entire measurement≤600s. Actual total51.578808s,2048 ordinary samples and12 heap samples; all complete. Exact fixture paths/bytes/hashes, arguments, env and executable hashes are in `r6/protocol.json`; all per-child values/digests in compact `ordinary-samples.jsonl`. Every paired difference is retained in `summary.json`. Stable candidate projections agree in every pair; heap candidate IDs agree with ordinary output.

Time criterion: for EACH fixture, one-sided95% distribution-free upper confidence bound for paired median lazy−base≤+0.05ms. Exact integer Binomial(512,0.5) calculation selects smallest rank k with CDF(k−1)≥19/20: rank275 coverage0.949041494485950 is insufficient; rank276 coverage0.957657467588908 suffices. Bound=D_(276). `r6_statistics.py` documents independent representative pair assumptions; alternating order does not establish independence. Confidence is per fixture, not simultaneous familywise95%. Six known small-n/signed/empty/insufficient-sample helper cases passed; this verifies statistics, not application behavior.

| Fixture | Paired median ms | Paired mean ms | Positive/negative/zero | Range ms | Upper bound ms | Result |
|---|---:|---:|---|---|---:|---|
| tiny12B | −0.065208 | −0.214534 |227/285/0|−44.715999…+5.337708|−0.019834|pass|
| small1771B | +0.021354 | −0.020568 |268/244/0|−34.688167…+9.396166|+0.067791|criterion not met|

Absolute base→lazy median/mean: tiny5.765083/6.426793→5.718251/6.212258ms; small5.908375/6.133913→5.966667/6.113345ms. Small's upper bound exceeds+0.05ms even though paired median is below that tolerance. The result is inconclusive for practical non-regression; it does not prove the true median cost exceeds+0.05ms, nor identify a function-level cause. No claim of whole-CLI no burden follows from an almost flat API pipeline.

Heap criterion required no increase in BOTH requested whole-plan peak and retained heap for ALL three matched observations per fixture (prespecified stricter interpretation). Every delta is0B: tiny peak165860→165860, retained27016→27016; small peak165862→165862, retained27015→27015. The unchanged allocator probe calls real public `run_with_writers` after runtime/caller setup; window includes file reads/discovery/preflight/rendering, retains output at final reading. This is not process-entry or child heap, stack, RSS, logical payload or disk. Context stack160→168B remains explicit. Ordinary process timings come from uninstrumented executables separately; no extra API matrix was run.

Commands all used existing bounded `run.py`: `r6-build-variants 500 python3 .../r6/build_variants.py` succeeded126.572s; `r6-statistics-tests 30 python3 .../r6/test_statistics.py` passed6 tests; `r6-measure 610 python3 .../r6/measure.py` exited1 deliberately because acceptance was not met, after preserving every observation. Exact argv/env/outputs are in `r6/logs/*.command.json` and `.log` (those receipts, not prose abbreviations, govern exact command reproduction). The measured internal batch stayed below600s. No broad or previously passed functional suite reran; original304 distinct focused tests remain historical evidence.

Required normal hooks: initial `r6-hooks 300 prek run --all-files` was blocked before hook execution because the filesystem sandbox denied opening existing `~/.cache/prek/prek.log`; retained receipt. Authorized elevated `r6-hooks-retry 300 prek run --all-files` passed all four workspace/vendor fmt and clippy hooks in15.136s. No hook bypass. `r6/restoration-verification.json` records matching all9 prior source hashes, all3 ordinary binary hashes, and empty product/test/Cargo/shape diff from fix base after hooks.

Evidence updates are confined to the existing measurement README/verification plus `r6/` scripts, exact fixtures, compact raw rows, statistics, build/restoration receipts and logs. The acceptance/spec is unchanged because R6 failed to discharge I1. Prior samples were not edited or deleted. No subagents, push, PR, CI, full integration or cleanup performed.

Scoped next step proposed to controller: keep I1 open; obtain an explicit practical-tolerance acceptance decision or authorize profile/code-layout diagnosis of the existing context/plan path before changing implementation or designing another experiment. Current data establish neither a specific root cause nor a justified product fix. Thresholds/source-owner scope remain frozen. Normal local commit and final status are recorded below.

R6 evidence commit `f72134d76d28d87cd728305ed224f298df41af4a` — `docs: preserve task-local source-index acceptance evidence` (27 files,4491 insertions,1 deletion). Normal commit hooks remained enabled and correctly skipped this evidence-only commit; explicit post-restoration all-files hooks had already passed all four checks. Exact commit stdout and command/env receipt remain `/tmp/hoimin-perf-three-task3/r6-commit.log` and `.command.json`. Final working tree is clean. I1 remains OPEN; no acceptance/spec decision is inferred. Sole Cargo/product ownership is released to controller at handoff.
