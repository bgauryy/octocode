#!/bin/sh
# follow2.sh <task> <tool> <startStep> <nextKey> : replays .results[0].data.next.<key>.query verbatim until absent
task=$1; tool=$2; step=$3; key=$4
dir=$(cd "$(dirname "$0")" && pwd)
while :; do
  prev="$dir/out/$task.oc.$step.txt"
  q=$(sed '/^--- STDERR ---$/,$d' "$prev" | jq -c --arg k "$key" '(.results[0].data.next[$k].query // .results[0].data.pullRequests[0].next[$k].query) // empty')
  [ -z "$q" ] && break
  step=$((step+1))
  jq -cn --argjson q "$q" '{queries:[$q]}' > "$dir/out/$task.oc.$step.query.json"
  SHOW=0 node "$dir/run.mjs" "$task" oc "$step" "octocode $tool \"\$(cat $dir/out/$task.oc.$step.query.json)\""
done
