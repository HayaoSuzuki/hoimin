import HoiminOracle.NestedTryFlowModel

namespace HoiminOracle.NestedMatchExit

open HoiminOracle.BindingFlow
open HoiminOracle.NestedTryFlow

def composeMatch : List Exits → Option Env → Exits
  | [], none => .empty
  | [], some environment => .fallthroughOnly environment
  | branch :: rest, unmatched => branch.merge (composeMatch rest unmatched)

def loopNaturalEntry (zeroIteration : Env) (body : Exits) : Env :=
  meetAll? ([zeroIteration] ++ body.fallthrough.toList ++ body.continues)
    |>.getD zeroIteration

def consumeLoop
    (zeroIteration : Env)
    (body : Exits)
    (orelse : Env → Exits) : Exits :=
  let afterElse := orelse (loopNaturalEntry zeroIteration body)
  { fallthrough := meetAll?
      (body.breaks ++ afterElse.fallthrough.toList ++
        afterElse.breaks ++ afterElse.continues)
    terminates := body.terminates ++ afterElse.terminates }

def nestedMatchOutgoing (branches : List Exits) (unmatched : Option Env) : Option Env :=
  outgoingEnv (composeMatch branches unmatched)

end HoiminOracle.NestedMatchExit
