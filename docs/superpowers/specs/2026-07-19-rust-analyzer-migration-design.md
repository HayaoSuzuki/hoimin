# Rust アナライザ段階移行の設計

## 目的

Python 3.14 専用の hoimin で、LibCST を使う Python アナライザを Rust 実装へ段階的に移行する。
移行中は既存の候補 JSONL 契約と CLI の挙動を変えない。

## 現在の境界

Python helper は対象ソースを解析し、候補ごとに path、byte span、original、replacement、operator、line、column、symbol を JSONL で返す。
Rust 側はその候補を検証、spool、worker への byte patch、テスト実行に使う。
この境界は第1段階でも維持する。

## 前提

変異対象は Ruff formatter を通過した Python 3.14 のソースに限定する。
したがって非標準の演算子空白や任意の改行スタイルとの互換性は要求しない。
コメントと Unicode を含む UTF-8 byte span は引き続き保持する。


## 第1段階: Rust アナライザを既定化する

`hoimin-cli` に Rust の Python 3.14 パーサ依存を追加する。
Rust アナライザは現在の MVP 演算子だけを検出し、既存 JSONL candidate record と同じ内容をメモリ上の値として返す。

書式を再生成しない。
元の UTF-8 byte slice と、パーサが返す range・トークン情報から original と replacement を決め、既存 worker が行う byte patch と同じ方式で変異を表す。
これによりコメントと Unicode は元ソースの未変更 byte を保持する。

## 短期比較

開発専用の比較テストは同じ fixture を Python helper と Rust アナライザへ渡し、候補を sequence、operator、span、original、replacement、line、column、symbol の順に完全一致比較する。
不一致は fixture 名と候補差分を含めて失敗する。
通常の CLI は Rust analyzer を既定にする。Python helper は比較テスト専用に残し、通常実行では起動しない。

必須 fixture は、全演算子、Ruff 標準の `not in` と `is not`、コメント付き演算子、Unicode、ネストした class/function、`__init__.py` である。

## 第2段階: Python helper の撤去

比較テストが継続して一致した後、Python helper、`--python`、LibCST の runtime dependency を削除する。
最後に Python helper、`--python`、LibCST の runtime dependency を削除する。

## 非対象

candidate schema、worker の mutation 適用、JSON report schema は変更しない。
Python 3.14 以外の構文互換性も追加しない。

## 検証

Rust unit test は各演算子と byte range を検証する。
cross-backend integration test は Python helper との候補完全一致を検証する。
既存の Rust、ty、Ruff、pytest、wheel smoke をすべて通す。
