# Issue #548: Annotation builtin review and correspondence

対象: `f11013542ccd735ab9741b5079c0b39a517df256` からのissue-548作業ツリー。確認者: Codex、2026-09-15。結果は本変更の入力集合に限定する。

## Worktree: five review rounds

1. `pwd` とbranchを照合し、issue-548への割当てを確認した。
2. `git status --short` が空であることを確認し、既存変更がない状態から開始した。
3. HEADがissue記載の `f110135` であることを確認した。
4. worktreeに `.venv` がないことを発見した。Pythonテストのため共有済みCPython環境へのローカルリンクを使用する。
5. 共有targetのbinaryが他branchのbuildで変わり得るため、Rustテストはcargoが選ぶartifactを使い、単独CLI観測は専用コピーに限定する。

## OKF preparation: five review rounds

1. overviewの過去の監査と現行契約の区別を確認した。
2. analyzer概念のIssue486はruntimeの証拠であり、注釈の両方向を保証しないことを確認した。
3. Issue479のstreaming callbackを確認した。scope provenanceはsnapshotを増やさずfact indexから参照する。
4. 新規spec/reportは両原文catalogへの登録対象、planは対象外と確認した。
5. 新規概念はdraft、未追跡資料は実内容のsha256を記録する方針を確定した。

## Design: five review rounds

1. 具体型sourceだけの検査では逆方向の問題が残るため、両方向を契約に明記した。
2. runtime resolverの直接流用では遅延評価後の再代入を見逃すため、module/classの後続束縛をunknownに含めた。
3. 関数自身のlocalと外側closureのlocalを区別した。前者はheaderをshadowしない。
4. generic classの型パラメータとclass変数は子scopeへの伝播条件が異なるため、classを飛ばして型パラメータを残す設計とした。
5. 型パラメータを具体型として使う元入力自体が評価エラーになる点を検証範囲へ追記し、全元入力の評価成功という誤った要求を避けた。

## Plan: five review rounds

1. regressionがproduction変更前に失敗する順序を確認した。
2. Lean期待値をRustで再計算しないadapterを計画に明記した。
3. Python3.12がPATHの既定であるため、3.14環境を明示する手順にした。
4. 抽象型同士のSequence/Iterableを維持する回帰試験を追加する方針とした。
5. commit後のpush/PRは親agent担当と確認し、PR本文の作成と最終レビューをhandoff対象にした。

## Implementation: five review rounds

1. `MUTABLE_BUILTINS` にdictがないことを発見した。dictを追跡対象へ追加し、dictだけshadowの記録が欠ける問題を防いだ。
2. abstract名の位置はruntimeのname occurrenceに登録されないことを確認した。annotationを訪問するときに位置とscopeを登録し、逆方向でも照会可能にした。
3. module/classの位置順lookupを流用せず、possible bindingsを参照した。後続代入とconditional代入はunknownとなる。
4. classを飛ばす判定をglobal/nonlocal処理より前に置き、methodの通常lexical探索へclassの宣言を伝播させないことを確認した。
5. Sequenceの分岐全体を抑制するとIterableへの独立した変異も失われるため、listへの候補だけを条件付きで追加する形にした。

## Tests: five review rounds

1. production編集前の78件は54 mismatch、24 matchだった。CPythonの元注釈評価テストと抽象型同士の回帰は成功した。Redログ: `/private/tmp/hoimin-548-red.log`。
2. 初回修正後の78件は全件一致した。元入力評価に加え、保持候補を1件ずつ適用し、注釈の型を期待する型と比較した。
3. global/nonlocal、wildcard、dynamic uncertainty、無関係なgeneric型パラメータ、generic methodからのclass参照を追加し、Lean corpusを114件へ拡張した。
4. 引数注釈だけでは不足するため、module/class/functionの変数注釈、戻り値、同名の引数、abstract import aliasとqualified名を42件の公開plan入力で確認するテストを追加した。
5. Leanの許可関数を常にtrueへ壊した一時ファイルは定理と2つの負例で拒否された。生成corpusのfreshnessは20秒の外部上限内で検査した。共有cargo targetの再build後に以前の不具合が再出現したため、最終結果は分離targetで取り直す。

## Evidence boundaries

Leanが証明したのは3値のprovenanceに対する許可条件。Python fixtureが各provenanceを持つこと自体やRust実装全体を証明したものではない。114件の元入力のうち、generic concrete source 6件はTypeVarに添字を付けるため、compile成功と期待するTypeErrorを確認する。残り108件は実際の注釈評価の成功を要求する。

CIはLeanからのcorpus再生成相当のfreshness検査を実行する。Rust adapterは期待値をfixtureから読み、公開planの候補数・operator・original・replacement・spanとCPythonによる候補適用後の注釈を比較する。予期しないCLI終了・timeout・Python実行失敗は検証失敗として扱う。

