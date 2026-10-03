#!/bin/bash
# usage: p.sh NAME TOOL JSON  -> saves probes/local/NAME.out, prints exit + bytes + head
D=/Users/bgaryy/code/octocode/octocode-local-testing/bench/validate/features/probes/local
cd /Users/bgaryy/code/octocode
out=$(octocode "$2" "$3" 2>$D/$1.err); ec=$?
printf '%s' "$out" > $D/$1.out
echo "[$1] exit=$ec bytes=${#out} stderr=$(head -c 300 $D/$1.err)"
printf '%s\n' "$out" | head -c ${HEADN:-1500}; echo
