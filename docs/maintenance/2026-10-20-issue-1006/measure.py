"""Bounded serial paired release API/CLI measurements; no regression time threshold."""
import hashlib,json,pathlib,platform,re,statistics,subprocess,time
BASE=pathlib.Path('/tmp/hoimin-perf-three-task1')

def run(command,cwd=None):
 result=subprocess.run(['python3',str(BASE/'resource_wrapper.py'),*command],cwd=cwd,capture_output=True,timeout=30,check=True)
 wrapper=json.loads(result.stdout)
 assert wrapper['code'] in [0,4],(command,wrapper)
 child=subprocess.CompletedProcess(command,wrapper['code'],wrapper['stdout'].encode(),wrapper['stderr'].encode())
 return child,wrapper['elapsed_ns'],wrapper['maxrss_bytes']

def digest(data):return hashlib.sha256(data).hexdigest()
def percentile_row(rows,keys):
 return {key:{'median':statistics.median(r[key] for r in rows),'min':min(r[key] for r in rows),'max':max(r[key] for r in rows)} for key in keys}

rows=[]
for n,mode in [(n,m) for n in [1000,2000,4000,8000] for m in ['sparse','control']]+[(n,m) for n in [8,128] for m in ['sparse','control','all','overlap']]:
 for sample in range(3):
  for revision in (['before','after'] if sample%2==0 else ['after','before']):
   result,elapsed,rss=run([str(BASE/f'probe-{revision}'),str(n),mode])
   row=json.loads(result.stdout);row.update(revision=revision,sample=sample,process_elapsed_ns=elapsed,process_max_rss_bytes=rss)
   rows.append(row)
 print('api',n,mode,flush=True)

root=BASE/'cli-project';root.mkdir(exist_ok=True)
source='flag = True\n\n'*1000
(root/'subject.py').write_text(source)
common=['plan','--root',str(root),'--operators','boolean_literal','--max-candidates','1','--min-free-space','1B','--allow-best-effort-memory']
line_args=[arg for n in range(1,1001) for arg in ['--line',f'subject.py:{2*n}-{2*n}']]
binaries={'before':'/tmp/hoimin-perf-three-baseline/release/hoimin','after':'/tmp/hoimin-perf-three-target/release/hoimin'}
cli_rows=[]
def cli_measure(mode,args):
 # Whole CLI warmup, including config, Git, source and machine output work.
 for revision in ['before','after']:run([binaries[revision],*common,*args,'--','python3','-c','pass'])
 for sample in range(3):
  for revision in (['before','after'] if sample%2==0 else ['after','before']):
   result,elapsed,rss=run([binaries[revision],*common,*args,'--','python3','-c','pass'])
   output=json.loads(result.stdout)
   stable={k:output[k] for k in ['candidates','diagnostics','truncated']}
   (BASE/f'cli-{mode}-{revision}-{sample}.json').write_bytes(result.stdout)
   cli_rows.append(dict(mode=mode,revision=revision,sample=sample,process_elapsed_ns=elapsed,process_max_rss_bytes=rss,stdout_sha256=digest(result.stdout),selection_sha256=digest(json.dumps(stable,sort_keys=True).encode()),output_document_bytes=len(result.stdout),fixture_input_bytes=(root/'subject.py').stat().st_size,candidate_count=len(stable['candidates']),diagnostic_count=len(stable['diagnostics']),truncated=stable['truncated'],argv_count=len(args)))
cli_measure('many-line',line_args)
cli_measure('selected-overlap',['--line','subject.py:5-9','--line','subject.py:1-7','--line','subject.py:3-3'])
# Noncontiguous changed blank lines preserve the same zero-candidate workload.
for args in [['init','-q'],['add','subject.py'],['-c','user.name=Fixture','-c','user.email=fixture@example.invalid','-c','commit.gpgsign=false','-c','core.hooksPath=/dev/null','commit','-qm','fixture']]:
 subprocess.run(['git',*args],cwd=root,check=True,capture_output=True,timeout=20)
(root/'subject.py').write_text(source.replace('\n\n','\n# changed\n'))
cli_measure('changed',['--source','.','--changed'])

for n,mode in sorted({(r['n'],r['mode']) for r in rows}):
 before=[r for r in rows if (r['n'],r['mode'],r['revision'])==(n,mode,'before')]
 after=[r for r in rows if (r['n'],r['mode'],r['revision'])==(n,mode,'after')]
 assert {r['identity_digest'] for r in before+after}.__len__()==1,(n,mode)
for mode in ['many-line','selected-overlap','changed']:
 assert len({r['selection_sha256'] for r in cli_rows if r['mode']==mode})==1

summaries=[]
for n,mode,revision in sorted({(r['n'],r['mode'],r['revision']) for r in rows}):
 subset=[r for r in rows if (r['n'],r['mode'],r['revision'])==(n,mode,revision)]
 summaries.append(dict(n=n,mode=mode,revision=revision,metrics=percentile_row(subset,['ns','requested_peak_delta','requested_live_delta','allocation_calls','process_max_rss_bytes'])))
result={'platform':platform.platform(),'source_commit_before':'34e3349099ce7fd61838629ee8c0d0b814e65a5d','source_commit_after_base':subprocess.check_output(['git','rev-parse','HEAD'],text=True).strip(),'api_samples':rows,'api_summary':summaries,'cli_samples':cli_rows,'method':{'warmup':'one API warmup inside every probe process; one separate CLI warmup per revision/workload','samples':'3 paired samples, alternating before/after execution order; all serial','timing':'API Instant surrounds analysis only; fixture generation and request construction excluded. CLI process_elapsed_ns surrounds the whole CLI subprocess; excludes Python wrapper startup.','heap':'requested live/peak byte deltas and allocator call counts sampled around API analysis; caller-owned source/ranges/request excluded. Does not represent malloc usable size or RSS.','rss':'macOS resource.getrusage(RUSAGE_CHILDREN).ru_maxrss bytes in a fresh wrapper with one bounded child; includes child runtime and API warmup, not API-only or simultaneous aggregate tree peak. /usr/bin/time -l was unusable because sandbox denies kern.clockrate sysctl.','payload':'output_string_bytes counts returned path/original/replacement/symbol bytes only; not full logical candidate payload charge. Sparse/control have no candidates and zero payload charge. Dedicated unit tests establish exact payload boundaries.','disk':'CLI fixture input sizes recorded; API retains no disk output. JSON stdout is captured, including all plan machine output, so its hash may expose nondeterministic metadata; selection hash projects candidates/diagnostics/truncated.','limits':'30s per invocation; Git fixture command 20s; no concurrent build/benchmark, no timing assertion; scope fixed synthetic workloads.'},'fixture_hashes':{'sparse_source_sha256':digest(source.encode()),'changed_source_sha256':digest((root/'subject.py').read_bytes()),'even_line_selectors_sha256':digest(json.dumps(line_args).encode())},'hashes':{str(p):digest(p.read_bytes()) for p in [pathlib.Path(binaries['before']),pathlib.Path(binaries['after']),BASE/'probe-before',BASE/'probe-after',BASE/'probe.rs',BASE/'measure.py',BASE/'resource_wrapper.py']}}
(BASE/'measurement.json').write_text(json.dumps(result,indent=2)+'\n')
print('wrote measurement.json',flush=True)
