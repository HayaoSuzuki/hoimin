---
type: Decision
title: Python解析・変異候補と入力規模
description: 構文・名前解決・変更するバイト範囲・候補保持上限を別々の契約として整理する。
status: draft
catalog_revision: a7daea0b557cd435c1e55b540392fbdd116348e1
sources:
- id: issue-692-hierarchy
  resource: ../../superpowers/specs/2026-10-05-issue-692-exception-hierarchy.md
  revision: fae3ce3e82a8a768c8dca49531384aa081a97f71
  working_tree: clean
  sha256: 540858afd18bf3ac21382ead8b016358aae9e5f2a4aa3e7fc37a2ae0591d2361
- id: issue-692-scopes
  resource: ../../superpowers/specs/2026-10-05-exception-scope-outcomes.md
  revision: fae3ce3e82a8a768c8dca49531384aa081a97f71
  working_tree: clean
  sha256: 08bcdc67d25554eb86990d7f8e83ca451d8f72639069ea72acbb6092d8a1f101
- id: issue-692-verification
  resource: ../../superpowers/reports/2026-10-05-exception-scope-lean-audit.md
  revision: fae3ce3e82a8a768c8dca49531384aa081a97f71
  working_tree: clean
  sha256: 290244cc4afcb16315fd2d0353da0dedf0e6c64d82c5dc03cee393dbd98088ff
- id: issue-556-design
  resource: ../../superpowers/specs/2026-09-24-issue-556-with-suppression.md
  revision: 282e941e4c1a5a23303d30beca881d7bbfde7763
  working_tree: clean
  sha256: 7ff74c9749274839e3cbe0ceb737544de0dd3b5fb5eef90aae17e3ff461269be

- id: issue-560-design
  resource: ../../superpowers/specs/2026-09-24-issue-560-evaluation-order.md
  revision: ffb65c051014f3d9601df2deb0bfeb5ff55c7c38
  working_tree: clean
  sha256: fec8835b826fb48eee6f58471f0e0d679cb0dcefe0a574b4bd1476051d99546e
- id: issue-560-code
  resource: ../../../crates/hoimin-cli/src/analyzer/rust.rs
  revision: aefa2a8
  working_tree: modified
  sha256: 0d93c8c393d50ca9793ade1ce487504138b13aa3845557481ce94f89662959be
- id: issue-560-tests
  resource: ../../../crates/hoimin-cli/tests/builtin_evaluation_order.rs
  revision: ffb65c051014f3d9601df2deb0bfeb5ff55c7c38
  working_tree: untracked
  sha256: 9775e388b7c951d7bcacea8a228e24af94d0864e240310f6fa5c377f6bc5b5b1

- id: issue-558-design
  resource: ../../superpowers/specs/2026-09-24-issue-558-deferred-imports-design.md
  revision: a49467ded359416a6ee743536100634f1cced3e2
  working_tree: modified
  sha256: 05fb86175690ff023986ef0bb2f0df2552ee8801ceab86910d01487db483ff6f
- id: issue-564-design
  resource: ../../superpowers/specs/2026-09-24-issue-564-nullable-provenance.md
  revision: f6f5d96c099fb880884b2b7cb29717ff33d70d75
  working_tree: clean
  sha256: a0237a6eca382f65069b6a15eb7681c155f6b94f468d8b09725cff7a9f514f8e
- id: issue-564-code
  resource: ../../../crates/hoimin-cli/src/analyzer/rust.rs
  revision: f6f5d96c099fb880884b2b7cb29717ff33d70d75
  working_tree: clean
  sha256: 977b7f871450b831854909f1d8d96a53992922f5688ffa103d5e7a94527e2b8a

- id: issue-565-design
  resource: ../../superpowers/specs/2026-09-24-issue-565-annotation-descendants.md
  revision: 75c1ddfd59a1c2ebf1bc9de05efb33c571c7c328
  working_tree: clean
  sha256: b13589887e2a7fc2f3386c0d284825582139503f3deab0504e79c13d7cb76f8f
- id: nullable-gates-audit
  resource: ../../audits/2026-09-15-nullable-gates/README.md
  working_tree: untracked
  sha256: 13a3b384def390e0596119cc5901ef1b12ac7cd32bbd4ae73266000a6e207b52
- id: declaration-only-audit
  resource: ../../audits/2026-09-15-declaration-only/README.md
  working_tree: untracked
  sha256: 4272c093852eab370489e8345c9ed515f20cdf26f2e5f13a4d495977db8636c9
- id: evaluation-order-audit
  resource: ../../audits/2026-09-15-evaluation-order/README.md
  working_tree: untracked
- id: annotation-followup
  resource: ../../audits/2026-09-15-annotation-followup/README.md
  working_tree: untracked
- id: with-finally-audit
  resource: ../../audits/2026-09-15-with-finally/README.md
  working_tree: untracked
- id: issue-549-report
  resource: ../../superpowers/reports/2026-09-15-issue-549-slice-tuple.md
  revision: f11013542ccd735ab9741b5079c0b39a517df256
  working_tree: untracked
  sha256: 80f6a99dabfdbe084de6cba0e211e55b4033bf7449c5dd344ab922358c1fac6b
