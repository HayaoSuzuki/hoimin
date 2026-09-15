---
type: Decision
title: Python解析・変異候補と入力規模
description: 構文・名前解決・変更するバイト範囲・候補保持上限を別々の契約として整理する。
status: draft
catalog_revision: a7daea0b557cd435c1e55b540392fbdd116348e1
sources:
- id: issue-546-design
  resource: ../../superpowers/specs/2026-09-15-issue-546-loop-transfer-design.md
  working_tree: untracked
  sha256: 13f80e2b6ea3088347ac6f7d5515f0973abdb745fc85ae2453cfaf9f6c0d4c66
- id: issue-489
  resource: ../../superpowers/specs/2026-09-14-issue-489-valid-python-corpus-design.md
  working_tree: untracked
  sha256: 47a53d5cee6dd179baf0bca4b3a0cd3ae8892873986ec9a2fd70f8a7ce37c1dc
- id: issue-471
  resource: ../../superpowers/specs/2026-09-14-issue-471-negative-neighbors-design.md
  working_tree: untracked
  sha256: c6b6af9099cdb3e6e15b504fd5cee6349e8906658f036a4775f41d02973ac110

- id: issue-480-spec
  resource: ../../superpowers/specs/2026-09-14-issue-480-source-encoding-design.md
  working_tree: clean
  sha256: 55a99855ee216bfff5afdfd5ffdeeab6c13e1877ec751b0ee88c9a35cb78c37e
  revision: 98969d7a840362f78dceb12f91cc5188214f68d5
- id: issue-480-codec
  resource: ../../../crates/hoimin-core/src/source_encoding.rs
  working_tree: clean
  sha256: 954deff5938c049804c83f9b903cc5742fd04f0c407a8859094b23344cef55d9
  revision: 98969d7a840362f78dceb12f91cc5188214f68d5
- id: issue-480-validator
  resource: ../../../crates/hoimin-core/src/candidate.rs
  working_tree: clean
  sha256: 122f8224aa03efca2ea3da2d061d87fa8e341af29076c9c6b2c4882f989386bc
  revision: 98969d7a840362f78dceb12f91cc5188214f68d5
- id: issue-480-writeback
  resource: ../../../crates/hoimin-cli/src/workspace/mutation.rs
  working_tree: clean
  sha256: 9df6269a7021d5ad604331ae14b952448b47cbc76d943b3665bf9b80541d9f86
  revision: 98969d7a840362f78dceb12f91cc5188214f68d5
- id: issue-480-tests
  resource: ../../../crates/hoimin-cli/tests/source_encoding.rs
  working_tree: modified
  sha256: 8e81fa46f9b3caa687569fbaae5e8bd046def854773432b425e175ce32df4acf
  revision: 98969d7a840362f78dceb12f91cc5188214f68d5
- id: issue-480-report
  resource: ../../superpowers/reports/2026-09-14-issue-480-source-encoding-review.md
  working_tree: modified
  sha256: f8cc3f5e7e8d5ae7afec4b3321751e3d0a36dee584256c930395e5571fa59102
  revision: a19bf3aadb0d56cc514d857a0e6359a563e67a18
- id: issue-482
  resource: ../../superpowers/specs/2026-09-14-issue-482-name-history-index-design.md
  working_tree: untracked
  sha256: d561460521f12160b3598596dc65d3e3e3898455d9fa93c22b6fdee21ac89f91

- id: issue-479
  resource: ../../superpowers/specs/2026-09-14-issue-479-stream-annotations-design.md
  working_tree: untracked
  sha256: 14ecae895345bbd5bf40452ed53655fc2ff4aec67e292010c2f26a10c39ee3e2

- id: issue-461
  resource: ../../superpowers/specs/2026-09-14-issue-461-lazy-candidates-design.md
  working_tree: untracked
  sha256: 683834f572abb49a7d2a5fc7c36890ce0ac57be4daebcb62467af5391edd1dc1

