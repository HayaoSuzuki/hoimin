from pathlib import Path
import yaml,re,hashlib,json
root=Path.cwd();bundle=root/'docs/knowledge';concepts=0;reserved=0;fingerprints=0;failures=[];links=[]
changed=['creusot-budget-proofs.md','dependency-policy.md','release-sbom.md','ci-validation.md','rust-toolchains.md','design/architecture.md','references/design-documents.md','references/audit-documents.md','index.md']
for p in sorted(bundle.rglob('*.md')):
 text=p.read_text()
 if p.name=='log.md':
  assert not text.startswith('---\n'),p
  reserved+=1;continue
 if p.name=='index.md':
  if p==bundle/'index.md':
   meta=yaml.safe_load(text.split('---',2)[1])
   assert meta=={'okf_version':'0.2'},p
  else: assert not text.startswith('---\n'),p
  reserved+=1;continue
 assert text.startswith('---\n'),p
 meta=yaml.safe_load(text[4:].split('\n---\n',1)[0]);assert isinstance(meta,dict),p
 assert isinstance(meta.get('type'),str) and meta['type'].strip(),p
 concepts+=1
 if p.relative_to(bundle).as_posix() in changed:
  for source in meta.get('sources',[]):
   if source.get('revision') in ('5033e1bb74c4b01c66dc86c3c9f70fc1b8bf11ee','46ef6b9ef240ce644a11a5ab03f82c69b79f2afd','70ea57b2517777dea8e1445063f9cfd96e877128','312be2c4759be00a26e2f04519258f1172057a67','fb89c55c1ab6b851302f36d0fef7fd9f7697655a'):
    target=(p.parent/source['resource']).resolve();assert target.is_file(),(p,target)
    assert hashlib.sha256(target.read_bytes()).hexdigest()==source['sha256'],(p,source['resource'])
    fingerprints+=1
  ids=re.findall(r'\[\^([^\]]+)\](?!:)',text);defs=set(re.findall(r'^\[\^([^\]]+)\]:',text,re.M));assert set(ids)<=defs,(p,set(ids)-defs)
for name in changed:
 p=bundle/name
 for value in re.findall(r'\]\(([^\s)]+)',p.read_text()):
  if '://' in value or value.startswith('#'):continue
  target=value.split('#')[0]
  if target and not (p.parent/target).exists():links.append({'page':name,'target':target})
result={'safe_yaml_concepts':concepts,'reserved_files':reserved,'current_source_fingerprints':fingerprints,'changed_page_missing_links':links,'scope':'OKF structure, current-source fingerprints and file links/footnote definitions; historical source hashes retained, anchors and remote URLs not fetched'}
print(json.dumps(result,indent=2));assert not links
