---
type: Decision
title: Python解析・変異候補と入力規模
description: 構文・名前解決・変更するバイト範囲・候補保持上限を別々の契約として整理する。
status: draft
catalog_revision: a7daea0b557cd435c1e55b540392fbdd116348e1
sources:
- id: operators
  resource: ../../superpowers/specs/2026-08-06-collection-and-structural-mutation-operators-design.md
  revision: a7daea0b557cd435c1e55b540392fbdd116348e1
  working_tree: clean
- id: implementation
  resource: ../../../crates/hoimin-cli/src/analyzer/rust.rs
  revision: a7daea0b557cd435c1e55b540392fbdd116348e1
  working_tree: clean
- id: operator-report
  resource: ../../superpowers/reports/2026-09-08-python-operator-coverage.md
  revision: a7daea0b557cd435c1e55b540392fbdd116348e1
  working_tree: clean
- id: span
  resource: ../../superpowers/reports/2026-08-15-lean-byte-span-preservation-audit.md
  revision: a7daea0b557cd435c1e55b540392fbdd116348e1
  working_tree: clean
- id: bounded
  resource: ../../superpowers/reports/2026-08-14-lean-bounded-candidate-discovery-audit.md
  revision: a7daea0b557cd435c1e55b540392fbdd116348e1
  working_tree: clean
- id: readme
  resource: ../../../README.md
  revision: a7daea0b557cd435c1e55b540392fbdd116348e1
  working_tree: clean
---

# 構文と名前解決の契約

コレクション・構造変異の設計では、既存の演算子ID、対象選択、候補順序、重複除去を保ちながら変異対象を増やす。組込み関数と同じ名前が別の値へ再束縛されている場合、その名前を組込み関数として変異しないことも条件に含む。名前の参照先が不確かな場合には、候補を保守的に除外する。[^operators]

現在のRust解析器はRuffの構文解析器を利用する。トークン（演算子や識別子などの字句）、抽象構文木（AST）、型注釈に関する候補を生成し、保持数を計測するための構造を持つ。[^implementation]

# 候補数と解析メモリの上限

候補件数を制限しても、解析に使うメモリ全体の上限にはならない。ソース本文、トークン列、ASTなどの大きさは入力サイズに依存するためである。READMEもこの適用範囲を明記している。[^readme]

変異候補を検証する際は、次の対象を分けて調べる。各行は異なる資料で扱われた契約であり、全組合せを一つの試験で確認したものではない。[^operator-report][^span][^bounded][^readme]

| 契約 | 確認する対象 |
| --- | --- |
| 構文・名前解決 | 候補位置の構文上の役割と、同名の別の束縛を誤って変異しないこと |
| 変更範囲（byte span） | UTF-8境界、元バイト列、行・列、候補ID、書込み前の再検証 |
| 意味の変化 | 元プログラムと変異後のプログラムをCPythonで実行した結果 |
| 候補保持 | 所定の順序で先頭から保持する件数、上限超過の通知、対象間の残予算と連番 |
| 入力規模 | 長い1行、ASTの深さ、名前の束縛数などによる時間・一時メモリの増加 |

# 候補の適用と意味を調べた監査

変更範囲の監査では、元バイト列が正しくても行・列やIDが不正な候補を、テスト実行用コピーの操作が受け入れる問題を見つけた。報告には、書込み前に共通の候補検証処理と正規のIDを確認する修正が記録されている。[^span]

候補保持の監査では、定義した順序・重複除去に従う先頭の候補集合と、上限で打ち切ったことの通知をモデル化した。前提を明示してRust実装との対応を調べているが、解析メモリ全体や任意のPython構文の意味は証明対象に含めていない。[^bounded]

意味の変化については、9月8日の演算子報告にCPythonでの実行結果がある。公開コマンド `plan` の候補を適用する8テスト・23動作ケースを確認し、意図的に壊した3種類の変異も検出した。実行環境は当時のmacOS arm64・CPython 3.14.7である。[^operator-report]

# 後続監査と再確認条件

[9月11日境界監査](../audits/boundary-2026-09.md)では、構文解析成功とPythonのコンパイル成功の差、ジェネリック型パラメータが有効な範囲、入力形状別の規模検証を追加課題に挙げた。先行報告で試験した入力と条件が異なるため、先行する成功結果だけではこれらの課題を判断できない。

演算子、名前解決、変更範囲の検証、候補順序、保持構造を変更したら、対応する監査と実装比較用のテストを再確認する。原文の演算子数は報告時点の数値として読み、現行一覧はREADMEと実装を照合する。

[^operators]: [2026-08-06-collection-and-structural-mutation-operators-design.md](../../superpowers/specs/2026-08-06-collection-and-structural-mutation-operators-design.md)。
[^implementation]: [rust.rs](../../../crates/hoimin-cli/src/analyzer/rust.rs)。
[^operator-report]: [2026-09-08-python-operator-coverage.md](../../superpowers/reports/2026-09-08-python-operator-coverage.md)。
[^span]: [2026-08-15-lean-byte-span-preservation-audit.md](../../superpowers/reports/2026-08-15-lean-byte-span-preservation-audit.md)。
[^bounded]: [2026-08-14-lean-bounded-candidate-discovery-audit.md](../../superpowers/reports/2026-08-14-lean-bounded-candidate-discovery-audit.md)。
[^readme]: [README.md](../../../README.md)。
