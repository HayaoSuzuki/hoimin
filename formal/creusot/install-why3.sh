#!/bin/bash
# Fixed Why3/Why3find archives remain checksummed even as local opam inputs.
set -euo pipefail
cd /recipe
python3 - <<'PY'
import json,pathlib,tarfile
m=json.loads(pathlib.Path('formal/creusot/toolchain.json').read_text())
repo=pathlib.Path('/opt/creusot/opam-pins');repo.mkdir()
(repo/'repo').write_text('opam-version: "2.0"\n')
base=next(pathlib.Path('/opt/creusot/src/opam').iterdir())
for name,version in [('why3','git-05bf4ddf'),('why3find','git-0f054b93')]:
 if name=='why3':
  recipe=(base/'packages/why3/why3.1.8.2/opam').read_text().split('\nurl {')[0]
  recipe=recipe.replace('["./autogen.sh"] {dev}', '["./autogen.sh"]')
  recipe=recipe.replace('"conf-autoconf" {build & dev}', '"conf-autoconf" {build}')
  # Pinned Why3 now builds through Dune. Serialize independent Make goals;
  # Dune itself remains bounded by the approved opam job count.
  recipe=recipe.replace('[make "-j%{jobs}%" "all" "byte"]', '[make "-j1" "DUNEJOBS=%{jobs}%" "all" "byte"]')
  recipe=recipe.replace('[make "install" "install-lib"]', '[make "-j1" "DUNEJOBS=%{jobs}%" "install" "install-lib"]')
 else:
  with tarfile.open('/opt/creusot/downloads/why3find') as archive:
   member=next(v for v in archive.getmembers() if v.name.endswith('/why3find.opam'))
   recipe=archive.extractfile(member).read().decode()
  recipe='\n'.join(line for line in recipe.splitlines() if not line.startswith('version:'))
 recipe+='\nurl {\n  src: "file:///opt/creusot/downloads/'+name+'"\n  checksum: "sha256='+m['artifacts'][name]['sha256']+'"\n}\n'
 package=repo/'packages'/name/(name+'.'+version);package.mkdir(parents=True)
 (package/'opam').write_text(recipe)
PY
opam repository add creusot-pins /opt/creusot/opam-pins --yes
opam install why3.git-05bf4ddf why3find.git-0f054b93 ocamlgraph camlzip zarith --yes --jobs=2
