# Python 3.15構文対応の検証記録

専用worktreeの `python-315-syntax` ブランチで、マージ済みの `d5042077` を起点に作業する。
Ruff 0.17.2 (`30516b42ab972ffa2569e5efcfee81135da466d3`) のparserと依存ASTを取り込む。
初回のCargo git依存はWindowsの長いパスでcheckoutに失敗したため、必要な9 crateをvendorに固定した。

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
