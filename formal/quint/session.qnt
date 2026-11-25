// One durable run. Status 0 is absent; 1/2 are determinate; 3..7 require rerun.
// 1 Killed, 2 Survived, 3 Timeout, 4 OutOfMemory, 5 ProcessLimit, 6 Error, 7 NotRun.
module SessionModel {
  const REPLACE_SEALED: bool
  var present: bool
  var complete: bool
  var owned: bool
  var results: List[int]
  var sealed: List[int]
  var op: str
  var slot: int
  var status: int
  var accepted: bool
  var observed: int
  action init = all { present'=false,complete'=false,owned'=false,
    results'=[0,0],sealed'=[0,0],op'="init",slot'=0,status'=0,accepted'=true,observed'=0 }
  action begin = all { not(present),present'=true,complete'=false,owned'=true,
    results'=results,sealed'=sealed,op'="begin",slot'=0,status'=0,accepted'=true,observed'=0 }
  action close = all { present'=present,complete'=complete,owned'=false,
    results'=results,sealed'=sealed,op'="close",slot'=0,status'=0,accepted'=true,observed'=0 }
  action load = all { present'=present,complete'=complete,owned'=(owned or (present and not(complete))),
    results'=results,sealed'=sealed,op'="load",slot'=0,status'=0,
    accepted'=(present and not(complete)),observed'=0 }
  action finish(c: bool): bool = {
    val ok=owned and present and not(complete)
    all { present'=present,complete'=if(ok) c else complete,owned'=if(ok) false else owned,
      results'=results,sealed'=sealed,op'="finish",slot'=0,status'=if(c) 1 else 0,accepted'=ok,observed'=0 }
  }
  action persist(m: int, s: int): bool = {
    val previous=results.nth(m)
    val ok=owned and not(complete) and (previous==0 or previous>=3 or REPLACE_SEALED)
    all { present'=present,complete'=complete,owned'=owned,
      results'=if(ok) results.replaceAt(m,s) else results,
      sealed'=if(ok and sealed.nth(m)==0 and s<=2) sealed.replaceAt(m,s) else sealed,
      op'="persist",slot'=m,status'=s,accepted'=ok,observed'=0 }
  }
  action lookup(m: int): bool = all { present'=present,complete'=complete,owned'=owned,
    results'=results,sealed'=sealed,op'="lookup",slot'=m,status'=0,accepted'=owned,
    observed'=if(owned) results.nth(m) else 0 }
  action step = {
    nondet m=Set(0,1).oneOf()
    nondet s=Set(1,2,3,4,5,6,7).oneOf()
    nondet c=Set(false,true).oneOf()
    any { begin,close,load,finish(c),persist(m,s),lookup(m) }
  }
  action recoveryStep = any {
    all {op=="init",begin},
    all {op=="begin",persist(0,3)},
    all {op=="persist",status==3,close},
    all {op=="close",load},
    all {op=="load",persist(0,1)},
    all {op=="persist",status==1,lookup(0)},
    all {op=="lookup",persist(0,2)},
    all {op=="persist",status==2,finish(false)},
    all {op=="finish",load},
  }
  val Safety=all { not(owned and complete),
    Set(0,1).forall(m=>sealed.nth(m)==0 or results.nth(m)==sealed.nth(m)) }
}
module Session { import SessionModel(REPLACE_SEALED=false).* }
module SessionOverwrite { import SessionModel(REPLACE_SEALED=true).* }
