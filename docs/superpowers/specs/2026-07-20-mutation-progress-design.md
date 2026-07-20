# ミューテーション進捗レポート

## 目的

`hoimin run --format json` が出力した複数の完了済みレポートを時系列で比較し、テスト改善の進捗と飽和を、開発者とエージェントが同じ基準で判断できるようにする。

この機能は、survived mutant を減らす反復作業において、改善が続いているか、後退したか、または改善が飽和したかを示す。

## 対象外

- 等価ミュータントの判定、推定、スコアからの除外
- ミューテーション実行、テスト生成、テストの自動修正
- JSONL のライフサイクルイベントを直接読むこと
- 不完全な run を成功した比較結果として扱うこと

survived は「この run のテストで検出されなかった」を意味し、等価であることを意味しない。

## CLI

新しいサブコマンドを追加する。

```console
hoimin progress REPORT [REPORT ...]
hoimin progress --patience 3 REPORT [REPORT ...]
hoimin progress --format json REPORT [REPORT ...]
```

`REPORT` は `hoimin run --format json` の単一 JSON 文書へのパスであり、指定順を古い順の時系列とする。
少なくとも二つのレポートを要求する。

オプションは次のとおりとする。

- `--patience N`: 飽和と判定する、改善なしの連続した比較回数。正の整数だけを受け付け、既定値は `3`。
- `--format human|json`: 出力形式。既定値は `human`。

このコマンドはレポートを読むだけであり、セッション DB や対象プロジェクトを変更しない。

## 入力の適格性

各入力はサポート対象の run-result schema version であることを検証する。
baseline が成功し、run が complete で、mutant status が最終状態として記録されているレポートだけを比較に使用する。

解析不能な JSON、未対応 schema、baseline failure、または incomplete な run は `unusable` としてレポートする。
`unusable` なレポートをまたぐ比較は作らず、飽和カウンタも更新しない。利用可能なレポートが二つ未満であれば、進捗および飽和は判定不能とする。

## Mutant の照合

二つの隣接する利用可能な run の mutant を、次のフィールドの組で照合する。

```text
path / original / replacement / operator / symbol
```

`id`、`sequence`、`line`、`column`、`span`、`file_hash` は照合キーに含めない。コード編集で変化し得るためである。

同一の照合キーが一つの run 内に複数あるとき、そのキーは曖昧である。曖昧キーの mutant は照合から外し、件数と警告を出力する。

照合された mutant を共通集合、古い run にだけあるものを消滅、新しい run にだけあるものを新規として集計する。

## 比較と飽和

score は共通集合に含まれる `killed` と `survived` からだけ計算する。`timeout`、`out_of_memory`、`process_limit`、`error`、`not_run` は判定不能として別集計し、score に含めない。

各隣接比較では、少なくとも次を集計する。

- `survived -> killed`（改善）
- `killed -> survived`（後退）
- 持ち越し survivor の残数
- 共通集合 score の差分
- 新規、消滅、曖昧、判定不能 mutant の件数

「改善」は `survived -> killed` が一件以上ある、または持ち越し survivor が減ることとする。改善があれば飽和カウンタを 0 に戻す。

改善も後退もない比較を「停滞」とし、飽和カウンタを 1 増やす。`killed -> survived` が一件以上ある比較は「後退」とし、停滞ではないため飽和カウンタを増やさない。比較可能な共通集合が空のときも、カウンタを更新しない。

連続する停滞回数が `--patience` 以上なら `saturated: true` とする。既定値では 3 回連続の停滞で飽和となる。

## 出力

human 形式は最新の有効比較について、共通集合 score、score 差分、改善・後退・持ち越し survivor・新規・消滅・判定不能の各件数、停滞連続回数、patience、`saturated` を表示する。`unusable` レポートと曖昧キーは警告として表示する。

JSON 形式は、入力レポートごとの利用可否、隣接比較ごとの上記集計、最新の判定、飽和カウンタと設定値を含む、バージョン付きの単一文書とする。エージェントは少なくとも次の状態を区別できる。

- `improving`: 最新の比較で改善がある。
- `regressing`: 最新の比較で後退がある。
- `stalled`: 比較可能だが改善も後退もない。
- `saturated`: 停滞回数が patience 以上である。
- `indeterminate`: 比較できる有効な隣接 run がない。

`saturated` は停滞の派生状態であり、同時に後退を示さない。

## エラーと終了状態

引数不足、存在しないパス、読み取り不能なファイル、JSON 構文エラー、未対応 schema、または `--patience` の不正値は CLI エラーとして終了する。

内容上 `unusable` なレポートは、他に比較可能な run があれば診断付きの正常な進捗レポートとして出力する。比較可能な有効 run が二つ未満の場合も `indeterminate` を出力する。進捗の状態自体は終了コードで成功・失敗を表現せず、エージェントは構造化出力の状態を使う。

## 検証

固定 JSON fixture により、少なくとも次をテストする。

- 改善でカウンタがリセットされること
- 後退が飽和として数えられないこと
- 停滞が三回連続で飽和になること
- `--patience` が既定値と上書き値の両方で効くこと
- 新規・消滅 mutant が照合集計に分離されること
- 不完全・baseline failure・未対応 schema が比較とカウンタから除かれること
- 重複照合キーが警告され、照合から除かれること
- 共通集合が空の比較がカウンタを更新しないこと
- human と JSON の状態が同じ判断を表すこと
