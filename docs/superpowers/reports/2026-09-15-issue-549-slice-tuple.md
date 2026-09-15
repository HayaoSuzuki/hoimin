# Issue #549: Slice tuple修正の検証

比較元: f11013542ccd735ab9741b5079c0b39a517df256。担当: Codex。環境: macOS arm64。PR公開は親担当が最終確認後に行う。

## Worktree: 5回の自己レビュー

1. pwdとbranchを照合し、専用issue-549 worktree内であることを確認した。
2. git status --shortは空で、既存変更を引き継いでいないことを確認した。
3. issue本文の対象HEADと割当ての比較元が一致することを確認した。
4. shared targetはbuild cacheに限定し、ソースは専用worktreeに置く方針を確認した。
5. .venvは専用worktreeにないため、テスト用Pythonを親workspaceの絶対パスで指定する必要を発見した。

## OKF: 設計前の5回の自己レビュー

1. overviewの歴史的検証結果は今回の証拠として転記しないことを確認した。
2. analyzerの既存#489節に適格条件を追記する方針とし、重複概念の作成を避けた。
3. 出典は未commit時点のhashと比較元revisionを記録する必要を確認した。
4. 設計書と報告の一覧の両方に新規原文が必要であることを確認した。
5. Lean内証明、adapter一致、実runの観測を別々に記す設計へ整理した。

## 設計: 5回の自己レビュー

1. colon文字列検索は文字列リテラルにも一致するため、AST要素の条件を採用した。
2. subscript全面除外は合法な式tupleを失うため、直接のSliceだけに限定した。
3. starredはリストでも合法なので除外条件に含めないことを確認した。
4. nested tupleの保持には「直接」の判定が必要と確認した。
5. Slice boundsの再帰が早期returnで失われないよう、visitorではなく収集関数にguardを置く設計を確認した。

## 計画: 5回の自己レビュー

1. production変更前の失敗を保存する順序を確認した。
2. 既存corpus adapterを再利用すると公開planとcompileの両方を確認できると確認した。
3. corpus生成元だけを編集し、CIの既存freshness対象を維持する計画を確認した。
4. import-onlyはcompile検査だけでは代替できないため独立のrun確認を加えた。
5. Python環境とCargo cacheの絶対パス、ログの分離を明記した。

## 実装: 5回の自己レビュー

1. collect_tuple_literalだけにguardを置き、visitor::walk_exprがその後に呼ばれることを確認した。
2. 判定はtuple.eltsの直接要素だけを走査する。nested subscriptや文字列中のcolonまで判定を広げていないことを確認した。
3. operator未選択、Load context、例外型、annotationの既存条件を保持した。
4. tuple_to_list_replacementの表記とバイト範囲を変更していないため、既存のID/span規則を保持した。
5. 要素検査はanyによる短絡走査であり、再parseやソース全体の検索を追加していないことを確認した。

## テスト: 5回の自己レビュー

1. 旧corpusの先行実行は成功したが、新規fixtureが未生成だった。これをRed証拠から除外し、生成後に再実行した。
2. 壊したモデルと正しいモデルが一致する正例にbroken値を付けるとadapterの感度検査が失敗することを発見した。生成元を直し、不一致の負例だけにbroken値を付けた。
3. 実際のRedは `slice_tuple_1` の `expected [], observed [(29, ":,", "[:,]")]`。同時にimport-onlyはbaseline Exit(0)、killed=3となった。205 passed / 2 failed / 3 ignored。
4. 元ソースもcompileする既存adapterを使用した。14入力とbounds7例に加え、starred、nested tuple、各boundの子tuple、nested subscriptとcolon文字列の正例を合計25例追加した。
5. import-only fixtureに合法な外側return tupleを含めるとsurvived候補が混ざるため、3つの添字を別の文に分けた。関数を呼ばず、候補数とkilled、baseline、complete、元ファイル不変を確認する。

## Leanの対応と限界

