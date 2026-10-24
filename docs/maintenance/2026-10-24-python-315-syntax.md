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

全体のテスト・配布検証・独立レビューは実行中。
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
これらの修正後の全体検証を完了してから、最終件数を記載する。

関連経路のセルフレビューでは、callableとenumでも遅延aliasを起源から除外する問題を再現した。
これらも可能な起源・書き換え・escapeの追跡を保持し、信頼する候補の適格性だけをeager importに制限した。
全走査のキャンセルは最初の観測で停止し、既存のpoll回数契約を維持する。

配布再構築では、PEP517の出力先をCargoの `target/wheels` と同一にするとWindowsの自己コピーがWinError 32になった。
出力先を分けると `uv build` によるsdistからのlocked wheel構築が成功した。
意味解析の最終修正後も、sdistを再生成して同じ再構築・smokeを行う。
