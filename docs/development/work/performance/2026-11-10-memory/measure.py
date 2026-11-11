"""macOS memory observations with the unchanged release binary; no third-party modules."""
import ctypes as C
import hashlib,json,os,random,signal,statistics,subprocess,sys,tempfile,time
from pathlib import Path
from collections import defaultdict
ROOT=Path.cwd();BINARY=ROOT/'target/release/hoimin';OUT=Path(sys.argv[1]);OUT.mkdir(exist_ok=False);RAW=OUT/'raw';RAW.mkdir()
lib=C.CDLL('/usr/lib/libproc.dylib',use_errno=True)
class Task(C.Structure):
 _fields_=[(n,C.c_uint64) for n in ['virtual','resident','total_user','total_system','threads_user','threads_system']]+[(n,C.c_int32) for n in ['policy','faults','pageins','cow','messages_sent','messages_received','mach_calls','unix_calls','csw','threadnum','numrunning','priority']]
lib.proc_pidinfo.argtypes=[C.c_int,C.c_int,C.c_uint64,C.c_void_p,C.c_int];lib.proc_pidinfo.restype=C.c_int
lib.proc_listchildpids.argtypes=[C.c_int,C.c_void_p,C.c_int];lib.proc_listchildpids.restype=C.c_int
assert C.sizeof(Task)==96

def rss(pid):
 t=Task();size=lib.proc_pidinfo(pid,4,0,C.byref(t),C.sizeof(t))
 return int(t.resident) if size==C.sizeof(t) else None

def children(pid):
 a=(C.c_int*1024)();n=lib.proc_listchildpids(pid,a,C.sizeof(a))
 assert n<len(a),'child pid buffer exceeded'
 return list(a[:max(0,n)])

def snapshot(pid):
 pending=[pid];seen=set();rows=[]
 while pending:
  p=pending.pop()
  if p in seen:continue
  seen.add(p);value=rss(p)
  if value is not None:rows.append((p,value))
  pending.extend(children(p))
 return rows

# ABI smoke check: self must have resident pages and our sleeping child must be listed.
assert rss(os.getpid()) and rss(os.getpid())>1024*1024
smoke=subprocess.Popen(['/bin/sleep','0.1']);assert smoke.pid in children(os.getpid());assert rss(smoke.pid);smoke.wait()
ENV=dict(os.environ);ENV.pop('RUST_LOG',None)

def invoke(argv,stem,env,expected):
 out=RAW/(stem+'.stdout');err=RAW/(stem+'.stderr');samples=[]
 with out.open('wb') as stdout,err.open('wb') as stderr:
  start=time.perf_counter_ns();p=subprocess.Popen(argv,stdout=stdout,stderr=stderr,env=env,start_new_session=True)
  while True:
   pid,status,usage=os.wait4(p.pid,os.WNOHANG)
   if pid:
    p.returncode=os.waitstatus_to_exitcode(status);break
   now=time.perf_counter_ns();rows=snapshot(p.pid)
   samples.append({'elapsed_ms':(now-start)/1e6,'rss_bytes':rows})
   if now-start>30_000_000_000:
    os.killpg(p.pid,signal.SIGKILL);p.wait();raise TimeoutError(stem)
   time.sleep(.002)
  end=time.perf_counter_ns()
 assert p.returncode==expected,(stem,p.returncode,err.read_text())
 parent=[v for s in samples for pid,v in s['rss_bytes'] if pid==p.pid]
 tree=[sum(v for _,v in s['rss_bytes']) for s in samples if s['rss_bytes']]
 assert parent and tree,(stem,'no RSS observations')
 return {'argv':argv,'exit_code':p.returncode,'wall_ms':(end-start)/1e6,'os_reported_max_rss_bytes':usage.ru_maxrss,
  'sampled_parent_max_rss_bytes':max(parent),'sampled_tree_max_sum_rss_bytes':max(tree),'sampled_max_descendants':max(len(s['rss_bytes'])-1 for s in samples),
  'samples':samples},out

