# ルート相対の完全一致 fingerprint file

## 目的

`--fingerprint-include pyproject.toml` は basename の glob として解釈されるため、project root
直下だけでなく `.worktrees/a/pyproject.toml` のような任意階層の同名 file にも一致する。
`run` と `plan` に `--fingerprint-file PATH` を追加し、`--root` からの相対 path で指定した
通常 file 一件だけを fingerprint 対象にできるようにする。

この変更は既存の `--fingerprint-include GLOB` の意味を変えない。glob を必要とする既存利用者との
互換性を保ちながら、nested worktree の無関係な変更で plan が stale になる問題を解消する。

## CLI 契約

```console
hoimin plan --root . --file src/example.py \
  --fingerprint-file pyproject.toml \
  -- python -m pytest -q tests/test_example.py > PLAN.json
```

- `--fingerprint-file PATH` は `run` と `plan` の繰り返し可能な option とする。
- `PATH` は `--root` 基準の相対 path であり、glob metacharacter を展開しない。
- `pyproject.toml` は `<root>/pyproject.toml` 一件だけを選び、nested worktree 内の同名 file は
  選ばない。
- `verify` はこの option を受理しない。plan manifest に保存された正規化 config と解決済み
  fingerprint input を使う。
- `--fingerprint-file` は `--fingerprint-include` と併用できる。両方が同じ file を選んだ場合は、
  正規化された root 相対 path ごとに一件へ deduplicate する。
- `--fingerprint-file` は workspace copy policy と独立している。worker にも file が必要な場合、
  利用者は既存の `--include` を別途指定する。

## path の検証と解決

完全一致 path は filesystem traversal や glob matcher を使わず、root に直接 join して解決する。
以下は baseline、session 作成、plan の candidate discovery より前に CLI error（exit 2）とする。

- 空文字、NUL、absolute path
- `/` または `\\` で区切った `..` component
- Windows drive prefix や UNC path を含む root 相対でない入力
- root 外への解決
- 存在しない path
- directory、symlink、その他の非 regular file
- 非 UTF-8 path または読み取り失敗

`.` component と path separator は lexical に正規化し、manifest と report には `/` 区切りの
root 相対 path を保存する。入力文字列は provenance と再検証のため `fingerprint_files` に保持する。
ファイル名に `*`、`?`、`[` などが含まれる場合も文字どおりの名前として扱う。

完全一致固有の失敗は、既存 glob error と区別できる次の code を使う。

- `fingerprint.file.invalid_path`: root 相対の安全な path として解釈できない。
- `fingerprint.file.not_found`: 指定した path が存在しない。
- `fingerprint.file.unsupported_file`: regular file でない、UTF-8 でない、または読み取れない。

## config、report、manifest

`RawRunConfig`、`RunConfig`、`PlanRunConfig` に `fingerprint_files: Vec<String>` を追加する。
既存の `fingerprint_inputs: Vec<FingerprintInputFile>` は glob と完全一致 file の共通解決結果として
引き続き使用する。

application preparation は次の順序で一度だけ入力を解決する。

1. `fingerprint_includes` を既存の glob semantics で path 列へ解決する。
2. `fingerprint_files` を完全一致 semantics で path 列へ解決する。
3. 両方の path を順序付き map に統合し、同じ path を一件にする。
4. 各 file を一度ずつ読み、BLAKE3 hash を持つ安定順の `fingerprint_inputs` を config に設定する。

同じ path をglobと完全一致の両方で選んでも内容は同一時点に一度だけ読み取る。resolverの公開境界を
統合し、選択 path をdeduplicateしてからhashすることで、二重readと解決途中の不整合を避ける。

JSON、JSONL、human report の normalized config には `fingerprint_files` と既存
`fingerprint_inputs` を含める。human report は完全一致指定がある場合にその path 一覧を表示する。
file内容は出力しない。

plan manifest は `normalized_config.fingerprint_files` と共通の `fingerprint_inputs` を保存する。
`verify` は manifest の `fingerprint_includes` と `fingerprint_files` を現在の root で再解決し、
path/hash の共通 record 列を比較する。追加、削除、内容変更、file種別変更は baseline より前に
`plan.fingerprint_input.changed` で拒否する。

## fingerprint と互換性

session fingerprint と plan verification は既存の解決済み `fingerprint_inputs` を入力にしているため、
fingerprint schema version は変更しない。同じ path/hash 列を得る指定は、globか完全一致かにかかわらず
同じ実行互換性を持つ。

一方、元の指定方法は normalized config とplan manifestに保持する。これによりreportの説明可能性と
`verify` 時の同じselectorによる再解決を保証する。`fingerprint_files` には明示的なserde defaultを
設定し、既存manifestでは空列として読み取って従来どおりglobだけを再検証する。

公開 run-event/run-result schema version と plan manifest version は変更しない。normalized config は
追加propertyを許容し、古いmanifestとの読み取り互換性を維持できるためである。

## 実装境界

- `crates/hoimin-core/src/config.rs`: raw/normalized/plan config の `fingerprint_files` を所有する。
- `crates/hoimin-cli/src/cli.rs`: `run`/`plan` の繰り返し option をparseする。
- `crates/hoimin-cli/src/fingerprint_inputs.rs`: glob selector と exact-file selector の検証、統合、
  path単位のdeduplicate、hashを担当する。
- `crates/hoimin-cli/src/shell.rs`: run preparation で統合resolverを呼ぶ。
- `crates/hoimin-cli/src/plan.rs`: manifestへの保存とverify時の再解決を行う。
- `crates/hoimin-cli/src/report/human.rs` と `README.md`: provenance表示と利用方法を文書化する。

target discovery、workspace copy、mutation state machine、session storage tableには変更を加えない。

## 検証

- CLI parse: `run` と `plan` が複数の `--fingerprint-file` を保持し、`verify` は拒否する。
- exact resolver: root直下とnested path、literal glob metacharacter、separator正規化、重複、存在しない
  path、absolute/parent/drive/UNC、directory、symlink、read failureを確認する。
- regression: `--fingerprint-file pyproject.toml` がnested worktreeの同名fileを選ばないことを再現fixtureで
  確認する。
- merge: globとexact指定が同じfileを選ぶ場合に一件だけhash/reportされることを確認する。
- run/session: exact fileの変更でsession fingerprintが変わり、nested同名fileの変更では変わらない。
- plan/verify: manifestに指定と解決結果が保存され、exact fileの変更はbaseline前に拒否される一方、
  nested同名fileの変更はverificationに影響しない。
- report/docs: JSON、JSONL、humanのprovenanceとREADMEのCLI契約を確認する。
- 全体: format、lint、workspace testを実行する。

## 対象外

- `--fingerprint-include` のglob semantics変更
- root anchoringを表す新しいglob記法
- directoryの再帰hash、symlink追跡、root外file
- fingerprint対象のworkerへの自動copy
- 暗黙的な`pyproject.toml`やlockfileの検出
