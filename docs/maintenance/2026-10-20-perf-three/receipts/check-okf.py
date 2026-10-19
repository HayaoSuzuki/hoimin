"""Check local OKF structure, references and recorded commit availability."""
from pathlib import Path
import hashlib,json,re,subprocess,urllib.parse
import yaml
import sys
baseline="--baseline" in sys.argv
ROOT=Path.cwd();bundle=ROOT/'docs/knowledge';pages=sorted(bundle.rglob('*.md'));missing_paths=[];unavailable=[];bad_ids=[];sources=0;revisions={};edges={};changed_sources=[]
changed=set(subprocess.check_output(['git','diff','--name-only'],text=True).splitlines())
for p in pages:
 t=subprocess.check_output(['git','show','f72134d7:'+str(p.relative_to(ROOT))],text=True) if baseline else p.read_text();header=t.startswith('---\n');body=t;meta={}
 if p.name in ('index.md','log.md'):
  if p==bundle/'index.md':
   assert header;front,body=t[4:].split('\n---\n',1);assert yaml.safe_load(front)=={'okf_version':'0.2'}
  else: assert not header,p
 else:
  assert header,p;front,body=t[4:].split('\n---\n',1);meta=yaml.safe_load(front);assert isinstance(meta,dict) and isinstance(meta.get('type'),str) and meta['type'].strip(),p
 ids=[]
 for s in meta.get('sources',[]):
  sources+=1;ids.append(s['id']);r=s['resource'];u=urllib.parse.urlparse(r)
  if not u.scheme:
   target=(p.parent/urllib.parse.unquote(u.path)).resolve()
   if not target.exists():missing_paths.append({'page':str(p.relative_to(ROOT)),'kind':'source','resource':r})
   if s['id'].startswith('perf-three-'):
    assert target.exists(),target
    assert hashlib.sha256(target.read_bytes()).hexdigest()==s['sha256'],(p,r)
    changed_sources.append({'page':str(p.relative_to(ROOT)),'id':s['id'],'resource':r,'hash_match':True})
  rev=str(s['revision']) if s.get('revision') is not None else None
  if rev and not u.scheme:
   if rev not in revisions:revisions[rev]=subprocess.run(['git','cat-file','-e',rev+'^{commit}'],stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL).returncode==0
   if not revisions[rev]:unavailable.append({'page':str(p.relative_to(ROOT)),'id':s['id'],'revision':rev})
 assert len(ids)==len(set(ids)),p
 defs=set(re.findall(r'^\[\^([^]]+)\]:',body,re.M));uses=set(re.findall(r'\[\^([^]]+)\](?!:)',body))
 for key in set(ids)-defs:bad_ids.append({'page':str(p.relative_to(ROOT)),'source_without_footnote_definition':key})
 for key in uses-defs:bad_ids.append({'page':str(p.relative_to(ROOT)),'undefined_footnote':key})
 edges[p.resolve()]=set()
 for match in re.finditer(r'\]\(([^)]+)\)',body):
  r=match.group(1).split(' "',1)[0].strip('<>');u=urllib.parse.urlparse(r)
  if u.scheme or not u.path:continue
  target=(p.parent/urllib.parse.unquote(u.path)).resolve()
  if not target.exists():missing_paths.append({'page':str(p.relative_to(ROOT)),'kind':'link','resource':r})
  if target in [x.resolve() for x in pages]:edges[p.resolve()].add(target)
seen=set();todo=[(bundle/'index.md').resolve()]
while todo:
 p=todo.pop()
 if p in seen:continue
 seen.add(p);todo.extend(edges.get(p,set())-seen)
catalogs={}
for name,directory in [('design-documents','docs/superpowers/specs'),('audit-documents','docs/superpowers/reports')]:
 cat=bundle/'references'/f'{name}.md';text=subprocess.check_output(['git','show','f72134d7:'+str(cat.relative_to(ROOT))],text=True) if baseline else cat.read_text();recorded={(cat.parent/r).resolve() for r in re.findall(r'  resource: (.+)',text) if not urllib.parse.urlparse(r).scheme};expected=set((ROOT/directory).rglob('*.md'))
 if name=='audit-documents':expected|=set((ROOT/'docs/audits').rglob('*.md'))
 catalogs[name]={'expected_markdown_files':len(expected),'missing_catalog_resources':[str(p.relative_to(ROOT)) for p in sorted(expected-recorded)]}
result={'structure_pages':len(pages),'source_entries':sources,'changed_source_checks':changed_sources,'local_missing_paths':missing_paths,'unavailable_commit_references':unavailable,'unique_unavailable_commits':[r for r,v in revisions.items() if not v],'footnote_findings':bad_ids,'reachable_pages':len(seen),'unreachable_pages':[str(p.relative_to(ROOT)) for p in pages if p.resolve() not in seen],'catalogs':catalogs,'scope':'Local path existence, source IDs/footnotes, added-source current hashes, commit object availability, catalog coverage and bundle reachability. External URLs and fragments NOT checked. Historical source hashes NOT refreshed.'}
Path('/tmp/hoimin-perf-three-integration/okf-baseline.json' if baseline else '/tmp/hoimin-perf-three-integration/okf-references.json').write_text(json.dumps(result,indent=2)+'\n')
print('baseline' if baseline else 'current', 'pages',len(pages),'reachable',len(seen),'missing paths',len(missing_paths),'unavailable refs',len(unavailable),'footnote findings',len(bad_ids))
