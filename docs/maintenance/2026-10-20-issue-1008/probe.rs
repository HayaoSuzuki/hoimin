//! Standalone allocator probe linked unchanged against each preserved release.
use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Instant;
use camino::{Utf8Path, Utf8PathBuf};
use hoimin_core::{CandidateValidationContext, CandidateDescriptor, CANDIDATE_SCHEMA_VERSION, MutationOperatorSelection, MutationProfile, PythonSourceIndex, TargetSlice, validate_candidate_with_context};
use hoimin_python::{AnalyzeRequest, analyze_source_cancellable, payload::PayloadBudget};
static LIVE: AtomicUsize = AtomicUsize::new(0);
static PEAK: AtomicUsize = AtomicUsize::new(0);
struct Alloc;
unsafe impl GlobalAlloc for Alloc {
 unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
  let ptr = unsafe { System.alloc(layout) };
  if !ptr.is_null() { let live=LIVE.fetch_add(layout.size(),Ordering::Relaxed)+layout.size(); PEAK.fetch_max(live,Ordering::Relaxed); }
  ptr
 }
 unsafe fn dealloc(&self, ptr:*mut u8, layout:Layout) { LIVE.fetch_sub(layout.size(),Ordering::Relaxed); unsafe {System.dealloc(ptr,layout)}; }
 unsafe fn realloc(&self, ptr:*mut u8, old:Layout, new:usize) -> *mut u8 {
  let result=unsafe {System.realloc(ptr,old,new)};
  if !result.is_null() { LIVE.fetch_sub(old.size(),Ordering::Relaxed); let live=LIVE.fetch_add(new,Ordering::Relaxed)+new; PEAK.fetch_max(live,Ordering::Relaxed); } result
 }
}
#[global_allocator] static ALLOCATOR:Alloc=Alloc;
fn begin()->usize { let base=LIVE.load(Ordering::Relaxed); PEAK.store(base,Ordering::Relaxed); base }
fn result(mode:&str, n:usize, start:Instant, base:usize, digest:impl std::fmt::Display, candidates:usize) {
 let ns=start.elapsed().as_nanos(); let peak=PEAK.load(Ordering::Relaxed).saturating_sub(base); let live=LIVE.load(Ordering::Relaxed).saturating_sub(base);
 println!("{}",serde_json::json!({"mode":mode,"n":n,"ns":ns,"requested_peak_delta":peak,"requested_live_delta":live,"digest":digest.to_string(),"candidates":candidates,"context_stack_bytes":std::mem::size_of::<CandidateValidationContext<'static>>()}));
}
#[tokio::main(flavor="current_thread")]
async fn main() {
 let args:Vec<_>=std::env::args().collect(); let root=Utf8PathBuf::from(&args[1]); let mode=&args[2]; let n:usize=args[3].parse().unwrap();
 let source=std::fs::read(root.join("source.py")).unwrap();
 let mut operators=MutationOperatorSelection::default();
 for name in operators.names() {for op in MutationOperatorSelection::parse_selector(&name).unwrap(){operators.exclude(op);}}
 for op in MutationOperatorSelection::parse_selector("boolean_literal").unwrap(){operators.include(op);}
 let request=AnalyzeRequest{payload:PayloadBudget::default(),path:Utf8Path::new("source.py"),lines:&[],symbols:&[],operators:&operators,profile:MutationProfile::Full,max_candidates:1};
 let target=TargetSlice{path:"source.py".into(),lines:vec![],symbols:vec![]};
 match mode.as_str() {
  "context"=>{let base=begin();let start=Instant::now();let context=CandidateValidationContext::new(&source).unwrap();result(mode,n,start,base,context.file_hash(),0);std::hint::black_box(context);}
  "pipeline"=>{let base=begin();let start=Instant::now();let context=CandidateValidationContext::new(&source).unwrap();let output=analyze_source_cancellable(&request,context.decoded_source().unwrap().text(),||false).unwrap();assert_eq!(output.candidates.len(),1);let c=&output.candidates[0];let descriptor=CandidateDescriptor{schema_version:CANDIDATE_SCHEMA_VERSION,path:c.path.clone(),span:c.span,original:c.original.clone(),replacement:c.replacement.clone(),operator:c.operator.as_str().into(),line:c.line,column:c.column,symbol:c.symbol.clone(),file_hash:context.file_hash().into()};let id=validate_candidate_with_context(&context,&descriptor).unwrap();result(mode,n,start,base,&id,1);std::hint::black_box(context);}
  "index-build"=>{let text=std::str::from_utf8(&source).unwrap();let hash=blake3::hash(&source);let base=begin();let start=Instant::now();let index=PythonSourceIndex::new(text).unwrap();result(mode,n,start,base,hash,0);std::hint::black_box(index);}
  "lookup"=>{let index=PythonSourceIndex::new(std::str::from_utf8(&source).unwrap()).unwrap();let offset=source.windows(4).position(|w|w==b"True").unwrap();let base=begin();let start=Instant::now();let mut checksum=0u64;for _ in 0..100_000 {let (line,col)=std::hint::black_box(&index).line_and_column(std::hint::black_box(offset)).unwrap();checksum+=u64::from(line)+u64::from(col);}result(mode,n,start,base,checksum,0);}
  "discover"=>{let base=begin();let start=Instant::now();let output=hoimin_cli::analyzer::discover_targets(&root,&[target],&operators,MutationProfile::Full,1).await.unwrap();assert_eq!(output.candidates.len(),1);result(mode,n,start,base,&output.candidates[0].id,1);}
  "plan"|"run"=>{
   // Source bytes above are dropped before the whole CLI measurement; its file
   // read, configuration, discovery and execution allocations are inside it.
   drop(source);
   let command:Vec<std::ffi::OsString>=vec!["hoimin".into(),mode.into(),"--root".into(),root.into_std_path_buf().into_os_string(),"--file".into(),"source.py".into(),"--operators".into(),"boolean_literal".into(),"--max-candidates".into(),"1".into(),"--jobs".into(),"1".into(),"--max-mutants".into(),"1".into(),"--allow-best-effort-memory".into(),"--min-free-space".into(),"1B".into(),"--".into(),"/usr/bin/true".into()];
   let out=Writer::default();let err=Writer::default();let base=begin();let start=Instant::now();let code=hoimin_cli::run_with_writers(command,out.clone(),err.clone()).await;let ns=start.elapsed().as_nanos();let peak=PEAK.load(Ordering::Relaxed).saturating_sub(base);let live=LIVE.load(Ordering::Relaxed).saturating_sub(base);
   assert_eq!(code,if mode=="plan"{0}else{1},"{:?}",*err.0.lock().unwrap());let value:serde_json::Value=serde_json::from_slice(&out.0.lock().unwrap()).unwrap();let candidates=if mode=="plan"{value["candidates"].as_array().unwrap()}else{value["mutants"].as_array().unwrap()};assert_eq!(candidates.len(),1);let id=if mode=="plan"{&candidates[0]["id"]}else{&candidates[0]["candidate"]["id"]};println!("{}",serde_json::json!({"mode":mode,"n":n,"ns":ns,"requested_peak_delta":peak,"requested_live_delta":live,"digest":id,"candidates":candidates.len()}));
  }
  _=>panic!("unknown mode"),
 }
}
#[derive(Clone,Default)]
struct Writer(std::sync::Arc<std::sync::Mutex<Vec<u8>>>);
impl std::io::Write for Writer {
 fn write(&mut self,b:&[u8])->std::io::Result<usize>{self.0.lock().unwrap().extend_from_slice(b);Ok(b.len())}
 fn flush(&mut self)->std::io::Result<()>{Ok(())}
}