result={'revision':subprocess.check_output(['git','rev-parse','HEAD'],text=True).strip(),'binary_sha256':hashlib.sha256(BINARY.read_bytes()).hexdigest(),'python':sys.version,'runs':[],'cases':[],
 'method':'unchanged release executable; wait4 kernel max RSS and libproc taskinfo/childpid sum sampled every 2ms plus API time; no ps subprocess. A sample traverses processes sequentially, so tree values are approximate sums, not atomic physical-memory peaks; shared pages can be counted repeatedly. The monitor parent is excluded. wait4 max RSS is saved as OS reported usage, not summed concurrent tree RSS. One discarded warmup per case, 3 shuffled measured rounds, seed20261110. Each CLI invocation has isolated TMPDIR. All output to files. No timing conclusions from monitored runs.'}
with tempfile.TemporaryDirectory(prefix='hoimin-mem-inputs-') as tmp:
 base=Path(tmp);cases=[]
 specs=[('tiny-python-64',64,0,0,1,'pass'),('32m-python-16',16,1,32*1024**2,1,'pass'),('512files-8m-python-16',16,512,8*1024**2,1,'pass')]
 specs.extend((f'workers-{j}-8m-128files',16,128,8*1024**2,j,'import time; time.sleep(0.03)') for j in [1,2,4,8])
 specs.extend([('child64m',1,0,0,1,'import time; data=bytearray(64*1024*1024); time.sleep(0.1)'),('child-control',1,0,0,1,'import time; time.sleep(0.1)')])
 for name,n,files,total,jobs,code in specs:
  root=base/name;root.mkdir();(root/'case.py').write_text('x = 1 + 2\n'*n)
  if files:
   (root/'data').mkdir()
   for i in range(files):(root/'data'/f'{i}.bin').write_bytes(b'x'*(total//files))
  argv=[str(BINARY),'plan','--root',str(root),'--file','case.py','--operators','binary_add_sub','--jobs',str(jobs),'--max-mutants','128','--max-candidates','128','--max-copy-size','512MiB','--max-workspace-size','2GiB','--allow-best-effort-memory','--',sys.executable,'-c',code]
  env=dict(ENV,TMPDIR=str(base));prep=subprocess.run(argv,stdout=subprocess.PIPE,stderr=subprocess.PIPE,env=env,timeout=30,check=True)
  plan=RAW/(name+'.plan.json');plan.write_bytes(prep.stdout);doc=json.loads(prep.stdout);assert len(doc['candidates'])==n and not doc['truncated']
  cases.append({'name':name,'mode':'verify','candidates':n,'padding_bytes':total,'padding_files':files,'jobs':jobs,'test_code':code,'argv':[str(BINARY),'verify',str(plan.resolve()),'--top',str(n)]})
 for n in [1000,4000,16000]:
  root=base/f'plan-{n}';root.mkdir();(root/'case.py').write_text('x = 1 + 2\n'*n)
  cases.append({'name':f'plan-{n}','mode':'plan','candidates':n,'source_bytes':10*n,'jobs':1,
   'argv':[str(BINARY),'plan','--root',str(root),'--file','case.py','--operators','binary_add_sub','--max-candidates','20000','--allow-best-effort-memory','--',sys.executable,'-c','pass']})
 result['cases']=cases
 def observe(case,repeat):
  with tempfile.TemporaryDirectory(prefix='hoimin-mem-owned-') as owned:
   r,out=invoke(case['argv'],f"{case['name']}-{repeat}",dict(ENV,TMPDIR=owned),0)
  if case['mode']=='plan':
   doc=json.loads(out.read_text());assert len(doc['candidates'])==case['candidates'] and not doc['truncated']
  else:
   events=[json.loads(line) for line in out.read_text().splitlines()];summary=next(e for e in events if e['kind']=='run_finished')
   assert summary['complete'] and summary['counts']['survived']==case['candidates']
  return dict(r,case=case['name'],repeat=repeat)
 for case in cases:observe(case,'warmup')
 rng=random.Random(20261110)
 for repeat in range(3):
  order=list(cases);rng.shuffle(order)
  for case in order:result['runs'].append(observe(case,repeat))
result['status']='passed'
(OUT/'result.json').write_text(json.dumps(result,indent=2)+'\n')
print(json.dumps({'status':'passed','cases':len(cases),'runs':len(result['runs'])}))
for c in cases:
 rs=[r for r in result['runs'] if r['case']==c['name']]
 print(c['name'],{k:round(statistics.median(r[k] for r in rs)/1048576,2) for k in ['os_reported_max_rss_bytes','sampled_parent_max_rss_bytes','sampled_tree_max_sum_rss_bytes']},'descendants',max(r['sampled_max_descendants'] for r in rs))
