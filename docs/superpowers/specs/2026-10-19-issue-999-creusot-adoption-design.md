# Issue #999: Creusotを予算計算の本番実装と必須CIへ適用する

状態: Q1〜Q7および仕様全体を利用者が承認済み（「はい、進めて」）。2026-10-19に実装開始。Tasks 1〜4の実装・独立レビューとnative Linuxのcold/warm証明を完了。Task 5のローカル配布検証・依存監査・文書統合、および現在の証明ソースでnative Linux CIとcold/warm証明を完了。Windowsの並列数制限ではreadiness失敗を解消できず、原因は未確定。後続の診断実行で見つかったlifecycleテストの終了待ちを修正し、312be2c4のnative CIは9ジョブすべて成功。修正範囲レビューのnative受入条件も満たした。fb89c55cの全ブランチ最終レビューはReady to merge: Yesで完了し、Critical／Important／Minorの指摘なし。判断対象外の6項目は承認済み範囲と実測に基づきcontrollerが整理済み。2026-10-19のPR公開前受入記録として、次はPR公開と通常の新規CI実行。以後の公開状態はIssue #999とリンクされたPRで追跡する。結果は[実行記録](../reports/2026-10-19-issue-999-creusot-adoption.md)を参照。
基点: main b65f62834b35a9ff85795b041e9687b21e5c51ba（PR #1000を含む）。

## 目的と完了の意味

予算見積もりとauto timeoutについて、本番が呼ぶRust実装そのものが仕様を満たすことをCreusotで検証し、以後の変更でも証明をPR CIの合否に組み込む。PoCの成立や導入可能性の報告だけでは完了としない。

対象実装、証明、再現可能な環境、CI、回帰検証、配布物、文書を変更し、Issue #999をこの仕様に更新してPRを作成する。マージ・リリース・新規コンテナイメージの外部公開はこの作業に含めない。

## 合意済みの判断

- Q1: 最終的な未証明、証明全体の時間切れ、検証不能はCI失敗。証明は専用Linuxジョブとし、通常の製品ビルドと配布に証明ツールを要求しない。
- Q2: 今回の必須対象は予算見積もり全体とauto timeout。BudgetLedgerの予約・解放・ID管理等の状態遷移の証明は後続の独立した拡張とする。
- Q3: 公式Creusotの標準ライブラリ仕様、コンパイラ、証明ツールを信頼境界として明記する。対象ロジックをhoimin独自の未証明なtrusted指定・外部仕様・assumeで穴埋めしない。
- Q4: x86_64-unknown-linux-gnu、aarch64-apple-darwin、x86_64-pc-windows-msvcの3ターゲットで実際に検証条件を生成・証明する。
- Q5: 製品で使用する純粋Rustクレートへ分離し、その同じ実装を証明する。検証用の複製実装・別ロジックへの切替・互換用実装を作らない。
- Q6: 毎回最新ソースから検証条件を生成し、必要な関数・非空の検証条件・固定ツールの存在と版を確認する。保存済みの成功キャッシュで成功扱いにしない。個々の試行の失敗を別の試行で解決できれば成功であり、全条件の最終的な証明が必要。
- Q7: 1ソルバープロセス30秒・2 GiB、ソルバー最大2並列、環境準備を含めCIジョブ30分。3ターゲットは逐次実行する。全環境の厳密なRSS上限とは主張しない。成立しなければ実測結果を示し、黙って上限や対象を緩和しない。

## 証明する仕様

Durationを非負整数のナノ秒として表し、Dmax = (2^64 - 1) × 10^9 + 999999999とする。公開APIが受け付ける全てのusize、正のjobs、Durationを対象にし、設定値のjobs上限256や100年上限だけに入力を絞らない。

