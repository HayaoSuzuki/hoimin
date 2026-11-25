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
  action init = all {rows'=[],op'="init",fingerprint'=0,budget'=1,complete'=false,selected'=-1,expected'=-1}
  action create(f: int,b: int,c: bool): bool = all {
    rows.length()<2, rows'=rows.append({fingerprint:f,budget:b,complete:c}),
    op'="append",fingerprint'=f,budget'=b,complete'=c,selected'=-1,expected'=-1 }
  action load(f: int,b: int): bool = {
    val eligible=rows.indices().filter(i=>{
      val row=rows.nth(i)
      row.fingerprint==f and row.budget<=b and not(row.complete)
    })
    val newest=if(eligible.size()==0) -1 else eligible.fold(-1,(a,i)=>if(i>a) i else a)
    val oldest=if(eligible.size()==0) -1 else eligible.fold(2,(a,i)=>if(i<a) i else a)
    val picked=if(OLDEST) oldest else newest
    all { rows'=if(picked<0) rows else rows.replaceAt(picked,rows.nth(picked).with("budget",b)),
      op'="load",fingerprint'=f,budget'=b,complete'=false,selected'=picked,expected'=newest }
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
  val Safety=all {selected==expected,rows.length()<=2,
    rows.indices().forall(i=>rows.nth(i).budget>=1 and rows.nth(i).budget<=2),
    selected<0 or not(rows.nth(selected).complete) }
}
module SessionSelection { import SessionSelectionModel(OLDEST=false).* }
module SessionSelectionOldest { import SessionSelectionModel(OLDEST=true).* }
