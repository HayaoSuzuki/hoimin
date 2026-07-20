# Explicit fingerprint inputs

## 目的

`hoimin run --session --resume` が、mutation target 以外で test 結果に影響する入力を
明示的に fingerprint へ含められるようにする。AI エージェントは invocation に glob を
記録するだけで、設定、fixture、lockfile、SQL などの変更による古い killed/survived 結果の
再利用を防げる。

この機能は agent plan/verify manifest と同じ input record を使う。`verify` は manifest に
保存された input を再検証し、CLI による追加・除外・上書きは許可しない。

## 対象外

- `pyproject.toml`、lockfile、`conftest.py` などの暗黙的な既定入力
- Git status を使った全非 Python file の自動検出
- directory の再帰ハッシュ、symlink の追跡、root 外のファイル、環境変数、ネットワーク、
  database、コンテナ image の fingerprint 化
- copy include/exclude の意味の変更、またはコピーされるべき test input の自動補完
- existing session database の table migration

呼び出し側は test 結果に影響する入力を列挙する責任を持つ。指定されなかった入力が変わっても、
この機能は session 再利用を無効化しない。

## CLI と config

```console
hoimin run --root . --source src --session .hoimin/session.sqlite --resume \
  --fingerprint-include pyproject.toml \
  --fingerprint-include 'fixtures/**/*.json' \
  -- python -m pytest -q
```

- `--fingerprint-include GLOB` は `run` と `plan` の繰り返し可能な option とする。
- glob の基準は `--root` であり、absolute path、`..` を含む root 外への参照、NUL を含む値を
  CLI error（exit 2）として拒否する。
- glob は少なくとも一つの root 内の通常ファイルに一致しなければならない。無効な glob、空の
  展開、directory、symlink、非 UTF-8 path は exit 2 とする。
- 複数 glob が同じファイルに一致しても、正規化された root 相対 path ごとに一件だけにする。
  最終列は path 昇順とし、入力 glob の指定順や filesystem enumeration に依存しない。
- `RawRunConfig` と `RunConfig` は `fingerprint_includes: Vec<String>` を保持する。実行準備後の
  `RunConfig` は追加で `fingerprint_inputs: Vec<FingerprintInputFile>` を持つ。後者は
  `path` と BLAKE3 `hash` を持つ resolved record である。
- `verify` はこの option を受理しない。manifest 内の config と resolved record だけを使う。

`--fingerprint-include` は copy policy と独立している。hash 対象にしても worker copy を変更しない。
test に必要なファイルが copy exclude によって worker から除かれる場合は既存どおり baseline が
失敗する。利用者はそのファイルを既存の `--include` でコピー対象に戻す。

## 解決、報告、fingerprint

application は CLI config を正規化した後、`RunState` と `ShellContext` を作る前に input glob を
解決する。この準備により state machine と report handler は同じ resolved config を共有できる。

1. root から glob を解決し、regular file だけを受理する。
2. 各 file を一回だけ読み、BLAKE3 hash を計算する。
3. path/hash の昇順列を `RunConfig.fingerprint_inputs` に保存する。
4. `run_started.normalized_config` は指定 glob を `fingerprint_includes`、解決結果を
   `fingerprint_inputs` として JSON/JSONL/human の開始時情報に含める。hash だけを出し、内容は
   report に出さない。
5. `FingerprintInput` にこの resolved record 列を追加し、encoding field 8 として path/hash を
   length-prefixed にエンコードする。`FINGERPRINT_SCHEMA_VERSION` は 3 から 4 に上げる。

path/hash の列が空である場合も field 8 は空列としてエンコードする。これにより、schema 3 の run は
新しい schema 4 fingerprint と一致せず、古い incomplete session は再利用されない。新しい schema
では glob の書き方が異なっても、解決 path/hash 列が同じなら結果再利用の意味も同じである。

`plan` manifest は完全な resolved record 列を保存する。`verify` は current root で再解決した glob と
manifest record の path/hash 列が完全一致することを、baseline より前に検証する。glob がいま空に
なった場合も `plan.fingerprint_input.changed` として拒否する。

公開 run-event/run-result schema は version 2 を維持する。`normalized_config` は既存 schema で
追加プロパティを許すため、上記 field はその拡張点に入れる。既存の summary counts に追加入力の
件数や hash 変更を混ぜない。

## エラーと互換性

- `fingerprint.include.invalid_glob`: syntax、absolute path、root escape、NUL。
- `fingerprint.include.unmatched`: glob が file を一件も選ばない。
- `fingerprint.include.unsupported_file`: directory、symlink、非 UTF-8 path、read failure。
- `plan.fingerprint_input.changed`: `verify` が manifest と異なる解決列を検出した。

通常の `run` では、input 解決の失敗は baseline と SQLite session 作成の前に exit 2 にする。
`--session` を使わない run でも同じ validation と report 表示を行うため、後から session を追加しても
入力の意味が変わらない。

schema 4 による resume 不互換性は意図した安全側の変更である。SQLite の table migration は不要で、
新しい `fingerprints` row が schema version 4 と新しい digest を保存する。candidate ID、mutation
status、score、終了コードは変更しない。

## 実装境界

- `crates/hoimin-core/src/config.rs` は raw/normalized include glob と
  `FingerprintInputFile` value object を所有する。
- `crates/hoimin-cli/src/cli.rs` は `run`/`plan` の option parse を担当する。
- `crates/hoimin-cli/src/fingerprint_inputs.rs` は root 内の glob 解決、file validation、hash、
  stable sort/dedup を担当する。target resolver や workspace copy handler へ副作用を持たない。
- application startup は resolved records を含む config を `RunState` と `ShellContext` の両方に渡す。
- `crates/hoimin-core/src/resume.rs` は schema 4 の encoding、`crates/hoimin-cli/src/shell.rs` は
  `FingerprintInput` の組み立てだけを担当する。
- agent plan/verify は同じ resolver と value object を使い、別の glob semantics を実装しない。

## 検証

- core config/fingerprint tests: 空列の安定性、path/hash の順序・重複除去、追加・変更・削除で digest が
  変わること、schema 4 を確認する。
- resolver tests: root 相対 glob、複数一致、重複一致、空一致、absolute/root escape、symlink、
  directory、read error を確認する。
- CLI tests: repeated option の parse、`plan` との共有、`verify` での拒否を確認する。
- E2E session tests: 同一 input で incomplete run を resume し、watch 対象の変更・追加・削除で
  新しい run を始めることを確認する。非指定 file の変更は fingerprint を変えないことも確認する。
- report tests: JSON、JSONL、human に pattern と path/hash が安定して現れ、schema version 2 の
  validator を通ることを確認する。
- plan/verify tests: manifest に resolved input が保存され、変更後の `verify` が baseline/test process を
  一切起動せず `plan.fingerprint_input.changed` で失敗することを確認する。
