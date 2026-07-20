# Agent plan and candidate verification

## 目的

AI エージェントが mutation を実行する前に候補集合を機械可読で検査し、
テストを追加・修正した後に選んだ候補だけを安全に再検証できるようにする。

`run` の既存の安全制御、隔離 workspace、baseline、出力契約を再利用しつつ、
候補発見と実行を JSON manifest で分離する。人間向け UI、SQLite を使う
planning session、または source tree への mutant 適用は導入しない。

## 対象外

- GUI、TUI、HTML report、mutant を元の source tree に書き込む `apply` コマンド
- plan の SQLite 保存、`--session`、`--resume`、保存済み mutant 結果の再利用
- plan manifest の署名、暗号化、または信頼できない manifest を実行しても安全にする仕組み
- coverage による候補選択、pytest の関連テスト選択、確率的 sampling
- candidate ID が source の変更をまたいで存続する relocation や推測的な再解決
- public run-result/run-event schema version 2 の変更

`verify` は manifest に保存された test argv を実行する。これは既存の `run` と同じく
利用者が選んだ信頼できる workspace で実行する契約であり、信頼できない JSON を
実行しても安全にする境界ではない。

## CLI

### `plan`

```console
hoimin plan --root . --source src --profile focused --max-candidates 500 \
  --fingerprint-include pyproject.toml -- python -m pytest -q > PLAN.json
```

`plan` は `run` と同じ target selector、copy option、operator selector、profile、
`--fingerprint-include`、安全上限、test argv を受理する。ただし `--format`、
`--session`、`--resume` は受理しない。出力は常に stdout の単一 JSON document とし、
stderr は診断だけに使う。

`plan` は target を解決して Rust analyzer で候補を生成するが、test command、baseline、
worker、workspace copy、SQLite を実行・作成しない。候補生成は `run` と同じ profile、
operator、line/symbol、stable sort、deduplication、`--max-candidates` の順序を使う。
このため manifest の candidate descriptor は同じ入力の `run` が実行する descriptor と
一致する。

候補上限に達したときは、部分集合を含む manifest を stdout に出し exit 4 とする。
manifest の `truncated` は `true` になり、agent はその plan が完全な候補集合を表さないと
判断できる。Python 構文不正、source 読み込み、target 解決、追加 fingerprint 入力の失敗は
manifest を出さず exit 2 とする。

### `verify`

```console
hoimin verify PLAN.json --candidate mut_01 --candidate mut_02 --format json
```

- `PLAN.json` は `plan` が出力した version 1 manifest でなければならない。
- `--candidate ID` は一つ以上必須であり、同一 ID の重複は Clap ではなく正規化後に除く。
- 指定 ID はすべて manifest 内に存在し、個数が manifest の `limits.max_mutants` 以下で
  なければならない。存在しない ID や上限超過は baseline 前に exit 2 とする。
- `verify` は manifest 内の正規化 config と test argv だけを使う。target、operator、profile、
  copy、limit、test argv を CLI で上書きできない。出力形式だけは `json`、`jsonl`、`human`
  から選べ、既定値は `json` とする。これは manifest validation 後に `RunConfig.output` へ
  適用する唯一の上書きであり、fingerprint と mutation/test の意味を変えない。
- session/resume は初版では利用できない。毎回 baseline を実行し、指定 candidate のみを
  実行する。

## Plan manifest

manifest は専用の versioned data contract とする。run report の一種ではない。

```json
{
  "schema_version": 1,
  "kind": "plan",
  "normalized_config": {},
  "sources": [{"path": "src/calc.py", "hash": "..."}],
  "fingerprint_inputs": [{"path": "pyproject.toml", "hash": "..."}],
  "candidates": [],
  "truncated": false,
  "diagnostics": []
}
```

- `normalized_config` は `session` と `resume` を持たない完全な `RunConfig` である。
  `fingerprint_include` glob とその解決済み `fingerprint_inputs` を含む。
