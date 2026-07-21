# Verify JSON `complete` Semantics Design

## 背景

Issue #15 では、`verify` の最終 JSON に判定不能な mutant が含まれ、終了コードが
`4` であるにもかかわらず、`complete: true` が出力される。

現在、`RunSummary.complete` は run 全体の打ち切り、インフラ障害、中断だけから
計算される。一方、終了コードはそれらに加え、mutant の集計結果に含まれる
`timeout`、`out_of_memory`、`process_limit`、`not_run` も不完全と判定する。この二つの
判定経路の違いが矛盾の原因である。

## 目的

最終 JSON の `complete` を、選択されたすべての候補について結論が得られたことを示す
機械可読な値にする。終了コード `4` となる不完全な結果では、`complete` も必ず
`false` にする。

## 非目標

- `execution_complete` や `all_candidates_conclusive` などの新しいフィールドは追加しない。
- JSON の構造や schema version は変更しない。
- mutant timeout の決定方法や Windows の Job Object 処理は変更しない。
- 終了コードの優先順位や各 mutant status の分類は変更しない。

## 公開契約

`complete` は、run が中断なくスケジューリングを終えたことだけではなく、選択された
すべての mutant が `killed` または `survived` の判定可能な状態に到達したことを表す。

次のいずれかを満たす場合、`complete` は `false` になる。

- `timeout`、`out_of_memory`、`process_limit`、`not_run` が1件以上ある。
- mutant の `error`、またはその他のインフラ障害が発生した。
- baseline が失敗した。
- ユーザーによる中断が発生した。
- candidate や mutant の上限、total timeout などにより run が打ち切られた。

`killed` と `survived` だけで構成され、上記の run-level failure がない場合は
`complete: true` になる。候補が0件の正常な run も `complete: true` のままとする。

既存の `complete` フィールドの意味を終了コード表および session の complete 状態と
一致させる修正であり、フィールドの追加・削除はない。そのため
`REPORT_SCHEMA_VERSION` と JSON Schema の `const` は `2` のままとする。

## 設計

`RunState` に、現在の outcome flags と `MutationSummary` から run の不完全性を一度だけ
定義する内部判定を設ける。この判定は既存の `ExitPolicy::from_summary` が持つ status
分類を再利用し、次の利用箇所で共有する。

- `RunSummary.complete`
- `FinishSession.complete`
- 終了コードの incomplete 入力

これにより、JSON レポート、session の再開可否、プロセス終了コードが同じ完全性判定を
参照する。公開型や CLI オプションは追加しない。

終了コードの優先順位は変えない。たとえば mutant `error` は引き続き終了コード `2` を
優先するが、`complete` は `false` になる。timeout を含む場合は
`complete: false` かつ終了コード `4` になる。

## データフロー

1. mutant の完了イベントが `MutationSummary` に status を記録する。
2. 最終化時に共通判定が run-level flags と summary-derived policy を統合する。
3. 同じ判定結果を session 完了状態と最終 `RunSummary.complete` に使用する。
4. 終了コードは同じ incomplete 状態を、既存の優先順位規則に入力する。

## テスト

状態機械のテストに、mutant timeout を完了まで遷移させる回帰ケースを追加する。最終
`RunSummary` が `complete: false` と `exit_code: 4` を同時に持ち、session を使用する場合は
`FinishSession.complete` も `false` になることを確認する。

既存テストまたは追加の表形式テストで、次を確認する。

- `killed` と `survived` のみ: `complete: true`
- `timeout`、`out_of_memory`、`process_limit`、`not_run`: `complete: false`
- mutant `error`: `complete: false`、終了コード `2`
- 候補0件の正常終了: `complete: true`

README の JSON 出力説明に `complete` の定義を追記し、終了コード `4` との対応を明記する。
JSON Schema は構造を変えず、既存の schema validation tests で version 2 の互換性を維持する。

## 完了条件

- Issue #15 の例に相当する timeout/inconclusive 結果が `complete: false` を返す。
- `complete: true` と終了コード `4` の組み合わせが生成されない。
- session と最終 JSON が同じ完全性を記録する。
- 公開 JSON schema version は `2` のままである。
- Rust workspace のテスト、format、lint が成功する。
