# モデル化前の対応表

対象HEAD: 5e631ef。契約は「builtin pairの変異は、そのlookup時点でsourceとdestinationの両方が組込みである場合だけ許可する」。ソース中の文字位置とPythonの実行順序は同一ではない。

| 前提・観測 | Lean表現 | 公開入力・観測 | mode |
| --- | --- | --- | --- |
| source/destinationは初期builtin | Boolの組、true/true | any/allに未束縛のmodule | strict |
| 代入済みの名前はcustom | bindSource/bindDestination | anyまたはallへの利用者関数代入 | strict |
| RHSがtarget式より先 | イベント順序 | slots[any([])]=(any:=custom) | strict |
| targetは左から順に格納 | bindSource→lookup | 多重代入・連鎖代入 | strict |
| starred引数がkeywordより先 | bindSource→lookup | sink(flag=any([]), *[(any:=custom)]) | strict |
| 候補可否 | lookup時のsource && destination | operator限定planの候補数 | strict |
| 全イベント列 | 3イベント、深さ0〜4 | 全列のPython fixtureは生成しない | model-only |
| 未選択methodの大きな確保 | モデル化しない | 公開discover_targetsと計測allocator | model-only |

性能にはLeanとの比較可能な前提がないため、strictにはしない。実装の確保量の独立した測定として報告する。型別の同値化・任意の動的hook・例外による途中停止・async・scopeをまたぐ再代入はモデル外。fixtureの一つのlookup位置を変異する。UnicodeのNFKC対照とanalyzer timeoutは別の補助調査とする。

Leanは20秒・2GiB・50ms監視、単一スレッド、既定heartbeat上限。深さ0から4まで1段ずつ、安定順序、対称性削減なし。allocator測定は>=500000bytesの成功した要求の累積量であり、peak RSSではない。
