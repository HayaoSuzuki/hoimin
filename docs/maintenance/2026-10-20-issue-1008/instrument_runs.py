import pathlib,subprocess
root=pathlib.Path.cwd(); evidence=root/'docs/maintenance/2026-10-20-issue-1008';base=pathlib.Path('/tmp/hoimin-perf-three-task3')
commands=[]
def run(args,timeout):
 print('COMMAND',args,flush=True);commands.append(args);subprocess.run(args,check=True,timeout=timeout)
try:
 for variant in ['eager','lazy']:
  run(['python3',str(evidence/'instrument.py'),variant],10)
  run(['cargo','build','--locked','--release','-p','hoimin-cli'],300)
  run(['python3',str(evidence/'build_probe.py'),'/tmp/hoimin-perf-three-target','instrument-'+variant],180)
  run(['python3',str(evidence/'count_indexes.py'),variant],120)
finally:
 run(['python3',str(evidence/'instrument.py'),'restore'],10)
 import json
 (base/'instrument-commands.json').write_text(json.dumps(commands,indent=2))
