---
type: Playbook
title: 入力形状別の性能検証
description: 決定的な回帰ゲートとrelease計測、未統合の依存、証拠の読み方を区別する。
status: draft
catalog_revision: 165a2d284a1af92eb02ffd214ba8c0070c2f3808
sources:
- id: expanded-measurement
  resource: ../../performance/2026-09-14-issue-491-expanded-measurement.json
  working_tree: untracked
  sha256: 1a75393f131903065634e61a8ed03d945acaeada8dce277df36d3074b2722013
- id: rust-cost
  resource: ../../superpowers/reports/2026-09-14-issue-491-rust-cost-review.md
  working_tree: untracked
  sha256: 1337e928052a03ca8080674b8f76ef50d7b5cba9e14b5399f6a5ecce77f7c185
- id: axis-review
  resource: ../../superpowers/reports/2026-09-14-issue-491-input-axis-review.md
  working_tree: untracked
  sha256: a04236891ab5f2058f8c65d6deb915405a7a6066dff7cc543d93c0a55c35b8c5
- id: cost-worksheet
  resource: ../../superpowers/reports/2026-09-14-issue-491-cost-correspondence-worksheet.md
  working_tree: untracked
  sha256: 199af4da50a8332e620e5a23ea45e23e22bda523c967834dc5999b05aef83dcd
- id: cost-review
  resource: ../../superpowers/reports/2026-09-14-issue-491-operation-cost-review.md
  working_tree: modified
  revision: f14aa1b34ec6d166882e720661aee0fceb3543ee
  sha256: 90036e84a9987231975f14b7d20525dcd2bea3a290d3d7aa66c9a98c552526f6
- id: cost-design
  resource: ../../superpowers/specs/2026-09-14-issue-491-operation-cost-design.md
  working_tree: untracked
  sha256: 7115aa8e9accf16b3013af153173679f558850b092a4d749ec2453d3a6eee010
- id: integration
  resource: ../../performance/2026-09-14-integration-check.md
  working_tree: untracked
  sha256: e11e40961e7b00a5e7781f87b0bec36123798830da6d9ebc3e2731a64cd62fce
- id: guide
  resource: ../../performance/README.md
  working_tree: modified
  revision: f14aa1b34ec6d166882e720661aee0fceb3543ee
  sha256: 6b53ad8f5bb11e7fd83d94abf15b8490b2b77341d3f9cdfaafa05429ce2baa80
- id: registry
  resource: ../../performance/shapes.json
  working_tree: modified
  revision: f14aa1b34ec6d166882e720661aee0fceb3543ee
  sha256: 128a6cfce3b1d25081a86018903086b6c3282ee5b11612b71f47616168c66525
- id: design
  resource: ../../superpowers/specs/2026-09-14-issue-491-performance-shapes-design.md
  working_tree: modified
  revision: da6b9cf5fd6eff71438c503b0b99a15571b5e696
  sha256: 89fb01871fdbcd7cfdb3a4f1fca9c3d30b2251e9656c7fe4a90c93a141eac1bf
- id: review
  resource: ../../superpowers/reports/2026-09-14-issue-491-performance-shapes-review.md
  working_tree: modified
  revision: da6b9cf5fd6eff71438c503b0b99a15571b5e696
  sha256: 903abf4a3d7010cd791a463fb8d1393583440e1150464ec4ec07bc28e85ad1f9
---

# 入力を増やす軸と測定指標

性能検証の台帳は、対象発見、fingerprint、ソース配置、AST、解析状態、verify、出力、workspaceの8次元を11形状へ対応付ける。同じ形状をN/2N/4Nで作るが、入力構造の説明と修正後の期待値は別欄である。[^registry][^design]

# 通常ゲートとrelease計測

通常ゲートはactiveなRustテストを実行し、0件一致やignoredだけの成功を認めない。pendingな依存テストは成功数へ含めない。release計測は新規の出力先へbinary digest、入力、stdout/stderr、時間、RSSと比較結果を保存する。時間には監視の固定費が含まれ、未観測のRSSはnullである。retained heap、allocator peak、sampled RSSは別の指標として読む。[^guide][^registry]

# 観測と残る作業

