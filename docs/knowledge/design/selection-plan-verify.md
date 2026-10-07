---
type: Contract
title: 対象選択・plan・verifyの契約
description: 候補発見と実行の分離、ランキング、部分集合、保存形式の版の不整合を整理する。
status: draft
catalog_revision: a7daea0b557cd435c1e55b540392fbdd116348e1
sources:
- id: issue-454
  resource: ../../superpowers/specs/2026-09-14-issue-454-fixed-batches-design.md
  working_tree: untracked
  sha256: 10e05c4a8d518d5334e191a2999d2bd9f5c198569a3db650969f51adcc097dd1

- id: issue-599
  resource: ../../superpowers/specs/2026-09-25-issue-599-verify-preview-design.md
  revision: 99655f08bb941db0e9e888f3b5d373ec6f11440e
  working_tree: clean
  sha256: c024231b308a39d92f38fe7035630d6f364be449c80f98267197933fcf286d93

- id: issue-599-preview
  resource: ../../../crates/hoimin-cli/src/plan/preview.rs
  revision: 99655f08bb941db0e9e888f3b5d373ec6f11440e
  working_tree: clean
  sha256: 75fcd7faf4cf52262f120bb68912ae33f7e9728c3ed12eb1b329593e6d3c725b

- id: issue-599-schema
  resource: ../../json-schema/verify-preview.schema.json
  revision: 99655f08bb941db0e9e888f3b5d373ec6f11440e
  working_tree: untracked
  sha256: 8bdb3d181c6acfc34c99253135b14a07c5367b60f05e1f92f7e4908b533246e4

- id: issue-458
  resource: ../../superpowers/specs/2026-09-14-issue-458-verify-metrics-design.md
  working_tree: untracked
  sha256: 9348d92fc7357d35f6092240f58cd681672864de808daa41b989f0cb717c4887

- id: issue-480-symbol
  resource: ../../../crates/hoimin-cli/src/target/mod.rs
  working_tree: modified
  sha256: 97fc31c5b4eb885660de54c8711c4f187974e3c0b82ffca172a10f41ca7b2529
  revision: 98969d7a840362f78dceb12f91cc5188214f68d5
- id: issue-480-symbol-spec
  resource: ../../superpowers/specs/2026-09-14-issue-480-source-encoding-design.md
  working_tree: clean
  sha256: 55a99855ee216bfff5afdfd5ffdeeab6c13e1877ec751b0ee88c9a35cb78c37e
  revision: 98969d7a840362f78dceb12f91cc5188214f68d5
- id: issue-476-report
  resource: ../../superpowers/reports/2026-09-14-issue-476-symbol-diagnostics-review.md
  working_tree: untracked
  sha256: bff2a183f0c702c556d19c832b59836613d9e7a20975d89893d5d1c7fa0292ec
- id: issue-476-design
  resource: ../../superpowers/specs/2026-09-14-issue-476-symbol-diagnostics-design.md
  working_tree: untracked
  sha256: 9d9087b95cf13cedc4d75626d4cb87b93c41bb45f399a33dbb9a1cbe333c0092
- id: issue-476-target
  resource: ../../../crates/hoimin-cli/src/target/mod.rs
  working_tree: modified
  sha256: 2679747599bc69451f05128d6f6ab08fb22fb65460179d4f9a40d71e936aa3d5
  revision: 8b33167a049e3cae0fc05e96ccf2253c660b7023
- id: issue-476-analyzer
  resource: ../../../crates/hoimin-cli/src/analyzer/rust.rs
  working_tree: modified
  sha256: b4e03399f3d7e6786fcecb7c876bd94232a16bea5c7d61f98feb31fdd5f69c19
  revision: 8b33167a049e3cae0fc05e96ccf2253c660b7023
- id: issue-476-tests
  resource: ../../../crates/hoimin-cli/tests/plan.rs
  working_tree: modified
  sha256: 410ad372b3ddf1ac57afcb9d1197f3395e02bf8e492553e74b7759f6ee1a3f47
  revision: 8b33167a049e3cae0fc05e96ccf2253c660b7023
- id: issue-453
  resource: ../../superpowers/specs/2026-09-14-issue-453-scoped-discovery-design.md
  sha256: cce7701d3552cc3887c8129b01589736e6defd31389db98de76ac32b619527aa
  working_tree: clean
  revision: e59ce2726f71e7c366b8855664b5cce43a13c6a6
- id: issue-475
  resource: ../../superpowers/specs/2026-09-14-issue-475-range-normalization-design.md
  sha256: a73a0b51c51b10d1d958585acbc0455d3c9d18ec340ef6acec053c2c5427081f
  working_tree: clean
  revision: f3440ac435747d831db6a8b0d93758242060a8cf
