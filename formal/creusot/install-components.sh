#!/bin/bash
# Run only in the task-owned Linux image; caller enforces the entire job deadline.
set -euo pipefail
umask 022
cd /recipe
mkdir -p /opt/creusot/{downloads,src,rust,data/bin,config/creusot}
# Every direct source/solver/Rust download is verified before extraction or execution.
python3 - <<'PY'
import hashlib,json,pathlib,shutil,subprocess,tarfile,zipfile
m=json.loads(pathlib.Path('formal/creusot/toolchain.json').read_text())
for name,a in m['artifacts'].items():
 p=pathlib.Path('/opt/creusot/downloads')/name
 subprocess.run(['curl','--fail','--location','--retry','1','--connect-timeout','15','--max-time','240',a['url'],'-o',str(p)],check=True)
 with p.open('rb') as f: actual=hashlib.file_digest(f,'sha256').hexdigest()
 if actual != a['sha256']: raise SystemExit('artifact hash mismatch: '+name)
 print('VERIFIED',name,actual,flush=True)
 if name in ('z3','cvc5'):
  with zipfile.ZipFile(p) as archive:
   entries=[v for v in archive.namelist() if v.endswith('/bin/'+name)]
   if len(entries)!=1: raise SystemExit('unexpected solver archive: '+name)
   out=pathlib.Path('/opt/creusot/data/bin')/name
   out.write_bytes(archive.read(entries[0]));out.chmod(0o755)
 else:
  dest=pathlib.Path('/opt/creusot/src')/name;dest.mkdir()
  with tarfile.open(p) as archive: archive.extractall(dest,filter='data')
  source=next(dest.iterdir())
  if name.startswith(('rust','cargo','llvm')):
   subprocess.run(['bash',str(source/'install.sh'),'--prefix=/opt/creusot/rust','--disable-ldconfig'],check=True)
   shutil.rmtree(dest)
 if name not in ('why3','why3find'): p.unlink()
# Diagnose actual metadata compilation before the OCaml/Creusot build.
smoke=pathlib.Path('/opt/creusot/target-smoke.rs')
smoke.write_text('pub fn identity(d: std::time::Duration) -> std::time::Duration { d }\n')
for target in m['targets']:
 subprocess.run(['/opt/creusot/rust/bin/rustc','--crate-name','target_smoke','--crate-type','lib','--emit','metadata','--target',target,str(smoke),'-o','/opt/creusot/'+target+'.rmeta'],check=True)
 print('TARGET_METADATA_OK',target,flush=True)
PY
