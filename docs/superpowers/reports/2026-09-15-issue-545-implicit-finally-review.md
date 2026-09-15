# Issue #545: 暗黙例外と finally の検証記録

基準版: `f11013542ccd735ab9741b5079c0b39a517df256`。対象: macOS、独立 worktree `issue-545`。

## Worktree 自己レビュー

1. `pwd` が指定 worktree と一致することを確認した。
2. `git branch --show-current` が `fix/issue-545` であることを確認した。
3. HEAD が指定基準版と一致することを確認した。
4. `git status --porcelain` が空で、他の変更を含まないことを確認した。
5. worktree に `.venv` がないため既存 CLI テストの Python パスが使えない。共有 venv を参照するリンクが必要と判断した。

## 設計自己レビュー

1. Issue の import 前後の対照を読んだ。try 入口を無条件合流する案は正例を壊すため除外した。
2. `ControlFlowExits` の terminates は return と raise を兼ねる。暗黙例外は独立フィールドとする設計へ絞った。
3. `visit_function_definition` が関数本体も走査する。外側へ遅延本体の例外を混ぜない境界を明記した。
4. `apply_finally` が注釈走査と各入口の遷移を分けることを確認した。注釈だけの修正では外側 finally を守れないため伝播を要件にした。
5. import 失敗と動的 hook は既存契約の完全モデルではない。証明の限界と、既存対応試験を別に保つことを明記した。

## 計画自己レビュー

1. 設計の全受入条件を Task 1 の正負ケースと Task 2 の伝播先へ対応付けた。
2. production より先に公開 CLI の失敗を得る順序になっていることを確認した。
3. fixture の期待値は Lean から生成し、Rust へ手書き複製しない計画とした。
4. 既存 oracle の正常/abrupt 分類を比較する検証を明示した。
5. OKF 原文一覧とハッシュ確認、親エージェントの PR レビューを Task 3 に含めた。

## 実装自己レビュー

1. 各 suite の到達可能な文だけが暗黙例外を追加することを確認した。明示的 return 後の文は既存の注釈走査を保ちつつ入口を追加しない。
2. AST visitor の `visit_body` を止め、関数本体を外側で二重評価しないことを確認した。lambda 本体と generator の遅延部分を除外し、default と最初の iterable は残した。
3. `merge_abrupt`、loop の body/else、try body/else/handler、finally の全伝播先を確認した。handler target の暗黙例外時の消去を追加した。
4. finally が通常終了した暗黙例外を fallthrough に混ぜないことと、finally 自身の abrupt が元の例外を置き換えることを確認した。入れ子と正常後続の独立テストを追加した。
5. 全モジュールの call で環境を保存すると通常解析のメモリが増える。追跡を finally がある try とその内側だけで有効にし、関数本体では外側の追跡フラグを解除する修正を加えた。

## テスト自己レビュー

1. 初回 Lean 実行は `toJson` の名前空間不足で失敗した。`open Lean` を追加し、このインフラ失敗と production の意味的不一致を区別した。
2. production 修正前の公開テストで `call_before_import` が期待0件、実際1件となる失敗を確認した。ログ: `/private/tmp/hoimin-545-red.log`。
3. 6件の Lean fixture を全件実行し、正例の候補数だけでなく原文、置換、バイト範囲を比較した。未知フィールドは拒否する。
4. 14件の追加 Rust ケースで入れ子、else、handler、loop、正常後続、遅延関数/lambda/generator、即時 default/iterable、finally 内再 import、到達不能、walrus、代入を確認した。既存の内部 NestedTryFlow を含む lib 637件が成功した。
5. CI の corpus 鮮度確認経路を読んだ。`lake exe ... -- --check` に対応する引数処理と generator 登録を追加した。Lean の20秒上限を保ち、6ケース・3イベントのモデルを拡大しなかった。

## OKF 自己レビュー

