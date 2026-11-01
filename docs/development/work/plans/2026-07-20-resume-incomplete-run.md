# 不完全 run の再開 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** `--max-mutants` 到達後の不完全 session run を、同じ上限で何度再開しても終了エラーなく再開候補として残す。

**Architecture:** SQLite の `complete` を再開可否の唯一の状態とする。`SessionHandler::finish(false)` は `complete=0` の run に対して冪等に終了記録を行い、完全化する `finish(true)` は従来どおり一回だけ成功させる。状態機械の effect と SQLite schema は変更しない。

**Tech Stack:** Rust 2024、rusqlite、Tokio、Cargo integration tests。

## Global Constraints

- `--resume` は最新の互換 `complete=0` run だけを選ぶ。
- `killed` と `survived` だけを再利用し、他の結果は再実行する。
- `complete=1` の run は再開候補にせず、追加の終了処理を受け付けない。
- SQLite の schema と公開 CLI のオプションは変更しない。

---

## File structure

- `crates/hoimin-cli/src/session/mod.rs`：SQLite の run 終了状態を更新する `SessionHandler::finish` を実装する。
- `crates/hoimin-cli/tests/session_handler.rs`：session handler の不完全終了と完全終了の状態遷移を検証する。
- `crates/hoimin-cli/tests/run_e2e.rs`：複数候補を持つ実プロジェクトで、上限終了後の繰り返し再開を検証する。

### Task 1: 不完全終了を冪等にする SQLite 状態遷移

**Files:**
- Modify: `crates/hoimin-cli/tests/session_handler.rs:196-236`
- Modify: `crates/hoimin-cli/src/session/mod.rs:275-306`

**Interfaces:**
- Consumes: `FinishSession { id: EffectId, run_id: String, complete: bool }`。
- Produces: `SessionHandler::finish(FinishSession) -> Result<SessionFinished, EffectFailed>`。
- Invariant: `complete=0` は再開可能、`complete=1` は完了済みである。

- [ ] **Step 1: 状態遷移の失敗テストを変更する**

`finish_state_table_allows_false_to_true_only_once` を `incomplete_finish_is_idempotent_but_completion_is_final` に改名する。
最初の `finish(false)` の直後に、同じ run への二度目の `finish(false)` が成功することを追加する。

```rust
handler
    .finish(FinishSession {
        id: EffectId(3),
        run_id: "partial".to_owned(),
        complete: false,
    })
    .unwrap();

handler.finish(finish_request(4, "partial")).unwrap();
assert_eq!(
    handler
        .finish(finish_request(5, "partial"))
        .unwrap_err()
        .failure
        .code(),
    "session.finish.state"
);
assert_eq!(
    handler
        .finish(FinishSession {
            id: EffectId(6),
            run_id: "partial".to_owned(),
            complete: false,
        })
        .unwrap_err()
        .failure
        .code(),
    "session.finish.state"
);
```

- [ ] **Step 2: テストが現在の実装で失敗することを確認する**

Run: `cargo test -p hoimin-cli --test session_handler incomplete_finish_is_idempotent_but_completion_is_final`

Expected: `finish(false)` の二度目が `session.finish.state` を返すため FAIL。

- [ ] **Step 3: `finish(false)` の SQL 条件を最小変更する**

`SessionHandler::finish` の `else` 側を次の SQL に置き換える。
`finished=0` を条件から外すことで、不完全 run の再度の終了を成功させる。

```rust
"UPDATE runs SET finished=1, complete=0 WHERE run_id=?1 AND complete=0"
```

`complete=true` 側の SQL、`changed != 1` のエラー、`SessionFinished` の戻り値は変更しない。

- [ ] **Step 4: handler テストを通す**

Run: `cargo test -p hoimin-cli --test session_handler`

Expected: PASS。二度目の不完全終了は成功し、完全終了後の二度目の終了は `session.finish.state` を返す。

