---
type: Contract
title: 対象選択・plan・verifyの契約
description: 候補発見と実行の分離、ランキング、部分集合、保存形式の版の不整合を整理する。
status: draft
catalog_revision: a7daea0b557cd435c1e55b540392fbdd116348e1
sources:
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
- id: issue-456-review
  resource: ../../superpowers/reports/2026-09-14-issue-456-validation-context-review.md
  working_tree: untracked
  sha256: 5042ef83fb71837543be9fb358b2453931bdf70ce77f6033381e5756632b09f0
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
