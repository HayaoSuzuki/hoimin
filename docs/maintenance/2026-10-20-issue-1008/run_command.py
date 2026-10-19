import os, subprocess, sys, time, json
from pathlib import Path
root='/Users/hayao/RustroverProjects/hoimin/.worktrees/issue-1006-line-selection-index-1007-shared-manifest-1008-source-index'
env=os.environ.copy(); env.update(CARGO_BUILD_JOBS='2',CARGO_TARGET_DIR='/tmp/hoimin-perf-three-target',CARGO_INCREMENTAL='0',RUST_TEST_THREADS='2',PREK_NO_CONCURRENCY='1',UV_PROJECT_ENVIRONMENT='/tmp/hoimin-priority-six-venv',UV_CACHE_DIR='/tmp/hoimin-priority-six-uv-cache')
env['PATH']='/tmp/hoimin-priority-six-venv/bin:'+env['PATH']
name,timeout,*cmd=sys.argv[1:]
start=time.monotonic()
p=Path('/tmp/hoimin-perf-three-task3')
(p/(name+'.command.json')).write_text(json.dumps(dict(command=cmd,timeout=int(timeout),cwd=root,environment={k:env[k] for k in ['CARGO_BUILD_JOBS','CARGO_TARGET_DIR','CARGO_INCREMENTAL','RUST_TEST_THREADS','PREK_NO_CONCURRENCY','UV_PROJECT_ENVIRONMENT','UV_CACHE_DIR','PATH']}),indent=2))
with (p/(name+'.log')).open('w') as out:
 try:
  result=subprocess.run(cmd,cwd=root,env=env,stdout=out,stderr=subprocess.STDOUT,timeout=int(timeout)); code=result.returncode
 except subprocess.TimeoutExpired:
  code=124
print((p/(name+'.log')).read_text()[-18000:]); print('EXIT',code,'ELAPSED',time.monotonic()-start)
sys.exit(code)
