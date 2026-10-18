from pathlib import Path
from urllib.parse import unquote,urlsplit
import hashlib,json,re,subprocess,yaml
root=Path('docs/knowledge').resolve(); repo=Path('.').resolve(); pages=sorted(root.rglob('*.md')); graph={}; count=0; sources=0; errors=[]; newchecks=0
for p in pages:
 text=p.read_text(); header=text.startswith('---\n'); meta={}
 if p.name in ('index.md','log.md'):
  if p==root/'index.md':
   assert header;front,body=text[4:].split('\n---\n',1);assert yaml.safe_load(front)=={'okf_version':'0.2'}
  else:assert not header
 else:
  assert header,p;front,body=text[4:].split('\n---\n',1);meta=yaml.safe_load(front);assert isinstance(meta,dict) and isinstance(meta.get('type'),str) and meta['type'].strip(),p
 ids=[s['id'] for s in meta.get('sources',[])];assert len(ids)==len(set(ids)),p
 definitions=set(re.findall(r'^\[\^([^]]+)\]:',text,re.M));refs=set(re.findall(r'\[\^([^]]+)\](?!:)',text))
 if set(ids)-definitions:errors.append((str(p.relative_to(repo)),'source-footnote missing',sorted(set(ids)-definitions)))
 if refs-definitions:errors.append((str(p.relative_to(repo)),'undefined footnote',sorted(refs-definitions)))
 for source in meta.get('sources',[]):
  sources+=1;resource=source['resource'];u=urlsplit(resource)
  if u.scheme or resource.startswith('/') or '*' in resource:continue
  target=(p.parent/unquote(u.path)).resolve()
  if not target.exists():errors.append((str(p.relative_to(repo)),'missing source',resource))
  if source.get('revision')=='4ebed4457519920e9a28f8f653d8ce96985acdad' or '/2026-10-18-issues-971-979/' in resource:
   newchecks+=1;assert hashlib.sha256(target.read_bytes()).hexdigest()==source['sha256'],(p,resource)
   if source['working_tree']=='clean':
    blob=subprocess.check_output(['git','show',f"{source['revision']}:{target.relative_to(repo)}"]);assert hashlib.sha256(blob).hexdigest()==source['sha256']
 graph[p]=[]
 # Local inline links; code examples are not treated as repository references.
 text=re.sub(r'```.*?```','',text,flags=re.S)
 for link in re.findall(r'(?<!!)\[[^\]\n]*\]\(([^)\n]+)\)',text):
  link=link.split(' "',1)[0].strip('<>');u=urlsplit(link)
  if u.scheme or u.netloc or not u.path:continue
  target=(root/u.path.lstrip('/') if u.path.startswith('/') else p.parent/unquote(u.path)).resolve();count+=1
  if not target.exists():errors.append((str(p.relative_to(repo)),'missing link',link))
  if target in pages:graph[p].append(target)
seen=set();todo=[root/'index.md']
while todo:
 p=todo.pop()
 if p in seen:continue
 seen.add(p);todo.extend(graph.get(p,[]))
assert len(seen)==len(pages),(len(seen),len(pages),set(pages)-seen)
# Catalogs cover originals only, excluding plans and maintenance.
for cat,dirs in [('design-documents.md',['docs/superpowers/specs']),('audit-documents.md',['docs/superpowers/reports','docs/audits'])]:
 text=(root/'references'/cat).read_text()
 for d in dirs:
  for p in Path(d).rglob('*.md'):
   rel='../../'+str(p.relative_to('docs'))
   if rel not in text:errors.append((cat,'catalog missing',str(p)))
print(json.dumps(dict(pages=len(pages),sources=sources,local_links=count,reachable=len(seen),new_source_hashes_and_clean_revision_blobs=newchecks,errors=errors),indent=2))
baseline=json.loads(Path('docs/maintenance/2026-10-18-issues-971-979/okf-audit.json').read_text())['baseline']
assert errors == baseline['errors'], 'new or changed reference/catalog finding'
print('Structure, local targets, navigation, new-source provenance and no-new-reference-findings checks passed; baseline catalog findings retained.')
