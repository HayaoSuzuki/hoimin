---
type: Contract
title: 対象選択・plan・verifyの契約
description: 候補発見と実行の分離、ランキング、部分集合、保存形式の版の不整合を整理する。
status: draft
catalog_revision: a7daea0b557cd435c1e55b540392fbdd116348e1
sources:
- id: issue-475
  resource: ../../superpowers/specs/2026-09-14-issue-475-range-normalization-design.md
  sha256: a73a0b51c51b10d1d958585acbc0455d3c9d18ec340ef6acec053c2c5427081f
- id: issue-477
  resource: ../../superpowers/specs/2026-09-11-issue-477-import-roots-design.md
  working_tree: untracked
  sha256: a30ed5f129c635e75431212d590171ab03e5973640a4d6624775ee43fd81dcdd

- id: issue-473
  resource: ../../superpowers/specs/2026-09-11-issue-473-symbol-ranking-design.md
  working_tree: untracked
  sha256: d1748256ef730d871a2158c57e8c5601145d78e32288c269cac720f60ad10618

- id: issue-459
  resource: ../../superpowers/specs/2026-09-11-issue-459-record-size-design.md
  working_tree: untracked
  sha256: ced8501765b2babcd15bac56139f575c00cdb779a5149bb4a6e231ea75e0af7f

- id: issue-452
  resource: ../../superpowers/specs/2026-09-11-issue-452-shared-exclusions-design.md
  working_tree: untracked
  sha256: c61ec9ebe8ec989d0e39ede76d57a3513ccb5768631b6e081275448a92e2b89a
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

# 明示symbolの子要素とランキング

Issue #473 の設計では、明示symbolと同じファイルにある子symbolにも `explicit_symbol` の250点を一度だけ加える。`Box` は `Box.check` や `Box.Inner.check` に一致し、`BoxOther` には一致しない。親子のselectorが複数一致しても加点を重ねず、別ファイルの同名symbolには適用しない。[^issue-473]

順位の意味が変わるため、ランキング規則の版を3から4へ進める。保存形式のschema版は実装の3を維持する。旧ランキング版のplanはbaseline前に拒否して再生成を案内し、保存済みの順位を暗黙に変更しない。公開planの順位とverify --topの実行候補を照合する。既存Lean oracleの明示symbol入力は真偽値であり、今回の名前階層の解決自体を証明しているわけではない。[^issue-473]

# カタログ作成時のmanifest版の不一致

| 出典 | 記載・静的に観測した値 |
| --- | --- |
| 7月21日初期設計 | plan manifest v1 |
| README・開発資料 | plan v2の記述。開発資料ではランキング規則の版をv3として区別 |
| カタログ対象の `plan.rs` | `PLAN_SCHEMA_VERSION = 3`。読込み時にこの値との一致を要求 |

初期設計から版が変わったことに加え、カタログ作成時の公開文書と実装にも不一致があった。上表はコードを読んだ結果であり、CLIで旧版を入力する試験は行っていない。この表は当時の不一致の記録であり、Issue #473 の設計では現行資料をschema3・ranking4へ揃える。保存形式の版とランキング規則の版も別々に確認する。[^initial][^readme][^development][^plan]

# 関連する監査と再確認条件

複数の対象指定や打切り条件は[境界監査](../audits/boundary-2026-09.md)、候補順序と保持上限は[解析器](analyzer.md)を参照する。保存形式、順位規則、候補集合、変更検知用入力、verifyの準備段階を変更したら、設計・実装・公開説明を再照合する。

# Issue 477: import設定の保存

`--import-root` を対象選択から独立した順序付き設定としてplanに保存し、verifyへ引き継ぐ。import rootだけでは変異対象を指定したことにならない。plan schemaは実装のv3からv4へ進め、旧planはbaseline前に拒否して再生成を案内する。この独立ブランチのranking規則はv3を維持する。以前の資料の版表記はその時点の記録として扱う。[^issue-477]

[^initial]: [2026-07-21-agent-plan-verify-design.md](../../superpowers/specs/2026-07-21-agent-plan-verify-design.md)。
[^ranked]: [2026-07-27-ranked-plan-top-verify-design.md](../../superpowers/specs/2026-07-27-ranked-plan-top-verify-design.md)。
[^diverse]: [2026-09-08-issue-433-diverse-selection-design.md](../../superpowers/specs/2026-09-08-issue-433-diverse-selection-design.md)。
[^readme]: [README.md](../../../README.md)。
[^development]: [development.md](../../development.md)。
[^plan]: [plan.rs](../../../crates/hoimin-cli/src/plan.rs)。

Issue #473と#477を統合した状態では、plan schemaは4、ranking ruleも4となる。前者はimport rootの保存形式、後者はsymbolの子孫への加点規則を表す独立した版である。個別の設計書にある版は、その設計時点の記録である。[^issue-473][^issue-477]

[^issue-477]: [2026-09-11-issue-477-import-roots-design.md](../../superpowers/specs/2026-09-11-issue-477-import-roots-design.md)。

[^issue-473]: [2026-09-11-issue-473-symbol-ranking-design.md](../../superpowers/specs/2026-09-11-issue-473-symbol-ranking-design.md)。

# 候補の保存サイズ上限（Issue #459）

planの候補にも実行用spoolと同じ2 MiBのレコード上限を適用する設計とした。JSONのエスケープとUTF-8、および末尾の改行1バイトを含むサイズで判定する。plan生成とverifyの事前検証で超過を拒否し、直接runする場合は既存のbaseline後の解析段階で不完全な実行として報告する。[^issue-459]

[^issue-459]: [Issue #459: Executable candidate record limits](../../superpowers/specs/2026-09-11-issue-459-record-size-design.md)。

# 対象探索とコピーの組込み除外（Issue #452）

仮想環境やキャッシュなどの組込み除外を対象探索とコピーで共有する設計とした。`--include` は組込み除外を解除しない。除外場所のファイルを `--file` または `--line` で指定した場合は、対象パスと除外場所の外を選ぶ対処方法を示して対象解決時に拒否する。[^issue-452]

[^issue-452]: [Issue #452: Shared workspace exclusions](../../superpowers/specs/2026-09-11-issue-452-shared-exclusions-design.md)。

# 明示した行範囲とsymbolの正規化（Issue #475）

同じファイルへ指定した行範囲とsymbolは、selectorを検証しながら収集し、全selectorの解決後にファイルごとに一度正規化する。行範囲は開始位置で整列して重複・重なり・隣接を統合し、symbolは整列して重複を除く。入力途中の不正な行範囲とパスの診断順は変えない。[^issue-475]

この処理は明示selectorの解決だけを対象とする。Git変更行との交差、候補ランキングの行索引、対象ファイルの発見に使う別の正規化処理には適用しない。[^issue-475]

[^issue-475]: [Issue 475: Normalize explicit selector groups once](../../superpowers/specs/2026-09-14-issue-475-range-normalization-design.md)。
