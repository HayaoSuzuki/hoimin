// Two already-loaded candidates. Stages: apply0,start1,run2,persist3,
// finish4,reset5,read-next6,idle7. Modes: normal0,drain1,verify2,
// cleanup3,report4,session5,done6,disk-diagnostic7,drain-EOF8.
// Results: absent0,killed1,survived2,not-run3,timeout4.
module WorkersModel {
  const SESSION: bool
  const DOUBLE_COUNT: bool
  type State={stages:List[int],status:List[int],counted:List[int],
    started:List[bool],finished:List[bool],mode:int,queue:List[int],
    drainStart:bool,exhausted:bool,cause:int,timedOut:Set[int]}
  var s:State
  type Command = Init | Ack({worker:int,outcome:int}) | Stop({cause:int})
    | Drain({worker:int}) | Global | Probe({kind:str,worker:int})
  var dispatch:Command
  def allIdle(t:State): bool = t.stages.indices().forall(i=>t.stages.nth(i)==7)
  def nextDrain(t:State): State = if(t.queue.length()==0) t.with("mode",if(t.exhausted) 2 else 8)
    else t.with("mode",1).with("drainStart",not(t.started.nth(t.queue.nth(0))))
  def drain(t:State): State = {
    val selected=List(0,1).select(i=>t.stages.nth(i)<=4)
    val status=t.status.indices().mapBy(i=>if(t.stages.nth(i)<=2) 3 else t.status.nth(i))
    val counts=t.counted.indices().mapBy(i=>if(t.stages.nth(i)==3) t.counted.nth(i)+1 else t.counted.nth(i))
    nextDrain(t.with("queue",selected).with("status",List(status.get(0),status.get(1)))
      .with("counted",List(counts.get(0),counts.get(1))).with("stages",[7,7]))
  }
  action init = all {s'={stages:[0,0],status:[0,0],counted:[0,0],started:[false,false],finished:[false,false],
    mode:0,queue:[],drainStart:false,exhausted:false,cause:-1,timedOut:Set()},dispatch'=Init}
  action complete(w:int,r:int): bool = {
    val stage=s.stages.nth(w)
    val nextStage=if(stage==0) 1 else if(stage==1) 2 else if(stage==2) {if(SESSION) 3 else 4}
      else if(stage==3) 4 else if(stage==4) 5 else if(stage==5 and not(s.exhausted) and not(s.stages.indices().exists(i=>s.stages.nth(i)==6))) 6 else 7
    val counted=stage==3 or (stage==2 and not(SESSION))
    val t=s.with("stages",s.stages.replaceAt(w,nextStage))
      .with("status",if(stage==2) s.status.replaceAt(w,r) else s.status)
      .with("counted",if(counted) s.counted.replaceAt(w,s.counted.nth(w)+1) else s.counted)
      .with("started",if(stage==1) s.started.replaceAt(w,true) else s.started)
      .with("finished",if(stage==4) s.finished.replaceAt(w,true) else s.finished)
      .with("exhausted",s.exhausted or stage==6)
      .with("timedOut",if(stage==2 and r==4) s.timedOut.union(Set(w)) else s.timedOut)
    all {s.mode==0,stage!=7,s'=if(allIdle(t)) t.with("mode",2) else t,dispatch'=Ack({worker:w,outcome:r})}
  }
  action stop(c:int): bool = all {s.mode==0,s.cause<0,
    s'=if(c==2) s.with("mode",7).with("cause",c) else drain(s.with("cause",c)),
    dispatch'=Stop({cause:c})}
  action drainAck = {
    val w=s.queue.nth(0)
    val count=if(s.drainStart) s.counted.nth(w)
      else if(s.status.nth(w)==3 or DOUBLE_COUNT) s.counted.nth(w)+1 else s.counted.nth(w)
    val t=s.with("started",s.started.replaceAt(w,true))
      .with("finished",if(s.drainStart) s.finished else s.finished.replaceAt(w,true))
      .with("counted",s.counted.replaceAt(w,count))
    all {s.mode==1,s'=if(s.drainStart) t.with("drainStart",false)
      else nextDrain(t.with("queue",s.queue.tail())),dispatch'=Drain({worker:w})}
  }
  action finish = all {Set(2,3,4,5,7,8).contains(s.mode),
    s'=if(s.mode==7) drain(s) else if(s.mode==8) s.with("mode",2).with("exhausted",true) else s.with("mode",if(s.mode==2) 3 else if(s.mode==3) 4 else if(s.mode==4 and SESSION) 5 else 6),
    dispatch'=Global}
  action probe(k:str,w:int): bool = all {s.mode==0,
    k!="identity" or s.stages.nth(w)==3,
    k!="worker" or Set(0,2,3,5,6).contains(s.stages.nth(w)),
    k!="wrong" or s.stages.nth(w)!=7,
    s'=s,dispatch'=Probe({kind:k,worker:w})}
  action step = {
    nondet w=Set(0,1).oneOf()
    nondet r=(if(s.stages.nth(w)==2) Set(1,2,4) else Set(1,2)).oneOf()
    nondet c=Set(0,1,2).oneOf()
    nondet k=Set("identity","worker","wrong").oneOf()
    any {complete(w,r),stop(c),drainAck,finish,probe(k,w)}
  }
  action reverseWith(outcome:int): bool = any {
    all {s.mode==0,s.stages.nth(1)!=7,complete(1,outcome)},
    all {s.mode==0,s.stages.nth(1)==7,complete(0,1)},drainAck,finish,
  }
  action reverseStep = reverseWith(2)
  action timeoutStep = reverseWith(4)
  action stopAfterResult(outcome:int): bool = any {
    all {s.mode==0,s.stages.nth(1)<3,complete(1,outcome)},
    all {s.mode==0,Set(3,4).contains(s.stages.nth(1)),stop(0)},drainAck,finish,
  }
  action completedStopStep = stopAfterResult(1)
  action timeoutStopStep = stopAfterResult(4)
  action startStopStep = any {
    all {s.mode==0,s.stages.nth(0)==0,complete(0,1)},
    all {s.mode==0,s.stages.nth(0)==1,stop(1)},drainAck,finish,
  }
  action runningStopStep = any {
    all {s.mode==0,s.stages.nth(0)<2,complete(0,1)},
    all {s.mode==0,s.stages.nth(0)==2,s.stages.nth(1)<2,complete(1,1)},
    all {s.mode==0,s.stages.nth(0)==2,s.stages.nth(1)==2,stop(2)},drainAck,finish,
  }
  val timeoutFinished = s.mode == 6 and s.status.nth(1) == 4 and s.counted.nth(1) == 1
  val timeoutPreservedOnStop = timeoutFinished and s.status.nth(0) == 3 and s.cause == 0
  val Safety=all {
    Set(0,1).forall(i=>s.counted.nth(i)<=1),
    Set(0,1).forall(i=>s.timedOut.contains(i) == (s.status.nth(i)==4)),
    Set(0,1).forall(i=>not(s.finished.nth(i)) or (s.started.nth(i) and s.counted.nth(i)==1)),
    not(Set(2,3,4,5,6).contains(s.mode)) or Set(0,1).forall(i=>s.finished.nth(i)),
  }
}
module Workers { import WorkersModel(SESSION=false,DOUBLE_COUNT=false).* }
module WorkersSession { import WorkersModel(SESSION=true,DOUBLE_COUNT=false).* }
module WorkersDoubleCount { import WorkersModel(SESSION=true,DOUBLE_COUNT=true).* }
