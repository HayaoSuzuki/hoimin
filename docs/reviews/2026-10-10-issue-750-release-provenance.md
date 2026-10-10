# Issue #750 リリース由来証明の検証記録

作業ブランチは `feat/issue-750-release-provenance`。
基準コミットは `a6965910bbc745e74aeb539628b3e2dc66c2a9d3`。
配布ファイルの形式・内容を変更せず、GitHub側に証明を登録するため、バージョン系列は0.3.xを維持する。
証明bundleと照合結果はActions artifactへ保存し、Releaseの添付ファイルには追加しない。

## 設計のセルフレビュー

1. 対象を確認した。archive 3、wheel 3、SBOM 6、SHA256SUMS 1の計13ファイルを証明し、SBOM内のartifact digestも既存検査で照合する。
2. バイト列の経路を確認した。検証後の `verified-release` を証明と公開で共用し、後段でchecksumを再生成しない。
3. 権限を確認した。署名用OIDCとattestation書込みは証明ジョブに限定し、PRと通常の手動previewでは実行しない。
4. ソース同一性を確認した。checkout、prepareのcommit、イベントのsource SHAが一致しなければ停止する。workflow SHAは別に記録し、証明書との照合に使う。
5. 失敗と再実行を確認した。証明失敗は公開を止め、公開済みReleaseを書き換えない既存のガードを保持する。古い公開物への事後的な証明追加は今回の対象外とする。

## 実装計画のセルフレビュー

1. テストの順序を確認した。checksum非書込み検証と公開ゲートを先に追加し、未実装による失敗を見てから実装する。
2. 変更範囲を確認した。既存release helperに検証モードを追加し、配布されるRust CLI、wheel構成、PyPIの公開処理を変更しない。
3. 証明APIの使い方を確認した。現在の公式 `actions/attest` を完全SHAで固定し、明示的な `dist/*` を対象にする。checksum入力だけでは一覧自身が対象から漏れるため使わない。
4. 検証方法を確認した。13ファイルの署名検証に加え、改変、別repository、異なるsource SHAを同じbundleで拒否する。ローカルではワークフロー構造・shellの停止動作とPython helperを検査する。
5. 運用と証拠を確認した。利用者手順、権限、再実行、未確認範囲をドキュメントに記録する。GitHubの実証明書と公開後のdigest照合はマージ後の最初の実リリースで確認する。

## 実装とテストの記録

最初のテスト実行はsandboxによる一時ディレクトリへのアクセス拒否で停止した。
必要なアクセス権限で再実行し、証明ジョブ未追加と `--verify` 未実装による6件の期待した失敗を確認した。
checksumの欠落、digest差替え、CRLFへの変更、余分な行の追加も、失敗時に入力を一切修復しないことを検査する。

## 実装のセルフレビュー

1. データ経路を読み直した。validateだけが一覧を生成し、attestとpublishは同じrunの同名immutable artifactを取得する。`--verify` は入力バイト列を変更しない。
2. 対象漏れを確認した。既存の完全なinventory検査を証明直前にも行い、`dist/*` によりchecksum一覧自身を含めた13ファイルを対象にする。bundleはdistへ入れない。
3. ソース境界を確認した。実checkout SHAとprepareのcommitを照合し、イベントsource SHAとの一致も要求する。生成後は証明書のsourceとsigner digestを別々に検査する。
4. 権限を確認した。contents read、id-token write、attestations writeだけを証明jobに設定した。固定Actionのソースを読み、OCI registryへpushしないfile証明ではartifact-metadata書込みが不要であることを確認した。
5. 停止条件を確認した。publishはattest successを要求する。署名生成成功後の検査が失敗してもbundleと部分的な照合結果を保存し、証拠保存の成功で先行失敗を取り消さない。
6. 再実行を確認した。既存の公開済みReleaseの早期return、draft再開、タグ予約の原子性を変更しない。再ビルドの証明が新たに登録されても既存公開物を置き換えない点を運用文書へ追記した。
7. artifact名を再確認した。証明用の旧案 `release-provenance` がvalidateの `release-*` と一致し、再実行時に混入し得ることを回帰テストで再現した。取得対象と競合しない `provenance-evidence` に変更した。

## テストのセルフレビュー

1. REDを確認した。機能追加前に6件が未実装の理由で失敗した。異常ケースは単なる引数エラーを成功扱いしないよう、期待する診断も検査した。
2. 境界を確認した。正常、欠落、digest差替え、CRLF、余分な行の5ケースで終了状態と全入力の前後バイト列を比較した。
3. 契約テストを確認した。previewでの書込み、checksum自身の証明漏れ、checksum再生成、別artifact、sourceガードの削除、signer条件の削除、attest依存の削除を拒否する7ケースを追加した。
4. shellの動作を確認した。実際のstep scriptをGit Bashで実行し、正常・通常検証失敗・改変受理・別repository受理・異なるsource受理の5ケースを確認した。ghのdoubleは制御フローの試験であり、暗号署名の試験とは区別する。
5. 回帰と品質を確認した。workflow156件、release/SBOM/CI selection計124件の一括検証が通過し、Windowsで既存のPOSIX専用10件はskipした。Ruff check、43ファイルのformat check、ty、全workflowのactionlint/ShellCheck/zizmor、cargo fmtが通過した。既存の局所的なzizmor除外3件以外に除外を追加していない。
6. 変異検証を確認した。verifyモード条件のTrue/False固定の2件をkillした後、別テストの編集により計画が無効になったので停止した。新規計画で入力を固定し、checksum検証条件のTrue/False固定、not削除、!=から==への変更の4件をすべてkillした。後半のrun IDは `86ea4dc5-434d-4a9f-a3a3-d9b5ede15abb`、所有領域の最大は38,642,442 bytes、最小空き容量は189,072,805,888 bytes、実行領域のcleanupはclean、一時ディレクトリも削除済みだった。selector指定の不備による1回のplan失敗もあり、正確な行範囲で作り直した。未完了の計画を全件通過とは数えていない。
7. 独立レビューを確認した。会話履歴を持たないreviewerは重大な問題なしと判断した。reviewer自身のテスト試行はsandboxのtempアクセス拒否を含んだため、成功証拠には採用していない。上記の件数は主担当による権限付き再実行の結果である。
8. 再実行時の混入を確認した。証明artifactがbuild取得globへ一致しないことを新たな回帰テストで検査し、名前変更前のREDと変更後のGREENを確認した。追加1件を含む関連15件を再実行して通過し、重複を除く関連テストの通過は計281件になった。

## 配布物と残る実機確認

Rustの配布コード、README、ライセンス、wheel metadata、archive構成は変更しない。
公開物に証明bundleを追加せず、既存の13ファイルを証明するため0.3.xを維持する。
文書はこのリポジトリ内のrelease手順とOKFカタログへまとめ、別サイトの変更は不要と判断した。
作業ツリーのCLIをビルドして変異試験に使用し、終了後に `cargo clean` を実施した。

GitHub OIDC証明書はローカルで発行できず、PR previewも書込みを行わない。
マージ後の最初の `pull_request_target` runでsource/workflow identity、正常・拒否の署名検証、公開物とverified-releaseのbyte一致を確認する必要がある。
手順は[リリース運用](../releases.md#github-build-provenance)に記載した。
この実機確認が済むまではIssue #750の完了条件をすべて達成したとは扱わず、PRはissueを自動closeしない。
