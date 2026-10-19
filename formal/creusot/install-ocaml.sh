#!/bin/bash
# The OCaml layer is independent of downstream package/build scripts.
set -euo pipefail
opam init --bare --disable-sandboxing --no-setup --yes /opt/creusot/src/opam/*
opam --cli=2.1 var --global in-creusot-ci=true
opam switch create /opt/creusot/opam-switch ocaml-base-compiler.5.3.0 --yes --jobs=2
