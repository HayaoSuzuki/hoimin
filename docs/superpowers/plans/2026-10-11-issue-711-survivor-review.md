# Survivor Review Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [x]`) syntax for tracking.

**Goal:** 保存planの候補に、人の評価と理由を正確に紐づけ、更新履歴を保存・表示できるようにする。

**Architecture:** review専用の純粋判定、入力検証、SQLite保存、表示をCLI内の独立moduleに分ける。plan/reportの既存検証を共有し、executorやランキングへreviewを渡さない。LeanモデルとRustの対応を小さなcorpusで検査する。

**Tech Stack:** Rust 2024、既存serde/serde_json/rusqlite/blake3/camino/clap、Lean 4.32.2、Python 3.14の開発ツール。

**Spec:** [承認済み設計](../specs/2026-10-11-issue-711-survivor-review-design.md)。2026-10-11にユーザー承認。設計元mainは `d46f13a43667323b8d0374d6c5409942f4c3c822`、設計commitは `fdf80af`。

## Global Constraints

- 評価による候補の抑制、ランキング変更、スコア補正、学習は行わない。
- 外部review manifestの取込み、URLの取得、プラグイン、実行コマンドの追加も対象外とする。
- storeはすべて明示指定し、自動探索しない。
- reasonは空白除去後に非空、UTF-8で4 KiB以下、evidenceは最大8個・各2 KiB以下とする。
- 改行とtab以外の制御文字を拒否し、text表示時にはterminal escapeを出力しない。
- `--expected-revision` は必須で、最初の記録には0、更新にはshowで得たrevisionを指定する。
- 初期版の対象は現在版v4のJSON reportとし、JSONL・旧版のreview連携は追加しない。
- JSON出力も専用schema v1とし、既存plan/reportの版は変更しない。
- SQLiteのロック待ちは1秒までとし、競合はbusyとして再試行可能なエラーを返す。
- plan/reportは各64 MiB、DBは16 MiB、履歴はDB全体で10,000イベントを上限とし、超過時は拒否する。
- 共有ネットワークファイルシステムでの同時更新は初期版の保証対象にしない。
- 実装が配布物に入る場合は、ユーザー指定に従って0.4.0とし、その時点のmainの版と整合させる。
- 設計、実装計画、実装、テストでそれぞれ最低5回のセルフレビューを行い、各段階の実施内容を記録する。
- Rust buildを行う際には空き容量を確認し、段階の切れ目でcargo cleanする。
- lintは原則deny。避けられない除外だけ理由をinlineに記す。Runner Pythonは理由がなければ3.14。
- #711は `feat/issue-711-survivor-review`、#759は上段 `research/issue-759-staged-verification`。mainへ直接commit/pushしない。

## Review Focus

1. sourceが消えた場合と変更された場合を区別し、どちらもreviewed集計から除く（Task 2/4）。
2. 同時初期化と、同じ候補への同時更新で履歴を落とさない（Task 3）。
3. 未完了report、別root、同じIDの偽descriptorを評価へ混ぜない（Task 2）。
4. stdout書込み失敗後に保存を再試行しても、同じexpected revisionで履歴を重複させない（Task 4）。
5. plan/report/sourceとstoreのalias、既存の無関係なDB、上限を超える入力を上書きしない（Task 2/3）。

## ファイル構成と共有型

新規 `crates/hoimin-cli/src/review/` を以下へ分ける。

| ファイル | 責務 |
| --- | --- |
| `mod.rs` | orchestration、ReviewError、公開する最小API |
| `model.rs` | 評価・identity・鮮度・履歴更新の純粋判定 |
| `input.rs` | plan/report/sourceとstore destinationの照合 |
| `store.rs` | DBのopen、schema、読取りsnapshot、transaction |
| `render.rs` | 専用JSON v1とtext、operator集計 |
| `tests.rs` | module内テスト、保存競合・rollback fixture |