- id: issue-470
  resource: ../../superpowers/specs/2026-09-14-issue-470-column-index-design.md
  working_tree: untracked
  sha256: 1e6e2edd408f35816863baad22f78426c506a099808846add62a33cd12b062a3
- id: issue-513
  resource: ../../superpowers/specs/2026-09-12-issue-513-parser-recursion-design.md
  working_tree: untracked
  sha256: 5f9dbe6895760a0675c9a74e3a98207327832450e33433ac869e118125485dc8
- id: issue-515
  resource: ../../superpowers/specs/2026-09-12-issue-515-class-directives-design.md
  working_tree: untracked
  sha256: b8b9af043c170b7eff1ecb0344a6eea2bddcfec9337c67cb548f1579452edcd0
- id: issue-514
  resource: ../../superpowers/specs/2026-09-12-issue-514-comprehension-effect-order-design.md
  working_tree: untracked
  sha256: adfc5116dd78b572a24680947062b4229857e776904dcf2eaf99d786d9033b18
- id: issue-478
  resource: ../../superpowers/specs/2026-09-11-issue-478-analysis-depth-design.md
  working_tree: untracked
  sha256: 705f04cacb4004c050b986288b9e200c353ea19e994a609c8ee80a2b4cdfbf95

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
- id: issue-486
  resource: ../../superpowers/specs/2026-09-11-issue-486-type-parameter-bindings-design.md
  working_tree: untracked
  sha256: 49ad4851c7549542b91e02077502d843b05e878a36b3cc00af4220d713f4ff8c

- id: issue-485
  resource: ../../superpowers/specs/2026-09-11-issue-485-mapping-pattern-keys-design.md
  working_tree: untracked
  sha256: 32041915ca7a4046cac8505f8b831b8280fb59d2b2833a929a2f0c32d44f7df0

- id: issue-481
  resource: ../../superpowers/specs/2026-09-11-issue-481-comprehension-bindings-design.md
  working_tree: untracked
  sha256: 499b6d9c2814c81f12562ecbaa2728c8763a535077a69fecef9c01797db415dd
---

# 構文と名前解決の契約

コレクション・構造変異の設計では、既存の演算子ID、対象選択、候補順序、重複除去を保ちながら変異対象を増やす。組込み関数と同じ名前が別の値へ再束縛されている場合、その名前を組込み関数として変異しないことも条件に含む。名前の参照先が不確かな場合には、候補を保守的に除外する。[^operators]

現在のRust解析器はRuffの構文解析器を利用する。トークン（演算子や識別子などの字句）、抽象構文木（AST）、型注釈に関する候補を生成し、保持数を計測するための構造を持つ。[^implementation]

内包表記の代入式は、反復変数とは異なり、最外の内包表記を囲むスコープに束縛する。global／nonlocal宣言とlambdaの境界を保ち、変異元・変異先の双方を確認する。空の内包表記やgeneratorの遅延実行では代入の可能性と実行済みの事実を区別する。Issue481の設計では、この束縛先と実行条件をRustの候補およびCPythonの観測と照合する。[^issue-481]

Issue #514では、内包表記本体の代入式が外側へ与える束縛の可能性を、内包表記全体の完了位置で反映する。先に評価される最初のiterableで有効な組込み関数の変異を残すためである。関数ローカルの静的な束縛、generator作成後の代入の可能性、次のループ反復で参照される束縛情報は、従来どおり保持する。4形式それぞれの呼出し元・変異先を検証し、Leanから生成した18例を公開CLIと照合する。Leanの証明は宣言情報を固定したモデル内の評価順に限られ、PythonやRust実装全体を証明するものではない。[^issue-514]

# BOMと候補の列番号

Issue #469 の設計では、解析器と共通候補バリデータで列番号の計算を共有する。列は0始まりのUnicodeコードポイント数とし、ファイル先頭にあるBOMだけを表示上の列数から除く。2行目以降や文字列内のU+FEFFは数える。元ソースのバイト列、ハッシュ、変更範囲はBOMを含む実データに対応させたまま保持する。[^issue-469]