- `sources` は解決済み target file 全件の root 相対 path と BLAKE3 hash を path 順に持つ。
  candidate を持たない target file も含める。
- `fingerprint_inputs` は `--fingerprint-include` により解決された root 相対の通常ファイルと
  BLAKE3 hash を path 順に持つ。詳細は fingerprint input の仕様に従う。
- `candidates` は既存の `MutationCandidate` JSON 形をそのまま用いる。ID、file hash、span、
  original、replacement、operator、line、column、symbol、sequence を省略しない。
- `diagnostics` は candidate limit や analyzer の非致命診断を stable な code、path、line、
  column、message で表す。`truncated` が `true` のとき candidate-limit diagnostic は必須である。

manifest の candidate ID は既存どおり source hash を含む。target source を編集すればその
candidate は stale になり、古い manifest で検証できないことが意図した動作である。テストだけを
変更した場合は、target と明示的 fingerprint input が不変なら同じ candidate を再検証できる。

## `verify` の検証と実行

`verify` は worker や test process を作る前に次を順に行う。

1. JSON schema version と `kind`、必須 field、UTF-8 root 相対 path、candidate ID の形式を検証する。
2. manifest の root から解決したすべての `sources` と `fingerprint_inputs` を読み、path 集合と
   BLAKE3 hash が manifest と完全一致することを検証する。
3. 指定 candidate が manifest に一意に存在し、その descriptor の file hash、span、original text、
   line/column、stable ID を現在の source に対して再検証する。
4. 検証済みの manifest config から既存の `RunState` と `ShellContext` を作り、baseline と指定
   candidate のみを既存の worker state machine で実行する。

不一致は candidate を再探索したり別の candidate を推測したりしない。source input の不一致は
`plan.source.changed`、追加 fingerprint input は `plan.fingerprint_input.changed`、descriptor の
破損は `plan.candidate.invalid` として exit 2 にする。manifest が `truncated` でも、含まれる
candidate の検証は許可する。全 candidate を検査したことだけは主張しない。

`verify` の JSON、JSONL、human 出力は既存の `run` report contract を用いる。status、score、
終了コード、resource mode、output spool の意味は変えない。個別実行は manifest 由来であることを
新しい event field としては出さず、入力 manifest が provenance を担う。

## 実装境界

- `crates/hoimin-core/src/config.rs` は plan が共有する正規化 config と、`session`/`resume` を除いた
  plan 用 config への変換を所有する。
- `crates/hoimin-cli/src/cli.rs` は `Plan` と `Verify` subcommand、共有 run option、candidate ID と
  output format の parse だけを担当する。
- `crates/hoimin-cli/src/plan.rs` は target 解決、analysis、manifest encoding/decoding、現在 source
  との manifest validation を担当する。SQLite session handler へ依存しない。
- `crates/hoimin-cli/src/analyzer/` は既存の candidate generator を共有し、plan 専用の mutation rule を
  持たない。
- `crates/hoimin-cli/src/shell.rs` は検証済み candidate subset を state machine に渡す小さな入口だけを
  持つ。worker、copy、resource enforcement、report handler の契約を分岐させない。

## 検証

- CLI tests: `plan` の必須 selector/test argv、禁止 option、`verify` の candidate 必須・重複除去・
  出力 format を確認する。
- plan tests: full/focused、operator/line/symbol selector、candidate limit で、`plan` の descriptor 列が
  同じ config の analyzer 出力と一致することを確認する。plan 中に test command、worker copy、
  SQLite file が作られないことも確認する。
- manifest tests: schema/version/kind、空/壊れた JSON、path traversal、candidate descriptor 改竄を
  baseline 前に拒否することを確認する。
- E2E tests: test のみを更新後に一 candidate を `verify` できること、target source または
  fingerprint input の変更で baseline を起動せず拒否すること、複数 candidate の上限拒否、
  truncated plan 内の candidate 実行を確認する。
- regression: `run` と `progress` の CLI、report schema、session/resume、candidate ID を変更しない。
