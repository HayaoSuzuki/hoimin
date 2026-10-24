# Python 3.15構文対応の検証記録

専用worktreeの `python-315-syntax` ブランチで、マージ済みの `d5042077` を起点に作業する。
Ruff 0.17.2 (`30516b42ab972ffa2569e5efcfee81135da466d3`) のparserと依存ASTを取り込む。
初回のCargo git依存はWindowsの長いパスでcheckoutに失敗したため、当初は必要な9 crateをvendorに固定した。
その後、同一版のcrates.io公開を確認し、利用者の承認を受けてparserだけをvendorに残す構成へ変更した。
以下の初回検証は9 crate版の履歴であり、registry移行後の検証は末尾に区別して記録する。

## 調査段階のセルフレビュー

1. Python 3.15公式変更一覧と上流parser実装を照合した。
2. CPython 3.15.1で遅延importと4種類のアンパック内包表記をコンパイルし、3.14.9の拒否も確認した。
3. CPythonとRuffのAST表現を個別に確認し、両者の辞書内包表記の省略フィールドが異なることを確認した。
4. 未ロードの一時モジュールで、通常のfrom importは値3、遅延from importは読み込み後の書き換えを反映して値999となることを確認した。
5. 既存の未コミット変更がある主checkoutを保持し、専用ブランチとリソース残量を確認した。

## 設計・計画段階のセルフレビュー

1. 利用者が指定した上流優先の方針を設計へ反映した。
2. 同一コミットのparser・AST・周辺crateを使い、型の世代混在を避けた。
3. 構文受付と意味解析、配布・ライセンス・SBOMを計画に含めた。
4. 3.14の既存候補、エラー回復、遅延alias、ソース範囲、sdist再構築を検証対象にした。
5. 計画の各段階に失敗確認と成功確認、セルフレビュー、全体検証を割り当てた。

## 上流移行段階のセルフレビュー

1. 上流tagからコミットを固定し、parser以外の取得ファイルを上流checkoutとバイト比較した。結果は作業ログの `source-verification.json` に記録した。
2. 新構文のBooleanLiteral候補と開始位置のテストは、旧parserでInvalidSyntaxとなり、上流更新後に成功した。
3. optionalな辞書内包表記のkeyを3か所の独自visitorで処理し、比較式・f-string参照のAPI変更も適用した。
4. `analysis_depth` の10テストが成功し、小さいスタック、投機的with/match、エラー回復と深さ制限を再検証した。
5. 移行直後の解析器ユニットテストは514成功・3無視だった。後続の意味解析変更後に全体を再実行する。

## 遅延import段階のセルフレビュー

1. callable・annotation・enumの既存の誤った信頼を追加テストの失敗として確認した。
2. project例外階層のテストを有効な候補が存在する条件へ直し、旧処理の失敗を確認した。
3. explicitな遅延importはaliasの静的証明に使わず、通常のeager importの対照テストを維持した。
4. `__lazy_modules__`の定数・動的設定と、ソースに見えるlazy設定APIは保守的に処理する。モジュール全体の設定がある場合、import由来の証明を抑制する。
5. 新構文・遅延importの5テストが成功した。標準ライブラリ名は3.14の予約集合を保持し、3.15の `_math_integer` と `profiling` を追加した。

## 検証状況

全体のテスト・配布検証・独立レビューを完了した。最終結果は後段に記載する。
並列ビルド数2、incremental無効、debug情報なしで既存targetを再利用する。

## 配布段階のセルフレビュー

1. SBOMのparser名とversionを実際の上流crateへ変更し、ASTを含む全vendor componentのcommit付きPURLと出典を検証した。
2. Ruffの通知に加えてUnicodeのMIT・Apache・Unicode通知を配布済みの共通license pathへ収録し、個別通知もsdistへ残した。
3. sdistのローカル依存が一時ファイルを自動収録する失敗を再現し、各Cargo manifestのinclude制限で除外した。
4. Cargo.tomlのLF正規化と、Rustソース・データ・ライセンスのバイト一致を区別し、134件のarchive/workflow検証が成功した。
5. uvの使用Pythonをmatrix値へ明示的に固定するテストを追加し、KeyErrorの失敗から設定後の成功まで確認した。workflow lintはactionlint・ShellCheck・zizmorで成功した。

## 独立レビューと修正

