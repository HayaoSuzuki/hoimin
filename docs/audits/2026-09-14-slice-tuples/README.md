# 追加監査: 多次元スライスのtuple-to-list変換

> 保存時の位置づけ（2026-09-15）: 以下は修正前の `f110135` に対する監査記録である。#545〜#549は修正済み。現在のテストとの対応は[監査結果と正式な回帰検証](../../knowledge/audits/analysis-2026-09.md)を参照。
対象HEAD: `f11013542ccd735ab9741b5079c0b39a517df256`。2026-09-14、macOS arm64、CPython 3.14.7、Lean 4.32.2。

[Issue #549](https://github.com/tokyogas-tech/hoimin/issues/549)を追加した。`x[:,]`をcollection_list_tupleで `x[[:,]]`へ変換する不具合である。元コードは有効だが変異後はSyntaxErrorになる。前回の例外経路・名前解決とは別の、ASTの構文種別の取り違えだった。実装修正は行っていない。

## 契約と反例

主張は、リストへの変換では全要素がリスト内で合法な式であること。RuffのExprTupleには多次元添字のslice要素も入るが、`collect_tuple_literal`はそれを通常のtuple expressionとして扱い、ソースを角括弧で包む。[事前対応表](correspondence.md)と[原因・再現手順](issue.md)を保存した。

最小反例は `[slice]`、Pythonでは `def f(x): return x[:,]`。期待候補0、実際1。`x[1:2, 3]`と`x[:, :]`も不正なリストへ変換した。通常の式だけの `x[1, 2]` はコンパイルできる変異になるため、全tuple候補の無条件抑制は適切でない。

[実run](run.json)では、テストを `import subject` だけにしてもbaseline成功、killed=1、score=1.0、complete=true、CLI exit 0になった。fの振る舞いを検査せずに、不正候補のSyntaxErrorだけでスコアが上がる。classificationは **confirmed bug**。

## Leanと実装の対応

[SliceTupleModel.lean](SliceTupleModel.lean)の要素種別はscalar/sliceの2種類。全要素が式である場合だけeligibleとする。eligibleなら全要素がscalarである定理と、slice先頭の拒否、全tupleを許可する壊した述語の固定反例をカーネル検査した。

[SliceTupleMain.lean](SliceTupleMain.lean)はimportされない実行入口。長さ1、2、3を順番に列挙する。2+4+8=14ケースで、壊したモデルとの差は11件。最小反例は長さ1のslice。意味上の対称性削減は行っていない。これは有限の構文集合の確認であり、任意の実行履歴を探索したものではない。中間状態はなく、全内部実行ステップ数も計数していない。

[corpus.jsonl](corpus.jsonl)のソースと期待候補数はLean生成。各ソースを公開plan CLIへ入力し、raw spanとoriginalを確認して変異を適用し、CPython3.14でcompileした。

| 実行 | 正例一致 | 不適切な候補 | 変異後のSyntaxError |
| --- | ---: | ---: | ---: |
| [release](release.json) | 3 | 11 | 11 |
| [debug](debug.json) | 3 | 11 | 11 |

各binaryで全14の元ソースはcompile成功。各3正例は変異後もcompile成功。全14件はstrictで、期待候補数を照合した。候補descriptorとcompile観測を保存したが、全descriptorをLeanの独立期待値と照合したとは扱わない。実装照合のない未知ケースをstrictへ混ぜていない。新規internal-fixtureはない。

感度は、sliceを許可する壊した述語で検査した。通常のscalarは許可される正例を残し、全拒否の検査になっていないことを確認した。モデルにtransactionや永続IDはないため、その原子性・重複の感度検査は対象外。検査対象の境界は式とsliceの区別である。

## 既存検証

`cargo test --offline -p hoimin-cli --test valid_python_corpus` は **206 passed、0 failed、3 ignored**、7.61秒。このtargetには解析器内部テストも含まれる。既存のコーパスと内部テストが通っても、今回のExprTuple×Sliceの組合せは検出できなかった。#489の検証基盤へこの入力軸を追加することをIssueに記載した。

## 再現コマンドと資源

リポジトリrootから実行する。既存の同HEADのdebug/release binaryと `.venv/bin/python` を使う。Leanの出力ディレクトリは未使用の絶対パスを指定する。

```sh
python3 docs/audits/2026-09-14-slice-tuples/verify_lean.py --output /tmp/hoimin-slice-recheck
python3 docs/audits/2026-09-14-slice-tuples/replay.py --binary target/release/hoimin --output /tmp/hoimin-slice-release.json
python3 docs/audits/2026-09-14-slice-tuples/replay.py --binary target/debug/hoimin --output /tmp/hoimin-slice-debug.json
python3 docs/audits/2026-09-14-slice-tuples/run_repro.py
cargo test --offline -p hoimin-cli --test valid_python_corpus
```

verify_lean.pyは一時ディレクトリにoleanとcorpusを生成して既存corpusと比較する。初回にcorpusがなければLean出力を保存する。実際に生成後の再実行でも鮮度一致を確認した。run_repro.pyは監査ディレクトリのrun.jsonを今回の観測で更新する。

Leanは各20秒・2GiBの既存guard、50msのRSS監視、1スレッドで直列に実行した。既定のheartbeat制限を残し、native_decideや無制限探索は使っていない。[モデル統計](lean-model.json)、[生成・列挙統計](lean-search.json)、[列挙ログ](lean-search.log)に実測を保存した。より大きな探索は行っていない。

生成器の初回コンパイルに、`.scalar`の型推論エラーがあった。`Item.scalar`と明示して解消した。これはinfrastructure errorであり、製品の反例には数えない。修正後の証明、コーパス生成、鮮度、実装照合は成功した。

## 範囲と残る判断

モデルは通常の整数式とcolonのみのsliceを含む、長さ1〜3の添字タプルに限定した。任意のPython式、starred要素、代入先、全operator、OSや非同期実行は証明していない。start/stop/step付きの例は最初の手動再現で確認したが、今回のLean生成14件には含めていない。

追加で読んだmethod引数の変換にはgenerator等を除外する処理があり、今回の確認だけでは別のIssueへする根拠がなかった。今回新たな性能実測は行っていない。

#549ではslice要素を通常のtupleから区別する修正と、合法なtuple候補を維持する回帰試験が必要になる。対象判定をどの層へ置くかは未決定。全コードの無欠陥を示す監査ではない。
