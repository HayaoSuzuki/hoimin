# Issue 513 Parser Recursion Implementation Plan

> **For agentic workers:** Use superpowers:executing-plans. Root coordinates the single Cargo lane, review, commits, PR. No merge without user authorization; one independent read-only reviewer may assist.

**Goal:** 深いPython式を解析時の異常終了ではなく既存の深さエラーにする。

**Architecture:** pinned parserへ上流の再帰箇所のstackerを移植し、既存の深さ検査・反復破棄につなぐ。試行解析の所有権も点検する。

**Tech Stack:** Rust 2024 / MSRV 1.88、littrs Ruff 0.6.2、stacker 0.1.24、CPython 3.14。

**Spec:** [設計](../specs/2026-09-12-issue-513-parser-recursion-design.md)

## Global Constraints

- 作業はissue-513 worktree。他のIssueの変更を混ぜない。
- AST深さ128、byte span、候補ID・順序、通常構文エラーとbaseline先行を維持する。
- 不要な文法更新、固定stack拡大、ソース長の制限、panicでの回復、mem::forgetを導入しない。
- Cargoはrootが1コマンドずつ実行する。新規依存のMSRVとnative対象を確認する。

## Task 1: 解析から破棄までの回帰と移植

**Files:** `crates/hoimin-cli/tests/analysis_depth.rs`、`crates/hoimin-cli/tests/rust_analyzer.rs`、`vendor/ruff_python_parser/`、`Cargo.toml`、`Cargo.lock`。

**Interfaces:** parserの既存public APIと0.6.2 AST型を維持する。hoiminの`analyze_source_cancellable`は同じ戻り値を使う。

- [x] 既存のbounded subprocess helperを使い、plan/runの深い式テストへ次の入力生成を追加する。まず各有効fixtureを独立したCPythonのcompileで確認し、修正前に通常終了と深さ診断のassertが失敗することを確認する。意図的な不正構文やsynthetic ASTは別テストとする。

```rust
let unary = format!("value = {}1\n", "-".repeat(1000));
let power = format!("value = {}\n", vec!["1"; 2000].join("**"));
let lambdas = format!("value = {}1\n", "lambda: ".repeat(2000));
let conditional = format!("value = {}1\n", "1 if x else ".repeat(2000));
```

Run: `cargo test -p hoimin-cli --all-features --test analysis_depth`。期待: 追加ケースの子プロセスがsignal終了するためRED。既存の加算と構文エラーのケースは保持する。

- [x] pinned registry packageのsrc/resources/Cargo.tomlと由来をvendorへ保存する。MIT licenseと上流commit、移植箇所をREADMEへ記録する。registry cacheは変更しない。root workspaceからvendorをexcludeし、`[patch.crates-io]`でparserだけをpath指定する。

```toml
[patch.crates-io]
littrs-ruff-python-parser = { path = "vendor/ruff_python_parser" }
```

- [x] vendorにstacker依存を追加する。初回parseと設計の8箇所を、返り値と所有権を変えずに以下の方針で包む。

```rust
fn with_recursion<T>(&mut self, f: impl FnOnce(&mut Self) -> T) -> T {
    stacker::maybe_grow(128 * 1024, 1024 * 1024, || f(self))
}
```

各入口で検査するためrecursion counterは不要。定数へ名前を付け、上流の遅延最適化を採用しない理由を記録する。文法上の再帰呼び出しが必ずguardを通ることを一覧とソースで照合する。

- [x] 同じ公開テストでGREENを確認する。小さいstackでの直接parse、繰り返し解析とheap回収、浅い広い入力、既存AST境界を追加・実行する。
- [x] 代入・delのhelperとpattern回復、with/matchの巻き戻し時に深いASTが捨てられる箇所を検査する。該当する失敗を子プロセスで再現した場合、反復的な所有権切り離しを再利用してその破棄経路を修正する。完了記録では試験入力が通った経路と未検証の一般化を区別する。
- [x] 通常候補の互換性は既存analyzer suiteと再監査の`/private/tmp/hoimin-reaudit/syntax/extra_matrix.py`と`extra-matrix.json`のBOM/改行/パターン20fixtureを用い、candidate ID/span/original/replacementを比較する。