1. `overview.md`、`design/analyzer.md`、`audits/lean-evidence.md` と開発契約を読み、過去の監査の成功を今回の暗黙例外の証拠にできないことを確認した。
2. analyzer の契約は既存ページへ追記し、独立して参照する証明境界を audit 概念に整理した。
3. 追加設計書と報告書を原文一覧の出典・行・脚注へ対応付けた。計画は原文一覧の対象外という既存規則を維持した。
4. 現行コード・新規資料の出典に基準 revision、参照時 working_tree 状態、SHA-256 を記録する。過去の出典は一括更新しない。
5. モデル内証明、公開 CLI の6例、追加 Rust の14例を別々に説明し、import 失敗、動的 hook、全構文を保証しないことを確認した。形式検査を内容の `verified` として扱わない。

## 検証コマンドと境界

```sh
cd formal/HoiminOracle
lake env lean --run ImplicitFinallyAuditMain.lean --check corpus/implicit-finally.jsonl
cd ../..
cargo test -p hoimin-cli --test lean_implicit_finally_oracle --test lean_nested_try_flow_oracle
cargo test -p hoimin-cli --lib
cargo fmt --all -- --check
```

Lean は `mayRaise` が custom 状態から始まる任意の後続列で候補を許可しないことを証明する。壊れたモデルはその入口を捨て、最短2イベントで異なる結果を返す。Python の評価全体や Rust 実装そのものの証明ではない。

本変更は call、subscript、attribute に加え、演算・比較・真偽判定・assert・反復・context manager・class 構築の例外入口を保守的に含める。import 自体の失敗、任意の動的 hook、Python の全構文・評価順序の完全な対応はモデル外。既存の明示的制御フローの分類は別の oracle が検証する。

## PR準備自己レビュー

1. 本文は `hazard()` が import を飛ばす具体的な入力と、誤候補の除去を先頭に置いた。
2. 差分が暗黙例外、対応試験、CI鮮度、関連文書に限定され、共有 `.venv` リンクをコミット対象に含めないことを確認した。
3. `Closes #545` と公開 plan の実際の失敗を記載し、モデル内証明を Rust 全体の証明として説明しないことを確認した。
4. PRテンプレートの OKF 欄に参照ページ、更新ページ、検査の種類を記載した。設計と検証記録への導線も確認した。
5. 未確認の式・class・動的挙動と途中のインフラ失敗を隠さず報告する方針を確認した。push と PR 作成は親エージェントの最終レビュー後に行うため、この節は提出本文の自己レビューである。


## 追加レビューと再検証

親エージェントの独立レビューで、演算・文単位の近接例外を対象外にすると同じ誤認が残ると指摘された。11入力を追加してすべて失敗することを確認した（`/private/tmp/hoimin-545-neighbor-red.log`）。その後、演算・比較・assert・反復・context manager・class・handler 型式を拡張した。class の global/nonlocal は外側の束縛を保守的に無効化し、class ローカルの状態を外へ渡さない。

拡張後の差分を再度5観点で点検した。

1. class 構築前と本体途中の例外の双方を覆うため、外側の入口を使い、外部束縛だけを無効化することを確認した。
2. handler 型式の呼出しを try 開始時に評価しないよう、handler の環境が定まった箇所で記録した。
3. loop の次回反復と with の終了処理は本体の後にも失敗する。本体 fallthrough/continue と with の各 exit から例外環境を追加した。
4. 即時実行と遅延実行の14入力を再実行し、演算の対象拡張が lambda/generator の遅延部分へ侵入しないことを確認した。
5. 設計、開発契約、OKF、PR本文の対象範囲を最終実装へ合わせ、演算と class を未対応とする古い文を削除した。

共有ビルド出力を使った途中の全体試験では、`blocked_monitor_join_defers_both_roots_without_recursive_cleanup` が periodic scan の待機で時間切れになった。worktree ごとに出力を分離した再実行と拡張後の再実行は成功した。原因を解析器と断定せず、途中の試験上の失敗として記録する。

最終実装の `cargo test -p hoimin-cli --lib` は638成功・12 ignored・0失敗（7.22秒）。14+11の追加入力は2テストにまとめており、既存の内部 finally/return/break/continue 対応試験も含む。


