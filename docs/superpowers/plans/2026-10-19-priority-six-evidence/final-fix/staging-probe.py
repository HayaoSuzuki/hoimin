import os, pathlib, subprocess, json, hashlib, time, uuid
root=pathlib.Path('/tmp/hoimin-priority-six-final-fix/staging')
root.mkdir()
project=root/'project'; project.mkdir()
(project/'calc.py').write_text('def add(a, b):\n    return a + b\n')
tmp=root/'tmp'; tmp.mkdir()
binary=pathlib.Path('/tmp/hoimin-priority-six-target/debug/hoimin')
cmd=[str(binary),'run','--root',str(project),'--file','calc.py','--operators','binary_add_sub','--min-free-space','1MiB','--allow-best-effort-memory','--format','json']
env=os.environ.copy();env['TMPDIR']=str(tmp)
records=[]
def run(case,verbose=False):
    args=cmd+(['--verbose'] if verbose else [])+['--','/usr/bin/true']
    output=subprocess.run(args,env=env,capture_output=True)
    (root/(case+'.stdout')).write_bytes(output.stdout)
    (root/(case+'.stderr')).write_bytes(output.stderr)
    report=json.loads(output.stdout)
    records.append({'case':case,'command':args,'exit':output.returncode,'janitor':report['summary']['disk']['janitor'],'stdout_sha256':hashlib.sha256(output.stdout).hexdigest(),'stderr_sha256':hashlib.sha256(output.stderr).hexdigest()})
    return records[-1]
control=run('control')
staging=tmp/'hoimin-workspaces-v1'/('.staging-'+str(uuid.uuid4()));staging.mkdir(mode=0o700)
before=staging.stat().st_mtime_ns
default=run('young_default')
after_default=staging.stat().st_mtime_ns
verbose=run('young_verbose',True)
after_verbose=staging.stat().st_mtime_ns
entries=list(staging.iterdir())
stale=time.time()-25*60*60;os.utime(staging,(stale,stale))
aged=run('stale_control')
receipt={'source_head':subprocess.check_output(['git','rev-parse','HEAD'],text=True).strip(),'binary_sha256':hashlib.sha256(binary.read_bytes()).hexdigest(),'TMPDIR':str(tmp),'staging':str(staging),'young_mtime_ns':[before,after_default,after_verbose],'young_entries':[str(p) for p in entries],'stale_after_exists':staging.exists(),'records':records}
(root/'receipt.json').write_text(json.dumps(receipt,indent=2)+'\n')
assert all(record['exit']==1 for record in records),records
assert control['janitor']['inspected_roots']==0
for record in [default,verbose]:
    summary=record['janitor']
    assert summary['inspected_roots']==summary['preserved_roots']==1,summary
    assert summary['preserved_by_reason']=={'young':1},summary
    assert summary['reclaimed_roots']==summary['unresolved_roots']==summary['removed_entries']==0,summary
assert default['janitor']['details']==[]
assert default['janitor']['detail_omissions']['suppressed_normal_details']==1
assert len(verbose['janitor']['details'])==1 and verbose['janitor']['details'][0]['reason']=='young'
assert staging.name in verbose['janitor']['details'][0]['message']
assert before==after_default==after_verbose and not entries
assert aged['janitor']['reclaimed_roots']==1 and aged['janitor']['unresolved_roots']==0
assert not staging.exists()
print('control, young default, young verbose, stale control passed; all exit 1 for surviving mutant')
