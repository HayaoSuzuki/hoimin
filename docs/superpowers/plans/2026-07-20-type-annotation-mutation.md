# Type Annotation Mutation Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 型アノテーションを局所的に変異し、`ty` または `mypy` などの利用者指定コマンドで型検査の網を測れるようにする。

**Architecture:** 演算子選択は `hoimin-core` の実行設定として正規化し、CLI、fingerprint、session 再開、analyzer request が同じ値を使う。Ruff AST から収集する標準ライブラリ由来の型注釈だけを、局所的な byte-span replacement として既存の analyzer candidate stream へ加える。型チェッカーは特別扱いせず、既存の baseline と mutant process を実行する。

**Tech Stack:** Rust 2024、clap、Ruff Python AST/parser、serde、SQLite、tokio、`uv`、ty、mypy。

## Global Constraints

- `hoimin-core` は I/O、tokio、rusqlite、tempfile、OS API crate に依存しない。
- 1 mutant は 1 箇所の byte-span だけを置換し、置換以外のソース bytes を変えない。
- `--operators` を省略した既存 run は全既存演算子だけを選び、型演算子を選ばない。
- 型演算子名と正規化した選択集合は JSON/JSONL の candidate、fingerprint、SQLite session 互換性に反映する。
- 型コメント、文字列化した前方参照、`Annotated`、`Any`、`object`、利用者定義 alias、`TypeVar`、`Protocol`、`Callable`、`Literal` は候補にしない。
- Python 3.14 を使用し、既存の Rust quality gates と wheel smoke test を維持する。

---

## File Structure

| Path | Responsibility |
| --- | --- |
| `crates/hoimin-core/src/config.rs` | 演算子 ID、系統展開、`RunConfig` の正規化済み選択集合 |
| `crates/hoimin-cli/src/cli.rs` | `--operators` と `--exclude-operators` の clap 入力 |
| `crates/hoimin-cli/src/shell.rs` | analyzer request と fingerprint への選択集合の受け渡し |
| `crates/hoimin-cli/src/analyzer/rust.rs` | Ruff AST を用いる型注釈候補の列挙 |
| `crates/hoimin-cli/src/analyzer/protocol.rs` | 新演算子 ID の protocol validation |
| `crates/hoimin-cli/src/analyzer/rust_tests.rs` | AST candidate、scope、line、非対象構文の単体テスト |
| `crates/hoimin-cli/tests/cli_config.rs` | CLI 正規化と不正な演算子指定のテスト |
| `crates/hoimin-cli/tests/analyzer_handler.rs` | JSONL protocol が型候補を受け入れるテスト |
| `crates/hoimin-core/tests/resume_policy.rs` | 演算子選択の順序非依存 fingerprint と変更時の不一致 |
| `crates/hoimin-cli/tests/run_e2e.rs` | ty/mypy による killed/survived の実行テスト |
| `pyproject.toml`, `uv.lock` | E2E に使う ty と mypy の開発依存 |
| `README.md` | 型検査専用の実行例と演算子選択の説明 |

### Task 1: 演算子選択を実行設定に追加する

**Files:**

- Modify: `crates/hoimin-core/src/config.rs`
- Modify: `crates/hoimin-core/tests/target_policy.rs`
- Modify: `crates/hoimin-cli/src/cli.rs`
- Modify: `crates/hoimin-cli/tests/cli_config.rs`

**Interfaces:**

- Produces: `pub enum MutationOperator`, `pub struct MutationOperatorSelection`, `RunConfig::operators: MutationOperatorSelection`.
- Consumes: `RawRunConfig { operators: Vec<String>, exclude_operators: Vec<String>, .. }`.

- [ ] **Step 1: CLI と core の失敗テストを書く**

`cli_config.rs` に、既定値、系統展開、除外、未知名を固定するテストを追加する。

```rust
#[test]
fn operator_flags_expand_groups_and_preserve_legacy_default() {
    let default = parse_config_from(["hoimin", "run", "--file", "x.py", "--", "check"])
        .unwrap();
    assert!(!default.operators.contains(MutationOperator::TypeNullableRemove));

    let selected = parse_config_from([
        "hoimin", "run", "--file", "x.py",
        "--operators", "type_nullable,type_collections",
        "--exclude-operators", "type_mapping",
        "--", "check",
    ]).unwrap();
    assert!(selected.operators.contains(MutationOperator::TypeNullableRemove));
    assert!(selected.operators.contains(MutationOperator::TypeListSequence));
    assert!(!selected.operators.contains(MutationOperator::TypeMapping));
}
```

