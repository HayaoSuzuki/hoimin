# proofreadtools探索実験 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** #790・#776・#777について、固定したproofreadtoolsの関数で変異生成・差分入力探索・具体例テストの検証を行い、採否を報告する。

**Architecture:** Rustの明示実行integration testに生成・適用・実行制御を置く。Pythonの固定サンプル専用補助スクリプトは入力探索と観測に限定する。既存のCandidateDescriptor検証、WorkerWorkspace、ProcessHandlerを使い、通常CLIへ機能を追加しない。

**Tech Stack:** Rust 1.99、既存proptest・Ruff Python AST parser、Python 3.14、Hypothesis、任意のCrossHair、pytest。

**Spec:** [承認済み設計](../specs/2026-10-11-issue-790-proofreadtools-design.md)

## Global Constraints

- 対象は#790・#776・#777。#763・#764・#712は対象外。
- proofreadtoolsは`409ed955c0695ec04976842ba9fd22c64c03c33a`を固定する。
- 通常CLI・plan schema・レポートschema・既定operatorを変更しない。
- Rust生成はtest-only。proptestやPython探索ライブラリを製品依存へ移さない。
- 各方式60秒・最大32候補・3反復。seedは`790`, `776`, `777`で事前固定する。
- CrossHair/Hypothesisの同候補比較はそれぞれ60秒まで。起動と終了を含むwall timeを記録する。
- jobs=1、max-workspace-size=8GiB、min-free-space=10GiB。
- 直接起動する探索・観測プロセスもProcessHandler経由で、timeoutを残予算以下、max_memory_bytes=1GiB、max_processes=16、max_output_bytes=1MiBとする。上限の変更は結果へ記録し、同条件比較をやり直す。
- 実験はまずWindowsの現在環境で実測し、未実測OSの成功を主張しない。CIには固定サンプルを必要としない安価な契約テストだけを含める。
- ドキュメントも専用ブランチへコミットする。lintは原則厳格、除外が不可避なら局所的な理由を付ける。
- 設計・計画・実装・テスト各5回レビュー。定期的にcargo cleanと空き容量確認。

## Review Focus

1. source/checkoutが固定SHAから変化した場合は実行前に拒否する（Task 1）。
2. 同じ元コードへ戻る縮小、無変更・重複候補は発見数に含めない（Task 1）。
3. 差分入力でのゼロ除算・NaN・import errorをkillに誤分類しない（Task 2）。
4. 抽出関数での差だけを実モジュールのkillと扱わない（Task 3）。
5. timeout・出力切詰め・清掃失敗は以降の比較を止め、予算切れと通常判定を区別する（Task 3）。

## File Structure

- `crates/hoimin-cli/tests/proofreadtools_search.rs`: 通常実行する小さな契約テストと、`#[ignore]`の実サンプル実験入口。
- `crates/hoimin-cli/tests/support/proofreadtools_search/generator.rs`: 固定関数のASTから許可した単一編集を作り、proptestで選択・縮小する。
- `crates/hoimin-cli/tests/support/proofreadtools_search/runner.rs`: worker、資源制限付きプロセス、比較結果・証拠の管理。
- `tools/proofreadtools_search.py`: 固定関数の入力strategy、補助的な抽出関数探索、実モジュール観測、具体例テスト出力。
- `tests/test_proofreadtools_search.py`: 誤成功を防ぐ小さな契約テスト。
- `docs/superpowers/reports/2026-10-11-issue-790-proofreadtools.md`: 実測、採否、再実行手順。
- `docs/reviews/2026-10-11-issue-790-proofreadtools.md`: 各段階のレビューと検証記録。
- `docs/knowledge/`: 実測後、既存の関連ページへ根拠リンクと採否を反映する。該当ページがなければ研究結果ページ1件とindexを追加する。

## Task 1: 固定サンプルとRust候補生成

