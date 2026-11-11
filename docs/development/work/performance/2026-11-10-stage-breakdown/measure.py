"""One-off phase measurement. Run from the original repository root on macOS."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import platform
import random
import signal
import statistics
import subprocess
import sys
import tempfile
import time
from collections import defaultdict

parser=argparse.ArgumentParser()
parser.add_argument('--baseline',type=Path,required=True)
parser.add_argument('--probe',type=Path,required=True)
parser.add_argument('--output',type=Path,required=True)
parser.add_argument('--repeats',type=int,default=7)
args=parser.parse_args()
args.output.mkdir(parents=True,exist_ok=False)
raw=args.output/'raw';raw.mkdir()
ENV=dict(os.environ);ENV.pop('RUST_LOG',None)
SEED=20261110
# (name, mutants, extra file count, extra total bytes, test executable)
specs=[('tiny-python-16',16,0,0,'python'),('tiny-python-64',64,0,0,'python'),
 ('tiny-true-16',16,0,0,'true'),('tiny-true-64',64,0,0,'true'),
 ('8m-python-16',16,1,8*1024**2,'python'),('32m-python-16',16,1,32*1024**2,'python'),
 ('8m-true-16',16,1,8*1024**2,'true'),('32m-true-16',16,1,32*1024**2,'true'),
 ('128files-8m-true-16',16,128,8*1024**2,'true'),('512files-8m-true-16',16,512,8*1024**2,'true')]
result={'revision':subprocess.check_output(['git','rev-parse','HEAD'],text=True).strip(),
 'platform':platform.platform(),'python':sys.version,'seed':SEED,'repeats':args.repeats,
 'binaries':{k:{'path':str(p.resolve()),'sha256':hashlib.sha256(p.read_bytes()).hexdigest()} for k,p in [('baseline',args.baseline),('probe',args.probe)]},
 'runs':[],'controls':[],'cases':[]}

def invoke(argv,stem,env):
 stdout=raw/(stem+'.stdout');stderr=raw/(stem+'.stderr')
 with stdout.open('wb') as out,stderr.open('wb') as err:
  start=time.perf_counter_ns()
  child=subprocess.Popen(argv,stdout=out,stderr=err,env=env,start_new_session=True)
  spawned=time.perf_counter_ns()
  def expire(signum,frame):
   os.killpg(child.pid,signal.SIGKILL);raise TimeoutError(argv)
  old=signal.signal(signal.SIGALRM,expire);signal.alarm(30)
  try:code=child.wait()
  finally:signal.alarm(0);signal.signal(signal.SIGALRM,old)
  end=time.perf_counter_ns()
 return {'argv':argv,'exit_code':code,'wall_ms':(end-start)/1e6,'outer_spawn_ms':(spawned-start)/1e6,'outer_wait_ms':(end-spawned)/1e6},stdout,stderr

with tempfile.TemporaryDirectory(prefix='hoimin-phase-inputs-') as temporary:
 base=Path(temporary)
 plans={}
 for name,n,files,total,command in specs:
  root=base/name;root.mkdir();(root/'case.py').write_text('x = 1 + 2\n'*n)
  if files:
   data=root/'data';data.mkdir()
   for i in range(files):(data/f'{i}.bin').write_bytes(b'x'*(total//files))
  test=[sys.executable,'-c','pass'] if command=='python' else ['/usr/bin/true']
  case={'name':name,'mutants':n,'padding_files':files,'padding_bytes':total,'source_bytes':(root/'case.py').stat().st_size,'test':test}
  result['cases'].append(case)
  # One identical manifest drives both binaries; no probe timings from plan enter execution records.
  env=dict(ENV,TMPDIR=str(base))
  argv=[str(args.baseline.resolve()),'plan','--root',str(root),'--file','case.py','--operators','binary_add_sub',
   '--jobs','1','--max-mutants','128','--max-candidates','128','--max-copy-size','256MiB',
   '--max-workspace-size','512MiB','--min-free-space','10GiB','--allow-best-effort-memory','--',*test]
  obs,out,err=invoke(argv,name+'-plan',env)
  assert obs['exit_code']==0,err.read_text()
  document=json.loads(out.read_text());assert len(document['candidates'])==n and not document['truncated']
  plans[name]=out

 def measure(case,label,repeat):
  stem=f"{case['name']}-{label}-{repeat}"
  with tempfile.TemporaryDirectory(prefix='hoimin-phase-owned-') as owned:
   env=dict(ENV,TMPDIR=owned)
   metrics=raw/(stem+'.metrics.json')
   binary=args.baseline if label=='baseline' else args.probe
   obs,out,err=invoke([str(binary.resolve()),'verify',str(plans[case['name']].resolve()),'--top',str(case['mutants']),'--metrics',str(metrics.resolve())],stem,env)
  assert obs['exit_code']==0,(stem,err.read_text())
  events=[json.loads(line) for line in out.read_text().splitlines()]
  summary=next(e for e in events if e['kind']=='run_finished')
  assert summary['complete'] and summary['exit_code']==0 and summary['counts']['survived']==case['mutants'],(stem,summary)
  assert not [e for e in events if e['kind']=='diagnostic'],(stem,events)
  process_events=[]
  for e in events:
   if e['kind']=='baseline_finished':process_events.append(e['elapsed_ms'])
   if e['kind']=='mutant_finished':
    t=e['outcome']['executed']['elapsed'];process_events.append(t['secs']*1000+t['nanos']/1e6)
  assert len(process_events)==case['mutants']+1
  probe=[json.loads(line.removeprefix('HOIMIN_PROBE ')) for line in err.read_text().splitlines() if line.startswith('HOIMIN_PROBE ')]
  assert len(probe)==(1 if label=='probe' else 0),(stem,err.read_text())
  probe=probe[0] if probe else []
  grouped=defaultdict(list)
  for name,ns in probe:grouped[name].append(ns/1e6)
  if label=='probe':
   for name in ['process.total','process.prepare','process.spawn','process.wait','process.supervisor_finish','process.output_finish']:
    assert len(grouped[name])==case['mutants']+1,(stem,name,len(grouped[name]))
   assert len(grouped['workspace.preflight'])==1
   assert len(grouped['workspace.create_worker'])==1
   assert len(grouped['workspace.apply'])==case['mutants']
   assert len(grouped['workspace.reset'])==case['mutants']+1,(stem,len(grouped['workspace.reset']))
  m=json.loads(metrics.read_text())
  return dict(obs,case=case['name'],label=label,repeat=repeat,metrics=m,
   process_event_ms=process_events,probe_events=probe,
   probe_totals_ms={k:sum(v) for k,v in grouped.items()},probe_counts={k:len(v) for k,v in grouped.items()},
   semantic_counts=summary['counts'])

 for case in result['cases']:
  for label in ['baseline','probe']:measure(case,label,'warmup')
 rng=random.Random(SEED)
 for repeat in range(args.repeats):
  cases=list(result['cases']);rng.shuffle(cases)
  for case in cases:
   labels=['baseline','probe']
   if repeat%2:labels.reverse()
   for label in labels:result['runs'].append(measure(case,label,repeat))
 # Direct subprocess controls use the same interpreter and cwd, without Hoimin.
 controls=[('python',[sys.executable,'-c','pass']),('true',['/usr/bin/true'])]
 for repeat in range(34):
  order=list(controls);rng.shuffle(order)
  for name,argv in order:
   obs,out,err=invoke(argv,f'control-{name}-{repeat}',ENV)
   assert obs['exit_code']==0
   if repeat>=3:result['controls'].append(dict(obs,case=name,repeat=repeat-3))
result['status']='passed'
(args.output/'result.json').write_text(json.dumps(result,indent=2)+'\n')
print(json.dumps({'status':'passed','runs':len(result['runs']),'controls':len(result['controls']),'output':str(args.output)}))