- [ ] **Step 2: 失敗を確認する**

Run: `cargo test -p hoimin-cli --test cli_config operator_flags_expand_groups_and_preserve_legacy_default`

Expected: `--operators` が未定義で compilation failure または clap parse failure。

- [ ] **Step 3: 正規化モデルと clap 引数を実装する**

`MutationOperator` は既存 13 種と、`type_nullable_remove`、`type_nullable_add`、`type_list_sequence`、`type_set_abstract_set`、`type_dict_mapping`、`type_iterable_iterator`、`type_sequence_iterable` を列挙する。
`MutationOperatorSelection` は `BTreeSet<MutationOperator>` を保持し、`all_legacy()`、`parse_selector(&str)`、`include()`、`exclude()`、`names()` を提供する。
系統名は `type_nullable`、`type_collections`、`type_iterables` を展開し、個別名と未知名は `ConfigError::UnknownMutationOperator { value }` として報告する。

```rust
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct MutationOperatorSelection(BTreeSet<MutationOperator>);

impl Default for MutationOperatorSelection {
    fn default() -> Self { Self::all_legacy() }
}
```

`RawRunArgs` と `RunArgs` へ `operators: Vec<String>`、`exclude_operators: Vec<String>` を追加し、clap 属性は `#[arg(long, value_delimiter = ',')]` にする。
`raw_config` は両方を `RawRunConfig` へ渡し、`RunConfig::try_from` が include を指定したときは空集合から、指定しないときは `all_legacy()` から構築して exclude を最後に適用する。

- [ ] **Step 4: テストを通す**

Run: `cargo test -p hoimin-core --test target_policy; cargo test -p hoimin-cli --test cli_config`

Expected: 両方の integration test target が PASS。

- [ ] **Step 5: コミットする**

```console
git add crates/hoimin-core/src/config.rs crates/hoimin-core/tests/target_policy.rs crates/hoimin-cli/src/cli.rs crates/hoimin-cli/tests/cli_config.rs
git commit -m "feat: configure mutation operators"
```

### Task 2: 選択集合を analyzer、protocol、fingerprint へ通す

**Files:**

- Modify: `crates/hoimin-cli/src/shell.rs`
- Modify: `crates/hoimin-cli/src/analyzer/mod.rs`
- Modify: `crates/hoimin-cli/src/analyzer/rust.rs`
- Modify: `crates/hoimin-cli/src/analyzer/protocol.rs`
- Modify: `crates/hoimin-cli/tests/analyzer_handler.rs`
- Modify: `crates/hoimin-core/tests/resume_policy.rs`

**Interfaces:**

- Consumes: `RunConfig::operators: MutationOperatorSelection` from Task 1.
- Produces: `AnalyzeRequest { operators: &MutationOperatorSelection, .. }` and `MutationOperatorSelection::names()` for `FingerprintInput::operators`.

- [ ] **Step 1: protocol と fingerprint の失敗テストを書く**

`analyzer_handler.rs` で `operator: "type_nullable_remove"` を含む candidate JSONL を受理するテストを追加する。
`resume_policy.rs` で型演算子を一つ追加した `FingerprintInput` が異なる fingerprint になることを追加する。

```rust
let changed = FingerprintInput { operators: vec![
    "type_nullable_remove".to_owned(),
], ..fixture_input() };
assert_ne!(fingerprint(&fixture_input()), fingerprint(&changed));
```

- [ ] **Step 2: 失敗を確認する**

Run: `cargo test -p hoimin-cli --test analyzer_handler; cargo test -p hoimin-core --test resume_policy`

Expected: protocol test は `invalid candidate fields`、fingerprint test は selection 未接続で期待どおりに失敗する。

- [ ] **Step 3: 選択集合を接続する**

`shell::mutation_operators()` を削除し、`context.config.operators.names()` を `FingerprintInput` に渡す。
`AnalyzerHandler::handle` と `AnalyzeRequest` に selection を渡し、既存の token 演算子も `selection.contains(operator)` のときだけ candidate にする。
`known_operator` は `MutationOperator::from_name(operator).is_some()` に置き換え、JSONL の許可名を config の単一の列挙から得る。

- [ ] **Step 4: テストを通す**

Run: `cargo test -p hoimin-cli --test analyzer_handler; cargo test -p hoimin-core --test resume_policy; cargo test -p hoimin-cli --test rust_analyzer`

Expected: すべて PASS。既定選択の既存 analyzer fixture の candidate 列は変わらない。

- [ ] **Step 5: コミットする**

