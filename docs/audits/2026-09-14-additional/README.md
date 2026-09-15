# 追加監査: 型注釈の具体型名の解決

> 保存時の位置づけ（2026-09-15）: 以下は修正前の `f110135` に対する監査記録である。#545〜#549は修正済み。現在のテストとの対応は[監査結果と正式な回帰検証](../../knowledge/audits/analysis-2026-09.md)を参照。
対象HEAD: `f11013542ccd735ab9741b5079c0b39a517df256`。2026-09-14、macOS arm64、CPython 3.14.7、Lean 4.32.2。前回の#545〜#547に加え、[Issue #548](https://github.com/tokyogas-tech/hoimin/issues/548)を起票した。実装修正はしていない。

## 結果

型注釈のlist/Sequence、set/AbstractSet、dict/Mappingの置換は、具体型名が組込み型を参照することを前提とする。しかし `KnownImports::resolved_name` は登録のない名前を綴りへ戻し、`collection_replacements` はその文字列を具体型とみなす。逆方向のreplacementも組込み名の束縛を確認していない。

例えば `from typing import Sequence; list = tuple` の後に `Sequence[int]` の注釈を書くと、`list[int]`へ置換する候補を出すが、その値はtuple[int]になる。`def record[list](value: Sequence[int]): pass` でも同じ候補を出し、変異後の `record.__annotations__` はTypeVarのsubscriptionでTypeErrorになる。コンパイルは成功するため、注釈の評価まで観測した。

この問題は例外経路もfinallyも不要であり#545と原因が異なる。#486が直したruntime builtin callのscope解決とも別経路である。同じ原因で生じる3型pair・両方向の問題を#548にまとめた。詳しい再現、ソース位置、受け入れ条件は[Issue本文](issue.md)を参照。

## 主張とモデルの境界

[モデル化前の対応表](correspondence.md)を作成した。主張は、具体型側の名前がその注釈のscopeでbuiltinと確定している場合だけ置換を許可すること。抽象型側のtyping importは正しく利用できる前提とし、import失敗・monkeypatch・動的namespaceはモデルに含めない。

[BuiltinAnnotationModel.lean](BuiltinAnnotationModel.lean)はbuiltin/shadowed/unknownの3値を持つ有限述語を定義する。許可ならbuiltinである定理、shadowed/unknownの拒否、綴りだけを信頼する壊したモデルの固定反例をカーネル検査した。状態遷移や任意長の実行履歴をモデル化したものではない。

[BuiltinAnnotationMain.lean](BuiltinAnnotationMain.lean)はライブラリからimportされない実行入口。3状態×2方向×3型pairの18述語ケースを列挙し、壊したモデルとの差12件を確認した。各方向・pairは同じeligibility規則を共有する。この数を12件の独立した製品バグとは数えない。

最小反例はshadowedという1状態。期待false、綴りだけを信頼するモデルはtrue。中間状態はない。分類は **confirmed bug**。判明した欠落を正例へ合わせて弱めず、正しい述語と誤った観測を別に残した。

感度はshadowed/unknownを許可する壊した述語で確認し、builtinの正例も検査した。transaction・永続ID・retryはこのモデルにないため、原子性・ID重複に関する壊した遷移は対象外。境界はbuiltinとshadowed/unknownの判別である。

## 実装との対応

Leanが[corpus.jsonl](corpus.jsonl)のソースと期待候補数を生成した。手編集していない。各型pairについて通常の2方向、module shadowの2方向、genericのdestination shadowを含む計15fixtureを、公開plan CLIに適用した。

- release: **6 match / 9 mismatch**。[全観測](release.json)
- debug: **6 match / 9 mismatch**。[全観測](debug.json)
- 全元ソースのCPython注釈評価は成功。各生成候補のraw span/originalを確認した後、変異を適用して注釈を評価した。
- genericの3種類ではTypeError、module shadowの6種類では別の型を観測した。正常6種類は本来の型pairを観測した。

比較対象はLeanが生成した候補数。全descriptorとPython観測は証拠として保存するが、全descriptorの独立したLean期待値を照合したとは扱わない。15fixtureはstrict、unknownの具体的な実装入力を用意していない部分はmodel-only。新規internal-fixtureはない。

既存 `lean_annotation_scope_oracle` 4件と `type_parameter_bindings` 8件、計12テストが成功した。既存テストの成功と今回の9つの負例は両立しており、検証範囲の不足を示す。

## 再現コマンド

リポジトリrootから実行する。Pythonは `.venv/bin/python` の3.14を使う。出力ディレクトリは新規パスを指定する。

```sh
python3 docs/audits/2026-09-14-additional/verify_lean.py --output /tmp/hoimin-additional-recheck
python3 docs/audits/2026-09-14-additional/replay.py --binary target/release/hoimin --output /tmp/hoimin-additional-release.json
python3 docs/audits/2026-09-14-additional/replay.py --binary target/debug/hoimin --output /tmp/hoimin-additional-debug.json
cargo test --offline -p hoimin-cli --test lean_annotation_scope_oracle --test type_parameter_bindings
```

verify_lean.pyは一時ディレクトリにoleanとcorpusを生成し、保存したcorpusとの鮮度を検証する。初回のみcorpusがなければLeanの出力を保存する。Lean処理は直列、各20秒・2GiB、50msのRSS監視、1スレッド、既定のheartbeat制限を維持した。native_decideや無制限探索は使っていない。

最終実行: モデル 2942ms / 667552KiB、述語検査・corpus生成 458ms / 678384KiB。[モデル統計](lean-model.json)、[列挙統計](lean-search.json)、[列挙ログ](lean-search.log)。この追加Lean検証にはtimeout・OOM・setup失敗はなかった。

## 残る範囲

追加で読んだplan/verifyのdescriptor検証、JSONLのsequence検証、source encodingのoffset処理からは、今回新たに裏付けられたIssueは得ていない。それら全体の無欠陥を証明したとは扱わない。今回新たな性能測定は行わず、前回の#546/#547が性能面の継続課題となる。

#548の修正では、型注釈におけるscopeの違いを維持したprovenance検証が必要になる。既存のruntime解決器をそのまま流用できるかは未決定。全候補を抑制して正例を失わないことをIssueの受け入れ条件に含めた。
