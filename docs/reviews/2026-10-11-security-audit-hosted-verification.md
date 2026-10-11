# 監査対応3件のHosted検証

対象の実装は#783の `2831bde6adf75facf0a379b8a915267c278d8511`。
#784と#785を含むスタック全体は `b3782a96e257b111b3793c5a1036b23ba2d5cf62`。
両リビジョン間でcrates以下の変更はなく、後段のOS別実行も同じRust修正を検証している。

## PRと実行証拠

| 対応 | PR | 検証 |
| --- | --- | --- |
| #783 検査したメトリクス出力親の保持 | [#786](https://github.com/HayaoSuzuki/hoimin/pull/786) | [Linux CI](https://github.com/HayaoSuzuki/hoimin/actions/runs/38115931463)成功、[release preview](https://github.com/HayaoSuzuki/hoimin/actions/runs/38115931461)成功 |
| #784 Linux資源制限の説明訂正 | [#787](https://github.com/HayaoSuzuki/hoimin/pull/787) | [文書CI](https://github.com/HayaoSuzuki/hoimin/actions/runs/38116066060)成功 |
| #785 maturin OCI digest固定 | [#789](https://github.com/HayaoSuzuki/hoimin/pull/789) | [スタック全体CI](https://github.com/HayaoSuzuki/hoimin/actions/runs/38116379428)成功、[固定後release preview](https://github.com/HayaoSuzuki/hoimin/actions/runs/38116381016)成功 |

gh stackの順序はmain → #786 → #787 → #789（stack #788）。
専用ブランチへ変更と文書をコミットし、mainへの直接push・マージは行っていない。

## メトリクスのOS別検証

Linuxのworkspaceテストで `retained_parent_receives_output_and_failure_cleanup_after_symlink_swap`、`failed_atomic_replacement_cleans_up_the_temporary_entry`、CLIの `metrics_parent_replacement_does_not_redirect_final_output` が成功した。
将来のsession ownership treeの衝突を、missing parent警告より優先して拒否する既存テストも成功した。
通常・contracts・ランダム順・cross-boundary・fuzz検証も成功した。
delegated cgroup専用ジョブは既存のmain限定・opt-in条件によりskipされており、Linuxのhard cgroup動作を新たに実測した証拠とはしない。

[非Linux CI](https://github.com/HayaoSuzuki/hoimin/actions/runs/38116383860)では、Windows/macOSのstrict qualityとwheel smokeが成功した。
macOSの全体Rust検証でも上記のsymlink差し替え・失敗時清掃・CLIの新しい回帰テストが成功した。
Windowsメトリクス専用ジョブは `HOIMIN_REQUIRE_WINDOWS_SYMLINKS=1` の条件でstrict Clippy・単体・統合テストが成功し、ローカルWindowsでのsymlink権限不足による内部skipを補った。

非Linux CI初回のWindows全体テストは、変更していないresource/windows.rsのfixture準備期限で2件失敗した。
該当ケースは `abnormal_runtime_root_crosses_real_job_notification_and_cleans_up_within_six_seconds` と `exited_root_is_observed_before_assigned_descendant_cleanup`。
同じSHAのWindows資源制御専用ジョブでは両ケースを含む単体25件・process-handler5件・CLI2件が成功した。
コード・期限・テスト条件は変更せず、Windows全体ジョブのみを再実行した。
[再実行（attempt 2）](https://github.com/HayaoSuzuki/hoimin/actions/runs/38116383860/attempts/2)ではworkspaceテストとCLI E2Eを含む全ステップが成功し、非Linux CI全体も成功した。
この記録の追加は文書のみであり、検証済みのRust実装・workflowには変更を加えていない。

## 固定コンテナと配布物の検証

Linuxビルドログで公式OCI index `sha256:1c897d72a79c083cdea8829662f7b28e7657cf5b0d7f44fb4231e55998cf52ed` のpullと、同じtag@digest参照の回収が成功したことを確認した。
Windows・Linux・macOS wheel、release asset検証、Linux glibc 2.17 smoke、3プラットフォームのpackaged archive smokeがすべて成功した。
タグ予約・attestation・GitHub Release/PyPI公開はpreview条件によりskipされた。
同じSHAで重複開始された手動previewは取り消し、成功したPR previewを証拠とした。

## レビューと作業領域

独立した最終コードレビューは `8ec93d7` から `b3782a9` を対象とし、指摘なし。
公式digestの取得とHosted実行の成否は、そのレビューの判定範囲とは区別して上記で検証した。
各Issueの設計・計画・実装・テストの各5回のセルフレビューは対応するレビュー記録へ保存した。
別ユーザーによる悪用可能性は未検証であり、確定した脆弱性や重大度を新たに主張しない。

ローカルRust検証終了後のcargo cleanは5,976ファイル・4.9 GiBを回収し、Cドライブの空き容量は約171.4 GiBだった。
作業前から存在した未追跡の `.idea/` は変更していない。