fresh contextのレビューを1回実施した。
遅延importのaliasを例外階層の起源追跡から完全に除外すると、別モジュールへの書き換えが伝播しない指摘を受けた。
通常importと書き換えなしの対照条件を持つテストで失敗を再現し、起源は残してeagerだけfalseにする修正を行った。
アンパック内包表記のtargetによるimport名の遮蔽と、最初のiterableが外側スコープで評価される対照テストも成功した。

standalone parserのstacker無効時のビルドもE0433で失敗することを確認し、上流同様のcfg分岐を追加して成功させた。
全体Rust検証ではキャンセル後のpoll回数が10ではなく109になる既存テストが失敗したため、走査のラッチとbodyの終了を修正した。
修正後の解析器ユニットテストは520成功・3無視。
修正後の最終コミットで、両Python版の全体Rust検証は各3770成功・32無視・失敗0。

関連経路のセルフレビューでは、callableとenumでも遅延aliasを起源から除外する問題を再現した。
これらも可能な起源・書き換え・escapeの追跡を保持し、信頼する候補の適格性だけをeager importに制限した。
全走査のキャンセルは最初の観測で停止し、既存のpoll回数契約を維持する。

配布再構築では、PEP517の出力先をCargoの `target/wheels` と同一にするとWindowsの自己コピーがWinError 32になった。
出力先を分けると `uv build` によるsdistからのlocked wheel構築が成功した。
意味解析の最終修正後にsdistを再生成し、再構築と両版smokeが成功した。

## レビュー修正段階のセルフレビュー

1. 外部moduleへの書き換えはeager/lazyの両方で検出し、書き換えなしの対照では既存候補を維持することを確認した。
2. 例外階層ではmay-importの起源を残し、遅延importをeagerな依存グラフのedgeとして扱わないことを確認した。
3. callableとenumも同じ観点で点検し、遅延aliasのwrite・escapeを検出する失敗から、可能な起源と適格性を分ける修正後の成功まで確認した。
4. 内包表記のtarget遮蔽と最初のiterableの外側スコープ、キャンセルの最初の観測での停止を回帰テストで確認した。
5. parserのstacker無効時ビルド、解析器522成功・3無視、全target・全featureのClippyを確認した。独立レビューの残存指摘はない。

## 配布再構築とリソース

最終実装コミットは `c9f17e694863aa512312bb570a61b9297073dc52`。
sdistから展開した独立ソースを `uv build` と `--locked` でwheelへ再構築した。
Cargo.lockと主要な解析・parser入力の展開後の内容を実装checkoutと照合した。
同一wheelの通常実行・変異の検出・復元を3.14.9と3.15.1で確認し、両smokeコマンドのexit 0を確認した。
wheel SHA-256: `5f3c86fa069afd02878b0e5d84977d6ac360270b3f5166aaf2a6a57277919bbf`。

不要になった上流の一時checkout、旧parserコピー、sdist展開コピーを、絶対パスが専用タスク配下に収まることを確認して削除した。
wheelとログを保持したまま `cargo clean --release` を実行し、1575 files、613.3 MiBを解放した。
debugキャッシュは最終の両版Rust検証とレビュー用に保持する。

## 最終検証のセルフレビュー

1. 実装のHEADを固定し、最後のalias修正後に全体検証を再実行した。3.15.1は194 result groups、3770成功・32無視・失敗0でexit 0を確認した。3.14.9も同じ実装で194 result groups、3770成功・32無視・失敗0、exit 0を確認した。
2. Python全体は両版で1346成功・48 skip。追加されたmatrixテストを含む最終の変更領域は両版344成功、最終source archiveとmatrixの134テストも両版で成功した。現時点のcollectionは1395件で、追加matrix 1件も検証済み。
3. 同じwheelのSHA-256が両smoke報告と一致すること、主要な再構築入力が実装checkoutと一致すること、両smokeのexit 0を確認した。
4. 全60 OKFページの構造、変更した脚注、14 source recordsのGit blob SHA-256とローカル参照、サイトビルドを検証した。文書の差分はreference/conceptとして10診断項目を点検し、10/10、未解決の検証項目なし。
5. 既存主checkoutの変更を含めず、専用branchの実装コミットを固定した。3.15用一時venvと重複wheelを削除し、標準 `target/wheels` の検証済みwheel・小さいログ・debugキャッシュを保持する。開発 `.venv` は3.14.9を維持する。