- id: issue-549
  resource: ../../superpowers/specs/2026-09-15-issue-549-slice-tuple-design.md
  revision: f11013542ccd735ab9741b5079c0b39a517df256
  working_tree: untracked
  sha256: 1e0e66ee142ae6e8e71019b67d4901bcc640797781ed8e8d3878cbe2e64460bf

- id: issue-545
  resource: ../../superpowers/specs/2026-09-15-issue-545-implicit-finally-design.md
  revision: f11013542ccd735ab9741b5079c0b39a517df256
  working_tree: untracked
  sha256: f2d56cddd7659add3ef30fed812ac8174bf45e725fa0ef380f2ca39c8b0dda51

- id: issue-547
  resource: ../../superpowers/specs/2026-09-15-issue-547-import-transfer-design.md
  working_tree: untracked
  sha256: 3fa1b369ff21d00b335d91fb0c83b18b61f54fcd45747ef07ee5386cae273ae8

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
- id: issue-598-design
  resource: ../../superpowers/specs/2026-09-25-issue-598-prepared-namespace-design.md
  revision: 61957866feae3cc5cd62cd026b677314e34957ea
  working_tree: clean
  sha256: b7740711d779a855a25dc8d658aa7df63be244f0059eda9bbdfbf87f7ec39ecf
- id: issue-598-report
  resource: ../../superpowers/reports/2026-09-25-issue-598-prepared-namespace.md
  revision: 61957866feae3cc5cd62cd026b677314e34957ea
  working_tree: modified
  sha256: 9cc6842eefa4850aa0d57c979507dbf7abc77eb9b7fc5219d14c537566674143
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
  resource: ../../usage.md
  revision: 5a45bdc3bd444771c4c57e30fe211f174850ce93
  working_tree: untracked
  sha256: 6894a34743882cc26fdd1f39cb536c94dfc20d5f1dcf33b3cb7f3da9b13e6bef
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
- id: issue-559-repair
  resource: ../../superpowers/specs/2026-09-24-issue-559-design.md
  revision: 98e78166b43940df38a8bb8099c9c6af6004ba5a
  working_tree: clean

- id: issue-696
  resource: ../../superpowers/reports/issue-696/design.md
  revision: 4cc710dea80e4198f48b05a8673ac921961927e4
  working_tree: untracked
  sha256: 4796ccaf60b518dc7f79385efa5058b9565626e1629a2064bb0bcf77cd9d86f6

- id: issue-697
  resource: ../../superpowers/reports/issue-697/design.md
  revision: 3438b3a
  working_tree: untracked
  sha256: c5ed107d2acd19f67814da53bd64f5873c35656a56e0920111a2f79b6f06ea6b

- id: issue-698
  resource: ../../superpowers/reports/issue-698/design.md
  revision: 93df0b4
  working_tree: untracked
  sha256: 323657b55670eeac6e4304645b7a2ef785551632de05c63441b5d7a86981aaa6

- id: issue-699
  resource: ../../superpowers/reports/issue-699/design.md
  revision: e4a359e
  working_tree: untracked
  sha256: c84756dcff1e913353ae772dc9bfabf5a1d50938367faad1974a75332f90d2e1

- id: issue-700
  resource: ../../superpowers/reports/issue-700/design.md
  revision: 10a0021
  working_tree: untracked
  sha256: e8615e151e19ee6a0699326b59823c4e65170b36865102c59ee639c77d3e25cc

- id: issue-701
  resource: ../../superpowers/reports/issue-701/design.md
  revision: c7920e0
  working_tree: untracked
  sha256: 874cc475bf39b5c0d99c6176ac18a66211e67dea1ec8548caa2490e25d20843a

- id: issue-702
  resource: ../../superpowers/reports/issue-702/design.md
  revision: 4b37fcb
  working_tree: untracked
  sha256: 1aade18c32612463dbb7cacb57cf05eceb01e6f5b8f8c47be1d46cd9163a1fd4

- id: issue-703
  resource: ../../superpowers/reports/issue-703/design.md
  revision: 625e68e
  working_tree: untracked
  sha256: 31f19345bca108b883d6083984c505b9860718b607fe0091105b0244e7b03b65

- id: issue-704
  resource: ../../superpowers/reports/issue-704/design.md
  revision: 22483e46cea1802a29c3fb376442aefc0dde5b27
  working_tree: untracked
  sha256: f1083311f2f119625c62ee1558b769969f96992b06f9727a353e0347e877fde7

- id: issue-705
  resource: ../../superpowers/reports/issue-705/design.md
  revision: dbb9635e291ca83ac6895bd650f6185a7f46ffca
  working_tree: untracked
  sha256: 00a9ef473ab17de502b60e2279e6c77117794153113da5468c3048d6f16b5a6c