- id: issue-474
  resource: ../../superpowers/specs/2026-09-14-issue-474-selector-index-design.md
  working_tree: untracked
  sha256: 0325588c52127c73cb41fbf33ef9d9e244915a6de4de5f6b3134b31312500ef6

- id: issue-456
  resource: ../../superpowers/specs/2026-09-14-issue-456-validation-context-design.md
  working_tree: untracked
  sha256: a3f08279a0c60e016cef85ba37827f2b3be4fd5006bbb2068cce2c75bdf8991b
- id: issue-600-design
  resource: ../../superpowers/specs/2026-09-25-issue-600-target-membership-design.md
  revision: fb7f1f1a44d94a8ea6e5e19dfba8efbb6dc656c1
  working_tree: clean
  sha256: cd568b65781ba9aee5262f8452e8516d707fcdd74081097fcca9855cd5a86aa9

- id: issue-456-review
  resource: ../../superpowers/reports/2026-09-14-issue-456-validation-context-review.md
  working_tree: untracked
  sha256: 5042ef83fb71837543be9fb358b2453931bdf70ce77f6033381e5756632b09f0
- id: issue-600-review
  resource: ../../superpowers/reports/2026-09-25-issue-600-target-membership-review.md
  working_tree: untracked
  sha256: d4823d9add9a31d43a0cd1e80be6126b0593b80e1ca5638e44f9f766dcb593fc
- id: issue-600-code
  resource: ../../../crates/hoimin-cli/src/plan.rs
  working_tree: clean
  revision: 5c5dbea5b97eb1a4a58be3e90c272ba86c61ab33
  sha256: 6c25091c136ce679c1d1b1d4530977afefa016c79322fcd5cd858c35c71fb44f

- id: issue-456-code
  resource: ../../../crates/hoimin-cli/src/plan.rs
  working_tree: modified
  sha256: c5fa356be1e661d21ca8fd1837eca63fa30cc472b4ac815391e8005f086e90e9
  revision: 165a2d284a1af92eb02ffd214ba8c0070c2f3808
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
  resource: ../../usage.md
  revision: 5a45bdc3bd444771c4c57e30fe211f174850ce93
  working_tree: untracked
  sha256: 6894a34743882cc26fdd1f39cb536c94dfc20d5f1dcf33b3cb7f3da9b13e6bef
- id: development
  resource: ../../development.md
  revision: a7daea0b557cd435c1e55b540392fbdd116348e1
  working_tree: clean
- id: plan
  resource: ../../../crates/hoimin-cli/src/plan.rs
  revision: a7daea0b557cd435c1e55b540392fbdd116348e1
  working_tree: clean
- id: issue-709-design
  resource: ../../superpowers/reports/issue-709/design.md
  working_tree: untracked
  sha256: 4b5d6005177bfb151a3a9a6dce053918c571ed970e9d2ed5c1761c2991773d21
- id: issue-709-plan
  resource: ../../superpowers/reports/issue-709/plan.md
  working_tree: untracked
  sha256: 4c475534f4f19a29ad98324afc6613cf6e897c1c1c7d3877205b2cd21e2f13eb
- id: issue-709-assessment
  resource: ../../superpowers/reports/issue-709/assessment.md
  working_tree: untracked
  sha256: c3fb86ebf25c5d4b8bfa2f9eb13fb57faf1d276c1e0e1d6706406da5d2b083fa
- id: issue-709-review
  resource: ../../superpowers/reports/issue-709/review.md
  working_tree: untracked
  sha256: 291049426d815f46c6b68d75c81415e80024165972fc60df65f0fcfb09ed9800
- id: issue-710-design
  resource: ../../superpowers/reports/issue-710/design.md
  working_tree: untracked
  sha256: 187d95a722fc212fa5748bd95b031035ecc34f89f43633a12d2b29e034d7701e

- id: issue-710-plan
  resource: ../../superpowers/reports/issue-710/plan.md
  working_tree: untracked
  sha256: 664949692555d984ab034f9468f745f958fb16b529dee9f0b75e10a30d060f7a

- id: issue-710-assessment
  resource: ../../superpowers/reports/issue-710/assessment.md
  working_tree: untracked
  sha256: 9e099f6d6125e40eed282924e41381fd0ed60d1a6f4f507dca3eeda14a21dece

- id: issue-710-review
  resource: ../../superpowers/reports/issue-710/review.md
  working_tree: untracked
  sha256: 37e8886b574a7226c882551b72683241c31bcd39ed0654f417c9569f63ed4e31

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
[^readme]: [利用方法](../../usage.md)。
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