## Task 2: native・静的検証と証拠

**Files:** この計画、設計、vendorの由来、`docs/knowledge/design/analyzer.md`、設計索引。

- [x] `cargo fmt --all -- --check`、厳格Clippy、`cargo test --workspace --all-features`をrootが順次実行する。
- [x] workspaceから除外したvendorにも`cargo fmt --manifest-path vendor/ruff_python_parser/Cargo.toml -- --check`と`cargo clippy --locked -p littrs-ruff-python-parser --lib --no-deps -- -D warnings`を直接実行する。配布packageのtest依存がない場合はその制約を記録し、公開CLIとparser直接回帰で移植箇所を検証する。
- [x] 普通のPythonソースについて同じbuild profile・入力・反復数で移植前後の時間を測り、per-entry probeの影響を記録する。
- [x] releaseでanalysis_depthを実行する。Linux/Windows CI、MSRV、配布wheelの依存同梱を確認し、実行できた対象を記録する。
- [x] 設計と実行証拠をOKFへ追記し、sources/脚注/リンク/内容を3回レビューする。YAMLパーサによる構造検証と全設計索引の検証を実行する。
- [x] 独立レビューで指摘を解消し、PR本文に不具合・移植・実行証拠・残る制約を記載する。PRとCI結果を報告する。マージは今回の作業に含めない。

## 計画セルフレビュー

1. 原因への対応: parserがAST検査へ到達する前の再帰を直接対象とし、深い有効入力の通常エラーと浅い入力の受理を対にした。
2. 所有権とAPI: unchecked parseと0.6.2型を維持し、返却前の破棄経路を別工程にした。上流の全体更新や未測定の遅延回数に依存しない。
3. 検証と依存: Cargoを直列化し、debug/releaseとnative対象・MSRV・wheelを分けた。3回レビューとOKF索引の更新が抜けないことを確認した。

## 実行記録

以下は既存作業の記録。2026-09-13の継続作業では各結果を新たに実行して確認する。

独立設計レビュー: vendorがworkspace検証対象外である点と、深いfixtureのCPython compile前提を指摘され、専用チェックと有効性検証を追加した。互換性fixtureの実体パスと計測条件も明記した。

### RED test stage

- Added separate public plan/run regressions for the recorded unary, power, lambda and conditional sources. Shared assertion first compiles each newly claimed-valid source with the repository CPython in a bounded subprocess, then requires normal nonzero CLI exit, path/depth128 diagnostic, unchanged bytes, and incomplete run output. Existing addition and invalid-partial-tree regressions remain.
- `rustfmt --edition 2024 crates/hoimin-cli/tests/analysis_depth.rs` and `git diff --check` completed without errors. Cargo execution is root-owned; production and dependencies remain unchanged pending RED.
- Review1 (contract): tests reject a parser abort and cannot pass on a successful empty plan; both plan and run paths retain their prior output contracts.
- Review2 (independent oracle): CPython compile precedes CLI execution for every claimed-valid source; malformed partial trees stay in the separate syntax-error test.
- Review3 (sensitivity/lifecycle): each independent recursive form has its own test, all subprocesses retain deadlines and file-backed output, and assertions check original bytes plus controlled diagnostics. A missing recursion checkpoint must fail a relevant source shape. No source-length/token-count expectation is introduced.

- Root review correction: old addition parser/disposal stress cases retain their original contract without a CPython-validity assertion, since interpreter limits vary. Only the four new valid-source claims require CPython compilation; the helper parameter explicitly marks this distinction.

### Initial backport stage

