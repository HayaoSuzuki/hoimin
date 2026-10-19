"""Observe real temporary constructor/drop logs, separately from ordinary timing."""
import hashlib,json,pathlib,re,subprocess,sys
BASE=pathlib.Path('/tmp/hoimin-perf-three-task3'); variant=sys.argv[1]; rows=[]
for label,n in [('262144',262144),('524288',524288),('1048576',1048576),('tiny',0),('small',128)]:
 for mode in ['context','pipeline','discover','plan','run']:
  for sample in range(3):
   command=[str(BASE/f'probe-instrument-{variant}'),str(BASE/f'fixture-{label}'),mode,str(n)]
   p=subprocess.run(command,capture_output=True,text=True,timeout=25,check=True)
   events=p.stderr.splitlines();builds=[re.fullmatch(r'INDEX_EVENT build count=(\d+) live=(\d+) bytes=(\d+)',line) for line in events];builds=[b for b in builds if b]
   drops=[line for line in events if line.startswith('INDEX_EVENT drop')]
   validations=[line for line in events if line.startswith('INDEX_EVENT validation')]
   rows.append(dict(label=label,mode=mode,sample=sample,variant=variant,command=command,command_sha256=hashlib.sha256(json.dumps(command).encode()).hexdigest(),probe_sha256=hashlib.sha256(pathlib.Path(command[0]).read_bytes()).hexdigest(),events=events,builds=len(builds),drops=len(drops),max_live=max([int(b[2]) for b in builds],default=0),validation_materializations=len(validations),output=json.loads(p.stdout)))
   assert len(builds)==len(drops),(command,events)
(BASE/f'counts-{variant}.json').write_text(json.dumps(rows,indent=2)+'\n')
print(variant,[(r['label'],r['mode'],r['builds'],r['max_live'],r['validation_materializations']) for r in rows if r['sample']==0])
