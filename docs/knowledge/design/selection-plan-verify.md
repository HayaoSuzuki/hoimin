---
type: Contract
title: 対象選択・plan・verifyの契約
description: 候補発見と実行の分離、ランキング、部分集合、保存形式の版の不整合を整理する。
status: draft
catalog_revision: a7daea0b557cd435c1e55b540392fbdd116348e1
sources:
- id: issue-459
  resource: ../../superpowers/specs/2026-09-11-issue-459-record-size-design.md
  working_tree: untracked
  sha256: ced8501765b2babcd15bac56139f575c00cdb779a5149bb4a6e231ea75e0af7f
- id: initial
  resource: ../../superpowers/specs/2026-07-21-agent-plan-verify-design.md
  revision: a7daea0b557cd435c1e55b540392fbdd116348e1
  working_tree: clean
- id: ranked
  resource: ../../superpowers/specs/2026-07-27-ranked-plan-top-verify-design.md
  revision: a7daea0b557cd435c1e55b540392fbdd116348e1
  working_tree: clean
- id: diverse
  resource: ../../superpowers/specs/2026-09-08-issue-433-diverse-selection-design.md
  revision: a7daea0b557cd435c1e55b540392fbdd116348e1
  working_tree: clean
- id: readme
  resource: ../../../README.md
  revision: a7daea0b557cd435c1e55b540392fbdd116348e1
  working_tree: clean
- id: development
  resource: ../../development.md
  revision: a7daea0b557cd435c1e55b540392fbdd116348e1
  working_tree: clean
- id: plan
  resource: ../../../crates/hoimin-cli/src/plan.rs
  revision: a7daea0b557cd435c1e55b540392fbdd116348e1
  working_tree: clean
---

# 候補発見と実行の分離

`plan` は変異候補と実行条件を記録したファイル（manifest）を作る。この段階では、元コードのテスト実行（baseline）、テスト用コピーの作成、結果保存用sessionの作成を行わない。[^initial][^readme]

続く `verify` は保存した条件で候補を実行する。対象ソースと、利用者が変更検知用に明示した入力（fingerprint入力）が変わっていないかをbaseline前に確認する。baselineは毎回実行し、plan経由ではsessionの結果を再利用しない。manifestには実行条件が含まれるため、利用者が信頼するローカルのファイルを使う契約である。[^initial][^readme]

# 対象を絞る指定の組合せ

対象ファイルを選んだ後、行範囲や関数・クラスなどのシンボル指定で、そのファイル内の範囲を狭める。`--changed` は明示的に選んだ対象とGitの変更行との共通部分を取る。複数指定を組み合わせる場合は、それぞれ単独で動くことに加え、範囲を交差させた結果も確認する必要がある。[^readme]

# 順位と実行集合

- `--candidate` は候補ID、`--top` は上位件数を指定する。両者は排他的で、どちらかの選択が必要である。
- `strict` は保存した順位の先頭から選ぶ。`diverse` は同点の候補群の中でファイルを順に巡回する。点数が高い候補群を先に消費する順序は保つ。
- `verify` は順位を再計算せず、planの実行上限を引き継ぐ。
- 候補発見を途中で打ち切った `truncated` なplanでは、上位N件は保持済み候補内の上位である。未発見の候補を含む全体の上位とは判定できず、verifyも完全実行とは報告しない。[^ranked][^readme]

`diverse` では、同点の候補をファイルごとの待ち行列（queue）に分ける。後続設計は、空になった待ち行列を巡回から外し、偏った分布で候補のないファイルを繰り返し調べる操作を減らす。総候補数をN、選ぶ件数をKとすると、待ち行列の作成はO(N)、選択時のqueue操作はO(K)を目標とする。選択順序を保つことが条件であり、この性能上の議論は同点の候補が特定ファイルに偏る場合を扱っている。[^diverse]

# manifest版の記述と実装の不一致

| 出典 | 記載・静的に観測した値 |
| --- | --- |
| 7月21日初期設計 | plan manifest v1 |
| README・開発資料 | plan v2の記述。開発資料ではランキング規則の版をv3として区別 |
| 現在の `plan.rs` | `PLAN_SCHEMA_VERSION = 3`。読込み時にこの値との一致を要求 |

初期設計から版が変わったことに加え、現在の公開文書と実装にも不一致がある。上表はコードを読んだ結果であり、CLIで旧版を入力する試験は行っていない。したがって、v2を現行の受理形式として案内できない。保存形式の版とランキング規則の版も別々に確認する。[^initial][^readme][^development][^plan]

# 関連する監査と再確認条件

複数の対象指定や打切り条件は[境界監査](../audits/boundary-2026-09.md)、候補順序と保持上限は[解析器](analyzer.md)を参照する。保存形式、順位規則、候補集合、変更検知用入力、verifyの準備段階を変更したら、設計・実装・公開説明を再照合する。

[^initial]: [2026-07-21-agent-plan-verify-design.md](../../superpowers/specs/2026-07-21-agent-plan-verify-design.md)。
[^ranked]: [2026-07-27-ranked-plan-top-verify-design.md](../../superpowers/specs/2026-07-27-ranked-plan-top-verify-design.md)。
[^diverse]: [2026-09-08-issue-433-diverse-selection-design.md](../../superpowers/specs/2026-09-08-issue-433-diverse-selection-design.md)。
[^readme]: [README.md](../../../README.md)。
[^development]: [development.md](../../development.md)。
[^plan]: [plan.rs](../../../crates/hoimin-cli/src/plan.rs)。

# 候補の保存サイズ上限（Issue #459）

planの候補にも実行用spoolと同じ2 MiBのレコード上限を適用する設計とした。JSONのエスケープとUTF-8、および末尾の改行1バイトを含むサイズで判定する。plan生成とverifyの事前検証で超過を拒否し、直接runする場合は既存のbaseline後の解析段階で不完全な実行として報告する。[^issue-459]

[^issue-459]: [Issue #459: Executable candidate record limits](../../superpowers/specs/2026-09-11-issue-459-record-size-design.md)。