- [ ] **Step 5: Task 1 をコミットする**

```console
git add crates/hoimin-cli/src/session/mod.rs crates/hoimin-cli/tests/session_handler.rs
git commit -m "fix: keep incomplete sessions resumable"
```

### Task 2: 上限到達後の繰り返し再開を E2E で固定する

**Files:**
- Modify: `crates/hoimin-cli/tests/run_e2e.rs:23-60`
- Modify: `crates/hoimin-cli/tests/run_e2e.rs:735-790`

**Interfaces:**
- Consumes: `hoimin_cli::run_with_io` と `--session PATH --resume --max-mutants 1`。
- Produces: `sqlite_session_can_be_resumed_after_repeated_mutant_limits()`。
- Invariant: 各実行は終了コード 4、同じ SQLite run は `complete=0`、stderr に `session.finish.state` を含まない。

- [ ] **Step 1: 複数候補プロジェクト用の session 実行 helper を追加し、E2E テストを書く**

`run_project` と同じ引数組み立てを使い、`root`、`session`、`resume`、`max_mutants`、`command` を受ける `run_project_with_session` を追加する。
`write_parallel_project` が作る加算式には少なくとも四つの候補があるため、`max_mutants=1` にする。

```rust
#[tokio::test]
async fn sqlite_session_can_be_resumed_after_repeated_mutant_limits() {
    let project = tempfile::tempdir().unwrap();
    let database = project.path().join("session.sqlite3");
    write_parallel_project(project.path());
    let command = "from src.calc import total; assert total(1, 2, 3, 4, 5) == 15";

    for resume in [false, true, true] {
        let run = run_project_with_session(project.path(), &database, resume, 1, command).await;
        assert_eq!(run.exit_code, 4, "stderr={}", run.stderr);
        assert!(!run.stderr.contains("session.finish.state"));
        let connection = rusqlite::Connection::open(&database).unwrap();
        let complete: i64 = connection
            .query_row("SELECT complete FROM runs", [], |row| row.get(0))
            .unwrap();
        assert_eq!(complete, 0);
    }
}
```

helper では `--session` を `--` の前に追加し、`resume` が true のときだけ `--resume` を追加する。
`--max-mutants` と `--allow-best-effort-memory` を同じ位置に入れ、`run_project` と同じ JSON の解析と `FixtureRun` の組み立てを使う。

- [ ] **Step 2: 変更前の E2E テストが失敗することを確認する**

Run: `cargo test -p hoimin-cli --test run_e2e sqlite_session_can_be_resumed_after_repeated_mutant_limits -- --nocapture`

Expected: 二回目の `--resume` 実行が終了コード 2 になり、stderr に `session.finish.state` を含むため FAIL。

- [ ] **Step 3: Task 1 の修正を含めて E2E テストを通す**

Run: `cargo test -p hoimin-cli --test run_e2e sqlite_session_can_be_resumed_after_repeated_mutant_limits -- --nocapture`

Expected: PASS。3 回とも終了コード 4、`runs.complete=0`、`session.finish.state` なし。

- [ ] **Step 4: 関連する crate テストと workspace 全体を検証する**

Run: `cargo test -p hoimin-cli --test session_handler && cargo test -p hoimin-cli --test run_e2e && cargo test --workspace`

Expected: すべて PASS。

- [ ] **Step 5: Task 2 をコミットする**

```console
git add crates/hoimin-cli/tests/run_e2e.rs
git commit -m "test: cover repeated resume after mutant limit"
```

## Self-review

- 仕様の不完全 run、完全 run、再開候補、同一上限での再終了を Task 1 と Task 2 が網羅する。
- SQL の変更は `finish(false)` の条件だけであり、schema、状態機械、CLI には変更を加えない。
- Task 2 の 3 回実行は、初回の上限終了、最初の再開での再終了、さらに次の再開を一つのテストで確認する。