- id: issue-706
  resource: ../../superpowers/reports/issue-706/design.md
  revision: bbd765087f8bdda2a5af7b4cea2de6dc8b23f4aa
  working_tree: untracked
  sha256: a79205f7322678af6112d30bd6e59728fb0bacfc592b64ddc11dc3c815433636

- id: issue-707
  resource: ../../superpowers/reports/issue-707/design.md
  revision: 38a15ddd4e8de36d29800f81b2e8314109b3f855
  working_tree: untracked
  sha256: 30d5553957b88c0d968dcf953995d3cd81fcf53a2579b36272d694e9d935e6ad

- id: issue-708
  resource: ../../superpowers/reports/issue-708/design.md
  revision: 3c2a0fda1fbd54719433e55a6d747ce11f47e1c7
  working_tree: untracked
  sha256: 4fa705263311a2b6497f74f9bdedd359fb1cf83a3c01520855e04d77622cb48c

- id: default-operators
  resource: ../../superpowers/reports/default-analyzer-operators/design.md
  working_tree: untracked
  sha256: 8aa57f82bc7545d7bf5d0d88fb7b2c1b07175272320cac19561bf413ec37f643
- id: default-operators-review
  resource: ../../superpowers/reports/default-analyzer-operators/review.md
  working_tree: untracked
  sha256: 9a0875f841d3b40d504c2a07b638ea03cbd403adb37b66ffd19c2ae43b3af719
- id: method-call-remove
  resource: ../../superpowers/reports/method-call-remove/design.md
  revision: 3596c98
  working_tree: untracked
  sha256: f1b09d5746badd9bb8adb53404d35739275c1185a80339a8241811793ab16192

- id: function-body-return-constant
  resource: ../../superpowers/reports/function-body-return-constant/design.md
  revision: 1865f47
  working_tree: untracked
  sha256: b505c8e3f175303e82106e08836df1a3f9bf12ad94bfc52e07b1521721c2dcaf

- id: recent-analyzer-defaults
  resource: ../../superpowers/reports/recent-analyzer-defaults/design.md
  revision: 8437243
  working_tree: untracked
  sha256: 98c8c6c858b6159203d518deaff36f5a186e4f1304bca81085549b401161db26

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

候補件数を制限しても、解析に使うメモリ全体の上限にはならない。ソース本文、トークン列、ASTなどの大きさは入力サイズに依存するためである。利用方法もこの適用範囲を明記している。[^readme]

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

演算子、名前解決、変更範囲の検証、候補順序、保持構造を変更したら、対応する監査と実装比較用のテストを再確認する。原文の演算子数は報告時点の数値として読み、現行一覧は利用方法と実装を照合する。

# 型パラメータとruntime候補の名前解決

Issue486では、generic function／classの型パラメータが導入する束縛を名前解決に反映する。変異元または変異先が型パラメータを参照する場合は、組込み関数の組として変異しない。通常の関数ローカルに名前を追加するだけで済ませず、定義時の式、本体、内側の関数・内包表記での可視範囲を区別する。[^issue-486]

関数のdecoratorや通常の引数defaultは型パラメータの外側で評価され、annotationやgeneric classの基底・keywordでは適切なannotation scopeを考慮する。class直下のannotationからの名前参照と、通常のmethodが外側のclass変数を参照できない規則も区別する。既存のtype positionでruntime候補を生成しない契約を保ち、候補が残る境界とCPythonの束縛観測を照合する。[^issue-486]

Issue515では、methodや内包表記から外側の束縛を探す際、途中のclassにあるglobal／nonlocal宣言もclass変数とともに読み飛ばす。class本体での直接参照と、method自身の宣言は引き続き考慮する。型パラメータと通常の外側の関数変数について、参照先の観測と候補の有無を照合する。共有Leanモデルのclass探索にも同じ順序の不整合があったため、その修正と通常のclosureの照合を設計に含める。[^issue-515]

# 準備済みクラス名前空間（Issue #598）

基底クラスまたはkeyword引数があるclassでは、metaclassの `__prepare__` が独自の名前空間を渡し得るため、class名前空間を読む組込み名を確定しない。この条件は継承metaclass、基底の展開、`**keywords` にも適用する。引数のないclassと空の `()` は従来の候補を維持する。`class C(object)` や `metaclass=type` も保守的に除外する。[^issue-598-design]

method・closure・内包表記の本体はclassを読み飛ばし、`global` は宣言した名前ごとにmoduleを参照する。内包表記の最初のiterable、関数default、classを参照できる型注釈は準備済み名前空間の判定を受ける。型注釈の組込み名とimport aliasの参照先は別経路であり、後者の `annotation_import_stable` は今回の保証対象に含めない。[^issue-598-design]

Leanから生成した39入力について、CPythonの両名の参照先と公開planの候補を照合した。独自名前空間へのanyのみ・allのみ・両方の注入は、公開runでもkilled件数に入らない。独自metaclassが空のdictを返す入力は、実行時には組込みでも静的には候補を抑制する。モデルの証明、実装との照合、適用範囲は検証報告に分けて記録した。[^issue-598-report]