1. wavesはselected/jobsの数学的な切り上げと一致する。selected=0、jobs=1、整数上限を含み、実装の加算・除算・剰余・変換でオーバーフローやpanicを起こさない。
2. auto timeoutは max(5秒, min(Dmax, 2 × baseline + 1秒)) と一致する。掛算・加算の両方の飽和を含む。見積もりと実際のRunMutant生成が同じ関数を使う。
3. Fixedでは指定された正のDurationをそのまま採用する。Autoでは上記の関数の結果を採用する。
4. projected_capacityは min(Dmax, effective_mutant_timeout × waves) と一致する。u128の中間積が表現範囲を超える場合も正しい飽和となる。
5. Durationへの再構成で秒・ナノ秒の範囲を保ち、切り詰めで値を失わない。必要な変換や上限処理は証明可能な演算へ本番側も整理する。
6. is_shortfallはprojected_capacity > remainingと厳密に一致し、等値ではfalseになる。
7. project_top_budgetが返すselected、jobs、planned_total_timeout、baseline、remainingなどの保存フィールドは入力と一致する。
8. 正のDurationを表す型など、対象計算が利用する型の不変条件とアクセサの意味を維持する。publicな結果structを外部で直接構築した場合にも、is_shortfallはそのフィールドの比較として正しい。全ての手製structがproject_top_budget由来であるとは仮定しない。

これは予算見積もり・timeout算術の証明である。実際の実行時間、OSによる強制終了、ディスク計測、SQLite、TokioやBudgetLedger全体を証明したとは説明しない。予算見積もりは現在の警告という役割を維持する。

## 実装の配置と通常ビルド

新しい内部クレート名はhoimin-budget-mathとする。現在のbudget_projection.rsの計算・結果型と、config.rsのauto_mutant_timeoutおよび必要な値型をこの境界へまとめる。既存coreと実行側はこの実装を利用し、変換adapterや二つ目の算術実装を残さない。Serde等の製品向け連携と証明に必要な依存の境界を整理する際も、対象の型の意味・演算・分岐を検証時だけ変えない。

通常のRust 1.99.0、all-featuresでの既存品質チェック、3OSのビルドを維持する。Creusot用nightlyは別の検証ツールチェーンとする。内部API呼出し側は必要に応じて一括更新し、互換adapterは追加しない。外部に観測される計算結果・CLI挙動・既存データ形式は変更しない。

新クレートに伴うworkspace／root・fuzz lockfile／依存監査／SBOM／release tooling／sdist・wheel入力を確認し、必要な箇所を同じ変更で更新する。小さな算術クレートをCLI、OS操作、永続化や状態管理へ依存させない。

## 固定する検証環境

初期基準はCreusot v0.14.0とその対応creusot-std、nightly-2026-08-03（rustc 1.99.0-nightly）、OCaml 5.3.0、Why3 commit 05bf4ddf55cb3042f34c3bc0ed3aff56a226978a、Why3find commit 0f054b93c86ac7ba20bca7df5d1563a9a4434a30とする。初期ソルバーは公式推奨版のZ3 5.1.0とCVC5 1.4.1とし、使用する設定と実体の版・配布物のハッシュを固定する。任意の未導入ソルバーへ黙って依存する設定にしない。

Linux x86_64のCI環境を受入基準とし、--targetによる3ターゲットの実行を検証する。設計承認時点ではローカルにLinux arm64のDocker Engineがあり、AMD64実行や3ターゲットの証明成功は未確認だった。現在の実測結果は上記の実行記録に残す。公式v0.14.0リリースには完成済みツール一式の添付assetがなく、環境構築を含むcold時間を実測する必要がある。

固定した環境やコンパイル済み依存のキャッシュは利用できるが、ソルバーの過去の成功結果を証明の代用にしない。キャッシュなしの初回構築も30分のジョブ上限内で成立することを調べ、超過時はQ7に従って再検討する。未計測の所要時間やメモリ量を達成済みとは扱わない。

## CIと失敗の扱い

- PRの既存CI集約に証明ジョブを組み込み、証明の失敗・cancel・全体timeout・意図しないskipを成功扱いにしない。対象ソースだけでなく仕様・依存・ツール・検証スクリプト変更でも検査する。
- ツールの存在、実体の版、必要な設定を明示検査する。cargo creusot versionの終了コードだけに頼らない。
- 各ターゲットでそのコミットのソースから新しく検証条件を生成する。古いComa等へのprove-only実行や空の対象を許さない。
- 保存するproof.json等は再実行のための戦略として扱う。初回に証明を成立させ、CIは--replay --no-cache等の適切な方法で新しい検証条件について全条件の証明を再確認する。
- 想定する関数と必要な保証が検証対象に残っていることを検査する。件数だけ合えばよいという検査にしない。
- 全ての指定ターゲットの成功を必要とする。1ターゲットの結果を残りのターゲットの証明として流用しない。
- 失敗は未証明、ツール不足、版不一致、生成失敗、対象欠落、資源上限などに分けて診断できるようにする。単発SMT試行がtimeoutしても、上限内で全条件を証明できれば成功とする。