既存 `cli.rs` と `lib.rs` にreviewのparse/dispatchだけを追加する。
plan読取りの共有入口は `plan.rs`、report読取りの共有入口は `progress/input.rs` に置く。
`UsableReport` の既存フィールド、progressのJSON/JSONL/旧版対応を変更しない。
CLI統合は新規 `crates/hoimin-cli/tests/review_cli.rs`、Lean対応は新規 `crates/hoimin-cli/tests/lean_survivor_review_oracle.rs` へ置く。

純粋型は `review::model` で定義する。oracleテストに必要なものだけ公開する。

- `ReviewKey { project: Utf8PathBuf, candidate_id: String, descriptor: CandidateDescriptor }`。descriptorから既存CandidateIdentityとstable IDを再検算する。rank/sequenceは含めない。
- `Assessment` は四評価のsnake_case enum。`unreviewed` はenum値にしない。
- `SourceState::{Fresh, Stale, Unavailable}`。
- `Applicability::{Current, Stale, SourceUnavailable, IdentityConflict}`。別projectは入力/store境界でエラーにもする。
- `ArtifactReference { path: PathBuf, content_hash: String, run_id: Option<String> }`。
- `ReviewDraft { assessment: Assessment, reason: String, evidence: Vec<String>, timestamp_ms: u64, plan: ArtifactReference, report: Option<ArtifactReference> }`。
- `ReviewEvent { revision: u64, draft: ReviewDraft }`、`ReviewState { history: Vec<ReviewEvent> }`。空historyのrevisionは0。
- `UpdateRejection::{NotCurrent, RevisionConflict, RevisionOverflow}`。保存・構文・IO等の失敗はReviewErrorで別に扱う。

### Task 1: 純粋な適用判定と履歴更新

**Files:** Create `review/mod.rs`, `review/model.rs`, `review/tests.rs`; Modify `crates/hoimin-cli/src/lib.rs` (module宣言のみ)。

**Interfaces:**

- `applicability(expected: &ReviewKey, stored: &ReviewKey, source: SourceState) -> Applicability`。
- `apply_update(state: &ReviewState, applicability: Applicability, expected_revision: u64, draft: ReviewDraft) -> Result<ReviewState, UpdateRejection>`。
- `validate_draft(draft: &ReviewDraft) -> Result<(), ReviewError>`。署名と上限は後続Taskでも共用する。

- [x] テスト `identity_and_update_contract` を先に書く。実在sourceから既存stable IDでfixtureを作り、project/descriptorの変更がIdentityConflict、FreshだけCurrent、古いrevisionがRevisionConflictになることをassertする。拒否前後のstate equality、成功時のprefixとrevision+1も確認する。
- [x] テスト `draft_limits` を書く。reason 4096 bytesは許可、4097 bytes・空白のみは拒否、evidence 8個/各2048 bytesは許可、9個/2049 bytesは拒否。ESC/NULは拒否、改行/tabは許可。日本語もbyte数で数える。
- [x] `cargo test -p hoimin-cli --lib review::` を実行し、未実装の判定により失敗することを確認する。
- [x] modelを実装する。revision加算にはchecked_add、履歴更新には新しいstateを返す純粋処理を使う。descriptor比較にoriginalも含める。
- [x] 同じコマンドでテスト成功を確認し、Task 1のファイルだけをcommitする。

### Task 2: 実行しないplan/report読取り

**Files:** Modify `plan.rs`, `progress/input.rs`; Create `review/input.rs`; Extend `review/tests.rs`。

**Interfaces:**

- `plan::ReadOnlyPlan { manifest: PlanManifest, canonical_path: PathBuf, content_hash: String, project: Utf8PathBuf }`。
- `plan::read_for_review(path: &Path) -> Result<ReadOnlyPlan, PlanError>` (pub(crate))。
- `progress::input::ReviewReport { project: Utf8PathBuf, run_id: String, mutants: Vec<MutantFinished> }`。
- `progress::input::read_review_json(path: &Path, bytes: &[u8]) -> Result<ReviewReport, ProgressError>` (pub(crate))。
- `review::input::load_plan(path: &Path) -> Result<ReadOnlyPlan, ReviewError>`、`load_report(path: &Path, plan: &ReadOnlyPlan) -> Result<(ReviewReport, ArtifactReference), ReviewError>`。
- `source_state(root: &Utf8Path, candidate: &MutationCandidate) -> Result<SourceState, ReviewError>`。読取り不能はUnavailable、hash変更はStale、同じhashで不正なspan/original/IDはエラー。
- `validate_store_destination(store: &Path, plan: &ReadOnlyPlan, report: Option<&Path>) -> Result<(), ReviewError>`。

