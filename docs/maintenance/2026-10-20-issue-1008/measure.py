"""Serialized paired measurement; subprocesses bounded to 25 seconds each."""
import hashlib,json,pathlib,platform,statistics,subprocess
HERE=pathlib.Path(__file__).resolve().parent
BASE=pathlib.Path('/tmp/hoimin-perf-three-task3')
WRAPPER=HERE.parent/'2026-10-20-issue-1006/resource_wrapper.py'
def sha(data):return hashlib.sha256(data).hexdigest()
def run(command):
 p=subprocess.run(['python3',str(WRAPPER),*map(str,command)],capture_output=True,text=True,timeout=30,check=True)
 data=json.loads(p.stdout);assert data['code'] in (0,1),(command,data)
 data['command']=list(map(str,command)); data['command_sha256']=sha(json.dumps(data['command']).encode())
 data['stdout_sha256']=sha(data['stdout'].encode());return data
workloads=[]
for n in [262144,524288,1048576]:workloads.append((str(n),n,'#'+'é'*n+'\nflag = True\n'))
workloads += [('tiny',0,'flag = True\n'),('small',128, '# Small arithmetic helpers\n'+''.join(f'def helper_{i}(value):\n    adjusted = value + {i}\n    return adjusted * 2\n\n' for i in range(24))+'flag = True\n')]
rows=[];cli=[];fixtures={}
binaries={'before':pathlib.Path('/tmp/hoimin-perf-three-baseline/release/hoimin'),'after':BASE/'hoimin-after'}
for label,n,source in workloads:
 root=BASE/f'fixture-{label}';root.mkdir(exist_ok=True);(root/'source.py').write_text(source)
 fixtures[label]={'bytes':len(source.encode()),'sha256':sha(source.encode())}
 for mode in ['context','index-build','pipeline','lookup','discover','plan','run']:
  for revision in ['before','after']:run([BASE/f'probe-{revision}',root,mode,n])
  for sample in range(3):
   for revision in (['before','after'] if sample%2==0 else ['after','before']):
    data=run([BASE/f'probe-{revision}',root,mode,n]);row=json.loads(data.pop('stdout'));row.update(label=label,revision=revision,sample=sample,process=data);rows.append(row)
  print('probe',label,mode,flush=True)
 for mode in ['plan','run']:
  tail=[mode,'--root',str(root),'--file','source.py','--operators','boolean_literal','--max-candidates','1','--jobs','1','--max-mutants','1','--allow-best-effort-memory','--min-free-space','1B','--','/usr/bin/true']
  for revision in ['before','after']:run([binaries[revision],*tail])
  for sample in range(3):
   for revision in (['before','after'] if sample%2==0 else ['after','before']):
    data=run([binaries[revision],*tail]);output=json.loads(data.pop('stdout'));candidates=output['candidates'] if mode=='plan' else [m['candidate'] for m in output['mutants']];assert len(candidates)==1
    cli.append(dict(label=label,mode=mode,revision=revision,sample=sample,candidates=len(candidates),digest=candidates[0]['id'],process=data))
for collection in [rows,cli]:
 for label,mode in {(r['label'],r['mode']) for r in collection}:
  assert len({r['digest'] for r in collection if (r['label'],r['mode'])==(label,mode)})==1,(label,mode)
summary=[]
for label,mode,revision in sorted({(r['label'],r['mode'],r['revision']) for r in rows}):
 group=[r for r in rows if (r['label'],r['mode'],r['revision'])==(label,mode,revision)]
 summary.append(dict(label=label,mode=mode,revision=revision,medians={key:statistics.median(r[key] for r in group) for key in ['ns','requested_peak_delta','requested_live_delta']}))
result=dict(platform=platform.platform(),fixtures=fixtures,samples=rows,cli_samples=cli,summary=summary,hashes={str(p):sha(p.read_bytes()) for p in [HERE/'probe.rs',HERE/'measure.py',HERE/'build_probe.py',WRAPPER,*binaries.values(),BASE/'probe-before',BASE/'probe-after']},method={'samples':'One separate warmup per revision/workload, then 3 paired samples alternating order; no simultaneous builds or benchmarks.','heap':'Allocator requested bytes, incremental live/peak. Source prepared before context/pipeline/lookup windows; file read inside discovery/CLI windows. Runtime initialized before windows. Instrumented allocator probe, not pristine executable.','time':'Probe Instant measured interval; ordinary CLI process elapsed from separate uninstrumented release binaries. Wrapper process RSS is RUSAGE_CHILDREN in bytes (macOS), not heap or aggregate process-tree peak.','limits':'25 s per child, 30 s per wrapper; fixed 1 candidate, max1, jobs1; /usr/bin/true executor. Source fixtures at most 2 MiB.','payload':'Logical payload unchanged; exact-limit tests independently verify 395 success/394 failure. Heap is not payload or workspace disk charges.','baseline':'Original 34e3349099ce7fd61838629ee8c0d0b814e65a5d frozen release. Candidate includes Tasks1 and2; no selected line ranges and one tiny workspace file minimize unrelated effects, not an isolated causal timing guarantee.'})
(BASE/'measurement.json').write_text(json.dumps(result,indent=2)+'\n')
print('saved measurement.json',flush=True)
