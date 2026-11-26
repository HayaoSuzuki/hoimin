// Public bootstrap stop protocol, before workers exist. Pending kinds:
// 0 preparation, 1 run-start, 2 disk diagnostic, 3 verification, 4 cleanup,
// 5 failure diagnostic, 6 run-finish, 7 session-finish, 8 none.
module StopModel {
  const COPY: bool
  const SESSION: bool
  const EARLY_SESSION: bool
  type DiskEvidence = { code:str, owned:int, available:int, message:str }
  type Diagnostic = { code:str, message:str }
  type State = {pending:int, phase:int, copy:bool, reported:bool, disk:int,
    firstDisk:int, diskEvidence:DiskEvidence, diskSecondary:List[Diagnostic],
    interrupted:bool, fatal:bool, incomplete:bool, deliveryFailed:bool,
    cleanupDone:bool, stopAfterStart:bool, started:bool, outputs:int,
    finishPublished:bool, finishCode:int, reportAck:bool, completed:bool}
  var s: State
  var op: str
  var arg: int
  var stops: int
  // Disk inputs: reserve threshold at 1024 (2), measurement error (3), reserve at zero (4).
  pure def evidence(input:int): DiskEvidence =
    if (input == 3) {code:"disk.measurement.failed",owned:0,available:0,message:"meter failed"}
    else if (Set(2,4).contains(input))
      {code:"filesystem.reserve.reached",owned:4096,available:if(input==2) 1024 else 0,message:""}
    else {code:"",owned:0,available:0,message:""}
  pure def failedEvidence(report:bool): Diagnostic =
    if (report) {code:"report.io",message:"write: fixture"}
    else {code:"fixture.failed",message:"fixture"}
  pure def appendDiagnostic(entries:List[Diagnostic], diagnostic:Diagnostic): List[Diagnostic] =
    if (entries.foldl(false, (found, entry) => found or entry == diagnostic)) entries
    else entries.append(diagnostic)
  def code(t:State): int = if(t.interrupted) 130 else if(t.fatal) 2 else if(t.incomplete) 4 else 0
  def emit(t:State,k:int,p:int): State = t.with("pending",k).with("phase",p)
    .with("outputs",t.outputs + if(Set(1,2,5,6).contains(k)) 1 else 0)
  def done(t:State): State = t.with("pending",8).with("phase",3)
  def cleanup(t:State): State = emit(t,4,2)
  def afterCleanup(t:State): State = if(t.deliveryFailed) done(t)
    else if(SESSION and EARLY_SESSION) emit(t,7,1)
    else emit(t,6,1).with("finishPublished",true).with("finishCode",code(t))
  def beginStop(t:State): State =
    if(t.disk==1 and (t.pending==2 or (t.stopAfterStart and t.pending==1))) t
    else if(t.disk==1 and t.reported) emit(t,2,t.phase)
    else if(not(t.reported)) emit(t,1,t.phase).with("stopAfterStart",true)
    else if(t.copy) emit(t,3,1) else cleanup(t)
  val frozen = Set(4,6,7,8).contains(s.pending)
  action init = all {
    s'={pending:if(COPY) 1 else 0,phase:0,copy:COPY,reported:false,disk:0,firstDisk:0,diskEvidence:evidence(0),diskSecondary:List(),
      interrupted:false,fatal:false,incomplete:false,deliveryFailed:false,
      cleanupDone:false,stopAfterStart:false,started:false,outputs:if(COPY) 1 else 0,
      finishPublished:false,finishCode:0,reportAck:false,completed:false},
    op'="init",arg'=0,stops'=0
  }
  action stop(c:int): bool = {
    val first = c >= 2 and s.disk == 0
    val t=s.with("started",true).with("interrupted",s.interrupted or c==0)
      .with("fatal",s.fatal or c>=2).with("incomplete",s.incomplete or c!=0)
      .with("disk",if(first) 1 else s.disk)
      .with("firstDisk",if(first) c else s.firstDisk)
      .with("diskEvidence",if(first) evidence(c) else s.diskEvidence)
    all {stops<2,s'=if(frozen) s else beginStop(t),op'="stop",arg'=c,stops'=stops+1}
  }
  action ack = {
    val t=s.with("completed",true)
    val following=if(s.pending==1) {
      val reported=t.with("reported",true).with("stopAfterStart",false)
      if(s.disk==1) beginStop(reported) else if(s.copy) emit(reported,3,1) else cleanup(reported)
    } else if(s.pending==2) beginStop(t.with("disk",2))
    else if(s.pending==3) cleanup(t)
    else if(s.pending==4) afterCleanup(t.with("copy",false).with("cleanupDone",true))
    else if(s.pending==5) {if(s.cleanupDone) afterCleanup(t) else cleanup(t)}
    else if(s.pending==6) {if(SESSION) emit(t.with("reportAck",true),7,1) else done(t.with("reportAck",true))}
    else done(t)
    all {s.started,s.pending!=8,s'=following,op'="ack",arg'=0,stops'=stops}
  }
  action failure(report:bool): bool = {
    val t=s.with("fatal",true).with("completed",true).with("deliveryFailed",s.deliveryFailed or report)
      .with("diskSecondary",if(s.disk==0) s.diskSecondary else appendDiagnostic(s.diskSecondary,failedEvidence(report)))
    val following=if(s.pending==1) cleanup(t)
      else if(s.pending==2 or s.pending==5) {
        val delivered=if(s.pending==2) t.with("disk",2) else t
        if(s.cleanupDone) afterCleanup(delivered) else cleanup(delivered)
      } else if(s.pending==3) emit(t,5,1)
      else if(s.pending==4) emit(t.with("copy",false).with("cleanupDone",true),5,1)
      else done(t)
    all {s.started,s.pending!=8,not(report) or Set(1,2,5,6).contains(s.pending),
      s'=following,op'="fail",arg'=if(report) 1 else 0,stops'=stops}
  }
  action probe(kind:str): bool = all {s.started,
    kind!="wrong" or s.pending!=8,
    kind!="duplicate" or s.completed,
    kind!="reservation" or s.pending==4,
    s'=s,op'=kind,arg'=0,stops'=stops}
  action step = {
    nondet cause=0.to(4).oneOf()
    nondet report=Set(false,true).oneOf()
    nondet kind=Set("retired","unknown","wrong","duplicate","reservation").oneOf()
    any {stop(cause),ack,failure(report),probe(kind)}
  }
  action cleanStep = any {all {stops==0,stop(0)},all {stops>0,s.pending!=8,ack},all {s.pending==8,probe("retired")}}
  action diskStep = any {all {stops==0,stop(2)},all {stops==1,stop(0)},all {stops==2,s.pending!=8,ack},all {s.pending==8,probe("retired")}}
  action failureStep = any {all {stops==0,stop(1)},all {stops>0,s.pending==1,ack},
    all {s.pending==3,failure(false)},all {s.pending==4,failure(false)},
    all {s.pending==5,failure(true)},all {s.pending==8,probe("retired")}}
  action terminalStep = any {
    all {stops==0,stop(1)},
    all {stops==1,s.pending!=6,s.pending!=8,ack},
    all {stops==1,s.pending==6,stop(0)},
    all {stops==2,s.pending!=8,ack},
    all {s.pending==8,probe("retired")},
  }
  action finalFailureStep = any {
    all {stops==0,stop(0)},
    all {stops>0,s.pending==1,failure(false)},
    all {s.pending==4,ack},
    all {s.pending==6,failure(true)},
    all {s.pending==8,probe("retired")},
  }
  action diskEvidenceOrder(first:int, later:int): bool = any {
    all {stops==0,stop(first)},
    all {stops==1,s.pending==1,ack},
    all {stops==1,s.pending==2,stop(later)},
    all {stops==2,s.pending!=8,ack},
    all {s.pending==8,probe("retired")},
  }
  action reserveEvidenceStep = diskEvidenceOrder(4,3)
  action measurementEvidenceStep = diskEvidenceOrder(3,2)
  action diagnosticEvidenceStep = any {
    all {stops==0,stop(3)},
    all {s.pending==1,ack},
    all {Set(2,4).contains(s.pending),failure(false)},
    all {s.pending==5,failure(true)},
    all {s.pending==8,probe("retired")},
  }
  val reserveEvidenceSurvives = s.reportAck and stops==2 and s.diskEvidence==evidence(4)
  val measurementEvidenceSurvives = s.reportAck and stops==2 and s.diskEvidence==evidence(3)
  val diagnosticEvidenceSurvives = s.pending==8 and s.diskEvidence==evidence(3) and
    s.diskSecondary == List(failedEvidence(false),failedEvidence(true))
  val secondarySet = s.diskSecondary.foldl(Set(),(entries,entry)=>entries.union(Set(entry)))
  val Safety=all {
    (s.disk==0) == (s.firstDisk==0),
    s.diskEvidence == evidence(s.firstDisk),
    s.disk==0 implies s.diskSecondary==List(),
    secondarySet.size()==s.diskSecondary.length(),
    secondarySet.subseteq(Set(failedEvidence(false),failedEvidence(true))),
    s.pending!=7 or s.reportAck,
    not(s.reportAck) or s.finishPublished,
    s.phase!=3 or s.pending==8,
    not(s.finishPublished and Set(6,7).contains(s.pending)) or s.finishCode==code(s),
    not(s.started) or s.pending!=0,
    not(s.finishPublished) or s.cleanupDone,
  }
}
module StopEarly { import StopModel(COPY=false,SESSION=false,EARLY_SESSION=false).* }
module StopCopy { import StopModel(COPY=true,SESSION=false,EARLY_SESSION=false).* }
module StopSession { import StopModel(COPY=true,SESSION=true,EARLY_SESSION=false).* }
module StopPrematureSession { import StopModel(COPY=true,SESSION=true,EARLY_SESSION=true).* }
