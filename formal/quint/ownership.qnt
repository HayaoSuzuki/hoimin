module OwnershipModel {
  const ALLOW_CONFLICT: bool
  type Lease = { actor: int, runId: int }
  var leases: Set[Lease]
  var op: str
  var actor: int
  var runId: int
  var result: str
  action init = all { leases'=Set(),op'="init",actor'=0,runId'=0,result'="init" }
  action acquire(a: int, r: int): bool = {
    val occupied=leases.filter(l=>l.runId==r)
    val permitted=occupied.size()==0 or (ALLOW_CONFLICT and not(occupied.exists(l=>l.actor==a)))
    all { leases'=if(permitted) leases.union(Set({actor:a,runId:r})) else leases,
      op'="acquire",actor'=a,runId'=r,result'=if(permitted) "ok" else "busy" }
  }
  action release(a: int, r: int): bool = all {
    leases'=leases.filter(l=>not(l.actor==a and l.runId==r)),op'="release",actor'=a,runId'=r,result'="released"
  }
  action close(a: int): bool = all { leases'=leases.filter(l=>l.actor!=a),op'="close",actor'=a,runId'=0,result'="closed" }
  action step = {
    nondet a=Set(0,1).oneOf()
    nondet r=Set(0,1).oneOf()
    any {acquire(a,r),release(a,r),close(a)}
  }
  action reacquireStep = any {
    all { op=="init" or (op=="release" and actor==1),acquire(0,0) },
    all { op=="acquire",actor==0,acquire(1,0) },
    all { op=="acquire",actor==1,result=="busy",release(0,0) },
    all { op=="release",actor==0,acquire(1,0) },
    all { op=="acquire",actor==1,result=="ok",release(1,0) },
  }
  val Safety=Set(0,1).forall(r=>leases.filter(l=>l.runId==r).size()<=1)
}
module Ownership { import OwnershipModel(ALLOW_CONFLICT=false).* }
module OwnershipConflict { import OwnershipModel(ALLOW_CONFLICT=true).* }
