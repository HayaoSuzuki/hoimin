#!/bin/bash
# Configuration changes do not rebuild the fixed Rust/OCaml toolchain.
set -euo pipefail
(
  cd /opt/creusot/src/creusot/*
  cargo run --locked --release --bin prelude-generator
  cp target/creusot/packages/creusot/creusot/* /opt/creusot/data/share/why3find/packages/creusot/creusot/
)
cp /recipe/formal/creusot/why3.conf /opt/creusot/config/creusot/why3.conf
cp /recipe/formal/creusot/creusot_why3.conf /recipe/formal/creusot/why3find.json /opt/creusot/data/
cd /recipe
python3 - <<'PY'
import hashlib,json,pathlib,subprocess
m=pathlib.Path('formal/creusot/toolchain.json');data=json.loads(m.read_text())
files=[pathlib.Path(t['path']) for t in data['tools'].values()]
for target in set(data['targets']+[data['host_target']]):
 libs=list(pathlib.Path('/opt/creusot/rust/lib/rustlib',target,'lib').glob('*'))
 if not libs: raise SystemExit('missing target standard library: '+target)
 files.extend(p for p in libs if p.is_file())
files.extend(pathlib.Path('/opt/creusot/rust/lib').glob('librustc_driver-*'))
for directory in ['/opt/creusot/data/share/why3find','/opt/creusot/opam-switch/_opam/share/why3','/opt/creusot/opam-switch/_opam/share/why3find']:
 root=pathlib.Path(directory)
 if root.is_dir(): files.extend(p for p in root.rglob('*') if p.is_file())
def digest(p):
 with p.open('rb') as f:return hashlib.file_digest(f,'sha256').hexdigest()
receipt={'manifest_sha256':digest(m),'files':{str(p):digest(p) for p in files},'opam_packages':subprocess.check_output(['opam','list','--installed','--columns=name,version','--color=never'],text=True)}
pathlib.Path(data['receipt']).write_text(json.dumps(receipt,indent=2)+'\n')
PY
python3 tools/creusot_environment.py check-installation --manifest formal/creusot/toolchain.json