- Root RED: `/private/tmp/hoimin-reaudit/513-red-depth.log`, exit101; both old tests passed and all four new tests failed at normal CLI exit after their independent CPython preconditions passed.
- Vendored published0.6.2 src/resources/normalized manifest with upstream MIT license and provenance. Workspace path patch selects only the parser. Added stacker0.1.24 without running dependency resolution locally.
- Adapted all eight final upstream checkpoints using per-entry128KiB/1MiB probing. Only parser mod/expression/pattern/statement source files differ from the original published package at this stage.
- Review1 (correspondence): checked initial parse plus binary/lambda/conditional/format-spec/pattern-LHS/suite/async entry mapping against PR25464. No upstream hard-limit/recovery or unrelated container changes were imported.
- Review2 (ownership): return types, ownership and ordinary recovery are unchanged. Internal with/match fallback drops remain independent paths, so new bounded ownership tests now cover those sites before any disposal change. They are labeled parser/disposal cases, without a cross-platform CPython resource-limit claim.
- Review3 (compatibility): AST/trivia/text-size dependencies stay pinned-compatible; existing analysis-depth128 and unchecked partial-tree handling remain unchanged. Repeated allocator test now exercises all four recursive forms after warming each; root will execute it after initial GREEN.

### 2026-09-13 resumed implementation

- Existing uncommitted worktree reused at base61c654f. Current main8f1613b was rebuilt for the initial reproduction; unary1000 aborted, unary150 returned depth128. No unrelated worktree changes were removed.
- Fixed the existing `Box<Expr>` cleanup compile error. All8 initial public regressions then passed (`/private/tmp/issue513-depth-resumed.log`). Historical RED remains `/private/tmp/hoimin-reaudit/513-red-depth.log`.
- Added direct parser subprocesses on a512KiB thread. `/private/tmp/issue513-small-stack-red.log` reproduced4 aborts: assignment context, discarded keyword pattern, discarded as pattern, pattern conversion. Added shared stack checks and iterative pattern destruction; first GREEN passed.
- Expanded list depth and added Python3.8 decorator mode; `/private/tmp/issue513-small-stack-expanded-red.log` reproduced3 additional aborts in assignment validation, delete validation, decorator traversal. Protected each recursive helper edge. All11 direct fixtures then passed, including suite, format-specification, async recovery and mixed grammar nesting.
- Extended public invalid-syntax controls and allocator checks for speculative and recovery disposal. Keep separate parse validity and CPython acceptance claims.
- Vendor standalone formatting needs its own workspace boundary because this checkout is nested inside the main workspace. Root-lockfile library Clippy uses `-p littrs-ruff-python-parser --lib --no-deps` and succeeded. Added those vendor checks to CI.
- The checks below record the final local validation. Remote CI remains separate and will be reported with the PR; Linux/Windows have not been executed locally.

### Final local validation (2026-09-13, macOS aarch64)

All Cargo commands used `CARGO_INCREMENTAL=0` and `CARGO_TARGET_DIR=/Users/hayao/RustroverProjects/hoimin/target`. Rust1.98.0 unless the command explicitly selects1.88. Repository CPython3.14.7. Native platforms other than macOS are not inferred from these runs.