[^issue-598-design]: [Prepared class namespace name resolution (#598)](../../superpowers/specs/2026-09-25-issue-598-prepared-namespace-design.md)。
[^issue-598-report]: [Prepared namespace verification and review (#598)](../../superpowers/reports/2026-09-25-issue-598-prepared-namespace.md)。

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
[^readme]: [利用方法](../../usage.md)。

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

Issue #482の設計では、module/classの束縛履歴をソースoffsetで索引化し、visitorの挿入順に解決変換を合成した。Issue #560では、この照会キーを評価イベント番号へ変更する。累積した束縛状態の二分探索と、従来の履歴foldとの一致を確認するテストは維持する。[^issue-482][^issue-560-code]

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

# Sliceを含むtupleの適格条件（Issue #549）

collection_list_tupleのtupleからlistへの変換は、直接の要素にSliceがある場合は生成しない。Sliceは `x[:,]` などの添字では有効だが、colonをlist要素へ移せない。通常の式tupleとstarredを維持し、Sliceのstart/stop/stepの子探索も続ける。これは候補生成時の条件であり、生成済みの不正候補を実行結果から除く処理ではない。[^issue-549]

[^issue-549]: [Issue #549: Sliceを含むtupleの候補除外](../../superpowers/specs/2026-09-15-issue-549-slice-tuple-design.md)。

有限モデルの14入力と11反例、公開plan/CPythonの照合、import-only runの修正前後を[検証報告](../../superpowers/reports/2026-09-15-issue-549-slice-tuple.md)に分けて記録する。[^issue-549-report]

[^issue-549-report]: [Issue #549: Slice tuple修正の検証](../../superpowers/reports/2026-09-15-issue-549-slice-tuple.md)。

# コレクション型注釈の組込み型provenance（Issue #548）

[型注釈の参照先の契約](annotation-builtins.md)では、具体型名の綴りだけで組込み型と判断せず、source・destinationの両方向で注釈scopeの束縛を確認する。runtimeの名前解決とは遅延評価の扱いが異なるため、module/classの後続束縛も考慮する。

# finally への暗黙例外入口

call、subscript、attribute、演算や比較、反復などの評価が後続 import より先に失敗した場合、finally の注釈は例外前の束縛も参照し得る。解析器はこの入口を正常入口と合流し、全入口で一致しない typing 由来の候補を抑制する。暗黙例外は正常後続へ混ぜず、finally の通常終了後も例外として外側へ渡す。[^issue-545]

[検証範囲](../audits/implicit-finally.md)に小モデル、公開 plan 対応、未対応の式や動的挙動を記録する。

[^issue-545]: [Issue #545 設計](../../superpowers/specs/2026-09-15-issue-545-implicit-finally-design.md)。

# 単一路のimport状態の受渡し

Issue #547の設計では、型注釈collectorの通常の文から次の文へ進む状態を所有権移動で渡す。import数Iと注釈数Aを別々に増やした場合にも、単一路の全状態コピーをsuite境界の一回に限定する。分岐、loop、finallyの合流とスコープ復元のための複製は保持し、すべての入力で線形時間になるとは主張しない。[^issue-547]

[^issue-547]: [Issue #547設計](../../superpowers/specs/2026-09-15-issue-547-import-transfer-design.md)。

# 入れ子ループの転送再解析（Issue #546）

固定点とは、loop本体のfallthrough・continueから求めた次反復のimport状態が現在のheadと一致する状態である。注釈記録を停止した解析では、収束を確認した最後の本体終了結果をその場で消費して二度目の本体走査を省く。別の入力・scopeへ結果を持ち越すcacheは追加しない。[^issue-546-design]

注釈callbackを実行する走査、最後の評価でclass fallbackが変化した走査、テストのprojection・変異を使う走査は従来どおり実行する。module/class内の単純な空状態のfor/whileの再解析を抑える変更であり、classや状態変化を含む全入力の計算量を保証するものではない。性能ゲートと実行結果は[入力形状別の性能検証](../audits/performance-shapes.md)から参照する。[^issue-546-design]

[^issue-546-design]: [Issue #546: 入れ子ループの転送結果の再利用](../../superpowers/specs/2026-09-15-issue-546-loop-transfer-design.md)。

# withの例外抑制とfinallyの走査（Issue #556/#557）

`5e631ef`の追加監査で、with本体の例外が抑制された後のimport合流に欠落を確認した。finallyの暗黙例外入口とは別に、例外から正常継続への変換が必要となる。finallyには記録無効でも記録用走査を行う経路が残り、入れ子のrelease計測で時間がほぼ倍増した。これらは監査時点の観測であり、[監査の証拠と修正後の確認範囲](../audits/with-finally-2026-09.md)を参照する。[^with-finally-audit]

[^with-finally-audit]: [withの例外抑制とfinallyの解析コスト](../../audits/2026-09-15-with-finally/README.md)。

Issue #556の修正では、通常のwithとasync withの本体で例外状態を収集し、抑制後の正常継続へ合流する。managerの実体が不明な場合も抑制経路を考慮する。明示raiseと、成功したreturn・break・continueは別々に保持し、finallyが終了方法を置き換えた場合も区別する。[^issue-556-design]

複数itemでは、先に入ったmanagerが後続itemの開始失敗や内側の終了処理の失敗を抑制し得る。最初のmanager自身の開始失敗は、そのmanagerの正常継続へ合流しない。import失敗と任意の動的hookは従来どおり対象外であり、importだけの本体やcallより前のimportの正例を保持する。#557の解析コストはこの修正の対象外である。[^issue-556-design]

[^issue-556-design]: [Issue 556: import facts after context-manager suppression](../../superpowers/specs/2026-09-24-issue-556-with-suppression.md)。


# 遅延注釈と集合ABCの綴り（Issue #558/#559）

`5e631ef`では、型注釈のtyping aliasに定義時のimport状態を使うため、3.14の初回評価前の再代入を見落とす。また、collections.abcの集合抽象型はSetであるのに、AbstractSetという候補を生成する。両件の実行観測、初回評価キャッシュのモデル証明、型名の対応は[追加監査](../audits/annotation-followup-2026-09.md)を参照する。この監査時点では製品修正は未実施だった。[^annotation-followup]

[^annotation-followup]: [遅延注釈とcollections.abc.Set](../../audits/2026-09-15-annotation-followup/README.md)。

## 評価順序と未選択methodの追加監査

`5e631ef`では、ソースoffsetによる名前解決が多重代入・引数展開の評価順序と一致せず、再代入済みの名前を組込みと誤認した（#560）。また、未選択のmethod変異のreplacement確保が残る（#561）。[監査の証拠と限界](../audits/evaluation-order-2026-09.md)に、7入力の照合とdebugの累積確保要求量を記録した。[^evaluation-order-audit]

[^evaluation-order-audit]: [評価順序と未選択methodの確保](../../audits/2026-09-15-evaluation-order/README.md)。

## 値なし注釈と候補精度

`5e631ef`では、値を伴わない名前の注釈も再束縛として扱い、builtin・module import aliasの候補が欠落した。#562では、関数のローカル宣言を維持しながら、値なし注釈で既知の参照先を保持する改善を提案する。既存設計は保守的な欠落を許容するため、安全性違反とは分類していない。[監査の証拠と限界](../audits/declaration-only-2026-09.md)を参照する。[^declaration-only-audit]

[^declaration-only-audit]: [値なし注釈と候補精度](../../audits/2026-09-15-declaration-only/README.md)。

## nullable型変異の適用条件

`5e631ef`ではnullable追加で名前の再束縛を見落とし（#564）、複数型引数のtuple内部で対象外要素の検査を省いていた（#565）。[監査の証拠と限界](../audits/nullable-gates-2026-09.md)に、15入力の照合と名前条件・子孫の再帰条件のLean証明を記録した。修正時には通常の組込み型と対象内だけの複数型引数を保持する。[^nullable-gates-audit]

[^nullable-gates-audit]: [nullable型変異の適用条件](../../audits/2026-09-15-nullable-gates/README.md)。

## 2026-09-24: Issue #559の修正

2026-09-24の#559修正では、集合抽象型の対応をtyping.AbstractSetとcollections.abc.Setへ訂正した。module importと直接importの両方で別名と双方向の候補を扱い、置換先の修飾名には参照元モジュールのメンバー名を使う。typing.Setはこの抽象型の組合せへ追加しない。#558の遅延評価は別Issueとして扱う。[^issue-559-repair]

[^issue-559-repair]: [修正設計](../../superpowers/specs/2026-09-24-issue-559-design.md)。

## 組込み名の評価イベント（Issue #560）

名前の出現位置はソースのバイトoffsetで識別し、束縛状態は別に記録した評価イベント番号で照会する。代入は右辺を評価してからtargetへ左から順に格納し、入れ子のtargetでも格納の間に後続式を評価する。呼出しは位置引数・starred引数をkeyword引数より先に評価する。sourceまたはdestinationの綴りがその時点で束縛済みなら、組込み名の置換候補を生成しない。[^issue-560-design][^issue-560-code]

拡張代入はtargetを一度評価してから右辺を評価するため、通常の代入とは順序が異なる。右辺で格納前に参照する組込み名の候補と、拡張代入のtarget内で先に参照する候補は保持する。例外で後続targetの格納が停止し得る場合は、既存の条件付き束縛を使って参照先を保守的に判定する。公開plan/runテストでは、CPythonで元入力の値を確認し、候補数・spanと誤ったkilled計上の抑制を照合する。任意の例外到達経路を判定する保証は含まない。[^issue-560-code][^issue-560-tests]

[^issue-560-design]: [Issue 560: builtin resolution in evaluation order](../../superpowers/specs/2026-09-24-issue-560-evaluation-order.md)。
[^issue-560-code]: [NameResolutionBuilderの実装](../../../crates/hoimin-cli/src/analyzer/rust.rs)。
[^issue-560-tests]: [公開plan/runの評価順序テスト](../../../crates/hoimin-cli/tests/builtin_evaluation_order.rs)。

## #558の遅延注釈の名前解決

#558の修正では、注釈位置のimport状態に加えて、評価時に参照するscopeの後続束縛を確認する。typingとcollections.abcの置換元・置換先に共通の条件を適用し、由来を確認できない綴りは候補に使わない。変更のないimport、注釈より前のimport復元、別scopeの独立したimportは維持する。[^issue-558-design]

条件分岐の合流でimport情報が失われた場合も、import名の履歴と参照先scopeを使って不確かな参照を除外する。関数内変数のsource-order例外には、既知のimport情報が必要である。[^issue-558-design]

評価済みキャッシュの後の再代入と、注釈より後かつ初回参照前のimport復元は、安全な場合でも保守的に除外する。Pythonの版を推定せず、future annotations、遅延type alias、generic bound/defaultにもこの条件を適用する。実行時に評価されない関数内変数の注釈は従来のsource-order条件を維持する。collections.abc.Setの綴りは、併合元の#559で修正済みである。[^issue-558-design]

[^issue-558-design]: [Issue #558: imported names in deferred annotations](../../superpowers/specs/2026-09-24-issue-558-deferred-imports-design.md)。
## nullable追加の組込み型の参照先（Issue #564）

nullable追加では、str/int/float/bool/bytesの名前とlist/set/dictの型構成子について、注釈scopeで組込み型を参照すると確認できる場合だけ候補を生成する。型引数内の組込み名も同じ条件で確認する。module/classの後続束縛、外側の関数local、型パラメータによって参照先が不確かになる場合は抑制する。関数自身の引数・本体のlocalは、その関数headerの注釈を隠さない。[^issue-564-design][^issue-564-code]

標準ライブラリのimport aliasは既存の名前対応で判定するため、`Sequence as list`のような綴りも保持する。nullable除去とcollection変異の条件は変更しない。複数型引数内の対象外構文を検査する#565と、typing aliasの後続再代入を扱う#558は別の変更として扱う。[^issue-564-design]

[^issue-564-design]: [Issue 564: nullable annotation builtin provenance](../../superpowers/specs/2026-09-24-issue-564-nullable-provenance.md)。
[^issue-564-code]: [nullable追加の名前解決](../../../crates/hoimin-cli/src/analyzer/rust.rs)。

### 複数型引数とunpackの再帰検査（Issue #565）

型演算子の共通除外条件は、Tuple・Listの全要素とStarredの値を再帰的に検査する。dictのkey/value、複数段の型引数、tuple/listのunpackにAnyなどの対象外要素が含まれる場合、注釈全体の候補を除く。対象内だけの複数引数やunpackは保持する。この条件はnullable追加・削除と五つのcollection/iterable演算子に共通する。[^issue-565-design]

Leanの子孫検査モデルと公開planの対応を通常の回帰テストへ追加する。名前の再束縛に関する#564の修正を併合し、その4入力も期待値を保持したstrict検証へ移行する。65入力すべてを照合し、report-only指定は拒否する。検証範囲と過去の不一致は[監査記録](../audits/nullable-gates-2026-09.md)を参照する。[^issue-565-design]

[^issue-565-design]: [Issue 565: recursively exclude disallowed annotation arguments](../../superpowers/specs/2026-09-24-issue-565-annotation-descendants.md)。

## ユーザ定義例外の継承関係（Issue #692）

`exception_hierarchy` は明示指定する演算子で、プロジェクトの Python ファイルを実行せずに例外クラスと明示 import を収集する。既存の名前で参照できる直接のユーザ定義親子、または同じユーザ定義の直接親を持つ兄弟を、`except`・`except*`・`raise` の置換候補にする。`raise` は継承経路全体で `Exception` のコンストラクタを継承する組に限る。既定の演算子集合は変更せず、import の追加もしない。[^issue-692-hierarchy]

索引用の入力には、変異対象に選ばれていない依存ファイルも含める。これらの追加・削除・内容変更を fingerprint で検出し、plan・run・verify の入力整合性を確認する。条件付き定義、多重継承、再 export、外部パッケージなどの解析範囲と資源上限は原設計を参照する。[^issue-692-hierarchy]

別名と属性書き込みの追跡にはスコープ番号と正規化名の組を使い、無関係な同名引数をモジュールの例外クラスと混同しない。global・nonlocal・自由変数を解決し、メソッドの暗黙の `__class__` は所属クラスへ結び付ける。解析を打ち切った参照は理由・件数・最初の位置を一つの診断に集約し、関連クラスなしやコンストラクタ条件による除外とは区別する。[^issue-692-scopes]

この変更の最終実装 `c5f288c` では、全体テスト2,601件成功・22件スキップ、Lean の18定理、公開160ケース・内部215ケースの照合を記録した。設計・計画・実装・テストの各段階のセルフレビューと、独立レビューで見つけた `__class__` 書き込みの修正も監査記録に残す。モデル上の証明は Rust 全体の正しさやプロセスのピークメモリ量の保証を意味しない。関数の引数・戻り値やコンテナを介する別名、型パラメータの注釈スコープなどはモデルの対象外である。[^issue-692-verification]

この節は2026-10-05に出典と照合した。PR作成時に追加したのはカタログと参照検査であり、上記の実装テストを再実行したという記録ではない。名前解決、継承・コンストラクタ条件、依存入力の探索、診断条件を変更するときは、これらの設計・テスト・Lean の対応範囲も再確認する。

[^issue-692-hierarchy]: [2026-10-05-issue-692-exception-hierarchy.md](../../superpowers/specs/2026-10-05-issue-692-exception-hierarchy.md)。

[^issue-692-scopes]: [2026-10-05-exception-scope-outcomes.md](../../superpowers/specs/2026-10-05-exception-scope-outcomes.md)。

[^issue-692-verification]: [2026-10-05-exception-scope-lean-audit.md](../../superpowers/reports/2026-10-05-exception-scope-lean-audit.md)。

# 呼び出し式文の削除（Issue #696）

`statement_delete` は既定で有効で、独立した呼び出し式文を `pass` に置換する。代入式、await、yield、yield from を含む呼び出しは除外する。[^issue-696]

Lean の証明は、与えられたノード分類に対する適用条件と単一置換のモデルを対象とする。Rust の解析器そのものの証明ではない。文字コード、構文、保存 plan の検証は公開 CLI と CPython 3.14 のテストで別途確認する。[^issue-696]

[^issue-696]: [設計・証明範囲](../../superpowers/reports/issue-696/design.md)、[実装計画](../../superpowers/reports/issue-696/plan.md)。

# 通常の整数リテラルの隣接値（Issue #697）

`integer_literal_neighbor` は既定で有効で、通常の十進整数と単項負号付き整数を隣接値に変更する。絶対値の上限は u64::MAX で、範囲外の隣接値は生成しない。添字・スライス、型式、パターン、代入・削除ターゲットは対象外とし、括弧付き置換で優先順位を保つ。Lean は隣接値の算術モデルを証明し、構文・AST との対応は別途テストする。[^issue-697]

[^issue-697]: [設計と証明範囲](../../superpowers/reports/issue-697/design.md)。

# if/elif 条件の定数化（Issue #698）

`condition_constant` は明示選択時だけ if/elif の条件全体を True/False に置換し、条件の評価と副作用を省く。既存の真偽値リテラル、代入式・await・yield を含む条件は除外する。focused profile の main guard 除外も維持する。Lean の分岐選択モデルと公開 CLI の検証を区別する。[^issue-698]

[^issue-698]: [設計とモデル範囲](../../superpowers/reports/issue-698/design.md)。

# 関数本体の空化（Issue #699）

`function_body_erase` は明示選択時だけ、値を返さない同期関数の本体を pass に置換する。docstring・シグネチャ・デコレータを残し、generator・特殊名・空の本体は除外する。行選択の起点は最初に消す文で、シンボルは外側の対象関数を使う。生存から実行の有無や同値性を判定せず、細かな演算子の代用ともしない。[^issue-699]

[^issue-699]: [設計と制限](../../superpowers/reports/issue-699/design.md)、[モデル・検証範囲](../../superpowers/reports/issue-699/review.md)。

明示選択の `enum_member_replace` は、同一モジュールの確定した Enum 定義を索引化し、異なる値の代表メンバーへ属性名だけを置換する。同値の別名は候補を増やさない。名前の隠蔽、動的な名前空間操作、未対応の定義や値は保守的に除外する。識別子の元の綴りを保存し、解析器が値を区別できないサロゲート文字列は診断する。[^issue-700]

[^issue-700]: [Enum の設計と制約](../../superpowers/reports/issue-700/design.md)・[レビューと検証](../../superpowers/reports/issue-700/review.md)

`augmented_to_assignment` は単純な名前への複合代入だけを対象に、演算子トークンを `=` に置換する。旧値の読み出しと in-place 演算がなくなることを変異の意味に含める。右辺・空白・コメントは保存し、既存の演算子置換候補と共存する。[^issue-701]

[^issue-701]: [複合代入置換の設計](../../superpowers/reports/issue-701/design.md)・[レビューと検証](../../superpowers/reports/issue-701/review.md)

`return_tuple_swap` は同期・非ジェネレーター関数が直接返す2要素タプルの単純な要素を交換する。AST の要素範囲を使い、区切り・コメント・括弧を保存してタプル全体を1範囲として置換する。名前の実行時型の一致は仮定しない。入れ子関数の本体と、その場で評価される引数初期値などのスコープを区別する。[^issue-702]

[^issue-702]: [戻り値タプル交換の設計](../../superpowers/reports/issue-702/design.md)・[レビューと検証](../../superpowers/reports/issue-702/review.md)

`string_literal_empty` は単一トークンの非空 str リテラル全体を空文字列に置換する。空判定には復号した値を使い、docstring・型の式・パターン・補間文字列を除外する。明示的な TypeAlias マーカーは引用・括弧を含め保守的に認識する。通常のメッセージ文字列を関数名の綴りだけで除外しない。[^issue-703]

[^issue-703]: [文字列の空文字化の設計](../../superpowers/reports/issue-703/design.md)・[レビューと検証](../../superpowers/reports/issue-703/review.md)

while_condition_false は while 条件全体を偽にし、条件評価と本体を省略する。else は保持し、真偽値定数と束縛・中断を含む条件を除外する。 [^issue-704]

[^issue-704]: [設計](../../superpowers/reports/issue-704/design.md)

condition_clause_delete は if/elif の最上位 BoolOp から1項だけ除き、残りの式の順序と木構造を保つ。短絡評価の変化を含む変異であり、束縛・中断を含む条件は除外する。 [^issue-705]

[^issue-705]: [設計](../../superpowers/reports/issue-705/design.md)

container_element_delete は list・括弧付き tuple・dict から1要素を除く。型、残る式の順序、dict の組を保持し、型式・パターン・代入ターゲットと展開・束縛・中断を含むリテラルを除外する。 [^issue-706]

[^issue-706]: [設計](../../superpowers/reports/issue-706/design.md)

conversion_call_remove は builtin と解決できる単純名の変換呼び出しを括弧付き引数に置換し、引数評価を1回残す。変換・コピー等は省略し、再束縛・展開・generator・束縛・中断・型式・代入ターゲットは除外する。 [^issue-707]

[^issue-707]: [設計](../../superpowers/reports/issue-707/design.md)

optional_keyword_delete は一意な同一モジュール関数への呼び出しの引数束縛を検証し、default のある明示キーワードを1つ除く。残る引数の順序と既存 default を保ち、曖昧な束縛や関数の流出、型式、代入ターゲットを除外する。 [^issue-708]

[^issue-708]: [設計](../../superpowers/reports/issue-708/design.md)

# 既定の演算子選択（2026-10-07）

`statement_delete`、`integer_literal_neighbor`、`augmented_to_assignment`、
`return_tuple_swap`、`string_literal_empty`、`while_condition_false`、
`conversion_call_remove` の7種類を最初に既定化し、43から50種類へ増やした。
`--exclude-operators` で個別に無効化できる。明示選択と保存済み plan は展開し直さず、
`all_legacy()` は従来の43種類を保持する。候補数・実行時間・上限内に残る候補は変化し得る。
残る6種類は、粗い変異との重複、要素数に応じた候補増加、追加の名前解決を理由に段階的な導入とした。
過去の Issue 別文書の opt-in 記述は導入時の履歴であり、現在の既定値は本節に従う。[^default-operators]

[^default-operators]: [設計](../../superpowers/reports/default-analyzer-operators/design.md)、[実装計画](../../superpowers/plans/2026-10-07-default-analyzer-operators.md)、[レビューと検証](../../superpowers/reports/default-analyzer-operators/review.md)。

既定値の選択規則を Lean モデルで証明し、公開 CLI で除外・明示選択・保存済み plan を検証した。モデルの証明範囲と実行結果は別々に記録している。[^default-operators-review]

[^default-operators-review]: [レビューと検証](../../superpowers/reports/default-analyzer-operators/review.md)。

`method_call_remove` は引数のない属性呼び出しを括弧付き receiver に置換する演算子である。receiver の評価を1回残し、属性参照と呼び出しを除く。bound method であることや型の一致は保証せず、束縛・中断・generator を含む receiver、型式、代入ターゲットなどは除外する。[^method-call-remove]

[^method-call-remove]: [設計と5回のレビュー](../../superpowers/reports/method-call-remove/design.md)、[評価と形式証明の範囲](../../superpowers/reports/method-call-remove/assessment.md)

`function_body_return_constant` は、組込みの `bool`・`int`・`str` と確定できる直接の戻り値注釈に応じて、同期関数の本体を相補的な定数 return に置き換える演算子である。docstring とヘッダーを残し、async・generator・dunder・自身の値 return がない関数・同じリテラルだけを返す候補を除外する。注釈の解決には遅延評価用の名前解決を使う。注釈は実行時型の保証ではなく、生存変異だけから pseudo-tested とも判定しない。[^function-body-return-constant]

[^function-body-return-constant]: [設計](../../superpowers/reports/function-body-return-constant/design.md)、[原論文・評価・形式証明の範囲](../../superpowers/reports/function-body-return-constant/assessment.md)

現在は `method_call_remove` と `function_body_return_constant` も既定で有効にし、合計52種類としている。`--exclude-operators method_call_remove,function_body_return_constant` で、この2種類を追加する前の50種類へ戻せる。明示選択・保存済み plan・従来の43種類を返す `all_legacy()` は拡張しない。元の機能設計書の opt-in 記述は導入時の方針であり、今回の依頼によって更新した。[^recent-analyzer-defaults]

[^recent-analyzer-defaults]: [既定化の設計](../../superpowers/reports/recent-analyzer-defaults/design.md)、[形式証明の範囲](../../superpowers/reports/recent-analyzer-defaults/formal-audit.md)