# exact selectorの対象発見（Issue #453）

fileまたはlineだけを指定した場合、rootからの探索は指定ファイルとその親ディレクトリに限定する。root起点の探索、ignore/include/exclude、組込み除外、platformの大小文字規則は維持する。sourceまたはsymbolを含む場合は、対象列挙が必要なため全体探索を継続する。[^issue-453]

限定探索では、選択経路の外にあるportable pathへ変換できない名前を診断しない。指定ファイル、親経路、または全体探索で発見した不正な名前は従来どおり診断する。この境界により、無関係なファイル名がexact selectorの成否を左右しない。[^issue-453]

[^issue-453]: [Issue 453: Scope discovery for exact selectors](../../superpowers/specs/2026-09-14-issue-453-scoped-discovery-design.md)。

# 明示した行範囲とsymbolの正規化（Issue #475）

同じファイルへ指定した行範囲とsymbolは、selectorを検証しながら収集し、全selectorの解決後にファイルごとに一度正規化する。行範囲は開始位置で整列して重複・重なり・隣接を統合し、symbolは整列して重複を除く。入力途中の不正な行範囲とパスの診断順は変えない。[^issue-475]

この処理は明示selectorの解決だけを対象とする。Git変更行との交差、候補ランキングの行索引、対象ファイルの発見に使う別の正規化処理には適用しない。[^issue-475]

[^issue-475]: [Issue 475: Normalize explicit selector groups once](../../superpowers/specs/2026-09-14-issue-475-range-normalization-design.md)。

# 明示selectorから対象ファイルを引く索引（Issue #474）

明示したfile、line、symbolのmoduleを対象ファイルへ対応させる処理では、発見済みファイルからプラットフォーム別のパス同値キーを使ったPythonファイル索引を一度作る。各selectorはこの索引を検索する。発見済みファイル数をF、selector数をSとすると、順序付き索引の構築と検索に要する比較回数はO(F log F + S log F)となる。[^issue-474]

元の発見済みファイル一覧は、source rootからの列挙と出力順を決める正本として残す。Windowsで大小文字だけが異なる複数のパスが同じキーになる場合は、従来の順序で最初に見つかるPythonファイルの表記を索引に保存する。LinuxとmacOSで実行した試験からWindows実機の動作は判断しない。[^issue-474]

[^issue-474]: [Issue 474: Explicit selector file index](../../superpowers/specs/2026-09-14-issue-474-selector-index-design.md)。

# verifyのファイル単位の前処理（Issue #456）

要求候補のdescriptor検証では、同じファイルのハッシュ、UTF-8検証、行索引を一度だけ構築して共有する。ファイル単位で検証した結果を要求IDの従来の順序で取り出すため、複数ファイルにまたがるエラーの優先順を維持する。候補の内容・位置・stable IDと、後続の再発見による照合は省略しない。[^issue-456][^issue-456-code]

ソースbytesと借用contextは一ファイルの検証が終わると解放し、要求候補の参照と結果だけを保持する。ソース全体の前処理は要求ファイルのbytes合計に比例する。行内の列計算や再発見を含むverify全体の計算量を保証する変更ではない。操作回数とrelease計測、実行環境、未測定のメモリ指標は今回の検証記録を参照する。[^issue-456-review]

[^issue-456]: [2026-09-14-issue-456-validation-context-design.md](../../superpowers/specs/2026-09-14-issue-456-validation-context-design.md)。

[^issue-456-review]: [2026-09-14-issue-456-validation-context-review.md](../../superpowers/reports/2026-09-14-issue-456-validation-context-review.md)。

[^issue-456-code]: [plan.rs](../../../crates/hoimin-cli/src/plan.rs)。

# verifyの対象所属索引（Issue #600）

要求候補が選択対象に含まれるかの確認には、対象一覧から一度作る `HashSet<&Utf8Path>` を使う。キーに `Utf8Path` を借用することで、従来のパス成分による同値性を保つ。文字列への変換、大小文字の統一、ファイルシステム上の実体への解決は行わない。IDの存在確認、対象所属、ファイル読取り、descriptorとstable IDの検証という順序も維持する。[^issue-600-design][^issue-600-code]

対象数Fの索引構築と要求候補数Cの検索により、従来の候補ごとの全対象走査を除く。ハッシュ集合の通常の前提では所属確認はO(F+C)となるが、パス長とハッシュ衝突の費用、候補検証や再発見、verify全体の計算量を含む保証ではない。保存形式、ランキング、strict/diverseの選択は変更しない。[^issue-600-design]