```console
git add crates/hoimin-cli/src/shell.rs crates/hoimin-cli/src/analyzer/mod.rs crates/hoimin-cli/src/analyzer/rust.rs crates/hoimin-cli/src/analyzer/protocol.rs crates/hoimin-cli/tests/analyzer_handler.rs crates/hoimin-core/tests/resume_policy.rs
git commit -m "feat: propagate mutation operator selection"
```

### Task 3: Ruff AST から型アノテーション候補を列挙する

**Files:**

- Modify: `crates/hoimin-cli/src/analyzer/rust.rs`
- Modify: `crates/hoimin-cli/src/analyzer/rust_tests.rs`

**Interfaces:**

- Consumes: `AnalyzeRequest::operators` from Task 2.
- Produces: `fn type_annotation_candidates(module: &ModModule, source: &str, imports: &KnownImports, request: &AnalyzeRequest<'_>) -> Vec<AnalyzerCandidate>`.

- [ ] **Step 1: annotation fixture の失敗テストを書く**

一つの fixture に引数、戻り値、ローカル変数、モジュール変数、クラス属性、直接 import、修飾名を含める。
`type_nullable_remove`、`type_nullable_add`、collection 三種、iterable 二種の candidate 名と source order を完全一致で検証する。

```rust
assert_eq!(candidates.iter().map(|c| (c.original.as_str(), c.replacement.as_str(), c.operator.as_str())).collect::<Vec<_>>(), vec![
    ("str | None", "str", "type_nullable_remove"),
    ("Optional[int]", "int", "type_nullable_remove"),
    ("str", "str | None", "type_nullable_add"),
    ("list[str]", "Sequence[str]", "type_list_sequence"),
]);
```

別テストで quoted annotation、`Annotated`、`Any`、`Callable`、`TypeVar`、利用者定義 `Sequence`、`from local import Optional` が候補を出さないことを確認する。
`--line` と `--symbol` のテストは引数、戻り値、ローカル変数、クラス属性をそれぞれ絞れることを確認する。

- [ ] **Step 2: 失敗を確認する**

Run: `cargo test -p hoimin-cli --lib analyzer::rust_tests::type_annotations`

Expected: 型 candidate が未実装のため assertion failure。

- [ ] **Step 3: import 解決と annotation walker を実装する**

`AstFacts` に `KnownImports` を加え、`Stmt::Import` と `Stmt::ImportFrom` を visit して `typing`、`collections.abc` からの直接 import、module alias を記録する。
同じ module scope で標準ライブラリ由来と確認できる名前だけを受理する。

関数定義の parameter annotations と returns、`Stmt::AnnAssign` の annotation を walk し、対象 annotation expression の `TextRange` と現在の scope を記録する。
`Expr::BinOp` の `BitOr` と片側 `None`、既知 `Optional[...]`、既知 `Subscript` を構文的に一致させる。
置換時は outer annotation expression 全体を span にし、文字列は source slice から取得する。
`T -> T | None` は許可済みかつ非対象型を含まない最外 annotation にだけ生成する。

candidate を既存 token candidates と結合し、開始 byte offset、次に operator name で安定ソートする。
同一 span と同一 replacement の重複は `BTreeSet<(u64, String, String)>` で除く。

- [ ] **Step 4: テストを通す**

Run: `cargo test -p hoimin-cli --lib analyzer::rust_tests; cargo test -p hoimin-cli --test rust_analyzer`

Expected: analyzer の unit と integration test target が PASS。

- [ ] **Step 5: コミットする**

```console
git add crates/hoimin-cli/src/analyzer/rust.rs crates/hoimin-cli/src/analyzer/rust_tests.rs
git commit -m "feat: mutate supported type annotations"
```

### Task 4: 型チェッカー E2E を固定する

**Files:**

- Modify: `pyproject.toml`
- Modify: `uv.lock`
- Modify: `crates/hoimin-cli/tests/run_e2e.rs`
- Create: `tests/fixtures/projects/type-checking/pyproject.toml`
- Create: `tests/fixtures/projects/type-checking/src/contracts.py`

**Interfaces:**

- Consumes: CLI selection and AST candidates from Tasks 1–3.
- Produces: ty と mypy で killed と survived を再現する E2E fixture.

- [ ] **Step 1: dev dependency と E2E の失敗テストを書く**

`uv add --group dev ty mypy` を実行し、生成された `pyproject.toml` と `uv.lock` を保持する。
`run_e2e.rs` に `run_type_checker(checker: &Path, expected_status: &str)` を追加し、`--operators type_nullable` と `--max-mutants 1` を渡す。
checker は root の管理済み `.venv` にある `ty` または `mypy` の実行ファイルの絶対パスとし、worker 内で `uv run` による再解決を起こさない。

