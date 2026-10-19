"""Three alternating pairs per bounded workload; one process and cleanup at a time."""
import hashlib
import json
import pathlib
import statistics
import subprocess
import sys
import time

before, after, destination = map(pathlib.Path, sys.argv[1:])
observations = []
started = time.monotonic()
for files in (1000, 4000):
    for payload in (0, 64):
        for mode in ('clone', 'worker'):
            for handles in (1, 4, 16):
                for pair in range(3):
                    order = [('before', before), ('after', after)]
                    if pair % 2:
                        order.reverse()
                    paired = []
                    for revision, binary in order:
                        command = [str(binary), mode, str(files), str(handles), str(payload)]
                        result = subprocess.run(command, check=True, text=True, capture_output=True, timeout=120)
                        row = json.loads(result.stdout)
                        row.update(revision=revision, pair=pair, command=command)
                        observations.append(row)
                        paired.append(row)
                        destination.write_text(json.dumps({'observations': observations}, indent=2) + '\n')
                    for key in ('digest', 'logical_bytes', 'worker_charge', 'snapshot_plus_worker_payload_bytes'):
                        assert paired[0][key] == paired[1][key], (key, paired)
                print(files, payload, mode, handles, round(time.monotonic() - started, 2), flush=True)
summary = []
for files in (1000, 4000):
    for payload in (0, 64):
        for mode in ('clone', 'worker'):
            for handles in (1, 4, 16):
                row = dict(files=files, payload_bytes=payload, mode=mode, handles=handles)
                for revision in ('before', 'after'):
                    samples = [r for r in observations if all(r[key] == row[key] for key in ('files', 'payload_bytes', 'mode', 'handles')) and r['revision'] == revision]
                    row[revision] = {key: dict(median=statistics.median(r[key] for r in samples), min=min(r[key] for r in samples), max=max(r[key] for r in samples)) for key in ('retained', 'peak', 'elapsed_ns')}
                summary.append(row)
destination.write_text(json.dumps(dict(observations=observations, summary=summary, elapsed_seconds=time.monotonic()-started, binary_sha256={str(p): hashlib.sha256(p.read_bytes()).hexdigest() for p in (before, after)}), indent=2) + '\n')
