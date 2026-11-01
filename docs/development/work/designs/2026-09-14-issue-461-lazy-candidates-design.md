# Issue 461: 未選択候補の文字列生成を遅延する設計

## 対象と契約

解析器は演算子、行、symbol、profile、候補上限を順に適用し、採用した候補だけに元文字列と置換文字列を保持する。現在はlist/tuple literalの全範囲置換を演算子選択前に生成し、共通 `make_candidate` も選択判定前にoriginalを複製する。巨大literalを深く入れ子にすると、保持されない候補の累積確保量が深さに比例して増える。

出力候補、stable ID、順位、truncation、診断を変えず、未選択候補に必要な所有文字列だけを生成しない。候補上限は保持数の契約であり、解析器全体のピークメモリ上限ではない。

## 採用する境界

`collect_list_literal` と `collect_tuple_literal` は `CollectionListTuple` が選択されている場合だけ置換helperを呼ぶ。これは現在のnodeに対する候補生成だけを省き、visitorの子探索は続ける。

`make_candidate` はoperator、range、数値変換、行・symbol選択を検査した後にoriginalを複製する。replacementは呼出し側で所有済みなので、巨大な構築helperには呼出し前のoperator guardを置く。公開APIや候補値は変更しない。

## 検証

テスト専用統計でcollection replacement helperの呼出し数とoriginal複製数を実際の構築位置で数える。深い巨大listと、その後に置く選択済み加算を解析し、前者が0回、後者が1回で、子探索と候補値が維持されることを確認する。選択済みcollectionの既存テストも実行する。

release実測では同じソース長でlist深さを変え、旧版と新版のwall timeを複数回記録する。時間は環境依存なので合否条件にしない。

## 設計セルフレビュー

1. 契約: operator guardをvisitor全体へ置くと選択済みの子候補も失うため、literal収集関数の候補構築だけをguardする構成へ修正した。
2. 費用: `make_candidate` 内の判定だけでは既に所有済みのreplacementを救えない。大きなliteral helperの前とoriginal複製の前の二段階を設計に含めた。
3. 境界: 不正rangeでsourceを参照しない順序、usizeからu64へのchecked conversion、行・symbol選択、profileによる後段選択を確認した。profileは候補内容を使うため今回の遅延範囲には含めない。
