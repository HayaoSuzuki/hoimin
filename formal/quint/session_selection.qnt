// Rows are appended in chronological order; each operation closes its DB handle.
module SessionSelectionModel {
  const OLDEST: bool
  type Row = { fingerprint: int, budget: int, complete: bool }
  var rows: List[Row]
  var op: str
  var fingerprint: int
  var budget: int
  var complete: bool
  var selected: int
  var expected: int
  var freshReason: str

  // With orderly closes and valid rows, no concurrent change can invalidate the read.
  pure def reasonForFresh(history: List[Row], fp: int, limit: int): str = {
    val matching = history.select(row => row.fingerprint == fp)
    if (matching.select(row => not(row.complete) and row.budget > limit).length() > 0)
      "budget_decreased"
    else if (matching.select(row => row.complete).length() > 0) "matching_run_complete"
    else if (history.select(row => not(row.complete)).length() > 0) "fingerprint_mismatch"
    else if (history.length() > 0) "no_incomplete_run"
    else "no_prior_run"
  }

  action init = all {
    rows'=[],op'="init",fingerprint'=0,budget'=1,complete'=false,selected'=-1,expected'=-1,
    freshReason'="",
  }
  action create(f: int,b: int,c: bool): bool = all {
    rows.length()<2, rows'=rows.append({fingerprint:f,budget:b,complete:c}),
    op'="append",fingerprint'=f,budget'=b,complete'=c,selected'=-1,expected'=-1,
    freshReason'="",
  }
  action load(f: int,b: int): bool = {
    val eligible=rows.indices().filter(i=>{
      val row=rows.nth(i)
      row.fingerprint==f and row.budget<=b and not(row.complete)
    })
    val newest=if(eligible.size()==0) -1 else eligible.fold(-1,(a,i)=>if(i>a) i else a)
    val oldest=if(eligible.size()==0) -1 else eligible.fold(2,(a,i)=>if(i<a) i else a)
    val picked=if(OLDEST) oldest else newest
    all {
      rows'=if(picked<0) rows else rows.replaceAt(picked,rows.nth(picked).with("budget",b)),
      op'="load",fingerprint'=f,budget'=b,complete'=false,selected'=picked,expected'=newest,
      freshReason'=if (picked < 0) reasonForFresh(rows, f, b) else "",
    }
  }
  action step = {
    nondet f=Set(0,1).oneOf()
    nondet b=Set(1,2).oneOf()
    nondet c=Set(false,true).oneOf()
    any {create(f,b,c),load(f,b)}
  }
  action latestStep = any {
    all {op=="init",create(0,1,false)},
    all {op=="append",rows.length()==1,create(0,1,false)},
    all {rows.length()==2,load(0,2)},
  }
  action fallbackStep = any {
    all {op=="init",create(0,1,false)},
    all {op=="append",rows.length()==1,create(0,2,false)},
    all {rows.length()==2,load(0,1)},
  }
  action freshPriorityStep = any {
    all { op == "init", load(0, 1) },
    all { op == "load", rows.length() == 0, create(0, 1, true) },
    all { op == "append", rows.length() == 1, load(0, 1) },
    all { op == "load", rows.length() == 1, create(0, 2, false) },
    all { rows.length() == 2, load(0, 1) },
  }

  val overBudget = rows.indices().exists(i => {
    val row = rows.nth(i)
    row.fingerprint == fingerprint and not(row.complete) and row.budget > budget
  })
  val matchingComplete = rows.indices().exists(i => {
    val row = rows.nth(i)
    row.fingerprint == fingerprint and row.complete
  })
  val incomplete = rows.indices().exists(i => not(rows.nth(i).complete))
  val fresh = op == "load" and selected < 0
  val noPriorRun = freshReason == "no_prior_run"
  val noIncompleteRun = freshReason == "no_incomplete_run"
  val fingerprintMismatch = freshReason == "fingerprint_mismatch"
  val matchingCompleteWins = freshReason == "matching_run_complete" and incomplete
  val decreasedBudgetWins = freshReason == "budget_decreased" and matchingComplete
  val FreshReasonPriority = if (fresh) all {
    (freshReason == "budget_decreased") == overBudget,
    (freshReason == "matching_run_complete") == (not(overBudget) and matchingComplete),
    (freshReason == "fingerprint_mismatch") == (not(overBudget or matchingComplete) and incomplete),
    (freshReason == "no_incomplete_run") == (not(matchingComplete or incomplete) and rows.length() > 0),
    (freshReason == "no_prior_run") == (rows.length() == 0),
  } else freshReason == ""
  val Safety=all {selected==expected,rows.length()<=2,FreshReasonPriority,
    rows.indices().forall(i=>rows.nth(i).budget>=1 and rows.nth(i).budget<=2),
    selected<0 or all {
      not(rows.nth(selected).complete),
      rows.nth(selected).fingerprint == fingerprint,
      rows.nth(selected).budget == budget,
    },
  }
}
module SessionSelection { import SessionSelectionModel(OLDEST=false).* }
module SessionSelectionOldest { import SessionSelectionModel(OLDEST=true).* }
