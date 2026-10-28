use super::{ContractCase, JSON_RUN_HARNESS, assert_contract};

#[tokio::test]
async fn continue_removal_keeps_context_cleanup_and_loop_else() {
    assert_contract(ContractCase {
        name: "continue fallthrough with cleanup",
        operator: "continue_remove",
        original: "continue",
        replacement: "pass",
        source: concat!(
            "from contextlib import nullcontext\n",
            "def run():\n events=[]\n for value in [-1,2]:\n",
            "  with nullcontext():\n   try:\n    if value<0:\n",
            "     events.append('reject')\n     continue\n",
            "    events.append('accept')\n   finally:\n    events.append('cleanup')\n",
            " else:\n  events.append('done')\n return events\n",
        ),
        harness: JSON_RUN_HARNESS,
        baseline_stdout: "[\"reject\",\"cleanup\",\"accept\",\"cleanup\",\"done\"]\n",
        mutant_stdout: "[\"reject\",\"accept\",\"cleanup\",\"accept\",\"cleanup\",\"done\"]\n",
    })
    .await;
}

#[tokio::test]
async fn comparison_mutation_prevents_reflected_comparison() {
    assert_contract(ContractCase {
        name: "reflected equality",
        operator: "comparison_not_implemented",
        original: "NotImplemented",
        replacement: "False",
        source: concat!(
            "class Token:\n def __eq__(self,other):\n",
            "  if isinstance(other,Token): return True\n  return NotImplemented\n",
            "class Peer:\n def __eq__(self,other): return isinstance(other,Token)\n",
            "def run(): return [Token()==Token(),Token()==Peer()]\n",
        ),
        harness: JSON_RUN_HARNESS,
        baseline_stdout: "[true,true]\n",
        mutant_stdout: "[true,false]\n",
    })
    .await;
}

#[tokio::test]
async fn cause_removal_preserves_context_and_suppression() {
    assert_contract(ContractCase {
        name: "explicit cause versus implicit context",
        operator: "exception_cause_remove",
        original: "reason",
        replacement: "None",
        source: concat!(
            "def fail():\n try:\n  int('bad')\n except ValueError as reason:\n",
            "  raise LookupError('invalid record') from (reason) # retain grouping\n",
            "def run():\n try:\n  fail()\n except LookupError as e:\n",
            "  return [type(e.__cause__).__name__,type(e.__context__).__name__,e.__suppress_context__,str(e)]\n",
        ),
        harness: JSON_RUN_HARNESS,
        baseline_stdout: "[\"ValueError\",\"ValueError\",true,\"invalid record\"]\n",
        mutant_stdout: "[\"NoneType\",\"ValueError\",true,\"invalid record\"]\n",
    }).await;
}