- [x] `read_only_plan_does_not_execute` を書く。analyzer/test argvに失敗markerを置いても有効planを読め、markerが生成されないことをassertする。未知版、重複ID、ID再計算不一致、範囲外pathを拒否する。
- [x] `source_freshness_and_plan_reordering` を書く。source変更→Stale、削除→Unavailable、rank/sequenceを整合した別planでも同じkeyになることを確認する。
- [x] `review_report_identity` を書く。完了v4 JSONは読めるが、未完了・baseline失敗・別root・summary不整合・同じIDの別descriptor・旧版・JSONLは拒否する。list対象の欠落候補は読取り自体のエラーにしない。
- [x] `bounded_artifact_and_alias` を書く。64 MiB超を拒否、storeとplan/report/sourceの同一ファイル・hardlink、およびstore symlinkを拒否する。Windowsでsymlink作成権限がない場合だけ理由付きskipし、hardlinkは通常ケースとして確認する。
- [x] 上記を `cargo test -p hoimin-cli --lib review::` で失敗確認後、共有入口とinputを実装する。review用の上限付き単一byte snapshotからhash/parseを行う。reportは同じbytesを既存のdocument validationへ渡し、validated documentからroot/run IDを得る。既存readerの二重openを使わない。
- [x] plan header/configの構造検査は共有するが、current fingerprint一致やanalyzer発見を要求しない。source検査は候補単位で行い、listのstale表示を妨げない。root外へ解決するcandidate pathは拒否する。
- [x] `cargo test -p hoimin-cli --lib` と既存 `lean_progress_input_oracle` / `lean_progress_decision_oracle` を実行し、既存readerの互換性を確認してcommitする。

### Task 3: 独立SQLite store

**Files:** Create `review/store.rs`; Extend `review/tests.rs`。

**Interfaces:**

- `ReviewStore::open_read(path: &Path, project: &Utf8Path) -> Result<Option<ReviewStore>, ReviewError>`。
- `ReviewStore::open_write(path: &Path, project: &Utf8Path) -> Result<ReviewStore, ReviewError>`。
- `ReviewStore::snapshot(&mut self) -> Result<Vec<(ReviewKey, ReviewState)>, ReviewError>`。
- `ReviewStore::append(&mut self, key: &ReviewKey, source: SourceState, expected_revision: u64, draft: ReviewDraft) -> Result<ReviewEvent, ReviewError>`。

- [x] `store_history_and_conflict` を書く。初回revision1、更新revision2、再openで同じhistory、古いrevision拒否、同一内容の明示更新でもrevision3になることをassertする。
- [x] `store_concurrent_initialization_and_updates` を書く。2 connectionとbarrierで新規初期化を競合させ、同projectの有効DBだけが残ることを確認する。同candidate同revisionのappendは一方だけ成功、異なるcandidateのappendは両方保存される。
- [x] `store_rollback_and_busy` を書く。イベント追加後commit前のtest-only failure hookでrollbackを確認する。別connectionでwrite lockを保持し、busyが返ってhistoryが変わらないことを確認する。厳密な秒数のassertは避ける。
- [x] `store_rejects_foreign_corrupt_and_oversized` を書く。application ID違い、未知user_version、既存空ファイル、view/trigger/余計なtable、破損、16 MiB超、10,000イベント後の追加を拒否し、ファイルの既存内容を保持する。境界fixtureは直接既知schemaを使って作り、10,000回CLIを起動しない。
- [x] テスト失敗確認後、固定schemaを実装する。application IDは `0x484D5256` (HMRV)、user_versionは1。tablesはmetadata、candidates、review_events。候補IDをunique、eventは(candidate_id, revision)をprimary keyにし、descriptorとdraftはserdeで検査する。
- [x] 新規storeのみ初期化を許す。`OpenOptions::create_new(true)` に成功したprocessだけが空ファイルを初期化でき、初期化とproject束縛はBEGIN IMMEDIATE内で行う。AlreadyExists側は最大1秒の範囲で完成schemaを待って検証する。期限後も空/不正schemaなら拒否し、他processの未初期化ファイルや既存空ファイルを初期化しない。creatorの中断で残った空ファイルは自動修復対象にしない。
- [x] journal_modeはDELETE、synchronousはFULL、foreign_keysはON、trusted_schemaはOFF、busy_timeoutは1000 ms。新規page_sizeは4096、max_page_countは4096。schemaの既知定義・列・制約とquick_checkを検査し、schema外のSQL objectを許さない。
- [x] appendのtransaction内で容量・project・descriptor・revisionを再確認し、Task 1のapply_update結果から新eventだけをINSERTする。SQL/commit失敗はrollbackして元のhistoryを保つ。新規DB作成後の初回append失敗では空の有効storeが残り得るが、評価eventを保存した成功とは扱わない。
- [x] module内テスト成功を確認してcommitする。ここで空き容量とtargetサイズを確認し、以降のbuildを見込んで `cargo clean` を実施する。