## 通常式の最終拡張と保持上限

さらに名前の読取り、dict/set のhash、書式化、starred展開、unpack、部分的な複数代入/with target を9入力で再現し、修正前の全9失敗を確認した（`/private/tmp/hoimin-545-ordinary-red.log`）。match capture の後で guard が失敗する1入力も先に失敗させた（`/private/tmp/hoimin-545-pattern-red.log`）。これらの入口を加え、部分束縛された名前を保守的に無効化した。

暗黙例外の保持は `Vec<KnownImports>` から `Option<KnownImports>` に改めた。同じ例外分類に属する入口の共通事実だけを合流時に保持するため、try 内の式の数に比例して環境を保存しない。finally を必要としない通常コードでは収集しない。生成器の正例は、遅延本体だけを検査できるよう外側 iterable を未定義の `items` から空tupleへ修正した。未定義名の読取り自体は即時の例外になり得る。

最終の設計とテストを再度5観点で確認した。

1. 名前読取り、hash、format、展開の各失敗を先に再現し、式分類へ明示的に追加した。
2. 代入先が複数または分解される場合、先行targetだけが書き換わった状態も含めるよう無効化した。
3. with/for target と match capture を、通常の代入だけとは別に検査した。
4. 合流後に1環境しか保持しないことを型と `merge_implicit` の実装で確認した。既存の正常/明示的abruptの型は変更していない。
5. 35入力を含む4追加テストと既存lib、公開対応を同一の最終ソースで実行した。過去の638/639成功は中間版の結果として残し、最終値を下に示す。

## 最終検証結果

- `cargo test -p hoimin-cli --lib --test lean_implicit_finally_oracle --test lean_nested_try_flow_oracle`: lib 640成功・12 ignored・0失敗（10.37秒）。公開 Lean 6入力の1テストと既存公開 finally の2テストが成功。
- `lake build generate_implicit_finally`: 成功（4.464秒）。`lake exe generate_implicit_finally -- --check corpus/implicit-finally.jsonl`: 成功（0.965秒）。各実行に20秒の外部上限を設定した。6ケース・3イベント、全探索なし。ピークRSSは未測定。
- `cargo fmt --all -- --check` と `git diff --check`: 成功。
- ビルド出力は `CARGO_TARGET_DIR=/Users/hayao/RustroverProjects/hoimin/.worktrees/issue-545/target` に分離した。
- 共有CI前提修正 `d523533`（元コミット `f6a676b`）を先行適用した。既存workflowの `merge_group` triggerをPythonの期待値へ加える1行であり、本修正の解析器動作とは独立している。

CIの28件のunittestで、新しい生成器がテスト側の登録表にないことを検出した。登録表とworkflowの順序を合わせ、生成器の `--sensitivity` を追加して再実行した結果、28件すべて成功（10.698秒）。最終Leanはbuild 4.238秒、鮮度1.137秒、感度0.184秒で成功し、感度出力は `drop_implicit_entry=true` となった。

OKFの最終検査はPyYAML 6.0.3で行う。21 MarkdownのYAML/予約ファイル構造、今回追加した7出典の脚注・実在・ハッシュ、変更した導線、設計書と報告の登録を確認した。過去の全出典ハッシュや外部URLの到達性は再検査していない。

## PR公開後の品質ゲート修正と5回の再点検

1. CIのClippy失敗をローカルで再現。visit_tryの109行と入れ子ifが原因で、意味の回帰ではないことを確認した。
2. handler名の終了時無効化を `clear_handler_target` に抽出し、正常/abrupt/暗黙例外の全分類とtest-onlyの意図的欠落を保持した。
3. 自己参照はtest-only変異の選択に必要であるため、production限定のunused_self expectationに理由を明記した。
4. 全targetの検査で公開adapterのu64→usize castも検出。checked conversionとchecked additionに変更し、範囲が表現できない場合を明示的に失敗させた。
5. workspace all-targets/all-features Clippy成功、analyzer225件成功（3既存ignored）、公開Lean6 fixtureの1テスト成功、fmtを確認した。
