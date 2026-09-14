# Issue #491: Remaining acceptance review and evidence

Base: 8b33167a049e3cae0fc05e96ccf2253c660b7023. Historical 21-gate evidence is not a new run. New executions, resource bounds, case modes and limitations will be recorded here.

## OKF: three reviews

1. Read the existing performance-shapes concept and its original design/review. It explicitly leaves extra Lean cost models incomplete; this is an extension of the same contract, not a second performance infrastructure.
2. Compared current source counters to previous claims. build_updates excludes sort/allocation, query_comparisons excludes model semantics, and retained heap is not peak heap. Keep these observation boundaries in the new worksheet.
3. Checked source/index rules and concurrent ownership. Add separate spec/report sources, retain historical revisions and hashes, and update current registry/guide hashes only after their owner's final edits.

## Design and plan: three reviews each

Design passes are recorded in the operation-cost design: existing-gap reconciliation, actual-step instrumentation and numeric/semantic bounds. Plan passes are recorded in the operation-cost plan: explicit ownership, boundary/semantic controls and actual broken operations with fresh release inputs. Each pass changed the selected work or strengthened its evidence requirements before implementation.

## Model and implementation: three reviews

1. Distinguished an abstract cost identity from the search algorithm. Added an arbitrary-branch binary-search recurrence, proved its cost at most the half-length recurrence, and proved that recurrence equals floor(log2 N)+1 for positive N. BuildTrace proves the per-leaf/ancestor count separately, and bounded retention reuses the existing theorem.
2. Reviewed semantic order and numerical preconditions. Generated ordered, duplicate and nonmonotone offset histories and independent eligible-event folds. Rust checks native conversion and intermediate allocation/product representability; it reads generated bounds instead of repeating their formulas.
3. Compared inventory assertions with required boundaries. A nonempty family check could lose 0/1, exact power-of-two or N/2N/4N rows silently; required all 56 cases and complete per-family sizes. Added independent annotation and flat-literal replacement checks alongside unchanged full candidate semantics in real broken paths.

## Proof development and sensitivity

Initial proof authoring found a changed induction name and recursive equation rewriting issues. These were corrected without increasing recursion depth or heartbeat limits. A subsequent isolated proof run failed only the deliberately false `buildUpdates 8 = 8` proposition, demonstrating detection of omitted ancestor updates. Restoring the true value 32 passed the general and boundary proofs. Finite cases search 0..32 and prove minimal witnesses: scan at3, full clone at1, eager byte build at1. This is a bounded witness search, not an unbounded asymptotic proof of the entire CLI.

Dependency compilation initially peaked 1,314,704 KiB under20s/2GiB. Isolated proofs passed under20s/768MiB (peak697232KiB). Generator attempts under768MiB ended with rss_limit/exit125 at818496,788992 and825952KiB, including isolated-module attempts; they do not count as semantic failures or passing evidence. The generator therefore uses the established2GiB ceiling with the unchanged20s deadline. No unbounded tactics or enlarged search domains were used.


## Tests: three reviews

1. The clone boundary test failed before instrumentation; the omitted-ancestor Lean proof failed before correction. Actual slow scan/clone/build paths then preserved semantic outputs but failed their bound predicates, instead of merely comparing fabricated bad counters. Both default and contracts builds passed all four cost adapter tests and the independent preflight test.
2. Reviewed the measured preflight failure and permitted-memory premise. Existing hashing uses fs::read for one whole file; revised peak allowance includes the largest-file buffer plus entries, then checked that real eager all-files×workers buffers still violate it. Reviewed clone width: the source adds N aliases plus required Sequence; the abstract N×N bad-copy witness is a lower bound, while the actual zero-callback-copy predicate counts every imported entry.
3. Reconciled final commands and environment. System Python3.12 lacked packaging and could not import the wheel tests (55 tests, one import error); the repository Python3.14.7 environment with process-monitor permissions passed all82 tests. Workspace/all-target/all-feature Clippy, format and diff checks passed. New code/CI was finalized before the successful suite; in-flight earlier failures remain recorded separately.

## Final execution evidence

