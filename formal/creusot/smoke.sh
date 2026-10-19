#!/bin/bash
set -euo pipefail
cd /tmp
cargo creusot new smoke_crate
cd smoke_crate
python3 - <<'PY'
from pathlib import Path
p=Path('Cargo.toml')
p.write_text(p.read_text().replace('creusot-std = "0.14.0"', 'creusot-std = { path = "/opt/creusot/src/creusot/creusot-65349025c67e69fcd7479f1f420fa5904f69bc27/creusot-std" }'))
PY
cp /opt/creusot/data/why3find.json why3find.json
python3 /recipe/tools/creusot_environment.py check --manifest /recipe/formal/creusot/toolchain.json --project-root "$PWD"
for round in cold_crate warm_crate; do
  for target in x86_64-unknown-linux-gnu aarch64-apple-darwin x86_64-pc-windows-msvc; do
    if [[ "$round" == warm_crate ]]; then sleep 1; touch src/lib.rs; fi
    printf '\nSMOKE_BEGIN round=%s target=%s utc=%s\n' "$round" "$target" "$(date -u +%FT%TZ)"
    /usr/bin/time -f 'GEN_ELAPSED_SECONDS=%e GEN_PEAK_RSS_KIB=%M' /usr/bin/timeout --signal=KILL 300s cargo creusot --only=coma -j 2 -- --target "$target"
    file=verif/smoke_crate_rlib/add_one.coma
    test -s "$file"
    printf 'COMA_BYTES=%s COMA_SHA256=%s\n' "$(wc -c < "$file")" "$(sha256sum "$file" | cut -d' ' -f1)"
    /usr/bin/time -f 'PROVE_ELAPSED_SECONDS=%e PROVE_PEAK_RSS_KIB=%M' /usr/bin/timeout --signal=KILL 300s cargo creusot --only=prove --no-cache -j 2 --why3find-arg=--goals "$file"
    printf 'SMOKE_END round=%s target=%s utc=%s\n' "$round" "$target" "$(date -u +%FT%TZ)"
  done
done
