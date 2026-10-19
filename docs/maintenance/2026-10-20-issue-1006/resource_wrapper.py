"""One measured child per wrapper process keeps RUSAGE_CHILDREN RSS independent."""
import json,resource,subprocess,sys,time
start=time.perf_counter_ns()
result=subprocess.run(sys.argv[1:],capture_output=True,timeout=25)
elapsed=time.perf_counter_ns()-start
print(json.dumps(dict(code=result.returncode,stdout=result.stdout.decode(),stderr=result.stderr.decode(),elapsed_ns=elapsed,maxrss_bytes=resource.getrusage(resource.RUSAGE_CHILDREN).ru_maxrss)))