```rust
#[tokio::test]
async fn ty_kills_a_nullable_contract_mutant() {
    let run = run_type_checker(&ty_executable(), "killed").await;
    assert_eq!(run.statuses, ["killed"]);
}

#[tokio::test]
async fn mypy_reports_a_surviving_nullable_contract_mutant() {
    let run = run_type_checker(&mypy_executable(), "survived").await;
    assert_eq!(run.statuses, ["survived"]);
}
```

- [ ] **Step 2: 失敗を確認する**

Run: `uv run cargo test -p hoimin-cli --test run_e2e ty_kills_a_nullable_contract_mutant ty_reports_a_surviving_nullable_contract_mutant mypy_kills_a_nullable_contract_mutant mypy_reports_a_surviving_nullable_contract_mutant`

Expected: fixture または type operator が未接続なら失敗する。

- [ ] **Step 3: fixture と command construction を実装する**

fixture の `contracts.py` は、nullable を除去すると型エラーになる call site と、型チェッカー設定が許す nullable 追加の call site を分ける。
E2E helper は worker 内の fixture root を current directory とし、root の `.venv` にある `ty check` または `mypy src` を絶対 argv として渡し、shell を経由しない。
ty と mypy のバージョン差で結果が揺れないよう、fixture pyproject に checker settings を明示し、lockfile に解決済み版を記録する。

- [ ] **Step 4: テストを通す**

Run: `uv run cargo test -p hoimin-cli --test run_e2e`

Expected: type-checking fixture を含む E2E target が PASS。

- [ ] **Step 5: コミットする**

```console
git add pyproject.toml uv.lock crates/hoimin-cli/tests/run_e2e.rs tests/fixtures/projects/type-checking
git commit -m "test: cover type checker mutation runs"
```

### Task 5: 利用者向けドキュメントと完全検証を追加する

**Files:**

- Modify: `README.md`
- Modify: `docs/json-schema/run-result.schema.json`
- Modify: `docs/json-schema/run-event.schema.json`

**Interfaces:**

- Consumes: CLI and operator names from Tasks 1–4.
- Produces: 型検査専用 run の正確な使用例と、既存 JSON schema による operator string の受理。

- [ ] **Step 1: documentation/schema の期待値をテストで固定する**

`crates/hoimin-cli/tests/analyzer_handler.rs` の型 operator JSONL test を JSON output path でも通し、schema の `operator` が文字列を受けることを fixture output で確認する。
README に記載する command は Windows と Unix で同じ native argv semantics を保つ。

- [ ] **Step 2: documentation/schema の失敗を確認する**

Run: `cargo test -p hoimin-cli --test analyzer_handler`

Expected: Task 2 を省いた状態では protocol rejection。Task 2 済みなら PASS なので、README command と schema をレビュー対象として扱う。

- [ ] **Step 3: README と schema を更新する**

README の operator 節に、既定では型演算子を実行しないこと、`--operators type_nullable,type_collections -- uv run ty check`、`--exclude-operators`、型チェック失敗を `killed` と扱うことを追記する。
schema が operator を enum 化している場合だけ、7 つの型 operator ID を追加する。単なる `string` なら schema version を変えない。

- [ ] **Step 4: 全品質ゲートを実行する**

Run:

```console
cargo fmt --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
uv run maturin build --release
uv run python tests/wheel_smoke.py
```

Expected: 全コマンドが exit code 0。wheel smoke では `hoimin --version` と isolated wheel invocation が成功する。

- [ ] **Step 5: コミットする**

```console
git add README.md docs/json-schema/run-result.schema.json docs/json-schema/run-event.schema.json crates/hoimin-cli/tests/analyzer_handler.rs
git commit -m "docs: describe type annotation mutation"
```

## Self-Review

- Spec coverage: Task 1 は演算子選択と既定の互換性、Task 2 は protocol、fingerprint、resume、Task 3 は全注釈位置と非対象構文、Task 4 は ty/mypy の killed/survived、Task 5 は README、schema、全品質ゲートを担当する。
- Placeholder scan: 実装する型、関数、ファイル、コマンド、期待結果を各 task に記載した。未決定の作業は残していない。
- Type consistency: `MutationOperatorSelection` を Task 1 で定義し、Tasks 2–4 は `RunConfig::operators`、`names()`、`AnalyzeRequest::operators` を同じ名称で使う。