モデルのTupleElement.expressionは通常式、starred、入れ子を一つのクラスへ射影したもの。Sliceは直接のAST要素を表す。tupleAllowedは全要素がexpressionなら許可し、allowed_elementsは許可から全要素の性質を導く。有限定理は長さ1〜3・2要素種の14入力と11反例、最短 `[slice]` を検査した。Python parser、Load/annotation/例外型条件、Rust全体は証明対象外。

既存ValidPythonAuditMainが期待値、置換後bytes、適格性をJSONLへ生成する。valid_python_corpus_correspondenceは実Rust解析器、公開plan、共有validator、元/変異ソースのCPython compileを照合する。CIの既存ValidPythonターゲットとfreshness commandが新しい生成元にも適用される。

初回Lean証明はBool等値から命題等値への変換で失敗し、要素の2ケースに分けて修正した。生成器のフィールド改行も修正した。有限探索の上限は14入力、定理のmaxHeartbeatsは100000。初回依存buildには60秒、それ以降各コマンドには20秒の外部timeoutを付け、成功したbuild/generate/check一式は3.85秒だった。メモリのpeakは未測定で、探索上限は増やしていない。

## 再現コマンド

リポジトリrootで実行する。HOIMIN_OPERATOR_TEST_PYTHONにはCPython 3.14の絶対パスを設定する。

```sh
cargo test --offline -p hoimin-cli --test valid_python_corpus -- --nocapture
cargo fmt --all -- --check
```

formal/HoiminOracleで各コマンドを20秒timeoutの下で実行する。

```sh
lake build HoiminOracle.ValidPythonModel ValidPythonAuditMain
lake env lean --run ValidPythonAuditMain.lean --check corpus/valid-python.jsonl
```

今回の実行ログ: /private/tmp/hoimin-549-red.log、/private/tmp/hoimin-549-green.log。ローカルログの永続保存は保証しないため、Redの差分と集計を本報告にも記録した。

## PR準備: 5回の自己レビュー

1. Issue #549の直接Sliceという原因と、killedへの影響をPR本文の冒頭に記載されていることを確認した。
2. RedとGreenの区別を明記し、旧corpus実行を修正検証として扱わないことを確認した。
3. Leanの有限モデルと実装照合の境界を本文に残し、全Python構文を保証しないことを確認した。
4. OKFの参照、更新、形式検査をPRテンプレートの各欄のパスと4出典・20ファイルの検査結果を照合した。
5. PR公開前の親担当レビューを残し、未公開のURLやcommitを本文へ仮記入しないことを確認した。

## 実行結果

CPython 3.14.7、macOS arm64、debugで全valid_python_corpusは207 passed / 0 failed / 3 ignored。361照合、262件の候補をvalidate/compileし、runtime候補131件を観測した。追加したimport-only runも成功し、baseline Exit(0)、killed=0、候補0、complete=trueをassertした。3件のignoredは既存の扱いを維持した。

cargo clippy --offline -p hoimin-cli --test valid_python_corpus -- -D warnings、cargo fmt --all -- --check、git diff --checkは成功した。release binary、Windows、Linuxでは今回の新規fixtureを実行していない。並行作業中のshared targetを使用した最初のGreenの後、専用worktreeの独立targetで再実行した。207 passed / 0 failed / 3 ignored、11.12秒で、同じ361照合・262候補compileを確認した。ログは /private/tmp/hoimin-549-green-isolated.log。最終Lean build/freshness/sensitivityは7.89秒で成功し、corpus全体は66例。

OKFの最終確認では、20 MarkdownのYAML・予約ファイル構造と今回の4出典のリンク、脚注、SHA-256を検査した。設計/報告一覧の追加行が表の外へ出ていたため、既存表の先頭へ移して先頭見出しと参照時点の状態を揃えた。内容点検では、直接Sliceの条件、子探索の継続、過去監査と今回の実行結果、未検証OSの区別を確認した。verified metadataは追加していない。
