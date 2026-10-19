use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Instant;
use camino::Utf8Path;
use hoimin_core::{LineRange, MutationOperator, MutationOperatorSelection, MutationProfile};
use hoimin_python::{AnalyzeRequest, analyze_source_cancellable, payload::PayloadBudget};
static LIVE: AtomicUsize = AtomicUsize::new(0);
static PEAK: AtomicUsize = AtomicUsize::new(0);
static CALLS: AtomicUsize = AtomicUsize::new(0);
struct Alloc;
unsafe impl GlobalAlloc for Alloc {
 unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
  let ptr = unsafe { System.alloc(layout) };
  if !ptr.is_null() { let live = LIVE.fetch_add(layout.size(), Ordering::Relaxed)+layout.size(); PEAK.fetch_max(live, Ordering::Relaxed); CALLS.fetch_add(1, Ordering::Relaxed); }
  ptr
 }
 unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) { LIVE.fetch_sub(layout.size(), Ordering::Relaxed); unsafe { System.dealloc(ptr, layout) }; }
 unsafe fn realloc(&self, ptr: *mut u8, old: Layout, new: usize) -> *mut u8 {
  let result = unsafe { System.realloc(ptr, old, new) };
  if !result.is_null() { LIVE.fetch_sub(old.size(), Ordering::Relaxed); let live=LIVE.fetch_add(new,Ordering::Relaxed)+new; PEAK.fetch_max(live,Ordering::Relaxed); CALLS.fetch_add(1,Ordering::Relaxed); }
  result
 }
}
#[global_allocator] static ALLOCATOR: Alloc = Alloc;
fn main() {
 let args: Vec<_> = std::env::args().collect();
 let n: u32 = args[1].parse().unwrap(); let mode=&args[2];
 let source="flag = True\n\n".repeat(n as usize);
 let lines: Vec<_> = match mode.as_str() {
  "sparse" => (1..=n).map(|i| LineRange{start:2*i,end:2*i}).collect(),
  "control" => vec![LineRange{start:2*n+1,end:2*n+1}],
  "all" => Vec::new(),
  "overlap" => vec![LineRange{start:n,end:2*n},LineRange{start:1,end:2*n},LineRange{start:1,end:3}],
  _ => panic!(),
 };
 let mut operators=MutationOperatorSelection::default();
 for name in operators.names() { for op in MutationOperatorSelection::parse_selector(&name).unwrap() { operators.exclude(op); } }
 operators.include(MutationOperator::BooleanLiteral);
 let request=AnalyzeRequest{payload:PayloadBudget::default(),path:Utf8Path::new("subject.py"),lines:&lines,symbols:&[],operators:&operators,profile:MutationProfile::Full,max_candidates:1};
 // Warm up once; fixtures, operators and request stay outside measured analysis.
 drop(analyze_source_cancellable(&request,&source,||false).unwrap());
 let initial=LIVE.load(Ordering::Relaxed); PEAK.store(initial,Ordering::Relaxed); CALLS.store(0,Ordering::Relaxed);
 let start=Instant::now(); let output=analyze_source_cancellable(&request,&source,||false).unwrap(); let ns=start.elapsed().as_nanos();
 let live=LIVE.load(Ordering::Relaxed)-initial;let peak=PEAK.load(Ordering::Relaxed)-initial;let calls=CALLS.load(Ordering::Relaxed);
 let identity=format!("{:?}|{:?}|{}",output.candidates.iter().map(|c| &**c).collect::<Vec<_>>(),output.diagnostics,output.truncated);
 println!("{{\"n\":{n},\"mode\":\"{mode}\",\"source_bytes\":{},\"ranges\":{},\"ns\":{ns},\"requested_live_delta\":{live},\"requested_peak_delta\":{peak},\"allocation_calls\":{calls},\"candidates\":{},\"diagnostics\":{},\"truncated\":{},\"identity_digest\":\"{}\",\"output_string_bytes\":{}}}",source.len(),lines.len(),output.candidates.len(),output.diagnostics.len(),output.truncated,blake3::hash(identity.as_bytes()),output.candidates.iter().map(|c|c.original.len()+c.replacement.len()+c.path.as_str().len()+c.symbol.as_ref().map_or(0,String::len)).sum::<usize>());
}