**Interfaces:** `generate_candidates(source: &str, seed: u64, limit: usize) -> Result<Vec<CandidateDescriptor>, String>`。第一関数のASTから編集対象を導出し、候補のIDは既存validate_candidateで確定する。`validate_sample(root: &Utf8Path) -> Result<(), String>`はSHA・tracked変更・対象ソースを検証する。

- [ ] サンプル非依存の小さいPythonソースをfixtureとして、同じseedで同じ候補、全候補の構文成立、1編集、無変更・重複除外を検査する失敗テストを先に書く。
- [ ] `cargo test --locked -p hoimin-cli --test proofreadtools_search`で実装前の失敗を確認する。
- [ ] ASTから対象式・同型オペランド・演算子siteを取得する。引数選択、演算子置換、オペランド選択の有限な適格集合を定義し、既存operatorとの重複を候補属性に記録する。
- [ ] proptestの生成とValueTreeを利用する。単に固定列挙をシャッフルしただけで新しい構造探索と主張しない。別々の独立編集を組み合わせない。
- [ ] source変更・入力範囲外・候補数0・縮小で無変更となる場合の意味をテストで確認する。
- [ ] 上記Rustテスト、fmt、対象integration testのstrict Clippyを通し、Task 1をコミットする。

## Task 2: Hypothesis入力探索と具体テスト候補

**Interfaces:** `find_witness(original_source: str, mutant_source: str, seed: int, seconds: float) -> SearchResult`。`SearchResult`はstatus、具体args、型付き観測値、試行数・elapsedを持つ。`render_regression(args: tuple[str, str], expected: float) -> str`は固定した実モジュールをimportするpytestテストを返す。

- [ ] `tests/test_proofreadtools_search.py`で、差のあるlen式から具体入力を返すこと、同一式をequivalentと断定しないこと、無効入力・例外・非有限値を成功としないことを先に検査する。
- [ ] `.venv/Scripts/python.exe -m pytest -q tests/test_proofreadtools_search.py`の実装前失敗を確認する。
- [ ] ASCII長1〜32の公開strategyと明示した観測比較を実装する。抽出関数による探索は補助証拠としてラベルを付ける。
- [ ] 元と変異を独立したnamespaceへ置き、この限定関数が外部状態を参照しないことを検査する。探索の途中でも残時間を確認する。
- [ ] 期待値の由来をcharacterizationとして出力し、既存ファイルを上書きしない。テスト内容に入力reprを埋める場合も、固定したプリミティブ型以外は拒否する。
- [ ] pytest、Ruff check/format、tyを通す。探索依存が通常製品依存へ追加されていないことを確認し、Task 2をコミットする。

## Task 3: 実適用・予算付き比較とCrossHair

**Interfaces:** `run_experiment(root: &Utf8Path, python: &Utf8Path, output: &Utf8Path) -> Result<ExperimentReport, String>`。開発用のignored testは`HOIMIN_PROOFREADTOOLS_ROOT`, `HOIMIN_RESEARCH_PYTHON`, `HOIMIN_RESEARCH_OUTPUT`を読む。出力は新規ファイルのみに保存する。未知のサンプルや任意strategyを受ける入口にはしない。

