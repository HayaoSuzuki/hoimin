// Command generation only. BudgetModel owns all acceptance and state updates.
module BudgetConnect {
  import BudgetModel(ALLOW_OVERDRAFT=false, MAX_RESERVATIONS=4) as B from "./budget"
  type Command = Init | Reserve({kind:int,amount:int}) | Release({id:int}) | Idle
  var dispatch:Command
  var ticks:int
  action init = all {B::init,dispatch'=Init,ticks'=0}
  action reserve(k:int,n:int):bool = all {
    B::reserveAmount(k,n),dispatch'=Reserve({kind:k,amount:n}),ticks'=ticks+1,
  }
  action release(i:int):bool = all {
    B::releaseId(i),dispatch'=Release({id:i}),ticks'=ticks+1,
  }
  action idle = all {
    B::idleBudget,
    dispatch'=Idle,ticks'=ticks,
  }
  action step = if(ticks>=8) idle else any {
    nondet k=Set(0,1,2).oneOf()
    nondet n=Set(0,1,2,3).oneOf()
    reserve(k,n),
    nondet i=0.to(4).oneOf()
    release(i),
  }
  action reuseStep =
    if(ticks==0) reserve(0,2)
    else if(ticks==1) reserve(0,1)
    else if(ticks==2) release(0)
    else if(ticks==3) release(0)
    else if(ticks==4) release(4)
    else if(ticks==5) reserve(1,0)
    else if(ticks==6) reserve(0,1)
    else if(ticks==7) release(2)
    else idle
  action zeroStep =
    if(ticks==0) reserve(0,0)
    else if(ticks==1) reserve(1,0)
    else if(ticks==2) reserve(2,0)
    else if(ticks==3) reserve(0,0)
    else if(ticks==4) release(3)
    else if(ticks==5) release(1)
    else if(ticks==6) release(1)
    else if(ticks==7) release(4)
    else idle
  val Safety = B::Safety and ticks<=8
}