## 回帰検証と受入条件

- 全対象の実装証明が3ターゲットで通り、証明ログ・前提・未証明の対象外領域を記録している。
- 切り上げ、飽和、auto timeout、等値境界などの代表的な誤りを一時的に導入し、対応する証明が失敗することを確認する。ビルド不能やツール不備を、誤った計算を検出した証拠として扱わない。変更は本番コードに残さない。
- 既存Rustのprojection／timeout／machineテスト、Leanのモデル・corpus照合、runtime contractsを維持して通す。32bit等の非配布ターゲットについて証明していない保証を追加しない。
- 必要な追加テストはAAA、1実行1ケースとし、有限の組合せはパラメタライズ、数値領域はproperty testを使う。検証スクリプトの重要な失敗経路もテストする。
- 通常のstableビルド、all-features品質チェック、root/fuzz依存検査、SBOM、sdistからのビルド・wheel実行を確認する。
- cold／warmの検証時間、メモリ、ディスク使用、追加コード・仕様量、誤りを直して証明を復旧する手順・手間を記録する。
- CIの成功、Issue更新、設計・運用文書／必要なOKF更新、PR作成まで完了する。PoC成功のみでIssueを完了扱いにしない。

## 資源・作業方法

実装用worktreeとissue名を含むブランチissue-999-creusot-budget-proofsを使う。既存worktreeと利用者の未追跡ファイルは保護する。重いビルドは並行させず、作業専用targetを共有し、不要になった生成物を適宜cargo cleanする。各段階で最低5回のセルフレビューを記録し、独立レビューを加える。

## 要件セルフレビュー（5回）

1. PoC評価から、本番コード・必須CI・3ターゲットの実行を伴う完了条件へ変更したことを確認。
2. 現行の全公開型領域を保ち、設定で小さく制限された入力への限定やゼロ・上限の抜けを確認。
3. 独自trusted、二重実装、証明用別分岐、予算強制の保証への誇張を除外。
4. target指定、信頼境界、公式ツールの版と未実行の事実を区別。
5. 資源上限・失敗の扱い・後続の状態機械の範囲をQ1〜Q7と照合。

## 設計セルフレビュー（5回）

1. 製品が実際に使用する算術クレートを唯一の実装とする境界を確認。
2. stable／all-features／Serde等の通常利用と証明ツールの分離条件を確認。
3. freshな検証条件・非空の対象・版確認・成功キャッシュ不使用の4条件を確認。
4. 新クレートに伴う配布・依存グラフ・SBOM・両lockfileの変更漏れを確認。
5. 未測定の環境構築・cross-target証明を達成済みとせず、失敗時も無断で要件を下げないことを確認。

## 事実確認の出典

- 現行実装: crates/hoimin-core/src/budget_projection.rs、config.rs、machine.rs。
- 現行Lean: formal/HoiminOracle/HoiminOracle/TopBudgetProjectionModel.lean、TopBudgetProjectionProofs.lean、crates/hoimin-core/tests/lean_top_budget_projection_oracle.rs。
- https://github.com/creusot-rs/creusot/releases/tag/v0.14.0
- https://github.com/creusot-rs/creusot/blob/v0.14.0/rust-toolchain
- https://static.rust-lang.org/dist/2026-08-03/channel-rust-nightly.toml
- https://github.com/creusot-rs/creusot/blob/v0.14.0/creusot-std/src/std/time.rs
- https://github.com/creusot-rs/creusot/blob/v0.14.0/creusot-std/src/std/num.rs
- https://guide.creusot.rs/v0.14.0/limitations.html
- https://guide.creusot.rs/v0.14.0/command_line_interface.html
- https://guide.creusot.rs/v0.14.0/installation.html
- https://github.com/creusot-rs/creusot/blob/v0.14.0/creusot-deps.opam
- https://github.com/creusot-rs/creusot/blob/v0.14.0/creusot-setup/src/tools_versions_urls.rs
- https://github.com/creusot-rs/why3find/blob/0f054b93c86ac7ba20bca7df5d1563a9a4434a30/README.md
