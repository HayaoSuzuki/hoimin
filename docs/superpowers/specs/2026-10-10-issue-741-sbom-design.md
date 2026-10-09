# Issue #741: リリース SBOM の設計

## 実装判断

基準コミットは `313b84f104be0b8db29586b2e18b2ac073fc8eef`。
[Issue #741](https://github.com/HayaoSuzuki/hoimin/issues/741) を実装する。
現行 release.yml は3 archive と3 wheelのみを保存し、Cargo依存グラフを保存しない。
`cargo audit` / `uv audit` は検査時点の脆弱性確認であり、リリース別の依存記録を提供しない。
Python wheelはRust executableを含むためPython依存一覧では目的を満たさない。
外部サービス、API送信、Dependency-Trackは範囲外とする。

## 選択肢と実測

1. cargo-cyclonedx 0.5.7 + 薄い補完・検証層を採用する。実リポジトリのmacOSグラフを生成できた。Cargo metadataのtargetとfeaturesを扱い、dev専用依存を除く。
2. Cargo.lockのみを変換する方式は、target別のグラフと開発依存の区別ができないため採用しない。
3. 独自Cargo resolverは既存ツールの機能を再実装するため採用しない。

公式実装: https://docs.rs/crate/cargo-cyclonedx/0.5.7/source/
0.5.7のローカルソース main.rs / generator.rs / purl.rs と実出力を確認した。
生成器には --locked がない。事前の cargo metadata --locked、lockfile bytesの事後検査と復元で変更を拒否する。

## 契約

- CycloneDX JSON 1.5。公式JSON Schemaをコミット `c320fc0f0b46873864927d9d5684eea7ba439728` から保存し、jsonschema 4.25.1でオフライン検証する。
- 各platformのstandaloneとwheelを分けた計6 SBOM。名前は `hoimin-v<VERSION>-<PLATFORM>-<KIND>.cdx.json`。
- 生成はrelease version設定後、ビルドと同じtarget / --no-default-featuresで行う。Linux wheelはビルドしたmanylinuxコンテナ内で取得する。
- release commit、Cargo.lock SHA256、target、features、build environment、生成時rustcを記録する。配布ファイル名とSHA256をSBOMに結び付ける。
- Cargo通常・build推移依存を対象とする。build依存はバイナリへの包含を意味しない。OSライブラリ、静的Cコードの完全列挙、Python開発依存、fuzz専用依存、コンパイラは対象外。
- parserは上流package、記録済VCS commit、修正由来Ruff commit、release commitに固定したローカルpathとREADMEを記録する。path packageを未改変crates.io packageとして表示しない。
- schemaだけでなくroot/version/commit/target、識別子の一意性、依存参照の閉包、到達可能性、parser provenance、対象配布物のhashを検証する。
- 全SBOMと配布物の検証が終わるまでSHA256SUMSを書かない。失敗は公開ジョブへ伝播する。PR/手動も集約検証しartifact保存する。
- 公開条件、予約済tagの再利用、公開済release保護、PyPIのwheel限定アップロードを維持する。

## 検証の境界

単体・CLI・property-based・決定的変異fuzzで不正なJSON、欠落、重複、参照破損、hash不一致を検査する。
Leanで公開ゲートのモデルを証明し、故意に壊したモデルで検出感度を確認する。
Leanモデルの証明をPythonまたはGitHub実行の証明とは扱わない。
ローカルで実targetグラフ生成とschema検証を実行する。Windows/Linuxのnativeビルド・hosted公開はCIで確認する。