解析器だけがBOMを除いていたため、有効な1行目の候補が公開discoveryの共通検証で拒否されていた。列計算をcoreにまとめることで、verifyとworkerの適用前検証も同じ規則を使う。検証では正しい座標の受理と従来の1列ずれの拒否を対にし、公開plan・verify・runで元ソースが保存されることを確認する。[^issue-469]

Issue #470では、同じ列契約を再利用可能な疎な索引で計算する。索引は物理行の開始位置と非ASCII文字の累積余剰バイト数を一度だけ記録し、候補ごとの照会では二分探索を使う。ASCIIだけの長い1行では行頭から候補位置まで再走査せず、解析器と候補検証が同じ索引を参照する。照会順は問わず、候補の順位と保持上限の意味も変更しない。[^issue-470]

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

# 型パラメータとruntime候補の名前解決

Issue486では、generic function／classの型パラメータが導入する束縛を名前解決に反映する。変異元または変異先が型パラメータを参照する場合は、組込み関数の組として変異しない。通常の関数ローカルに名前を追加するだけで済ませず、定義時の式、本体、内側の関数・内包表記での可視範囲を区別する。[^issue-486]

関数のdecoratorや通常の引数defaultは型パラメータの外側で評価され、annotationやgeneric classの基底・keywordでは適切なannotation scopeを考慮する。class直下のannotationからの名前参照と、通常のmethodが外側のclass変数を参照できない規則も区別する。既存のtype positionでruntime候補を生成しない契約を保ち、候補が残る境界とCPythonの束縛観測を照合する。[^issue-486]

Issue515では、methodや内包表記から外側の束縛を探す際、途中のclassにあるglobal／nonlocal宣言もclass変数とともに読み飛ばす。class本体での直接参照と、method自身の宣言は引き続き考慮する。型パラメータと通常の外側の関数変数について、参照先の観測と候補の有無を照合する。共有Leanモデルのclass探索にも同じ順序の不整合があったため、その修正と通常のclosureの照合を設計に含める。[^issue-515]

# Mapping patternのキー変異とコンパイル制約

Issue485では、同じmapping pattern内で他のリテラルキーと等しくなる変異候補を除外する。`True == 1` のような数値間の等値性や、複素数を作る際の丸めも対象となる。文字列やASTの構造だけで比較せず、変更後のキーの値を比較する。単独キー、非衝突のキー、値側のpattern、通常のdict式の有効な候補は維持する。[^issue-485]

Ruffや `ast.parse` が受け入れても、重複リテラルキーはCPythonのコンパイル時に拒否される。そのため、元の入力をコンパイルした上で、公開 `plan` の候補を適用した結果も `compile(..., 'exec')` で照合する。実行時に属性キーが返す任意の値の等値性や、独立した単項符号の問題まで解決したという主張には広げない。[^issue-485]

# Issue 478: 再帰解析前の深さ検査

長い二項演算式では、候補を保持する前のAST走査がスタックを使い切る。構造に基づく深さ検査をfacts・名前解決・注釈解析より前に置き、上限を超えたファイルはpathと原因を伴う失敗として扱う。候補上限による打切りや、完全な候補ゼロとは区別する。検査だけでなく、拒否したASTの破棄とキャンセル時の後始末もdebug・releaseの実プロセスで確認する。[^issue-478]

2万項の有効な式では、通常のAST破棄も2MiBのスレッドで異常終了した。子ノードを所有する作業リストへ切り離して浅いノードから破棄し、拒否・キャンセル・構文エラー時の所有権をそろえる。この破棄用走査はRuffの子ノード定義との対応を維持する必要がある。[^issue-478]

