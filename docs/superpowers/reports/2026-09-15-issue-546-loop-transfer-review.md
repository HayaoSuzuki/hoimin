# Issue #546: ループ転送再利用の自己レビュー

基準版: f11013542ccd735ab9741b5079c0b39a517df256。作業日: 2026-09-15。確認者: Codex agent。人によるレビューや新しいLean証明を表す記録ではない。

## Worktree — 5 rounds

1. pwdとbranchを照合し、対象を `.worktrees/issue-546` / `perf/issue-546` に限定した。
2. HEADがIssueの確認版f110135と一致することを確認した。別修正の差分を前提にしない。
3. `git status --short` が空であり、既存の利用者変更がないことを確認した。
4. 共有targetは同名実行ファイルの競合があるため、親agentが用意したworktree専用targetを使う方針へ修正した。
5. `.venv` はlocal excludeで除外済み。APFS cloneのfingerprintを信頼せず、最初のbuild前にhoimin-cliだけcleanする手順を加えた。

## Design — 5 rounds

1. visit_loopとloop_head_fixed_pointを読み、record_annotations=falseでも二回のbody走査が再帰することを確認した。単なる記録停止を解決案から除いた。
2. 永続cache案の状態キー・保持量を点検し、同一評価の終了値を消費する方式へ変更した。
3. class_body_fallbackの関数定義時更新を確認し、単にheadが等しいだけでは全可変状態の再利用条件にならないためguardを追加した。
4. marker/handler/try projectionの副作用を確認し、テスト観測がある場合は再利用しない条件を追加した。
5. O(d)という主張を除き、単純な空状態ループでは末端d+1、全文三角数までと明記した。class、finally、状態変化を含む一般上界には拡張しない。

## Plan — 5 rounds

1. 実装前にcounter付きテストを実行する順序を確認し、redログの保存先を追加した。
2. 返り値変更がtest-only loop head adapterにも届くことを確認し、最終的な`.head`更新を手順に含めた。
3. 候補0件だけではannotationを飛ばす誤りを検出できないため、callback内容と件数の比較を追加した。
4. 未選択operatorの対照と、実際に旧走査を使う感度検査を別条件として追加した。
5. spec/report一覧、出典hash、performance gateの更新を実装と同じ変更へ含め、PRのpush/createは親agentへの引継ぎ範囲とした。

## Execution results

実装前の失敗を確認してから制御フローを変更した。共通のCI前提修正として、既存workflowのmerge_groupをテスト期待値へ反映したcommit `1942b5d`（親agentのf6a676bのcherry-pick）を含む。


## Implementation — 5 rounds

1. 最終transferのexitを保持したまま注釈収集へ入ると、不要なsnapshotが再帰中に残ることを確認した。再走査の前にtransfer.exitsを明示的にdropし、再利用時も不要なheadをdropするよう修正した。
2. 旧コードが所有していたfallthrough/continueをcloneすると状態の複製が増えるため、共通部分の更新を借用に変更した。target無効化が名前の削除だけであり、共通部分へ配れることをinvalidate_targetの実装で確認した。
3. 初期guardがclass内の単純なloopも除外する問題を、親agentの指摘から新しい回帰テストで再現した。fallbackの有無から評価前後の等価性へ条件を変更した。classの深さ2で赤、深さ20で232文・21末端の緑を確認した。
4. guardがrecord_annotations、marker/handler/try projection、test mutationを確認することをコードで点検した。callbackの収集走査を省略せず、既存adapterの観測経路を維持している。
5. headだけの固定点とclass fallbackの変更を区別するため、返り値を名前付きLoopHeadTransferへ整理した。fallback snapshotは評価のブロック内で破棄し、AST別cacheやannotation一覧を追加していないことをdiffで確認した。

## Tests — 5 rounds

1. production変更前の実行が、深さ2の4末端訪問で失敗したことをredログで確認した。深さ1は旧実装も通るため、深さ2以上を必須とした。
2. 全文のcounterはvisit_statement_flow入口、末端のcounterはAnnAssign分岐で増える。理論値を返す模擬counterではないことを計数点から点検した。for/whileと未選択operatorの0回対照を保持した。
3. 旧再走査を実際に有効にした深さ8の感度テストで、256末端訪問が同じ上界を超えることを確認した。候補数とtruncationは同じ検査を通る。
4. 10種の制御フローと、既知alias/遮蔽aliasの追加2例で、callbackの範囲・symbol・imports、最終exitを旧走査と比較した。callback件数と非空import事実も断定し、空の観測だけが一致する検査を避けた。classがloop末尾でimportsを戻してもfallbackを変更したままになる例を追加した。
5. default/contractsのanalyzerと既存8 adapterを実行した。公開CLIはCPython3.14.7でコンパイル済みの深さ20を8条件で検査し、型演算子選択＋Sequence importでのみ候補1件を確認した。Clippyのformat_collect指摘でfixture生成をpush方式へ修正し、再検査した。

