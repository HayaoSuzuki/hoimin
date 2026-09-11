---
type: Decision
title: Python解析・変異候補と入力規模
description: 構文・名前解決・変更するバイト範囲・候補保持上限を別々の契約として整理する。
status: draft
catalog_revision: a7daea0b557cd435c1e55b540392fbdd116348e1
sources:
- id: issue-469
  resource: ../../superpowers/specs/2026-09-11-issue-469-bom-column-design.md
  working_tree: untracked
  sha256: b8c06ba180a55c43f93558d9d6e4ec812313388373e4cf72edefdbf191d4d6ce

- id: issue-468
  resource: ../../superpowers/specs/2026-09-11-issue-468-pattern-unary-design.md
  working_tree: untracked
  sha256: 5d76141fbf41f5811ba5b9132fcc3504962f67181bde62cfa726dd4e92c6dc31

- id: issue-455
  resource: ../../superpowers/specs/2026-09-11-issue-455-python-newlines-design.md
  working_tree: untracked
  sha256: 2f3470c6ec0971d2e526d7134aecf56c3c089599bcd9dc7e33373f5c24d31db4

- id: exception-parentheses
  resource: ../../superpowers/specs/2026-09-11-issue-451-exception-parentheses-design.md
  working_tree: untracked
  sha256: 73ff551e95721e2bfb0d188c2b76b1ce1ff1b1ef431b9eedd187af8af4298790
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

# BOMと候補の列番号

Issue #469 の設計では、解析器と共通候補バリデータで列番号の計算を共有する。列は0始まりのUnicodeコードポイント数とし、ファイル先頭にあるBOMだけを表示上の列数から除く。2行目以降や文字列内のU+FEFFは数える。元ソースのバイト列、ハッシュ、変更範囲はBOMを含む実データに対応させたまま保持する。[^issue-469]

解析器だけがBOMを除いていたため、有効な1行目の候補が公開discoveryの共通検証で拒否されていた。列計算をcoreにまとめることで、verifyとworkerの適用前検証も同じ規則を使う。検証では正しい座標の受理と従来の1列ずれの拒否を対にし、公開plan・verify・runで元ソースが保存されることを確認する。[^issue-469]

# パターン内の単項符号

Issue #468 の設計では、`case -1` などの数値パターンに通常の単項符号変異を適用しない。Pythonのリテラルパターンでは先頭の負符号は有効だが、正符号への置換は構文エラーになるためである。除外はASTの構文上の役割に基づいてトークン候補の登録時に行い、通常の式・ガード・case本体の符号と、複素数の二項符号や真偽値パターンの変異は維持する。[^issue-468]

この契約はCPythonによるコンパイルと、importだけを行う公開CLI試験で確認する。構文エラーによるkillを正常な変異の検出として数えないことが目的であり、全パターンや他の演算子の構文安全性を一括して保証するものではない。[^issue-468]

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

[^issue-469]: [2026-09-11-issue-469-bom-column-design.md](../../superpowers/specs/2026-09-11-issue-469-bom-column-design.md)。

[^issue-468]: [2026-09-11-issue-468-pattern-unary-design.md](../../superpowers/specs/2026-09-11-issue-468-pattern-unary-design.md)。

# Pythonの物理行と元バイト列（Issue #455）

LF・CRLF・CRが混在する入力でも、解析器の行選択と候補検証が同じ行境界を使う設計とした。元バイト列を変換せず、CRLFは一つの改行として数える。過去に誤ったCR行位置で作られたplanは再生成が必要となる。初行のBOMによる列検証の不一致は別Issue #469の対象である。[^issue-455]

[^issue-455]: [Issue #455: Python physical newline indexing](../../superpowers/specs/2026-09-11-issue-455-python-newlines-design.md)。

# 括弧付き例外ハンドラの削除（Issue #451）

例外名のAST範囲は外側の括弧を含まない。裸の `except` へ変える際は括弧を含む例外式全体を削除し、タプル要素を削除する際もその要素の括弧を削除対象に含める設計とした。削除対象の内部にあるコメントは式とともに削除し、その外側のコメントと残す例外の表記は維持する。構文解析に加えて、生成候補をCPythonで実行して捕捉する例外を検証する。検証結果は実装計画書に記録する。[^exception-parentheses]

[^exception-parentheses]: [Issue #451 design](../../superpowers/specs/2026-09-11-issue-451-exception-parentheses-design.md)。