深さ128は、moduleを1としてRuffが報告する子ノードごとに1を加えて数える。128で実際の候補を維持し、129で拒否する境界を2MiBのスレッドで確認する実装上の上限であり、任意のOS stackやRuff parserの安全性を形式的に証明する値ではない。候補保持に関する既存Lean監査の証拠を、これらの安全性へ拡張して解釈しない。[^issue-478]

[^operators]: [2026-08-06-collection-and-structural-mutation-operators-design.md](../../superpowers/specs/2026-08-06-collection-and-structural-mutation-operators-design.md)。
[^implementation]: [rust.rs](../../../crates/hoimin-cli/src/analyzer/rust.rs)。
[^operator-report]: [2026-09-08-python-operator-coverage.md](../../superpowers/reports/2026-09-08-python-operator-coverage.md)。
[^span]: [2026-08-15-lean-byte-span-preservation-audit.md](../../superpowers/reports/2026-08-15-lean-byte-span-preservation-audit.md)。
[^bounded]: [2026-08-14-lean-bounded-candidate-discovery-audit.md](../../superpowers/reports/2026-08-14-lean-bounded-candidate-discovery-audit.md)。
[^readme]: [README.md](../../../README.md)。

[^issue-486]: [2026-09-11-issue-486-type-parameter-bindings-design.md](../../superpowers/specs/2026-09-11-issue-486-type-parameter-bindings-design.md)。

[^issue-485]: [2026-09-11-issue-485-mapping-pattern-keys-design.md](../../superpowers/specs/2026-09-11-issue-485-mapping-pattern-keys-design.md)。

[^issue-481]: [2026-09-11-issue-481-comprehension-bindings-design.md](../../superpowers/specs/2026-09-11-issue-481-comprehension-bindings-design.md)。

[^issue-478]: [2026-09-11-issue-478-analysis-depth-design.md](../../superpowers/specs/2026-09-11-issue-478-analysis-depth-design.md)。

[^issue-469]: [2026-09-11-issue-469-bom-column-design.md](../../superpowers/specs/2026-09-11-issue-469-bom-column-design.md)。
[^issue-470]: [2026-09-14-issue-470-column-index-design.md](../../superpowers/specs/2026-09-14-issue-470-column-index-design.md)。

[^issue-468]: [2026-09-11-issue-468-pattern-unary-design.md](../../superpowers/specs/2026-09-11-issue-468-pattern-unary-design.md)。

# Pythonの物理行と元バイト列（Issue #455）

LF・CRLF・CRが混在する入力でも、解析器の行選択と候補検証が同じ行境界を使う設計とした。元バイト列を変換せず、CRLFは一つの改行として数える。過去に誤ったCR行位置で作られたplanは再生成が必要となる。初行のBOMによる列検証の不一致は別Issue #469の対象である。[^issue-455]

