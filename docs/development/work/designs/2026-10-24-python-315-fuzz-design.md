# Python 3.14・3.15のfuzz入力生成

利用者は3.15対応のfuzzingを承認した。既存PR #1109の専用branchで実装する。

## 構成と選択理由

一つのcampaign内で3.14・3.15のHypothesmith grammar/libcst生成とCPython compileによる選別を順に実行する。
3.15では新構文専用の構造化Hypothesis生成も実行し、遅延importと4種のアンパック内包表記・async変種のseedを追加する。
生成物は既存のhash名を使って共通のpython_analyzer corpusへ合流する。ソースは実行しない。
Python環境は一時ディレクトリに分離し、uvのsync/runへminor版を明示する。生成完了または失敗で環境を破棄し、通常開発venvを変更しない。
Rust fuzz対象はPythonインタープリターに依存しないため、matrixで二重にbuild/runする案は採らない。既存の単一buildと7対象のcampaignを維持する。

## 制約と受け入れ条件

PR job11分・active600秒、scheduled60分・active3300秒、2 build jobs、1入力4096 bytes、timeout5秒、RSS1024MiBを維持する。
Python setup/download/sync/generationも既存deadlineで制限する。未実行や失敗を成功として扱わず、Python版付きstage名・実行argv・選択環境・generatorの実版を診断へ残す。
3.14で3.15専用strategyを要求したら、明示的に拒否する。
両Python版で関連pytest・実生成を検証する。WindowsではPOSIX process groupとLinux sanitizer campaignを検証したと主張せず、既存Linux CIで確認する。
各段階5回以上セルフレビューし、1回の独立レビューを受ける。必要な一時ファイルだけを削除する。