## Independent review and follow-up

親agentのレビューで、type aliasのvalueと型パラメータのbound/defaultが通常の式として走査され、注釈位置の登録とaliasの型パラメータscopeが欠ける点を発見した。追加48件のテストは `type Alias[list] = list[int]` で期待0・実際1となった（`/private/tmp/hoimin-548-alias-red.log`）。この失敗後にscope登録を修正し、公開planとCPython3.14でalias value・bound・defaultを評価した。48件中12件のgeneric concrete sourceはTypeErrorを期待し、残り36件と保持された24件の変異は型の値まで比較する。

共有targetとそのcloneには別worktreeのlib fingerprintが残り、初回の分離targetでも古いlibが再利用された。分離targetで `cargo clean -p hoimin-cli` を行い、以後の結果を取り直した。共有targetでのshell monitor timeout（636 passed / 1 failed / 12 ignored）はこの問題の修正結果として採用しない。

clean buildでは既存2テストの期待値が新しい契約と衝突した。後続wildcard importは遅延評価時のlistを変更し得る。また、その後にSequenceだけをimportし直してもlistのprovenanceは復元しない。この2つの期待値を候補なしへ更新し、理由をテスト内に記載した。型注釈のimport自体の全遅延状態を今回証明したという主張はしない。

## Final verification

最終Rust検証は `CARGO_TARGET_DIR=$PWD/target` の分離targetで実行した。環境はmacOS arm64、CPython3.14.7。releaseの統合検証は親agentが実施するため、本報告の成功件数に含めない。

| コマンド | 今回の結果 |
| --- | --- |
| `cargo test -p hoimin-cli --test collection_annotation_builtins --test lean_annotation_scope_oracle --test type_parameter_bindings` | 5 + 4 + 8テスト成功。新規はLean生成114件、追加42件、alias/bound/default48件、抽象型回帰1件の公開plan入力 |
| 同じコマンドに `--features contracts` | 同じ17テスト成功 |
| `cargo test -p hoimin-cli --lib -- --test-threads=4` | 636成功、12 ignored |
| `cargo clippy -p hoimin-cli --all-targets -- -D warnings` | 成功 |
| `lake env lean --run CollectionAnnotationAuditMain.lean --check corpus/collection-annotation.jsonl` | 定理・負例・114件corpus freshness成功。Python subprocessで20秒の外部上限を設定 |
| 許可関数を常にtrueへ置換した一時Leanファイル | 定理とshadowed/unknownの負例で期待どおり失敗 |

最終ログは `/private/tmp/hoimin-548-verified-focused.log`、`/private/tmp/hoimin-548-contracts.log`、`/private/tmp/hoimin-548-final-lib.log`、`/private/tmp/hoimin-548-final-clippy.log`。一時ログを証拠の唯一の保存先とせず、入力・期待値・再実行手順を本変更に含めた。

## PR preparation: five review rounds

1. タイトルをcollection annotationの具体型provenanceに限定し、runtime全体の修正と誤読されないようにした。
2. Issue #548の全受入条件をspec・テスト・本報告へ対応付け、alias/bound/defaultのレビュー指摘も本文へ追記した。
3. Red/Greenと最終clean buildを区別し、共有cache再利用時の結果を最終件数から除いた。
4. Leanの証明範囲とCPythonの観測範囲を分け、元から評価エラーになるgeneric sourceも明記した。
5. PR本文にOKF参照・更新ページ、実行した検査、releaseが親agentの統合検証である点を記載した。push/PR作成は親agentへのhandoffであり、この段階で公開済みとは扱わない。

## CI and OKF completion

共通の前提修正 `017afef`（親agentの `f6a676b` のcherry-pick）は、既存workflowのmerge_group triggerをCI契約テストの期待集合へ加える。今回追加したLean generatorも、lake executable、CIのmodule順序とgenerator一覧、CI契約テストのcorpus一覧へ登録した。最初の独立した追加コマンドは完全なgenerator一覧の検査で拒否されたため、既存の登録方式へ統一した。

`python -m unittest tests.test_ci_workflow` は28件成功した。Pythonの変更は既存テストの登録一覧のみであり、mutation対象となるproduction Python変更はない。

最終Lean executableは20秒を上限としてbuild（13.79秒）、`--check`（2.09秒）、`--sensitivity`（0.48秒）が成功した。後者は3状態中2状態、3組×2方向では18件中12件で壊したgateと異なる。探索範囲を増やす試行はなく、peak memoryは計測していない。

OKFはYAMLと予約ファイル構造21ページ、今回の出典7件のID・脚注・hashを検査した。入口から全21ページへの到達性とローカルリンク、新規概念の全出典の引用対応も確認した。日本語の本文は、型注釈の規則、実装、今回の観測、未検証範囲を分けて見直した。`cargo fmt --all -- --check` と `git diff --check` も成功した。