### Task 4: CLI・text/JSON表示と既存結果の不変性

**Files:** Modify `cli.rs`, `lib.rs`, `review/mod.rs`; Create `review/render.rs`, `crates/hoimin-cli/tests/review_cli.rs`, `docs/json-schema/review-result.schema.json`。

**Interfaces:**

- `cli::ReviewArgs` はlist/show/record subcommandを表すenum。共通plan/store/report/format、show/recordのcandidate、recordのassessment/reason/evidence/expected_revision、listのstaleをparseする。
- `review::run(args: ReviewArgs, stdout: &mut dyn Write) -> Result<i32, ReviewError>`。成功0、IO/検証/競合は既存dispatchでstderrとexit2へ変換する。
- `ReviewDocument { schema_version: u32, command: String, project: Utf8PathBuf, candidates: Vec<ReviewRow>, stale_candidates: Vec<ReviewRow>, operators: Vec<OperatorReviewCounts> }`。
- `ReviewRow { candidate: MutationCandidate, execution: ReviewExecution, review: Option<ReviewEvent>, applicability: Applicability, history: Vec<ReviewEvent> }`。executionはNotLoaded/NotInReport/Result(MutationStatus)、serde tagged enumで表す。
- `OperatorReviewCounts { operator: String, total: u64, reviewed: u64, unreviewed: u64, stale: u64, source_unavailable: u64, assessments: BTreeMap<Assessment, u64> }`。各current rowはreviewed/unreviewed/stale/source_unavailableの一つに数え、reviewedは評価別件数の和。
- `render::write(document: &ReviewDocument, format: ReviewOutputFormat, writer: &mut dyn Write) -> Result<(), ReviewError>`。formatはText/Json、CLI値はtext/json。

- [x] `review_cli_contract` を書く。設計の三コマンド、必須store/reason/revision、四評価、text/json、list --stale、unknown IDのexit2をassertする。
- [x] `review_status_and_counts` を書く。未評価≠not_useful、stale/Unavailable除外、reportなしnot_loaded、欠落not_in_report、report付きrecordの欠落拒否、operator集計の和を確認する。show/recordは対象1件、listはplan順、旧候補枠はID順で決定的に出力する。
- [x] `review_read_never_creates_store_or_processes` を書く。不存在storeのlist/showは未評価を返しファイルを作らず、analyzer/test markerを起動しない。
- [x] `review_output_failure_is_not_duplicate_write` を書く。書込み失敗writerでもeventのcommit済み状態は残る。stderrへ「保存済みrevision・表示失敗」を返し、同じexpected revisionでの再試行は競合してhistoryを増やさない。
- [x] `review_does_not_change_execution_results` を書く。同じfixtureのrun report bytes、MutationStatus、summarize、progress compare結果を評価登録前後で照合する。既存plan bytesも不変とする。新しいテスト実行を繰り返さず固定reportを使う。
- [x] 失敗確認後、parse/dispatch、recordのcommit後render、集計と専用schemaを実装する。historyのないrowのreviewはnullとし、textではunreviewedを表示する。stale rowの保存評価は履歴として表示し、有効な現評価欄はnullとする。
- [x] `cargo test -p hoimin-cli --test review_cli` を成功させ、JSON出力を専用schemaで検査する。既存completionテストを更新・実行し、commitする。