操作回数の回帰テスト、パス同値性と複数エラーの順序確認、公開API・CLIの観測結果と限界は検証記録を参照する。[^issue-600-review]

[^issue-600-design]: [Issue #600: Index verify target membership](../../superpowers/specs/2026-09-25-issue-600-target-membership-design.md)。

[^issue-600-review]: [Issue #600: Target membership review and measurements](../../superpowers/reports/2026-09-25-issue-600-target-membership-review.md)。

[^issue-600-code]: [plan.rs](../../../crates/hoimin-cli/src/plan.rs)。

# 固定バッチの範囲選択（Issue #454）

`verify --top N --offset K` は、指定した選択規則による全体の順序からK件を飛ばし、続く最大N件を選ぶ。strictは保存順位、diverseは同点内でファイルを巡回する全体順序に対して範囲を適用する。同じplanと選択規則で隣接範囲を指定すれば、各範囲のID集合は重複しない。[^issue-454]

offsetは0始まりでtopとの併用を必須とし、候補ID指定とは併用しない。保持数以上のoffsetと空planはbaseline前に拒否する。末尾を越える件数は残りに切り詰め、max_mutantsは飛ばした件数を除く実際のバッチに適用する。truncated planの範囲は保持済み候補だけであり、完全実行の扱いへ変更しない。[^issue-454]

JSON/JSONLのmutant記録に実際の候補IDを残す。バッチの再実行では範囲と選択規則を保存し、同じID集合のレポートだけでprogress履歴を作る。異なるバッチのscoreや飽和判定は合成しない。[^issue-454]

[^issue-454]: [Issue 454: Fixed verify batches](../../superpowers/specs/2026-09-14-issue-454-fixed-batches-design.md)。

# 実行前の候補preview（Issue #599）

`verify PLAN --dry-run` は通常のverifyと同じplan・ソース・fingerprint・候補の検証を行い、選択結果を出力して終了する。baselineと変異テスト、workerコピー、session作成、実行metricsは発生しない。`--metrics` との併用は拒否する。有効なpreviewはtruncated planでも終了コード0、入力不正は2となる。実行時の資源確保までは確認しない。[^issue-599]

JSONとJSONLは、独立したschema 1の `verify_preview` オブジェクトを1件出力する。`candidates` 配列は選択順で、各行にID、保存rank、バッチ内の `selection_order`、path、lineを含む。rank・選択順・行番号は1始まりである。top指定の `offset` は0始まり、明示ID指定ではnullとなる。既存の選択metadataに加え、保持数を `retained_candidates` に記録する。[^issue-599-preview][^issue-599-schema]

strict/diverseの順序は既存の選択結果を使い、明示IDは候補の再発見順を使う。引数の順序や編集可能な保存sequenceには依存しない。選択順は実行予定の順序であり、並列workerの完了順や資源不足時の実行完了を保証しない。truncated planのpreviewは保持済み候補だけを対象とする。変異結果がないためprogress履歴には使わない。[^issue-599][^issue-599-preview]

[^issue-599]: [Issue 599: Verify selection preview](../../superpowers/specs/2026-09-25-issue-599-verify-preview-design.md)。
[^issue-599-preview]: [preview.rs](../../../crates/hoimin-cli/src/plan/preview.rs)。
[^issue-599-schema]: [verify-preview.schema.json](../../json-schema/verify-preview.schema.json)。

# verifyの運用metrics（Issue #458）

`verify --metrics PATH` は候補ID指定とstrict/diverseの上位選択に対応する。相対パスは呼出し時の作業ディレクトリを基準とし、planへ保存しない。出力先の正規化とshellのmetrics収集・保存はrunと共用する。出力設定以外の実行条件と候補選択は維持する。[^issue-458]

planの形式、候補、ソース、fingerprint、再発見の検証に失敗した場合、metrics出力先は変更しない。shell実行開始後のbaseline失敗、総時間制限、保存失敗はrunと同じ規則で扱う。metricsの各段階の時間には、先行するplan検証を含めない。[^issue-458]

[^issue-458]: [Issue 458: Verify operational metrics](../../superpowers/specs/2026-09-14-issue-458-verify-metrics-design.md)。

# 明示symbolの定義存在確認

Issue #476 の変更では、対象ファイルの解決後、Git変更行との交差前に、ASTの関数・クラス定義に指定qualnameが存在するかを確認する。メソッド、ネストした定義、async関数、packageの `__init__.py` を含む。代入名やimport先は定義とみなさない。不存在ならファイル、qualname、対応する元のselectorを含む診断で、run・plan・verifyをbaseline前にexit 2で拒否する。[^issue-476-design][^issue-476-target][^issue-476-analyzer]

定義が存在すれば、operator、profile、行範囲、Git差分によって候補がなくても有効である。候補上限やverifyの候補部分集合で検査対象を省略しない。構文不正は存在確認不能として扱い、不存在とは診断しない。今回の公開CLIテストでは、空のGit差分、別定義だけの変更、候補のない定義、verifyで実行候補のないファイルのselectorも確認する。[^issue-476-tests]

symbol指定ファイルは候補解析前にもparseする。既存のAST深さ制限を使うが、この対象解決時のI/Oとparseはanalyzer discovery timeoutの対象外であり、全処理の時間上限を新たに保証する変更ではない。[^issue-476-design]

[^issue-476-design]: [2026-09-14-issue-476-symbol-diagnostics-design.md](../../superpowers/specs/2026-09-14-issue-476-symbol-diagnostics-design.md).

[^issue-476-target]: [mod.rs](../../../crates/hoimin-cli/src/target/mod.rs).

[^issue-476-analyzer]: [rust.rs](../../../crates/hoimin-cli/src/analyzer/rust.rs).

[^issue-476-tests]: [plan.rs](../../../crates/hoimin-cli/tests/plan.rs).

今回の自己レビュー、検証結果と未確認範囲は[Issue #476 の報告](../../superpowers/reports/2026-09-14-issue-476-symbol-diagnostics-review.md)に記録する。[^issue-476-report]

[^issue-476-report]: [Issue #476: symbol diagnostics review](../../superpowers/reports/2026-09-14-issue-476-symbol-diagnostics-review.md).

# symbol定義確認での文字コード

Issue #480 は、Issue #476 の定義存在確認にも共通decoderを使う。Latin-1で書かれた関数名をUnicodeのqualnameとして照合し、候補解析と異なるUTF-8限定の読み込みを残さない。文字コードの対応範囲と元バイト位置の扱いは[解析契約](analyzer.md)を参照する。[^issue-480-symbol][^issue-480-symbol-spec]

[^issue-480-symbol]: [target/mod.rs](../../../crates/hoimin-cli/src/target/mod.rs).
[^issue-480-symbol-spec]: [Issue #480 design](../../superpowers/specs/2026-09-14-issue-480-source-encoding-design.md).

# 反復検証の導入判断（Issue #709）

反復実行の組み込みは保留とする。論文が測った個別テストの不安定化と、hoimin が観測するコマンド全体の終了状態は一致しない。固定候補の既存 verify 反復による限定試行では、制御された交互終了を観測できた一方、実パッケージの狭いチェックで分類の変化は得られなかった。これは不要・安定性の証明ではない。実利用で判断を変える終了状態の揺らぎと追加コストを記録し、必要性を再評価する。新しい CLI オプションや分類は追加していない。[^issue-709-assessment]

[^issue-709-design]: [design.md](../../superpowers/reports/issue-709/design.md)。

[^issue-709-plan]: [plan.md](../../superpowers/reports/issue-709/plan.md)。

[^issue-709-assessment]: [assessment.md](../../superpowers/reports/issue-709/assessment.md)。

[^issue-709-review]: [review.md](../../superpowers/reports/issue-709/review.md)。

# Issue #710: 開始行による選択の分散

`verify --top N --selection-policy line-diverse`は、同じscoreの候補をファイルと開始行でまとめ、保存rankに従う順で巡回する。上位scoreを先に処理し、全体の順序を決めてからoffset/topを適用する。複数行にまたがる変更は開始行のグループに属し、2巡目以降の候補も削除しない。既定はstrictのままとし、既存のdiverseはファイル単位の巡回を続ける。[^issue-710-design]

reportのpolicyは`line_round_robin_v1`。新しいpolicyを拒否する旧readerには更新が必要になる。planのrankや実行制限は変更せず、scoreの対象は選択した部分集合のままとする。少数候補で触れる行を増やす機能であり、不具合検出率の向上を保証するものではない。実装・Leanの照合計画と作成例の比較結果を別途記録した。[^issue-710-plan][^issue-710-assessment][^issue-710-review]

[^issue-710-design]: [design.md](../../superpowers/reports/issue-710/design.md)。

[^issue-710-plan]: [plan.md](../../superpowers/reports/issue-710/plan.md)。

[^issue-710-assessment]: [assessment.md](../../superpowers/reports/issue-710/assessment.md)。

[^issue-710-review]: [review.md](../../superpowers/reports/issue-710/review.md)。