- [ ] 手作りfixtureでWorkerWorkspaceへの候補適用・resetを通し、変更前の観測・変更後のassertion失敗・reset後の観測一致を確認する失敗テストを書く。
- [ ] ProcessHandlerの既存終了判定を使い、正常終了以外・切詰め出力をkillから除外する小さいテストを先に書く。既存の子孫清掃テストを複製しない。
- [ ] 新規環境でPython 3.14とサンプルのコア依存だけを準備する。Hypothesis/CrossHairの実版を記録し、heavy extraを入れない。
- [ ] サンプルの関連テストを通常実行する。失敗時は探索を始めず診断する。全体軽量suiteは最終候補で1回だけ追加確認する。
- [ ] 作業ツリーのhoimin binaryから固定対象のplanを取得して固定operator対照を作る。古いPATH上の0.1.0を使わない。
- [ ] WorkerWorkspace、コピー予約、ProcessHandler、実source importを使い、生成候補にも同じ実適用・復元を通す。外部manifest受付やPython独自の変異エンジンを製品へ追加しない。
- [ ] 同じ空間・同じseed・予算で無作為と誘導探索を実行する。誘導探索は観測差と構造の多様性を使って候補種別の重みを更新し、選択規則を結果とともに記録する。生存だけを最大化しない。
- [ ] 生存と差の両方を満たす候補だけproptestで縮小し、Hypothesisで入力を縮小する。全工程を予算に含める。対象が強くテストされ全候補killなら結果を記録し、第二関数へ進む根拠と予算を明示する。
- [ ] 選んだ同じ候補について、抽出した純粋関数に明示した事前条件を付けたCrossHair diffbehaviorを60秒以内で実行する。Hypothesisと同じ入力領域に制限できない結果は比較対象外とする。
- [ ] 差分入力を通常CPythonで実モジュールに渡し、元と変異それぞれ別プロセスで複数回確認する。追加テストを含めた新規manifest/workerでoriginal-pass/mutant-assertion-failを確認する。
- [ ] test collect数、skip/xfailなし、実source path、source/テストhash、判定と費用を証拠へ保存する。人の採用は未レビューと区別する。
- [ ] cleanup成功と元checkout不変を確認する。ディスク停止・清掃失敗なら次の反復を始めない。
- [ ] Task 3の実装と証拠をコミットする。

## Task 4: 検証、採否、PR

- [ ] 実装のセルフレビューを5回行う。候補空間の公平性、生成/縮小、oracle、実適用、資源・cleanupを別々に点検し、修正を記録する。
- [ ] テストのセルフレビューを5回行う。RED/GREEN、無効・等価・不安定、実import、結果再現、過剰テストと費用を点検する。
- [ ] `cargo fmt --all -- --check`、`cargo clippy --locked -p hoimin-cli --all-targets --all-features -- -D warnings`、対象Rust/Pythonテスト、Ruff・ty、文書整合性を確認する。新しい失敗がなければ無関係な重いテストを反復しない。
- [ ] ツール版、反復ごとの分母・発見・kill・重複・未確定・時間・起動回数・レビュー費用を報告する。実測値のない欄を成功扱いしない。
- [ ] #790の探索制御、#776のテスト候補、#777のCrossHairについて採用/限定/見送りをそれぞれ結論にする。小実験のみで汎用機能のIssueを完了扱いしない。
- [ ] cargo cleanと空き容量を記録し、不要な一時worker・出力を回収する。再現に必要な小さい証拠だけコミットする。
- [ ] 独立した最終レビューを受け、指摘を検証する。専用ブランチを明示pushし、実装・実験結果をレビューできるPRを作成する。マージしない。

## 計画セルフレビュー

1. 設計網羅を確認した。3 Issueの役割、固定サンプル、1編集、同一予算、負の結果、非製品化を各Taskへ割り当てた。
2. 手順を確認した。生成・探索・接続にそれぞれ先行失敗テストを置き、環境準備を承認後に行う。大規模テスト追加を目的にしない。
3. インターフェースを確認した。既存の公開WorkerWorkspace/ProcessHandlerを再利用でき、製品CLIの変更なしにtest-only接続を計画できる。
4. 誤成功を確認した。固定SHA不一致、無変更縮小、例外、抽出関数のみの差、出力切詰め・清掃失敗をReview Focusと各Taskの検証に対応付けた。
5. 比例性を確認した。固定サンプルと2関数までに絞り、全OS・高次変異・汎用adapterを除外した。性能便益がなくても報告で完了できる研究部分と、未実装の製品機能を分けた。

実装・テスト段階のレビューと実測結果は未実施であり、実行後に別記録へ残す。