- Lean proofs/cases passed; final generator output took9.383s, peak1,271,728KiB under20s/2GiB. Freshness and sensitivity passed under20s/768MiB; sensitivity reports56cases,0/1/7/8/9/16/32,three effects,max32events/max34queries,search0..32 and minimal scan/clone/eager witnesses3/1/1. This is the new modules and import dependencies, not a local aggregate build of all128 repository package modules.
- Rust: four cost tests pass under both default and contracts, covering56rows and three real broken families; preflight allocator test passes under both modes. Existing annotation snapshot1 and collection literal3 tests pass. Detailed peak values and commands are in the Rust cost review.
- All26 registered active gates executed and passed at registry SHA256 `36f30fa8aa02c0270e7822263229cb7650a460c79f79347f460ff6785f92752b`; original21 remained intact. Raw gate results are `/private/tmp/issue-491-final-gates/result.json`. Subsequent registry edits added only Lean cost reference metadata; gates and29 measurement shapes did not change. Registry validation passed again.
- Full Python suite82pass; Clippy `--workspace --all-targets --all-features -- -D warnings`, `cargo fmt --all -- --check` and `git diff --check` passed. Module/generator inventory is128/31/28 sensitivity generators, confirmed from the passing workflow contract helper rather than counting unrelated root scratch modules.
- Fresh release lane:522 executions (29shapes×3sizes×3repeats×2binaries),174medians,87baseline/candidate and116within-binary growth comparisons. Committed expanded-measurement JSON contains actual environment/digests and raw-result hash/path. A selected production completeness mutation was killed with clean execution cleanup. See the input-axis report for exact selection and limits.

## PR: three reviews

1. Inspected complete Rust diff: instrumentation is test-only, the production replacement observer is an inline identity, and default KnownImports Clone remains derived. Parent independently reviewed the actual clone/builder paths, semantic comparisons, native bounds and allocator controls without further blocking findings.
2. Matched each acceptance item with a model, actual counter gate or bounded workload. Kept named operation vector, internal-fixture counter modes, public release observations, largest-file peak premise and unsupported native/OS proofs separate. No elapsed-time speedup or whole-CLI constant-memory claim is made.
3. Re-read final design/worksheet/report against generated rows,26 executed gates,522 fresh measurements and source inventories. Corrected stale package count using the workflow's actual roots, documented the metadata-only registry digest difference, and preserved historical21-gate evidence. Final source hashes and indexes are validated after contributors finish; publication uses the repository PR template.

## Remaining limits

This closes the missing validation infrastructure, not every possible asymptotic regression. Measurements are bounded macOS arm64 observations with concurrent-build noise; Linux/Windows performance and stack capacity are not established locally. CI covers the aggregate Lean build. The cost vector excludes sorting/allocation details and does not prove compiled Rust from Lean. Sources/AST memory and one-file fingerprint buffer remain input-proportional. Nested size0/1 share a valid minimal fixture. All counter modes remain internal-fixture; release timing/RSS are separate public measurements without fixed speed thresholds.


## Publication

