import pathlib,subprocess,sys
root=pathlib.Path(sys.argv[1]);name=sys.argv[2]
deps=root/'release/deps'
args=['rustc','--edition=2024','-O','-C','lto=thin','-C','embed-bitcode=yes','/tmp/hoimin-perf-three-task1/probe.rs','-L',f'dependency={deps}']
for crate in ['camino','hoimin_core','hoimin_python','blake3']:
 matches=list(deps.glob(f'lib{crate}-*.rlib'))
 assert len(matches)==1,(crate,matches)
 args+=['--extern',f'{crate}={matches[0]}']
args+=['-o',f'/tmp/hoimin-perf-three-task1/probe-{name}']
print(args,flush=True)
subprocess.run(args,check=True,timeout=180)
