"""Bounded macOS CLI observation; records final payload, not retained peak."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import subprocess
import tempfile
import threading
import time

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument('--binary', type=Path, required=True)
parser.add_argument('--inputs', type=Path, required=True)
parser.add_argument('--revision', required=True)
parser.add_argument('--repetitions', type=int, default=3)
parser.add_argument('--build-profile', choices=['debug','release'], default='debug')
parser.add_argument('--names', nargs='+', default=['proofreadtools', 'django', 'generated'])
args = parser.parse_args()
exe = args.binary.resolve(strict=True)
print(json.dumps({'binary': str(exe), 'sha256': hashlib.sha256(exe.read_bytes()).hexdigest(),
                  'revision': args.revision, 'profile': args.build_profile + '; default features; incremental=0',
                  'scope': 'public plan CLI; wall/RSS observations, final candidate payload is not charged peak'}), flush=True)
fields = ('path', 'original', 'replacement', 'operator', 'symbol', 'file_hash', 'id')
for name in args.names:
    source = args.inputs / name / 'source.py'
    for operator, profile in (('default', 'full'), ('boolean_literal', 'full'), ('default', 'focused')):
        for repetition in range(args.repetitions):
            command = [str(exe), 'plan', '--root', str(source.parent), '--file', 'source.py',
                       '--profile', profile, '--max-candidates', '10000', '--allow-best-effort-memory']
            if operator != 'default':
                command.extend(('--operators', operator))
            command.extend(('--', 'python3', '-c', 'pass'))
            with tempfile.TemporaryFile() as output, tempfile.TemporaryFile() as error:
                start = time.perf_counter()
                child = subprocess.Popen(command, stdout=output, stderr=error)
                timer = threading.Timer(20, child.kill)
                timer.start()
                try:
                    _, status, usage = os.wait4(child.pid, 0)
                    child.returncode = os.waitstatus_to_exitcode(status)
                finally:
                    timer.cancel()
                    timer.join()
                elapsed = time.perf_counter() - start
                error.seek(0)
                stderr = error.read().decode()
                if child.returncode != 0:
                    print(json.dumps({'input': name, 'operator': operator, 'profile': profile, 'repetition': repetition, 'seconds': elapsed, 'peak_rss_macos_bytes': usage.ru_maxrss, 'exit': child.returncode, 'stderr': stderr}), flush=True)
                    break
                output.seek(0)
                plan = json.load(output)
            candidates = plan['candidates']
            encoded = json.dumps(candidates, ensure_ascii=False, sort_keys=True, separators=(',', ':')).encode()
            payload = sum(len((candidate.get(key) or '').encode()) for candidate in candidates for key in fields)
            print(json.dumps({'input': name, 'operator': operator, 'profile': profile, 'repetition': repetition,
                              'source_sha256': hashlib.sha256(source.read_bytes()).hexdigest(),
                              'candidates': len(candidates), 'truncated': plan['truncated'],
                              'candidate_sha256': hashlib.sha256(encoded).hexdigest(),
                              'final_candidate_string_bytes': payload, 'candidate_json_bytes': len(encoded),
                              'seconds': elapsed, 'cpu_seconds': usage.ru_utime + usage.ru_stime,
                              'peak_rss_macos_bytes': usage.ru_maxrss, 'exit': child.returncode, 'stderr': stderr}), flush=True)
