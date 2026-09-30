#!/bin/zsh
# usage: p.sh name tool 'json' [extra env]
D=/Users/bgaryy/code/octocode/octocode-local-testing/bench/validate/features/probes/github
name=$1; tool=$2; json=$3
echo "$tool $json" > $D/$name.cmd
octocode $tool "$json" > $D/$name.out 2> $D/$name.err; ec=$?
echo "exit=$ec bytes=$(wc -c < $D/$name.out) errbytes=$(wc -c < $D/$name.err)" | tee $D/$name.meta