今回のmacOS実行では11形状の198回比較と最終top1追試18回が意味検証を通過した。既存Leanのbuild・感度・freshnessとRust adapterも成功したが、新しいコストモデルを追加したわけではない。全8次元の漸近的回帰がCIで阻止されること、全体定数メモリ、別OSでの性能は結論しない。[^review]

依存修正を取り込むときは、テスト名、実際の計数点、モデルの仮定を照合し、ゲートを実行してからpendingをactiveへ変更する。入力上限、測定指標、CLI出力契約、監視方式を変更した場合も台帳と本ページを再確認する。[^guide]

[^guide]: [README.md](../../performance/README.md)。
[^registry]: [shapes.json](../../performance/shapes.json)。
[^design]: [2026-09-14-issue-491-performance-shapes-design.md](../../superpowers/specs/2026-09-14-issue-491-performance-shapes-design.md)。
[^review]: [2026-09-14-issue-491-performance-shapes-review.md](../../superpowers/reports/2026-09-14-issue-491-performance-shapes-review.md)。

# 個別修正の統合確認

全10件のRust差分をローカルで組み合わせ、追加テスト46件の欠落がないことと、全workspace試験1,814件の成功を確認した。これは初回PR公開時の統合確認であり、この結果だけではpendingをactiveに変更しなかった。順次マージ時には依存実装を取り込み、21ゲートすべての実行成功によりactiveへ昇格した。[^review][^integration]

[^integration]: [2026-09-14-integration-check.md](../../performance/2026-09-14-integration-check.md)。

[^cost-design]: [2026-09-14-issue-491-operation-cost-design.md](../../superpowers/specs/2026-09-14-issue-491-operation-cost-design.md)。

[^cost-review]: [2026-09-14-issue-491-operation-cost-review.md](../../superpowers/reports/2026-09-14-issue-491-operation-cost-review.md)。

[^cost-worksheet]: [2026-09-14-issue-491-cost-correspondence-worksheet.md](../../superpowers/reports/2026-09-14-issue-491-cost-correspondence-worksheet.md)。

[^axis-review]: [2026-09-14-issue-491-input-axis-review.md](../../superpowers/reports/2026-09-14-issue-491-input-axis-review.md)。

[^rust-cost]: [2026-09-14-issue-491-rust-cost-review.md](../../superpowers/reports/2026-09-14-issue-491-rust-cost-review.md)。

[^expanded-measurement]: [2026-09-14-issue-491-expanded-measurement.json](../../performance/2026-09-14-issue-491-expanded-measurement.json)。


# 追加のコスト対応と入力軸

Issue #491の追加検証では、既存21ゲートを保持し、実際のtree更新・照会、annotation callback中の全状態clone、replacement builderの回数・bytesをLean生成56ケースと照合する。候補保持の既存証明と、並べ替え・allocationを含まない操作数の上界は別の契約である。counterの観測には内部seamが必要なためinternal-fixtureとし、公開CLIの保証へ昇格させない。[^cost-design][^cost-worksheet][^rust-cost]

preflightのallocator peakはmanifest entriesと既存hasherの最大1ファイル分のbufferを許容し、全ファイルをworker数だけ保持する対照を検出する。作成後retained heap、preflight peak、sampled RSSを別の測定値として扱う。新しい5ゲートを含む26ゲートと、default/contracts両方の追加counter試験が成功した。[^cost-review][^rust-cost]

release laneは対象ファイル・selector、fingerprint files/bytes、Unicode、AST幅と左深さ、多file・部分verify、record長、worker数を追加し、29形状のN/2N/4Nを各binaryで3回実行した。今回の522回は全件で意味検証を通過した。macOSの時間・RSSと出力bytesを保存するが、全入力の漸近上界や他OSの性能は結論しない。[^axis-review][^expanded-measurement]


progressの通常ゲートは1reportの内容500/1,000/2,000件と履歴2/4/8を独立に増やし、実際のserialized bytesを記録する。元の2,000件×16履歴も保持し、各reportサイズで履歴2からの追加peakを512KiB以下に制限する。候補対応数・score・停滞判断を同時に検査し、実際に全履歴のparsed reportを保持する対照は同じ上界を超えた。このallocator検証は29形状・522回のrelease実測とは別の結果である。[^cost-review][^registry]
