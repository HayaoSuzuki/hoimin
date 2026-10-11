# Issue785 maturinビルドコンテナの固定

対象は [#785](https://github.com/HayaoSuzuki/hoimin/issues/785)。
基点は#784の `fd416a6`、ブランチは `ci/issue-785-pin-maturin-image`。
現在のmaturin 1.15.0を維持して、Linux wheelのコンテナ参照をタグとOCI index digestの組へ固定する。

## 設計のセルフレビュー

1. 現行のLinux wheelのみがタグ指定のdocker build経路であり、Windows/macOSのuvx経路は変更不要と確認した。
2. バージョン更新と固定を混ぜず、既存v1.15.0の公式GHCR manifestを選んだ。
3. タグを保守用の情報として残し、実体選択はsha256 digestへ固定する。MATURIN_VERSIONとの一致を契約テストで要求する。
4. ビルドとディスク回収で同じMATURIN_IMAGEを参照し、巨大イメージの回収を維持する。
5. manylinux2014、x86_64、Python 3.14、SBOM、smoke、公開権限と配布仕様は変更しない。

## 実装計画のセルフレビュー

1. digestを公式registryのmanifest responseから取得し、OCI indexの対応プラットフォームを確認してからworkflowへ記録する。
2. 既存のwheel build契約を拡張し、タグだけの現行workflowが失敗することを先に確認する。
3. 不正digest、異なるregistry、バージョン不一致、タグのみを拒否するケースを既存テストへ追加する。
4. バージョン更新を許容する既存テストはimageのタグも同時更新し、固定値に過剰依存しない。
5. 厳格workflow lint、Ruff、型検査、workflow契約全体、Hosted Linux previewを確認する。実際の公開やバージョン変更は行わない。

## 実装のセルフレビュー

1. 公式GHCRの `https://ghcr.io/v2/pyo3/maturin/manifests/v1.15.0` を2026-10-11に参照した。OCI indexは `sha256:1c897d72a79c083cdea8829662f7b28e7657cf5b0d7f44fb4231e55998cf52ed`、linux/amd64 manifestは `sha256:0d1711efd2d3aab276f738bdcda48d378bceef2eeb7f8a8caf59451defddee72`。arm64とattestation manifestも含むindexだった。
2. MATURIN_IMAGEをLinux jobへ限定した。最初の編集でWindows jobへ置かれた誤りを既存契約テストが検出し、Linuxへ移動して修正した。
3. docker runとdocker image rmが同じ引用付き変数を使い、container内のビルド・SBOMコマンドを変更していないことをdiffで確認した。
4. テストはregistry・tag version・sha256形式を検証し、未来の正当なバージョン更新を許容する。既存のbuild引数やcleanup必須条件を緩めていない。
5. リリース文書へ版・tag・digestの同時更新手順を記載した。新規権限、外部入力、lint除外、依存関係は追加していない。

## テスト段階のセルフレビュー

1. 修正前のタグ指定を新しい契約が拒否する失敗を確認した。固定後のmaturin/Linux wheel関連45件は成功した。
2. 不正参照8ケースと既存のバージョン不一致・追加引数・build/cleanup必須条件を含むため、正規の参照だけを比較する自己確認にはしていない。
3. バージョン更新2ケースも成功し、版を更新するたびにテストの固定値を手で修正する契約にはしなかった。
4. actionlint、ShellCheck、zizmorを含む厳格workflow lintが成功した。既存のPython shellを解析できない3警告は残り、新規findingやignoreはない。
5. workflow契約は166件成功、既知のWindows Git Bash上のLean補助fixture `test_toolchain_setup_executes_pinned_lean_without_cache` 1件は今回は選択しなかった。関連45件も成功。Ruff check/formatとty checkは成功し、OKF38ページ・変更リンク・更新した根拠ハッシュを検証した。Hosted previewは後続結果として確認し、成功していない実行を完了扱いにしない。