## OKF — 5 rounds

1. overview、analyzer、performance-shapes、okf-workflowと原文一覧を読み、過去の性能観測を今回の結果に置換しない方針を確認した。
2. 独立概念を増やす必要がないため既存analyzer/performanceページへ追記し、入口から既存の導線で到達することを確認した。
3. spec/reportの表・出典・脚注を対応付け、引用見出しを原文と一致させた。planは原文一覧の対象外というリポジトリ規則を維持した。
4. class guard変更後、設計・plan・概念・性能ガイドを再読し、「class fallbackがあるだけで除外」という古い説明を修正した。snapshot保持量がfallbackサイズと実行中の深さに依存する限界を追加した。
5. 新規・更新出典のhashを編集後に更新し、YAML・予約ファイル20件と今回の5出典の脚注・hash検査を通した。statusはdraftのままで、人による検証や新しいLean証明は付与していない。


## PR preparation — 5 rounds

1. PR本文の主題を、import状態の固定点に達した後も転送専用bodyを繰り返す問題と、その場での最終結果の再利用へ絞った。class guardの旧案は最終変更の説明に残していない。
2. Issueの受け入れ条件を再照合し、決定的counter、未選択対照、continue/break/finally、性能台帳を本文から検査結果へ対応付けた。
3. 「全て線形」「全入力二次」「定数メモリ」と読める表現を点検し、限定された上界とfallback snapshotの保持条件を明記した。タイミング/RSSや新規Lean証明の主張を加えていない。
4. PRテンプレートのOKF欄に参照・更新・形式検査を記入した。過去の成功件数を今回の件数へ混ぜず、3件の既存ignoredを明記した。
5. branch差分に共有CI前提修正が含まれることを確認し、本文へ追記した。pushと公開PR作成は親agentが行うため、公開URLやremote CI成功を未確認のまま記入しない。

## 最終検証結果

| 対象 | 実行と結果 |
| --- | --- |
| 回帰RED | 修正前moduleは深さ2で7文・4末端、初期class guardも深さ2で8文・4末端として失敗 |
| 深さ系列 | module/class×for/while、1/2/4/8/16/20で上界を満たす。深さ20は231/232文・21末端 |
| 未選択対照 | 同系列で型注釈flowの文・末端訪問が0、候補0、診断なし、truncated=false |
| 感度 | 深さ8の旧再走査は256末端訪問で同じ上界を超える |
| focused | `cargo test -p hoimin-cli [--features contracts] --lib nested_loop_transfer_tests` は各4成功 |
| analyzer | `cargo test -p hoimin-cli [--features contracts] --lib analyzer::` は各223成功・既存3 ignored |
| Rust adapters | 8 binary・26テストをdefault/contractsで実行。各26件成功。下記の対象を参照 |
| 公開CLI | debug CLI、CPython3.14.7、深さ20の8条件で候補・診断・truncationを確認。Sequence import＋型演算子選択時のみ1候補 |
| Python | `.venv/bin/python -m unittest tests.test_performance_shapes tests.test_ci_workflow` は44成功 |
| registry | `.venv/bin/python tools/performance_shapes.py check` は既存29形状と追加exact gateの構造を検査 |
| exact gates | 台帳のrun_gateで追加3ゲートを実行し、各tests=1・passedを確認 |
| static | `cargo fmt --all -- --check`、`cargo clippy -p hoimin-cli --all-targets -- -D warnings`、`git diff --check` 成功 |
| OKF | 20ファイルの形式・今回の5出典、guide/registryの追加2hash、設計206件・報告117件のカタログ収録、入口から全20ページへの到達を検査 |

adapterは `lean_binding_flow_oracle`、`lean_annotation_scope_oracle`、`lean_exception_match_binding_oracle`、`lean_nested_try_flow_oracle`、`lean_multiple_handler_join_oracle`、`lean_except_star_flow_oracle`、`lean_nested_match_exit_oracle`、`lean_compound_pattern_guard_oracle`。既存corpusとの対応検証であり、全Pythonや新たな計算量定理の証明ではない。

macOS上のdebug解析と操作数を確認した。Linux/Windowsのネイティブ実行、release時間/RSS、入力状態が変化する任意loopの漸近上界は未検証である。旧経路を有効にするtest-only switchはDropで復元し、スレッドローカルなcounterとともに他テストへ状態を漏らさない。

初回のPython実行はpytest未導入で起動できなかったため、リポジトリのunittest形式で実行して44件を確認した。registryのモードは初回にvalidateを指定してusage errorとなり、公開されているcheckモードで再実行した。これらを成功回数へ含めていない。
