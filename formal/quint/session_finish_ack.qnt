// A fresh session with cleanup complete, through the final report and session ACK.
module SessionFinishAckModel {
  const ACCEPT_MISMATCH: bool
  type Phase = Ready | Report | Closing | Finished
  type Reply = {runMatches:bool, complete:bool}
  type Command = Init | Publish(bool) | AckReport | Finish(Reply) | Fail | Deadline | Idle
  type Result = Accepted | Mismatch | Duplicate
  type Snapshot = {phase:Phase, complete:bool, reportAck:bool, reply:Reply,
    sessionAck:bool, failed:bool, result:Result, exit:int}
  var s:Snapshot
  var dispatch:Command

  action init = all {
    s' = {phase:Ready, complete:false, reportAck:false, reply:{runMatches:false, complete:false},
      sessionAck:false, failed:false, result:Accepted, exit:0},
    dispatch' = Init,
  }

  action publish(following:Snapshot, command:Command):bool = all {
    s' = following,
    dispatch' = command,
  }

  action issue(complete:bool):bool = all {
    s.phase == Ready,
    publish({ ...s, phase:Report, complete:complete, exit:if (complete) 0 else 4 }, Publish(complete)),
  }

  action ackReport = all {
    s.phase == Report,
    publish({ ...s, phase:Closing, reportAck:true, result:Accepted }, AckReport),
  }

  action finish(reply:Reply):bool = all {
    s.reportAck,
    if (s.phase == Finished) publish(s.with("result", Duplicate), Finish(reply))
    else if (ACCEPT_MISMATCH or (reply.runMatches and reply.complete == s.complete))
      publish({ ...s, phase:Finished, reply:reply, sessionAck:true, result:Accepted }, Finish(reply))
    else publish(s.with("result", Mismatch), Finish(reply)),
  }

  action failRequest = all {
    s.phase == Report or s.phase == Closing,
    publish({ ...s, phase:Finished, failed:true, result:Accepted, exit:2 }, Fail),
  }

  action deadline = all {
    s.phase != Ready,
    publish(s, Deadline),
  }

  action idle = publish(s, Idle)
  action step = any {
    nondet complete = Bool.oneOf()
    issue(complete),
    ackReport,
    nondet runMatches = Bool.oneOf()
    nondet complete = Bool.oneOf()
    finish({runMatches:runMatches, complete:complete}),
    failRequest,
    deadline,
    all { s.phase == Finished, idle },
  }

  action matchingReply = finish({runMatches:true, complete:s.complete})
  action cleanStep =
    if (s.phase == Ready) issue(true) else if (s.phase == Report) ackReport else matchingReply
  action wrongFlagStep =
    if (s.phase == Ready) issue(false)
    else if (s.phase == Report) ackReport
    else if (s.phase == Closing and s.result == Accepted) finish({runMatches:true, complete:true})
    else matchingReply
  action wrongRunStep =
    if (s.phase == Ready) issue(true)
    else if (s.phase == Report) ackReport
    else if (s.phase == Closing and s.result == Accepted) finish({runMatches:false, complete:true})
    else matchingReply
  action sessionFailureStep =
    if (s.phase == Ready) issue(true)
    else if (s.phase == Report) ackReport
    else if (s.phase == Closing) failRequest
    else matchingReply
  action reportFailureStep =
    if (s.phase == Ready) issue(true) else if (s.phase == Report) failRequest else idle
  action freezeStep =
    if (s.phase == Ready) issue(false)
    else if (dispatch == Publish(false) or dispatch == AckReport) deadline
    else if (s.phase == Report) ackReport else matchingReply

  val Safety = all {
    s.sessionAck implies s.reportAck and s.reply.runMatches and s.reply.complete == s.complete,
    s.phase == Finished implies (s.sessionAck != s.failed),
    s.phase == Closing implies s.reportAck,
    s.failed implies s.exit == 2,
    s.phase != Ready and not(s.failed) implies s.exit == (if (s.complete) 0 else 4),
  }
}

module SessionFinishAck { import SessionFinishAckModel(ACCEPT_MISMATCH=false).* }
module SessionFinishAckAcceptsMismatch { import SessionFinishAckModel(ACCEPT_MISMATCH=true).* }