### Task 5: Leanの三性質とRust差分検証

**Files:** Create `formal/HoiminOracle/HoiminOracle/SurvivorReviewModel.lean`, `SurvivorReviewProofs.lean`, `SurvivorReviewCases.lean`, `formal/HoiminOracle/SurvivorReviewAuditMain.lean`, `formal/HoiminOracle/corpus/survivor-review.jsonl`, `crates/hoimin-cli/tests/lean_survivor_review_oracle.rs`; Modify `formal/HoiminOracle/HoiminOracle.lean`, `lakefile.toml`, `.github/workflows/ci.yml`, `tests/test_ci_workflow.py`。

**Interfaces:** Lean namespace `HoiminOracle.SurvivorReview` に `applicability` と `applyUpdate` をTask 1と同じ条件・拒否理由・順序で定義する。revisionはNatで表し、Rustのu64::MAX超を明示的に拒否する。generator名は `generate_survivor_review`。

- [x] Rust oracleテストを先に書く。corpus各行のproject/descriptor一致条件、鮮度、current/expected revision、history、draftからTask 1の実処理を呼び、適用判定・成功/拒否・更新後historyを比較する。corpus欠落時の失敗を確認する。
- [x] Lean modelと、`applied_requires_exact_identity_and_fresh_source`、`rejected_update_preserves_state`、`accepted_update_preserves_prefix_and_increments_revision` を実装する。sorry/axiomで証明を埋めない。拒否時は元stateを返すモデル表現とRust Resultをoracleで対応付ける。
- [x] 小さな決定的corpusをLeanから生成する。project/descriptor各不一致、三鮮度、revision一致/不一致/overflow、空/非空history、同じ時刻の複数更新を含める。symbol/originalを省いた弱いidentity判定を検出できるケースも含める。
- [x] generatorへ `--output`、`--check`、`--sensitivity` を実装する。identity guardの削除とrevision guardの削除をそれぞれ検出し、検出できなければsensitivityを失敗にする。
- [x] CIのserial module build一覧とgenerator一覧、workflow契約テストに追加する。既存guardで各commandを30秒、2048 MiB、250 ms samplingに制限する。WindowsのguardはPOSIX未対応なので、ローカル形式検証は利用可能なWSLの既存Lean環境かLinux CIで実施し、未実施を成功と扱わない。新たな巨大toolchainの導入はしない。
- [x] `cargo test -p hoimin-cli --test lean_survivor_review_oracle`、`uv run --frozen --no-sync pytest tests/test_ci_workflow.py` を実行する。Leanで証明・freshness・sensitivityの成功を確認してcommitする。

### Task 6: 文書、版、最終レビューとstack

**Files:** Modify `docs/usage.md`, `docs/development.md`, `docs/knowledge/design/survivor-review.md`, `docs/knowledge/references/design-documents.md`, `Cargo.toml`, `Cargo.lock`, `pyproject.toml`, `uv.lock`; Create `docs/superpowers/reports/2026-10-11-issue-711-survivor-review.md`; Update `docs/knowledge/references/audit-documents.md`。必要なら既存release説明の版参照を同じcommitに含める。

