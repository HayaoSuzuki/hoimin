"""Archive exact receipts and top-level test outcomes without build-log bulk."""
from pathlib import Path
import hashlib,json,re,shutil
out=Path('/tmp/hoimin-perf-three-integration');dest=Path('docs/maintenance/2026-10-20-perf-three/receipts');runs=[]
pattern=re.compile(r'^test result: (\w+)\. (\d+) passed; (\d+) failed; (\d+) ignored; (\d+) measured; (\d+) filtered out;')
for receipt in sorted(out.glob('*.receipt.json')):
 row=json.loads(receipt.read_text());name=row['name'];log=out/(name+'.log');lines=log.read_text(errors='replace').splitlines();sections=[];section=None
 for i,line in enumerate(lines):
  if line.startswith('     Running ') or line.startswith('   Doc-tests '):
   if section is not None:sections.append(section)
   section={'header':line.strip(),'start_line':i+1,'summaries':[]}
  m=pattern.match(line)
  if m and section is not None:section['summaries'].append({'line':i+1,'text':line,'passed':int(m[2]),'failed':int(m[3]),'ignored':int(m[4]),'measured':int(m[5]),'filtered_out':int(m[6])})
 if section is not None:sections.append(section)
 top=[{'header':s['header'],**s['summaries'][-1],'nested_summaries_excluded':len(s['summaries'])-1} for s in sections if s['summaries']]
 row['top_level_rust_summaries']=top
 if top:row['top_level_rust_counts']={k:sum(s[k] for s in top) for k in ['passed','failed','ignored','measured','filtered_out']}
 row['notices']=[{'line':i+1,'text':line} for i,line in enumerate(lines) if re.search(r'(^warning|\bwarning:|\bSKIP\b|\.\.\. ignored)',line,re.I)]
 row['header']=lines[:20];row['tail']=lines[-8:];runs.append(row);shutil.copy2(receipt,dest/receipt.name)
result={'counting':'For each top-level Running/Doc-tests section, count only its final harness summary. Earlier nested helper summaries are excluded. Counts from separate suites overlap and are not added as unique cases. Python/gate summaries retain their own meaning.','runs':runs}
(dest/'integration-summary.json').write_text(json.dumps(result,indent=2)+'\n')
for r in runs:print(r['name'],r['exit_code'],r.get('top_level_rust_counts'),len(r['notices']),'notices')