ローカルのOS検証範囲はWindows x86_64。
Linux/macOSの実行・Linuxのbounded fuzzはこのローカル作業では実行しておらず、既存CIをその検証経路とする。
3.15におけるglibc最低環境の新しい検証結果は主張しない。

## 最終結果とPR準備のセルフレビュー

1. 送信対象は専用branch `python-315-syntax`、baseは `main`。mainの追加更新と重複PRがないことを確認し、主checkoutの既存 `.gitignore` 変更を保持した。
2. 最終実装のRust全体検証は、3.14.9・3.15.1ともに `cargo test --locked --workspace -- --test-threads=8` がexit 0。各194 result groups、3770成功・32無視・失敗0。
3. PR本文のPython全体・最終変更領域・同一wheel・Clippy・fmt・Ruff・ty・workflow lint・文書検証の範囲を実ログと照合し、ローカル未実行のOS/fuzz条件を明記した。
4. parserと8依存crate、Unicode通知、sdist収録制限、全vendor SBOM出典、既存operatorとCLI/schemaの不変、遅延importの保守的な候補抑制を説明した。新しい構文を実行するときはPython 3.15を使う。
5. PR [#1109](https://github.com/HayaoSuzuki/hoimin/pull/1109) を作成し、このタスクへ紐付けた。本文は今回の実測値と過去の証跡を区別する。

PR: https://github.com/HayaoSuzuki/hoimin/pull/1109

### 保持した成果物

- `target/wheels/hoimin-0.14.0-py3-none-win_amd64.whl`: 両版で検証した同一wheel。
- 専用タスクの小さい検証ログとsmoke/hash報告、および再検証用debugキャッシュ。
- 一時的なsource checkout、旧parserコピー、sdist展開コピー、3.15用追加venv、重複wheelとrelease build出力は削除済み。

開発 `.venv` はPython 3.14.9。uvで導入したグローバルのPython 3.15.1も維持する。

## CIのfuzz依存同期とセルフレビュー

1. PRのfuzz依存ポリシー検証が独立した `fuzz/Cargo.lock` の未更新で失敗した。ローカルでも `cargo metadata --manifest-path fuzz/Cargo.toml --all-features --locked` の失敗を再現した。
2. fuzz用lockだけを更新し、旧littrs依存を除去した。実行時のRustソースと主Cargo.lockは変更せず、前述の両版テスト・wheel検証の対象コードを維持した。
3. `libfuzzer-sys 0.4.13` と `arbitrary 1.5.0` を保持し、`deny.toml` のライセンス例外やsource制約を緩和していないことを確認した。
4. fuzzの解決グラフ内の7 Ruff crateがversion 0.0.19、sourceなし、ローカルvendor manifestを参照することを確認した。通常・offline両方のlocked metadataがexit 0になった。
5. lock差分と `git diff --check` を確認した。依存ポリシー自体の最終結果は更新後のCIで確認する。ローカルでbounded fuzzを実行したという主張は追加しない。

## Registry移行の設計・計画セルフレビュー

1. 利用者が承認した構成を設計へ反映した。parserの安全修正を維持し、未変更の8 crateを外部依存へ戻す。
2. 9 crateすべてにnon-yankedな0.0.19公開版が存在することを確認した。
3. 隔離したWindows実験で、実際に取得した6外部crateのVCS情報が採用中の上流コミットと一致した。
4. 通常Git取得やWindows全体の設定変更を必要とせず、registry依存でコンパイルと新構文の解析が成功した。
5. 移行計画に両lockfile、配布、SBOM、両Python版、独立レビュー、リソース整理を含めた。既存の独自構文実装は追加しない。

## Registry依存・配布設定のセルフレビュー

1. SBOMの外部8 crateをvendorへ書き換えないテストと、sdistがparser以外のvendorを含まないテストは変更前に計9件失敗し、変更後は関連73件が成功した。
2. parserの全Rustソースは移行前のハッシュと一致し、安全修正・再帰保護・反復破棄を変更していない。
3. 主workspaceとparser manifestの直接Ruff依存は =0.0.19。main/fuzz両lockfileのRuff crateは0.0.19、parser以外はregistry source/checksumを持つ。
4. main/fuzz両lockfileでRuff以外のpackage名・version集合が移行前と一致することを確認した。fuzzer本体・arbitrary・依存ポリシーは変更していない。
5. 8 vendorディレクトリは専用worktreeのvendor直下であることを絶対パスで確認して削除した。sdist収録一覧、ライセンス通知、SBOM説明も現在の構成へ合わせた。

## Registry移行の独立レビューと文書セルフレビュー

1. fresh contextの独立レビューを1回実施した。main/fuzzのlocked・offline metadataを別途確認し、修正が必要な指摘はなかった。
2. レビュアーはactiveな6外部crateのRustソース・データ計62ファイルを削除前のsnapshotと比較し、改行以外の差分がないことを確認した。未使用のcache/macrosは解決グラフに現れない。
3. UnicodeのMIT・Apache・data通知がparserの共通LICENSEに完全に残ることを確認した。SBOMはparserだけにvendor由来を付け、外部componentのidentityとmetadataを維持する。
4. 開発・release・vendor README・設計と関連OKFの現在の説明をregistry構成に合わせた。初回9 crate版の検証記録は履歴として残し、新しい検証と区別する。
5. 9件の変更対象source recordsを実装コミット `a8578a958702c8889dd8ad37ca813a09fe59b6a2` のGit blobハッシュへ更新した。全60 OKFページの構造と更新したsource pins、strict site build、Ruff/ty、workspace/parser fmtが成功した。reference/conceptとして構成・根拠・読者の作業・用語・リンク・範囲・時点・重複・簡潔さ・未検証条件の10項目を点検し、10/10で未解決項目なし。

## Registry移行の配布検証セルフレビュー

1. 実装コミット `a8578a958702c8889dd8ad37ca813a09fe59b6a2` の構成でPython全体を再実行した。3.15.1は1255成功・48 skip、3.14.9も1255成功・48 skip、両方exit 0。
2. 初回版の最終collection1395件から1303件への差は、削除したvendorファイルのarchive検証100件を除き、SBOMの外部identity検証7件とarchive除外検証1件を加えた結果である。機能の回帰テストは削除していない。
3. 新sdistは1584375 bytes、SHA-256 `08727384120c3b518470e0a169bf320d453f8d3ae137169d1e61c0018deda0b8`。parser以外のvendorがないことと、展開後のlockfile・主要ソースがcheckoutと一致することを確認した。
4. 展開した独立ソースを `uv build --wheel`、`MATURIN_PEP517_ARGS=--locked` で再構築した。並列数2、既存targetを再利用し、releaseビルドはexit 0。
5. 再構築した同じwheelで両Python版の通常実行・変異検出・復元を確認し、両smokeのexit 0と同一SHA-256を照合した。wheel SHA-256は `02319f1f6e082e1edc1ade728a941434a594dc3ddd1d7303d639eccc6c759ea3`。Clippyのworkspace全target/全feature・parser単独、stacker無効のstandalone locked/offlineビルドも成功した。

ローカル検証OSはWindows x86_64。macOS/Linuxの実行とbounded fuzzの新しいローカル成功は主張しない。

## Registry移行後の最終確認・PR更新セルフレビュー

1. 実装を固定した全体Rust検証は、3.15.1と3.14.9の両方で194 result groups、3770成功・32無視・失敗0、exit 0だった。Python全体・配布・Clippy・書式・型検査・独立レビューの結果も実ログと照合した。
2. 現在の実装は `a8578a958702c8889dd8ad37ca813a09fe59b6a2`。source refsとその後の変更は文書のみで、検証したRustソース・主lockfile・fuzz lockfileを変更していない。
3. PR本文をparserのみvendorの構成、registry由来のSBOM identity、移行後のテスト件数とwheel hashへ更新する。旧9 crate版の結果を新構成の証拠として流用せず、保守記録で区別した。
4. 検証済みwheelとsdist・小さい証跡・debugキャッシュを保持し、展開ソース、追加3.15 venv、重複wheel、生成siteを削除した。`cargo clean --release` は1575 files、613.2 MiBを解放した。削除先は専用worktree配下の絶対パスを確認した。
5. 送信対象は既存PR #1109の専用branch `python-315-syntax`。元のcheckoutの既存 `.gitignore` 変更を含めない。新HEADのCIはpush後に別途確認し、ローカルのWindows結果をmacOS/Linuxの実行結果へ拡張しない。