| Check | Result | Raw evidence |
| --- | --- | --- |
| `cargo test --offline --workspace --all-features` | exit0;75 result groups,1,750 passed executions,0 failed,13 ignored | `/private/tmp/issue513-workspace.log` |
| `cargo test --offline -p hoimin-cli --test analysis_depth --test rust_analyzer` before final test extensions | exit0;9 public tests,191 analyzer tests,2 ignored | `/private/tmp/issue513-focused.log` |
| `cargo test --offline --release -p hoimin-cli --test analysis_depth` | exit0;9 passed, including all11 small-stack child cases | `/private/tmp/issue513-release-public.log` |
| `cargo test --offline --release -p hoimin-cli --test analysis_depth --test rust_analyzer depth` | exit0;4 public and7 analyzer depth tests, including allocation recovery | `/private/tmp/issue513-release-depth.log` |
| `cargo clippy --offline --workspace --all-targets --all-features -- -D warnings` | exit0 | `/private/tmp/issue513-clippy.log` |
| `cargo clippy --offline -p littrs-ruff-python-parser --lib --no-deps -- -D warnings` | exit0, selects the root lockfile | `/private/tmp/issue513-vendor-clippy.log` |
| `cargo +1.88 check --offline --workspace --all-targets --all-features --locked` | exit0 | `/private/tmp/issue513-msrv.log` |
| workspace and standalone vendor `cargo fmt ... -- --check`; `git diff --check` | exit0 | terminal output |
| `.venv/bin/maturin build --release --locked --offline --out /private/tmp/issue513-wheels` | exit0,macosx_11_0_arm64 wheel | `/private/tmp/issue513-wheel-build.log` |
| `HOIMIN_WHEEL=... .venv/bin/python tests/wheel_smoke.py` | exit0, isolated installation and mutation run | `/private/tmp/issue513-wheel-smoke.log` |
| Candidate compatibility against saved baseline in `/private/tmp/hoimin-reaudit/syntax` |20 newline/BOM/pattern fixtures;64 complete candidate objects identical (including IDs/spans/original/replacement) | terminal output; baseline `matrix_*/stdout.json` |
| OKF safe YAML and reserved-file structure |16 Markdown files passed | terminal output |
| Changed OKF source hashes,footnotes,relative links; full design-index coverage | passed | terminal output |

The first wheel smoke attempt was blocked by sandbox access to the uv cache. The authorized rerun completed; no product code changed. The first ad-hoc timing link attempted to hand LLVM22 bitcode to Apple's LLVM17 linker; rebuilding the same harness with `-C lto=thin` resolved that tooling mismatch.

Normal-input timing used the original registry0.6.2 and the local parser release rlibs, Rust1.98 and thin LTO, with the same harness:200 shallow functions,500 parses per sample,20 warmup parses,5 alternating samples per version. Median before100.2235ms; after102.156084ms (ratio1.0193). This small observation does not establish a universal performance bound or statistical equivalence. Raw samples: `/private/tmp/issue513-timing.json`; harness: `/private/tmp/issue513-parser-timing.rs`.

Three documentation reviews: (1) contract and source correspondence, distinguishing parser stack control, post-parse depth128 and disposal; (2) test evidence and native-platform boundaries, including Ruff-only stress fixtures versus CPython-compiled public fixtures; (3) OKF YAML,source hash,footnote/link/index consistency and Japanese wording. No Lean or arbitrary-memory safety claim is made.

Independent integration review found no blocking issue and identified missing vendor commands in the local quality gate; docs/development.md now contains them. The suggested nonzero/empty-plan assertions for invalid syntax were adopted after confirming the existing exit2/no-stdout behavior. Public depth tests were rerun after this test-only strengthening. The final Rust review compared all six changed parser files with the registry version and found no additional correctness issue.

### Delivery status

- [x] Prepare commit and draft PR; delivery identifiers are reported with the PR.
- [x] Draft PR #519 created; remote CI pending at delivery. Local macOS checks do not establish Windows/Linux results.

Final test-only review follow-up: strengthened invalid-syntax assertions pass in both profiles (9 public tests each): `/private/tmp/issue513-public-final.log`, `/private/tmp/issue513-release-public-final.log`. Final strict workspace Clippy also exits0 (`/private/tmp/issue513-clippy-final.log`). Vendored `.gitattributes` disables newline normalization for parser resource fixtures; all526 staged resource files were compared byte-for-byte against the registry package and match. The final Rust reviewer reported no correctness findings in checkpoint closures, discarded ownership, or shared destruction.

PR: https://github.com/tokyogas-tech/hoimin/pull/519 (draft). Integrated latest main8f1613b into the PR branch after GitHub reported conflicts in the two OKF pages. Both Issue513 and515 source entries, sections and citations are preserved. Post-integration workspace all-feature test: exit0, 1753 passed executions across 76 result groups,0 failed,13 ignored (`/private/tmp/issue513-merged-workspace.log`). Merged OKF YAML,source hashes,citations,links and design-index checks passed again. This is an integration into the feature branch, not a merge of the PR into main.