Published [PR #544](https://github.com/tokyogas-tech/hoimin/pull/544) from enhancement/issue-491, implementation commit7213011. Delivery checklist is complete. Final OKF checks passed19 reserved/YAML pages,416 source IDs/footnotes,768 local links, root reachability, complete design/report indexes and all eight newly read performance-concept hashes. All35 nonempty generated cost sources also compiled with repository CPython3.14. This publication-state edit changes documentation only; implementation validation above remains the final code evidence.

## Hosted CI follow-up: ordinary output fixture isolation

PR544's randomized Linux run failed an unchanged paused-output unit fixture before its writer was entered (`Ok(2)`); the same head's ordinary Linux run passed that test. PR537 had shown the same symptom. The original test discarded stderr and the owned wrapper maps an inner error to exit2, so the precise original cause cannot be established from that log.

Review1 found a concrete isolation gap: output_failure_test_config used the CLI's default10GiB free-space reserve despite an existing host-independent ordinary-test helper. Adding that helper's reserve assertion to the paused test produced deterministic RED (10737418240 versus1). The shared output-failure config now uses the existing1B helper. Production limits, mutation-plan reserve and explicit disk-threshold tests are unchanged.

Review2 traced owned-writer lifetime and both tokio::select failure branches. A short-lived mutex-backed stderr capture now includes diagnostics for premature run completion or a dropped pause signal. This changes test failure visibility; stdout pause/release,50ms shutdown grace,1s post-cancel deadline, spool retention, execution cleanup and no-ack assertions remain intact.

Review3 checked all helper consumers and validation scope. All three affected ordinary output tests passed under both default and contracts builds, and all-feature lib/tests Clippy passed. Final formatting/whitespace checks and updated source hashes are checked before push. The parent did not claim the historical CI error was proven to be low disk or remove any output-lifecycle assertion.

Independent agent review found no blocker: capture lock scope and ownership are bounded, both early failure paths preserve diagnostics, ordinary output tests are isolated from host reserve, and explicit disk-stop tests retain their own settings. Hosted CI for this follow-up is tracked separately from these local results.


## Progress history axis follow-up

Design reviews: (1) The existing progress gate fixed a2,000-mutant report and measured only histories2/16, so it did not explicitly cover the requested independent report-bytes×history N/2N/4N axis. (2) Preserve the same gate and production entry point; vary report content500/1,000/2,000 and histories2/4/8, recording actual serialized bytes rather than claiming headers scale exactly. Preserve the old2,000×16 corner. (3) Keep a single allocator-test function to avoid concurrent global peak sessions, finish fixture allocation before measuring, and allow one-report memory plus compact history/output metadata.

Plan reviews: (1) Observe the first smaller fixture request fail against the existing fixed-size generator, then parameterize content and summary coherently. (2) Capture bounded JSON output and check input/comparison semantics after finishing heap tracking; success alone is insufficient. (3) Add an actual eager Vec<InputReport> read/compare control at the largest history, compare its semantics and require it to exceed the same peak allowance. Keep the original522release measurements separate; rerun only the expanded exact gate and relevant validation before parent review.


Progress implementation reviews: (1) The first requested500-mutant fixture failed against the original fixed2,000 output (actual2000 versus expected500). Parameterized both mutant entries and summary sequence/count rather than changing only the filename or metadata. (2) Fixture documents are dropped before peak tracking; captured JSON is compact legitimate output, and parsing/assertions happen after finish. Explicit comparison and usable-input checks replace sink-only success. (3) The sensitivity path retains real public read_report results and uses public compare_reports, with all comparison fields matched against streamed output; no fabricated heap counts or production changes.

A separate read-only full-acceptance review by agent476 found no further material gap beyond this progress axis. Its review does not count as executing the new test or add progress to the522release observations. Parent independently confirmed the fixed2000-versus500 RED before implementation completion.


Progress test reviews: (1) Default exact gate passed after the fixed-size fixture RED. All nine matrix points plus the original16-history corner pass the same512KiB growth allowance; three separate history2 measurements establish each report-size reference. Actual report lengths are251978/502484/1010484 bytes, not an asserted exact byte-doubling sequence. (2) The real retained-history control reaches15038287 peak bytes versus4779743 at the normal largest-report/history2 reference, exceeding its5304031-byte bound. Every comparison count, score and state matches the streamed output, and the allocation is retained through measurement finish. (3) Re-ran the same exact gate with contracts: one test passed in each mode (2.52s default,2.57s contracts). All16 performance-tool Python tests, registry check29shapes, all-feature Clippy for progress_heap, formatting and diff checks passed. No new release measurement is inferred from these allocator results.

| Mutants per report | Actual report bytes | Peak H=2 | Peak H=4 | Peak H=8 |
| --- | --- | --- | --- | --- |
| 500 | 251978 | 1191910 | 1192456 | 1193068 |
| 1000 | 502484 | 2384590 | 2385136 | 2385748 |
| 2000 | 1010484 | 4779743 | 4780289 | 4780901 |

The retained2000×16 probe peaks at4782125 bytes. These allocator requested-byte observations are identical in the two local feature modes; they are not RSS or a proof for arbitrary history/report sizes. Logs: `/private/tmp/issue491-progress-{red,green,contracts,clippy,metadata-python}.log`.

Progress OKF reviews: (1) The performance concept's opening still used11 shapes in present tense; marked that as the initial design and connected the current29 release shapes to the added deterministic history gate. (2) Registry metadata now declares both independent dimensions, actual serialized-byte observation, the retained16 corner, semantic checks and the real eager control. The26 gate names and29 release-shape entries are unchanged. (3) Re-read updated guide, registry and report, refreshed their hashes and comparison revision, and checked the complete source/link inventories rather than treating old hashes as current.

Progress PR/publication reviews: (1) Parent independently reviewed fixture drop timing, measured allocation intervals, output parsing after finish, coherent summary counts and actual retained-report sensitivity with no blocker. (2) Agent466 independently reviewed the matrix, semantics and same-bound control with no blocker; agent476's separate full-acceptance audit found no further material gap. (3) Compared final diff with the requested test-only scope: progress_heap, registry metadata, guide and review/provenance only. Existing production/runner code,26 gate identities,29 release inputs and the historical522 executions are unchanged. The parent owns the follow-up commit and hosted-CI confirmation; this records the concrete pre-publication review.


## 全 enhancement PR の統合確認

ユーザーの順次マージ指示に基づき、#536 → #540、続いて #538 → #543 → #534 → #535 → #537 → #539 → #541 → #542 → #544 の順で先行変更を取り込んだ。各統合headのCIを確認してからmainへマージする。以下は最終統合ツリーのローカル検証であり、完了前のCIを成功とは扱わない。

レビュー1では、索引・設計節・追加テストの競合を両親の内容と照合した。planテストは共通prefixと双方の追加関数を保持し、関数名の和集合と重複なしを確認した。最終のLean/CI競合13箇所では、両generatorへ個別のlean_exeヘッダーを残した。

レビュー2では、別担当が実装の自動統合を照合した。offsetとmetricsのCLI引数、prepare成功後の両verify dispatch、元バイトspanと再エンコード、負号を含む候補範囲、symbol定義確認、baseline診断と既存イベント処理、test-only操作数観測をすべて保持した。Lean登録は両親の集合の和と一致し、130 modules、32 corpora、29 sensitivity generatorsとなった。登録検証をLean証明の再実行とは扱わない。

レビュー3では、統合ツリーでRustのplan72件、baseline出力4件、負の添字2件、文字コード5件が成功した（既存ignored 1件）。最初はworktreeの.venv参照がなく4件がprocess.spawn ENOENTで失敗したため、既存root環境への一時symlinkを接続して再実行した。コード変更で回避していない。CI構成28件、boundary runner10件、performance tool16件、全workspace/all-targets/all-features Clippyも成功した。OKF20ページ844リンク、全原文の索引包含、fmtと差分を検証した。

実CLIの追加確認は初回で成功した。Latin-1のcafé関数内の負の添字について、元span85/2、反復planの同一候補、baselineと2workerの実バイトを照合した。strict/diverseのoffset1・top2は異なる期待ID順となり、metrics有無で候補が一致し、sidecarのrun_id・discovered4・executed2を確認した。元ソースと保存planは不変だった。CPython3.14.7、debug実行ファイルSHA-256は0809a065cc63f73785fdca88bf0a9c8709d91035b6975a6d781a27f3dc009db5。独立したBLAKE3の再計算は行っていない。

ローカル証跡は/private/tmp/hoimin-cumulative-rust-tests-ready.log、/private/tmp/hoimin-cumulative-clippy.log、/private/tmp/hoimin-cumulative-smoke-results/result.jsonに保存した。統合時のレビューは既存522回のrelease計測を再実行したという意味ではない。


### contractsビルドのディスク圧迫への対応

統合後のPR535/537/539/541で、contractsのCLIテスト実行前に異なる実行ファイルのlinkがBus errorで停止した。PR539は直前の空き容量34MB、PR541は77MBをrunnerが報告した。他2件の個別原因まで証明したとは扱わない。初回ログは/private/tmp/pr{535,537,539,541}-integration-contracts-failure.logと/private/tmp/contracts-integration-disk-failures.jsonに保持し、同じ失敗を繰り返す再実行は行わなかった。

レビュー1:失敗時点と複数runnerの容量を比較し、test assertionではなくbuild資源への対応として範囲を限定した。レビュー2:contractsジョブだけにCARGO_PROFILE_DEV_DEBUG=0、CARGO_PROFILE_TEST_DEBUG=0、CARGO_INCREMENTAL=0、CARGO_BUILD_JOBS=2を追加し、両cargo testコマンドとfeaturesが変更前と等しいことを構造比較で確認した。debug情報の抑制はdebug assertionの無効化ではない。別担当もこの区別とtest filterの追加がないことを確認した。レビュー3:CI構成28件が成功し、先行535から全後続PRへ通常mergeで同じ修正を適用した。CI再実行の結果を確認するまで解消済みとは扱わない。


続いて全変更を含むPR544の通常Rustジョブも、空き容量9MBでcoreテスト実行ファイルのlinkがSIGBUSになった。証跡は/private/tmp/pr544-integration-rust-failure.log。追加レビュー1でこの実測値とテスト前の失敗を確認し、2で通常Rustとランダム順Rustにも同じ4設定だけを追加して、全ジョブ定義から追加envを除くと変更前と同一になることを構造比較した。追加レビュー3ではCI構成28件が成功した。通常テスト・性能ゲート・shuffleの実行コマンド、assertion、featureは維持した。最新CIで効果を確認する。
