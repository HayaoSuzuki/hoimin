"""Preserve, then investigate noisy initial small-ASCII timings with 12 more pairs."""
import hashlib,json,pathlib,statistics,subprocess
BASE=pathlib.Path('/tmp/hoimin-perf-three-task3')
WRAPPER=pathlib.Path(__file__).resolve().parent.parent/'2026-10-20-issue-1006/resource_wrapper.py'
rows=[]
for label,n in [('tiny',0),('small',128)]:
 for mode in ['pipeline','ordinary-plan']:
  for sample in range(12):
   for revision in (['before','after'] if sample%2==0 else ['after','before']):
    root=BASE/f'fixture-{label}'
    if mode=='pipeline':command=[str(BASE/f'probe-{revision}'),str(root),'pipeline',str(n)]
    else:
     binary='/tmp/hoimin-perf-three-baseline/release/hoimin' if revision=='before' else str(BASE/'hoimin-after')
     command=[binary,'plan','--root',str(root),'--file','source.py','--operators','boolean_literal','--max-candidates','1','--jobs','1','--max-mutants','1','--allow-best-effort-memory','--min-free-space','1B','--','/usr/bin/true']
    result=subprocess.run(['python3',str(WRAPPER),*command],capture_output=True,text=True,check=True,timeout=30)
    data=json.loads(result.stdout); assert data['code']==0
    output=json.loads(data.pop('stdout'))
    rows.append(dict(label=label,mode=mode,sample=sample,revision=revision,command=command,command_sha256=hashlib.sha256(json.dumps(command).encode()).hexdigest(),binary_sha256=hashlib.sha256(pathlib.Path(command[0]).read_bytes()).hexdigest(),ns=output['ns'] if mode=='pipeline' else data['elapsed_ns'],digest=output['digest'] if mode=='pipeline' else output['candidates'][0]['id'],process=data))
summary=[]
for label in ['tiny','small']:
 for mode in ['pipeline','ordinary-plan']:
  assert len({r['digest'] for r in rows if r['label']==label and r['mode']==mode})==1
  for revision in ['before','after']:
   samples=[r['ns'] for r in rows if (r['label'],r['mode'],r['revision'])==(label,mode,revision)]
   summary.append(dict(label=label,mode=mode,revision=revision,median_ns=statistics.median(samples),min_ns=min(samples),max_ns=max(samples)))
(BASE/'small-controls.json').write_text(json.dumps(dict(samples=rows,summary=summary,reason='Initial 3-pair small plan and pipeline medians varied. These are additional serial observations, not replacement/removal of those samples. Fresh child per sample; original workload/commands/source/probe binaries retained.'),indent=2)+'\n')
print(json.dumps(summary,indent=2))