- [x] usageへ設計のCLI例、revision競合時のshow→再評価、staleと疑似等価の意味、storeの移動制約を追記する。developmentへLean実行と保証範囲を追記する。
- [x] 実装レビューを最低5回実施してreportに記録する。順にidentity/source境界、transaction/並行更新、入力/出力制限、既存結果への非干渉、Lean/Rustの対応を読み直し、指摘と修正内容を残す。
- [x] テストレビューを最低5回実施してreportに記録する。順に異常入力のoracle、競合の再現性、失敗注入でのrollback、score/progress不変の判定根拠、corpus freshness/sensitivityとWindows/Linux差を確認する。レビュー回数を増やすためだけの同一テスト再実行はしない。
- [x] 配布版のfloorを既存 `uv run --frozen --no-sync python tools/release.py set-version 0.4.0` で揃える。この既存処理がCargo.toml/pyproject.tomlと両lockの自package版を一緒に更新する。実行前にmain側が既に0.4.0以上なら降格せず、差異を報告する。`uv run --frozen --no-sync pytest tests/test_release.py tests/test_pypi_release.py` で既存release契約を確認する。
- [x] `cargo fmt --all -- --check`、`cargo clippy --workspace --all-targets --all-features -- -D warnings`、`cargo test --workspace` を実行する。追加のcontracts変更がある場合は `cargo test -p hoimin-cli --features contracts` も実行する。既存の無関係な失敗は今回の成功と混同せず原因を報告する。
- [x] 変更にPythonが含まれる場合は既存ruff/type checkを適用する。OKFのYAML、相対リンク、出典ID/hash、索引からの到達性を確認する。設計から実装済み契約へ変更した箇所だけ根拠を更新する。
- [x] 検証結果と未確認OS条件をreportへ残してcommitする。`cargo clean` と空き容量確認を行う。`gh stack rebase --no-trunk` で上段へ反映し、後述#759調査commitの準備へ進む。
- [x] `gh stack submit` でPRを作る段階では各PRのbaseを確認し、#711→main、#759→#711とする。作成した全PRをCodexにattachする。ユーザーのmerge操作を待つ。

## #759の調査への引継ぎ

上段では、同じsource・候補集合でscreen/confirmの二つのplanを作り、screenで確実にsurviveしたIDだけを独立したverifyでconfirmする。
verifyで保存planのcommandをoverrideできないため、別planが必要である。
候補IDと完全descriptorが一致することを確認し、timeout/error/cancelはsurvivorとしてconfirm対象に流さない。
各verifyが新しいworkerを使うことでstage間の生成物を持ち越さない。
小さなfixtureでscreen miss→confirm kill、screen kill→confirm省略、失敗/中断の除外を試し、代表的な遅いテストでbaselineとworkspace再作成を含む総時間を比較する。
既存commandの組合せで足りるなら調査報告と運用手順だけを上段へcommitし、product APIを増やさない。
#759の設計・調査計画・実験実装・検証にも各5回のセルフレビュー記録を残す。
この計画は#711の実装計画であり、#759機能の実装承認を兼ねない。

## 計画セルフレビュー（5回）

1. 設計網羅: 四評価、履歴、stale、任意report、容量、形式証明をTask 1–5へ対応付けた。未評価と実行状態の欠落を区別するTask 4のassertを加えた。
2. 手順具体性: 各Taskを失敗確認→実装→成功確認→commitで区切った。入力snapshotを共有し、二度読んだ異なる内容のhash/検証が混ざらない手順をTask 2へ加えた。
3. 型と境界: Task 1のReviewKey/ReviewState/ReviewDraftを保存・表示・oracleで共有する形に統一した。既存UsableReportへの必須field追加を避け、review専用readerとした。
4. 障害とレビュー焦点: 競合、同時初期化、未知DB、alias、stdout失敗後の重複再試行を各Taskのテストへ割り当てた。SQLite初期化の新規/既存判別とcommit後表示失敗の契約を明記した。
5. 費用と保証: 巨大OSSや全operator試験を追加しないことを確認した。LeanのCI制限、Windows guard非対応、モデル証明と実装テストの範囲を分けた。設計・計画のレビューを実装/テストの5回へ流用しない。

## 実行方法

ユーザーの承認を受け、Native方式でTask 1–6を順番に実装した。
検証証拠と設計・計画からの判断は[実装報告](../reports/2026-10-11-issue-711-survivor-review.md)へ記録する。
最終レビューのImportant 4件を修正し、ローカル全体検証後にgh stackで #793/#794 を提出した。GitHub CIの最新状態はPRで確認する。
