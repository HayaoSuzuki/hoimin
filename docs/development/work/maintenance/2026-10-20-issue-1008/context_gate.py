"""Apply the context-only heap bound to a preserved public-API executable.

Frozen-original RED is supplementary sensitivity evidence, run after the initial
constructor-count/discovery RED→GREEN cycle. It is not misdated as a pre-edit run.
"""
import json,pathlib,subprocess,sys
base=pathlib.Path('/tmp/hoimin-perf-three-task3')
command=[str(base/f'probe-{sys.argv[1]}'),str(base/'fixture-262144'),'context','262144']
p=subprocess.run(command,capture_output=True,text=True,check=True,timeout=25)
row=json.loads(p.stdout);print(json.dumps(dict(command=command,measurement=row)),flush=True)
assert row['requested_peak_delta'] < 1024, row