[^issue-455]: [Issue #455: Python physical newline indexing](../../superpowers/specs/2026-09-11-issue-455-python-newlines-design.md)。

# 括弧付き例外ハンドラの削除（Issue #451）

例外名のAST範囲は外側の括弧を含まない。裸の `except` へ変える際は括弧を含む例外式全体を削除し、タプル要素を削除する際もその要素の括弧を削除対象に含める設計とした。削除対象の内部にあるコメントは式とともに削除し、その外側のコメントと残す例外の表記は維持する。構文解析に加えて、生成候補をCPythonで実行して捕捉する例外を検証する。検証結果は実装計画書に記録する。[^exception-parentheses]

[^exception-parentheses]: [Issue #451 design](../../superpowers/specs/2026-09-11-issue-451-exception-parentheses-design.md)。

# Issue 513: 構文解析中のスタック制御

ASTの深さ検査より前に、Ruffによる構文解析がスタックを使い切る入力があった。Issue #513では利用中のパーサーを同梱し、再帰箇所でスタック残量を確認して必要な領域を確保する上流の対策を移植する。構文解析後の深さ128、ファイル名付きエラー、不正構文の診断は維持する。[^issue-513]

解析途中の代入先検査やパターン変換も、完成した部分木を再帰的にたどるため、同じスタック検査で保護する。試行解析の結果や無効なパターンを捨てる箇所では、子ノードを切り離して反復的に破棄する。構文解析中のスタック検査だけで、部分木の通常の破棄まで保護したとは扱わない。[^issue-513]

公開plan/runの4種の有効例はCPythonでコンパイルを確認する。512 KiBスレッドでの深いリスト・suite・format specificationや不正構文は、Ruffの走査と所有権を調べる別の試験である。メモリ確保の総量や任意のOS・ビルド条件での安全性は保証しない。依存版、再帰箇所、AST型、破棄経路の変更時は、深さ境界・不正構文・繰り返し回収の回帰試験を再実行する。[^issue-513]

[^issue-513]: [2026-09-12-issue-513-parser-recursion-design.md](../../superpowers/specs/2026-09-12-issue-513-parser-recursion-design.md)。
[^issue-515]: [Issue515 class directive design](../../superpowers/specs/2026-09-12-issue-515-class-directives-design.md)。
[^issue-514]: [Issue514 comprehension effect order design](../../superpowers/specs/2026-09-12-issue-514-comprehension-effect-order-design.md)。

# 名前束縛履歴の照会（Issue #482）

module/classの名前束縛eventはvisitorの挿入順を意味順として保持する。各名前の履歴をoffset範囲と累積解決変換を持つ木へ構築し、通常のsource順履歴では参照位置までの状態を対数node訪問で求める。非単調offsetと同一offsetでも左右を挿入順に合成し、従来の全走査foldと一致させる。[^issue-482]

[^issue-482]: [Issue #482: Name history index](../../superpowers/specs/2026-09-14-issue-482-name-history-index-design.md)。

# 型注釈候補の状態保持（Issue #479）

型演算子を選択しない解析では、型候補用のimport flow収集を実行しない。型演算子を選択する場合は、各annotation時点のimportsを借用して候補を逐次生成し、全mapのsnapshotをsiteごとに保持しない。runtime候補を注釈内で抑止する範囲索引と、binding-flow correspondence用の所有snapshotは維持する。[^issue-479]

[^issue-479]: [Issue #479: Streaming annotation candidates](../../superpowers/specs/2026-09-14-issue-479-stream-annotations-design.md)。

# 未選択候補の所有文字列（Issue #461）

演算子や対象選択で除外できる候補は、元ソース範囲の複製より先に判定する。list/tuple literalのように置換生成自体が入力範囲に比例する場合は、演算子選択をhelper呼出し前に確認する。この省略は現在のnodeの候補生成に限り、子nodeの探索は継続する。候補上限は保持数を制約するが、解析中の全確保量を制約しない。[^issue-461]

[^issue-461]: [Issue #461: Lazy candidate strings](../../superpowers/specs/2026-09-14-issue-461-lazy-candidates-design.md)。

# 有効Pythonを起点とする候補契約（Issue #489）

CPython3.14で元入力をコンパイルしてから、宣言済みの適格・不適格位置とRust解析器・公開planの候補を照合する。各候補には共有validator、独立した位置計算、1件ずつ適用した結果のCPythonコンパイルを適用する。scope・key・spanの小さいLean契約を再利用し、producer間で未観測の組合せは未検証として出力する。[^issue-489]

[^issue-489]: [Issue #489: Valid Python candidate contract corpus](../../superpowers/specs/2026-09-14-issue-489-valid-python-corpus-design.md)。
# 負の添字とslice境界（Issue #471）

単項負号と十進整数の組を境界値演算子の対象へ追加する。負号を含むAST範囲全体を置換し、slice stepのゼロ候補を除く。整数の絶対値はu64の最大値以下に限定し、範囲を超える入力と隣接値を除外する。既存の符号なしゼロは+1だけを保持し、`-0`は+1と-1を生成する。型注釈・代入先・削除対象の除外は維持する。[^issue-471]

[^issue-471]: [Issue #471: Negative index and slice neighbors](../../superpowers/specs/2026-09-14-issue-471-negative-neighbors-design.md)。

# ソース文字コードと元バイト位置

Issue #480 では、Rustの共通decoderでUTF-8、ASCII、Latin-1の宣言を認識する。宣言なしはUTF-8とする。先頭行の独立したコメント、または先頭行が空白・コメントだけの場合の2行目を調べる。BOMがある場合はCPython tokenizerと同じUTF-8名の正規化条件を使う。未知・未対応codec、BOMとの衝突、UTF-8・ASCIIの不正バイトは、元の宣言名と対象pathを診断に含める。Latin-1は全バイトに対応するため、別の文字コードを意図して保存されたかどうかまでは判定できない。[^issue-480-spec][^issue-480-codec]

解析用テキストはUnicode、候補spanとfile hashは元バイト列を基準にする。Latin-1では非ASCIIバイトの疎な索引を作り、UTF-8位置と元バイト位置の境界を二分探索で対応させる。共通候補検証は元codecでoriginalを照合し、対応後のUnicode列とreplacementの表現可能性を確認する。IDのschemaは1を維持し、元hash・元span・Unicode replacementという既存の入力を使う。[^issue-480-codec][^issue-480-validator]

workerには元バイトのprefix、元codecでエンコードしたreplacement、元バイトのsuffixを書く。ファイル全体をUTF-8へ変換しない。公開CLIテストでは、アクセント付き文字の前後の候補と、それを含むcollectionの置換について、plan・run・verifyのID、元span、hash、CPythonが読む実バイトと値を照合する。[^issue-480-writeback][^issue-480-tests]

通常のrunはbaseline後に解析するため、baselineの失敗が文字コード診断より先になることがある。planにはbaselineがなく、明示symbolは対象解決で先にdecodeする。これは既存の実行順序を変更する機能ではない。[^issue-480-spec]

UTF-8とASCIIは入力を借用するが、Latin-1のテキストと索引のメモリは入力に比例する。過去のUTF-8対象のLean保証をcodec全体へ拡張して解釈しない。[^issue-480-spec][^issue-480-report]

[^issue-480-spec]: [2026-09-14-issue-480-source-encoding-design.md](../../superpowers/specs/2026-09-14-issue-480-source-encoding-design.md).

[^issue-480-codec]: [source_encoding.rs](../../../crates/hoimin-core/src/source_encoding.rs).

[^issue-480-validator]: [candidate.rs](../../../crates/hoimin-core/src/candidate.rs).

[^issue-480-writeback]: [mutation.rs](../../../crates/hoimin-cli/src/workspace/mutation.rs).

[^issue-480-tests]: [source_encoding.rs](../../../crates/hoimin-cli/tests/source_encoding.rs).

[^issue-480-report]: [2026-09-14-issue-480-source-encoding-review.md](../../superpowers/reports/2026-09-14-issue-480-source-encoding-review.md).

# 入れ子ループの転送再解析（Issue #546）

固定点とは、loop本体のfallthrough・continueから求めた次反復のimport状態が現在のheadと一致する状態である。注釈記録を停止した解析では、収束を確認した最後の本体終了結果をその場で消費して二度目の本体走査を省く。別の入力・scopeへ結果を持ち越すcacheは追加しない。[^issue-546-design]

注釈callbackを実行する走査、最後の評価でclass fallbackが変化した走査、テストのprojection・変異を使う走査は従来どおり実行する。module/class内の単純な空状態のfor/whileの再解析を抑える変更であり、classや状態変化を含む全入力の計算量を保証するものではない。性能ゲートと実行結果は[入力形状別の性能検証](../audits/performance-shapes.md)から参照する。[^issue-546-design]

[^issue-546-design]: [Issue #546: 入れ子ループの転送結果の再利用](../../superpowers/specs/2026-09-15-issue-546-loop-transfer-design.md)。
