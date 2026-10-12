"""Run the bounded discovery driver against fixed sources and selectors.

Usage: python analysis-matrix.py EXE LABEL
"""
import json
import subprocess
import sys
from pathlib import Path

executable,label=sys.argv[1:]
root=Path('.superpowers/runtime-improvements')
rows=[]
for name in ('proofreadtools','django','generated'):
    for selector,profile in (('default','full'),('boolean_literal','full'),('default','focused')):
        output=root/f'analysis-{label}-{name}-{selector}-{profile}.json'
        result=subprocess.run([
            sys.executable,str(Path(__file__).with_name('measure-windows.py')),
            str(output),'60','0',executable,str(root/'analysis-inputs'/name),selector,profile,
        ],capture_output=True,text=True,check=True,timeout=65)
        row=json.loads(result.stdout)
        records=[json.loads(line) for line in row['stdout'].splitlines()]
        assert len(records)==7 and len({r['candidate_hash'] for r in records})==1
        row.update(name=name,selector=selector,profile=profile)
        rows.append(row)
        print(name,selector,profile,records[0]['candidates'],row['peak_rss_bytes'])
Path(f'docs/performance/runtime-2026-10-12/analysis-{label}.json').write_text(json.dumps(rows,indent=2),encoding='utf-8')
