module FinalizationModel {
  const ALLOW_UNJOINED: bool
  var process: int
  var output: int
  var monitor: int
  var measurement: int
  var cleanup: int
  var rootCleanup: int
  // 0: not queried, 1: failed, 2: succeeded (possibly with no readings).
  var availability: int
  var availableReadings: int -> int
  var removedKnown: bool
  var removedBytes: int
  var unsafeGrant: bool
  var op: str
  var arg: int
  var result: str
  pure def flag(mask:int,bit:int):int = if ((mask / bit) % 2 == 1) 1 else 2
  pure def readingsFor(value: int): int -> int =
    if (value == 1) Map(0 -> 0)
    else if (value == 2) Map(0 -> 1)
    else if (value == 3) Map(1 -> 0)
    else if (value == 4) Map(0 -> 0, 1 -> 1)
    else Map()
  val safe = process==1 and output==1 and monitor==1 and measurement!=3
  action init = all {
    process'=0,output'=0,monitor'=0,measurement'=0,cleanup'=0,rootCleanup'=0,availability'=0,
    removedKnown'=false,removedBytes'=0,availableReadings'=Map(),unsafeGrant'=false,
    op'="init",arg'=0,result'="init",
  }
  action apply(command:str,value:int):bool = {
    val batch = Set("inner","outer").contains(command)
    val outer = command=="outer"
    val newP=if(batch and process==0) flag(value,1) else process
    val newO=if(batch and output==0 and (outer or process==0)) flag(value,2) else output
    val newM=if(batch and monitor==0 and (outer or (process==0 and output==0))) flag(value,4) else monitor
    val newMeasurement=if(command=="measure" and value==3) 3
      else if(command=="measure" and value<2 and measurement!=3) value+1 else measurement
    val newSafe=newP==1 and newO==1 and newM==1 and newMeasurement!=3
    val admission=command=="request" and cleanup==0 and process==1 and output==1 and monitor==1 and (measurement!=3 or ALLOW_UNJOINED)
    val deferred=command=="defer" and cleanup==0 and process!=0 and output!=0 and monitor!=0 and rootCleanup==0
    val completed=command=="complete" and rootCleanup==1 and (value==4 or safe)
    val newRoot=if(admission) 1 else if(deferred) 4 else if(completed) value else rootCleanup
    val newCleanup=if(command=="complete") (if(completed) value else 1)
      else if(command=="request" or command=="defer") (if(cleanup!=0) cleanup else if(deferred) 4 else 1) else cleanup
    val queryAvailable = command == "available" and availability == 0
    val newAvailability = if (queryAvailable) (if (value == 0) 1 else 2) else availability
    val knownRemoved=newCleanup==2 and Set(1,2).contains(newMeasurement)
    val response=
      if(command=="inner") (if(process!=0 or output!=0 or monitor!=0) "error" else if(newSafe) "yes" else "no")
      else if(command=="outer") (if(value==7 and newSafe) "yes" else "no")
      else if(command=="request") (if(admission) "yes" else "no")
      else if(command=="defer") (if(deferred) "yes" else "no")
      else if(command=="complete") (if(completed) "yes" else "no")
      else if(command=="measure") (if(value>=2) "error" else "ok")
      else if(queryAvailable and value==0) "error" else "ok"
    all {
      process'=newP,output'=newO,monitor'=newM,measurement'=newMeasurement,cleanup'=newCleanup,rootCleanup'=newRoot,availability'=newAvailability,
      removedKnown'=knownRemoved,removedBytes'=if(knownRemoved) newMeasurement-1 else 0,
      availableReadings' = if (queryAvailable) readingsFor(value) else availableReadings,
      unsafeGrant'=(unsafeGrant or (admission and not(safe))),op'=command,arg'=value,result'=response,
    }
  }
  action step = {
    nondet command=Set("inner","outer","measure","request","defer","complete","available").oneOf()
    nondet value=(if(Set("inner","outer").contains(command)) 0.to(7) else if(command=="measure") 0.to(3) else if(command=="complete") Set(2,3,4) else if(command=="available") 0.to(5) else Set(0)).oneOf()
    apply(command,value)
  }
  action refusedStep = if(op=="init") apply("request",0) else if(process==0) apply("outer",7) else apply("request",0)
  action unjoinedStep = if(op=="init") apply("measure",3) else if(op=="measure" and arg==3) apply("measure",1)
    else if(process==0) apply("outer",7) else apply("request",0)
  action cleanStep = if(process==0) apply("outer",7) else if(measurement==0) apply("measure",1)
    else if(cleanup==0) apply("request",0) else if(rootCleanup==1) apply("complete",2) else apply("measure",3)
  action availableStep = if(op=="init") apply("available",0) else apply("available",2)
  action pairedAvailableStep = if (availability == 0) apply("available", 4) else apply("available", 2)
  action missingAvailableStep = if (availability == 0) apply("available", 3) else apply("available", 1)

  val distinctFilesystems = if (availableReadings.keys() == Set(0, 1))
    availableReadings.get(0) != availableReadings.get(1) else false
  val missingFirstFilesystem = availableReadings.keys() == Set(1)
  val successfulEmptyQuery = availability == 2 and availableReadings.keys() == Set()
  val failedQueryStaysFailed = availability == 1 and op == "available" and arg > 0
  val Safety = all {not(unsafeGrant),not(removedKnown) or (cleanup==2 and measurement!=3),
    availability == 2 or availableReadings.keys() == Set(),
    availableReadings.keys().subseteq(Set(0, 1)),
  }
}
module Finalization { import FinalizationModel(ALLOW_UNJOINED=false).* }
module FinalizationUnjoined { import FinalizationModel(ALLOW_UNJOINED=true).* }
