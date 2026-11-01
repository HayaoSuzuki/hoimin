import pathlib,subprocess,sys,json,hashlib
root=pathlib.Path(sys.argv[1]); name=sys.argv[2]
deps=root/'release/deps'; here=pathlib.Path(__file__).resolve().parent; out=pathlib.Path('/tmp/hoimin-perf-three-task3')
args=['rustc','--edition=2024','-O','-C','lto=thin','-C','embed-bitcode=yes',str(here/'probe.rs'),'-L',f'dependency={deps}']
hashes={}
for crate in ['camino','hoimin_core','hoimin_python','hoimin_cli','blake3','serde_json','tokio']:
 matches=list(deps.glob(f'lib{crate}-*.rlib')); assert len(matches)==1,(crate,matches)
 args+=['--extern',f'{crate}={matches[0]}']; hashes[str(matches[0])]=hashlib.sha256(matches[0].read_bytes()).hexdigest()
args+=['-o',str(out/f'probe-{name}')]
subprocess.run(args,check=True,timeout=180)
hashes[str(out/f'probe-{name}')]=hashlib.sha256((out/f'probe-{name}').read_bytes()).hexdigest()
(out/f'build-{name}.json').write_text(json.dumps(dict(command=args,hashes=hashes),indent=2))
