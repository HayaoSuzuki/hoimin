// Bounded observation histories; measurements are data, not real disk pressure.
module DiskObservationModel {
  const REPLACE_PRIMARY: bool
  type Evidence = {reason:int, owned:int, free:int, elapsed:int, message:int}
  type Command = Init | Observe({owned:int, free:int, elapsed:int})
    | Fail({message:int}) | Dispatch | Idle
  type Snapshot = {primary:Evidence, first:Evidence, secondary:List[Evidence],
    dispatched:int, accepted:bool, inputs:int}
  var s:Snapshot
  var dispatch:Command
  pure val empty = {reason:0,owned:0,free:0,elapsed:0,message:0}
  pure def observation(reason:int, owned:int, free:int, elapsed:int):Evidence =
    {reason:reason,owned:owned,free:free,elapsed:elapsed,message:0}
  pure def present(items:List[Evidence], value:Evidence):bool =
    items.foldl(false, (found, entry) => found or entry == value)
  pure def appendEvidence(items:List[Evidence], primary:Evidence, value:Evidence):List[Evidence] =
    if (value == primary or present(items,value)) items else items.append(value)
  action init = all {
    s'={primary:empty,first:empty,secondary:[],dispatched:0,accepted:true,inputs:0},
    dispatch'=Init,
  }
  action record(evidence:List[Evidence], command:Command):bool = {
    val primary = if (s.primary.reason == 0 or REPLACE_PRIMARY) evidence.head() else s.primary
    val diagnostics = evidence.foldl(s.secondary, (items,entry) => appendEvidence(items,primary,entry))
    all {
      s'=s.with("primary",primary).with("first",if(s.first.reason==0) evidence.head() else s.first)
        .with("secondary",diagnostics).with("inputs",s.inputs+1).with("accepted",true),
      dispatch'=command,
    }
  }
  action observe(owned:int, free:int, elapsed:int):bool = {
    val command = Observe({owned:owned,free:free,elapsed:elapsed})
    if (owned < 2 and free > 1) all {
      s'=s.with("inputs",s.inputs+1).with("accepted",true),dispatch'=command,
    } else {
      val primary = observation(if(free<=1) 2 else 1,owned,free,elapsed)
      val evidence = if(free<=1 and owned>=2) [primary,observation(1,owned,free,elapsed)] else [primary]
      record(evidence,command)
    }
  }
  action measurement(message:int):bool =
    record([empty.with("reason",3).with("message",message)],Fail({message:message}))
  action requestDispatch = all {
    s'=s.with("dispatched",s.dispatched+ (if(s.primary.reason==0) 1 else 0))
      .with("accepted",s.primary.reason==0).with("inputs",s.inputs+1),
    dispatch'=Dispatch,
  }
  action idle = all {s'=s,dispatch'=Idle}
  action step = if (s.inputs>=3) idle else any {
    nondet sample = Set({owned:1,free:2,elapsed:0},{owned:2,free:2,elapsed:0},
      {owned:1,free:1,elapsed:0},{owned:2,free:1,elapsed:0},
      {owned:2,free:1,elapsed:1},{owned:3,free:0,elapsed:1}).oneOf()
    observe(sample.owned,sample.free,sample.elapsed),
    nondet message = Set(0,1).oneOf()
    measurement(message),
    requestDispatch,
  }
  action priorityStep = if(s.inputs==0) observe(2,1,0)
    else if(s.inputs==1) observe(2,1,1)
    else if(s.inputs==2) requestDispatch else idle
  action duplicateStep = if(s.inputs<3) observe(2,1,0) else idle
  action failureStep = if(s.inputs==0) measurement(0)
    else if(s.inputs==1) observe(3,0,1)
    else if(s.inputs==2) measurement(1) else idle
  val Safety = all {
    s.primary == s.first,
    not(present(s.secondary,s.primary)),
    s.secondary.length() == s.secondary.foldl(Set(),(items,entry)=>items.union(Set(entry))).size(),
    s.inputs<=3,s.dispatched<=s.inputs,
  }
}
module DiskObservation { import DiskObservationModel(REPLACE_PRIMARY=false).* }
module DiskObservationReplace { import DiskObservationModel(REPLACE_PRIMARY=true).* }
